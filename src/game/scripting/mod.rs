//! Scripting engine — replaces `c_src/scripting.c`.
#![allow(non_snake_case, dead_code, unused_variables)]

pub mod async_coro;
pub mod ffi;
pub mod globals;
pub mod types;

use mlua::Lua;
use std::ffi::{CStr, c_char, c_int};
use std::os::raw::c_void;

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
/// Must only be called after `sl_init()` and from the game thread.
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
    g.set("REG",      ctor!(lua, RegObject))?;
    g.set("REGS",     ctor!(lua, RegStringObject))?;
    g.set("NPCREG",   ctor!(lua, NpcRegObject))?;
    g.set("MOBREG",   ctor!(lua, MobRegObject))?;
    g.set("MAPREG",   ctor!(lua, MapRegObject))?;
    g.set("GAMEREG",  ctor!(lua, GameRegObject))?;
    g.set("QUESTREG", ctor!(lua, QuestRegObject))?;
    g.set("ITEM",     ctor!(lua, ItemObject))?;
    g.set("BITEM",    ctor!(lua, BItemObject))?;
    g.set("BANKITEM", ctor!(lua, BankItemObject))?;
    g.set("PARCEL",   ctor!(lua, ParcelObject))?;
    g.set("RECIPE",   ctor!(lua, RecipeObject))?;
    g.set("FL",       ctor!(lua, FloorListObject))?;
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

unsafe fn bl_to_lua(lua: &Lua, bl: *mut c_void) -> mlua::Result<mlua::Value> {
    if bl.is_null() { return Ok(mlua::Value::Nil); }
    let bl_type = *(bl as *const c_int);
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
