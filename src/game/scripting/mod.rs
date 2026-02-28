//! Scripting engine — replaces `c_src/scripting.c`.
#![allow(non_snake_case, dead_code, unused_variables)]

pub mod async_coro;
pub mod ffi;
pub mod globals;
pub mod types;

use mlua::Lua;
use std::ffi::{CStr, CString, c_char, c_int, c_uint};
use std::os::raw::c_void;

use crate::database::map_db::BlockList;
use types::floor::FloorListObject;
use types::item::*;
use types::mob::MobObject;
use types::npc::NpcObject;
use types::pc::PcObject;
use types::registry::*;

// ---------------------------------------------------------------------------
// Global Lua state — single instance, lives for the process lifetime.
// Mirrors `lua_State *sl_gstate` in scripting.c.
// ---------------------------------------------------------------------------
static mut SL_STATE: Option<Lua> = None;

/// Returns a reference to the global Lua state.
/// # Safety
/// Must only be called after `sl_init()`.  All scripting runs on the LocalSet
/// thread (timer_do + session_io_task), so no external locking is needed.
pub unsafe fn sl_state() -> &'static Lua {
    SL_STATE.as_ref().expect("sl_init() not called")
}

/// Raw lua_State pointer — exported so C code using `sl_gstate` still compiles.
/// Set after init. Leave null if mlua does not expose a stable raw accessor.
#[no_mangle]
pub static mut sl_gstate: *mut c_void = std::ptr::null_mut();

// ---------------------------------------------------------------------------
// sl_init
// ---------------------------------------------------------------------------
pub fn sl_init() {
    unsafe {
        // LuaJIT on 64-bit requires luaL_newstate() — Lua::new() uses it.
        // Lua::new_with(ALL_SAFE, ...) uses a custom allocator that LuaJIT rejects.
        let lua = Lua::new();

        register_types(&lua).expect("failed to register scripting types");
        globals::register(&lua).expect("failed to register scripting globals");

        SL_STATE = Some(lua);

        // Capture the raw lua_State* via exec_raw and store in sl_gstate so C
        // helpers (sl_compat.c) and async_coro.rs can access it without going
        // through the mlua lock (safe: pointer is stable for process lifetime).
        let _ = SL_STATE.as_ref().unwrap().exec_raw::<()>((), |L| {
            sl_gstate = L as *mut c_void;
        });

        // Reload scripts (lua_dir comes from config).
        sl_reload();
    }
}

/// Convert a Lua value (integer id or light userdata pointer) to a C pointer.
fn lua_val_to_ptr(v: mlua::Value) -> *mut c_void {
    match v {
        mlua::Value::Integer(i)         => i as usize as *mut c_void,
        mlua::Value::LightUserData(ud)  => ud.0,
        _                               => std::ptr::null_mut(),
    }
}

macro_rules! ctor {
    ($lua:expr, $T:ident) => {
        $lua.create_function(|_, v: mlua::Value| Ok($T { ptr: lua_val_to_ptr(v) }))?
    };
}

fn register_types(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    g.set("PC",       ctor!(lua, PcObject))?;
    g.set("MOB",      ctor!(lua, MobObject))?;
    g.set("NPC",      ctor!(lua, NpcObject))?;

    // Player — callable namespace table for PC scripts.
    // Player(id)   → map_id2sd(id), nil if not found.
    // Player(name) → map_name2sd(name), nil if not found.
    let player_tbl = lua.create_table()?;
    let player_mt  = lua.create_table()?;
    player_mt.set("__call", lua.create_function(|lua, (_tbl, v): (mlua::Value, mlua::Value)| -> mlua::Result<mlua::Value> {
        let ptr = match v {
            mlua::Value::Integer(id) => unsafe { ffi::map_id2sd(id as c_uint) },
            mlua::Value::Number(f)   => unsafe { ffi::map_id2sd(f as c_uint) },
            mlua::Value::String(ref s) => {
                let cs = CString::new(s.as_bytes().to_vec()).map_err(mlua::Error::external)?;
                unsafe { ffi::map_name2sd(cs.as_ptr()) }
            }
            _ => std::ptr::null_mut(),
        };
        if ptr.is_null() { return Ok(mlua::Value::Nil); }
        Ok(mlua::Value::UserData(lua.create_userdata(PcObject { ptr })?))
    })?)?;
    player_tbl.set_metatable(Some(player_mt));
    g.set("Player", player_tbl)?;

    // Mob — callable namespace table for mob scripts.
    // Mob(id) → map_id2mob(id), nil if not found.
    let mob_tbl = lua.create_table()?;
    let mob_mt  = lua.create_table()?;
    mob_mt.set("__call", lua.create_function(|lua, (_tbl, v): (mlua::Value, mlua::Value)| -> mlua::Result<mlua::Value> {
        let ptr = match v {
            mlua::Value::Integer(id) => unsafe { ffi::map_id2mob(id as c_uint) },
            mlua::Value::Number(f)   => unsafe { ffi::map_id2mob(f as c_uint) },
            _ => std::ptr::null_mut(),
        };
        if ptr.is_null() { return Ok(mlua::Value::Nil); }
        Ok(mlua::Value::UserData(lua.create_userdata(MobObject { ptr })?))
    })?)?;
    mob_tbl.set_metatable(Some(mob_mt));
    g.set("Mob", mob_tbl)?;
    g.set("REG",      ctor!(lua, RegObject))?;
    g.set("REGS",     ctor!(lua, RegStringObject))?;
    g.set("NPCREG",   ctor!(lua, NpcRegObject))?;
    g.set("MOBREG",   ctor!(lua, MobRegObject))?;
    g.set("MAPREG",   ctor!(lua, MapRegObject))?;
    g.set("GAMEREG",  ctor!(lua, GameRegObject))?;
    g.set("QUESTREG", ctor!(lua, QuestRegObject))?;
    // ITEM/RECIPE/FL need custom ctors that perform DB/id-db lookups.
    g.set("ITEM", lua.create_function(|lua, v: mlua::Value| -> mlua::Result<mlua::Value> {
        let ptr: *mut c_void = match v {
            mlua::Value::Integer(id) => unsafe {
                crate::ffi::item_db::rust_itemdb_search(id as c_uint) as *mut c_void
            },
            mlua::Value::Number(f) => unsafe {
                crate::ffi::item_db::rust_itemdb_search(f as c_uint) as *mut c_void
            },
            mlua::Value::String(ref s) => {
                let text = s.to_str()?;
                let cs = CString::new(text.as_bytes()).map_err(mlua::Error::external)?;
                unsafe { crate::ffi::item_db::rust_itemdb_searchname(cs.as_ptr()) as *mut c_void }
            }
            _ => std::ptr::null_mut(),
        };
        if ptr.is_null() { return Ok(mlua::Value::Nil); }
        Ok(mlua::Value::UserData(lua.create_userdata(ItemObject { ptr })?))
    })?)?;
    g.set("BITEM",    ctor!(lua, BItemObject))?;
    g.set("BANKITEM", ctor!(lua, BankItemObject))?;
    g.set("PARCEL",   ctor!(lua, ParcelObject))?;
    g.set("RECIPE", lua.create_function(|lua, v: mlua::Value| -> mlua::Result<mlua::Value> {
        let ptr: *mut c_void = match v {
            mlua::Value::Integer(id) => unsafe {
                crate::ffi::recipe_db::rust_recipedb_search(id as c_uint) as *mut c_void
            },
            mlua::Value::Number(f) => unsafe {
                crate::ffi::recipe_db::rust_recipedb_search(f as c_uint) as *mut c_void
            },
            mlua::Value::String(ref s) => {
                let text = s.to_str()?;
                let cs = CString::new(text.as_bytes()).map_err(mlua::Error::external)?;
                unsafe { crate::ffi::recipe_db::rust_recipedb_searchname(cs.as_ptr()) as *mut c_void }
            }
            _ => std::ptr::null_mut(),
        };
        if ptr.is_null() { return Ok(mlua::Value::Nil); }
        Ok(mlua::Value::UserData(lua.create_userdata(RecipeObject { ptr })?))
    })?)?;
    g.set("FL", lua.create_function(|lua, id: c_uint| -> mlua::Result<mlua::Value> {
        let ptr = unsafe { ffi::map_id2fl(id) };
        if ptr.is_null() { return Ok(mlua::Value::Nil); }
        Ok(mlua::Value::UserData(lua.create_userdata(FloorListObject { ptr })?))
    })?)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// sl_reload
// ---------------------------------------------------------------------------
pub unsafe fn sl_reload() -> c_int {
    let lua = sl_state();
    let cfg = crate::ffi::config::config();
    match load_lua_dir(lua, &cfg.lua_dir) {
        Ok(_)  => 0,
        Err(e) => { tracing::error!("[scripting] sl_reload failed: {e:#}"); -1 }
    }
}

fn load_lua_file(lua: &Lua, path: &std::path::Path) -> mlua::Result<()> {
    let src = std::fs::read(path)
        .map_err(|e| mlua::Error::external(e))?;
    let name = path.to_string_lossy();
    lua.load(src.as_slice()).set_name(name.as_ref()).eval::<()>()
}

fn load_lua_dir(lua: &Lua, dir: &str) -> mlua::Result<()> {
    // Load sys.lua first.
    let sys = std::path::PathBuf::from(format!("{dir}/sys.lua"));
    if sys.exists() {
        load_lua_file(lua, &sys)?;
    }
    load_dir_recursive(lua, dir)
}

fn load_dir_recursive(lua: &Lua, dir: &str) -> mlua::Result<()> {
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r, Err(_) => return Ok(()),
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || name == "sys.lua" { continue; }
        if path.is_dir() {
            load_dir_recursive(lua, path.to_str().unwrap_or(""))?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("lua") {
            if let Err(e) = load_lua_file(lua, &path) {
                tracing::warn!("[scripting] error loading {}: {e}", path.display());
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// sl_fixmem + sl_luasize
// ---------------------------------------------------------------------------
pub unsafe fn sl_fixmem() {
    if let Ok(gc) = sl_state().globals().get::<mlua::Function>("collectgarbage") {
        let _ = gc.call::<()>("collect");
    }
}

pub unsafe fn sl_luasize() -> c_int {
    sl_state().globals()
        .get::<mlua::Function>("collectgarbage")
        .and_then(|f| f.call::<f64>("count"))
        .map(|kb| kb as c_int)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

pub(crate) unsafe fn bl_to_lua(lua: &Lua, bl: *mut c_void) -> mlua::Result<mlua::Value> {
    if bl.is_null() { return Ok(mlua::Value::Nil); }
    let bl_type = (*(bl as *const BlockList)).bl_type as c_int;
    match bl_type {
        ffi::BL_PC  => lua.pack(PcObject  { ptr: bl }),
        ffi::BL_MOB => lua.pack(MobObject { ptr: bl }),
        ffi::BL_NPC => lua.pack(NpcObject { ptr: bl }),
        _           => Ok(mlua::Value::Nil),
    }
}

unsafe fn call_lua(
    root: *const c_char,
    method: *const c_char,
    args: mlua::MultiValue,
) -> bool {
    let lua = sl_state();
    let root_s   = match CStr::from_ptr(root).to_str()   { Ok(s) => s, Err(_) => return false };
    let method_s = match CStr::from_ptr(method).to_str() { Ok(s) => s, Err(_) => return false };

    let tbl: mlua::Table = match lua.globals().get(root_s)   { Ok(t) => t, Err(_) => return false };
    let func: mlua::Function = match tbl.get(method_s)        { Ok(f) => f, Err(_) => return false };

    if let Err(e) = func.call::<mlua::MultiValue>(args) {
        tracing::warn!("[scripting] {root_s}.{method_s}: {e}");
    }
    true
}

pub unsafe fn sl_doscript_blargs_vec(
    root: *const c_char, method: *const c_char,
    nargs: c_int, args: *const *mut c_void,
) -> c_int {
    let lua = sl_state();
    let mut mv = mlua::MultiValue::new();
    for i in 0..nargs as usize {
        mv.push_back(bl_to_lua(lua, *args.add(i)).unwrap_or(mlua::Value::Nil));
    }
    call_lua(root, method, mv) as c_int
}

pub unsafe fn sl_doscript_strings_vec(
    root: *const c_char, method: *const c_char,
    nargs: c_int, args: *const *const c_char,
) -> c_int {
    let lua = sl_state();
    let mut mv = mlua::MultiValue::new();
    for i in 0..nargs as usize {
        let s = CStr::from_ptr(*args.add(i)).to_string_lossy().into_owned();
        mv.push_back(lua.pack(s).unwrap_or(mlua::Value::Nil));
    }
    call_lua(root, method, mv) as c_int
}

pub unsafe fn sl_doscript_stackargs(
    root: *const c_char, method: *const c_char, _nargs: c_int,
) -> c_int {
    // Args-on-stack path requires direct stack access not exposed by mlua.
    // Return 0 (not found) until map_parse.c is ported and this path is audited.
    tracing::warn!("[scripting] sl_doscript_stackargs not yet implemented");
    0
}

pub unsafe fn sl_exec_str(user: *mut c_void, code: *const c_char) {
    let s = CStr::from_ptr(code).to_string_lossy();
    let lua = sl_state();
    if let Err(e) = lua.load(s.as_ref()).eval::<()>() {
        tracing::warn!("[scripting] sl_exec error: {e}");
    }
}

pub unsafe fn sl_updatepeople_impl(_bl: *mut c_void, _ap: *mut c_void) -> c_int {
    // Implement when map_foreachinarea is ported to Rust.
    0
}
