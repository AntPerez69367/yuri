//! Lua scripting engine.
#![allow(non_snake_case, dead_code, unused_variables, non_upper_case_globals)]

pub mod async_coro;
pub mod ffi;
pub mod globals;
pub mod object_collect;
pub mod pc_accessors;
pub mod map_globals;
pub mod pending;
pub mod thread_registry;
pub mod types;

use mlua::Lua;
use std::ffi::{CStr, CString};

use crate::database::map_db::BlockList;
use types::floor::FloorListObject;
use types::item::*;
use types::mob::MobObject;
use types::npc::NpcObject;
use types::pc::PcObject;
use types::registry::*;

// ---------------------------------------------------------------------------
// Global Lua state — single instance, lives for the process lifetime.
// ---------------------------------------------------------------------------

/// Wrapper around `Lua` to allow storage in `OnceLock`.
///
/// SAFETY: The Lua state is initialised once on the game thread and only
/// accessed from the same thread via the tokio `LocalSet` executor.
/// No concurrent access ever occurs.
struct LuaWrapper(Lua);
unsafe impl Send for LuaWrapper {}
unsafe impl Sync for LuaWrapper {}

impl std::fmt::Debug for LuaWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LuaWrapper").finish_non_exhaustive()
    }
}

/// The global Lua state, set once by `sl_init`.
static SL_STATE: std::sync::OnceLock<LuaWrapper> = std::sync::OnceLock::new();

/// Raw `lua_State` pointer, captured once during `sl_init`.
static SL_GSTATE: std::sync::OnceLock<usize> = std::sync::OnceLock::new();

/// Returns a reference to the global Lua state.
///
/// # Safety
/// Must only be called after `sl_init()`.  All scripting runs on the LocalSet
/// thread (timer_do + session_io_task), so no external locking is needed.
pub unsafe fn sl_state() -> &'static Lua {
    &SL_STATE.get().expect("sl_init() not called").0
}

/// Returns the raw `lua_State*` pointer captured at init time.
pub fn sl_gstate_ptr() -> *mut std::ffi::c_void {
    SL_GSTATE.get().copied().unwrap_or(0) as *mut std::ffi::c_void
}

// ---------------------------------------------------------------------------
// sl_init
// ---------------------------------------------------------------------------
pub fn sl_init() {
    // LuaJIT on 64-bit requires luaL_newstate() — Lua::new() uses it.
    // Lua::new_with(ALL_SAFE, ...) uses a custom allocator that LuaJIT rejects.
    let lua = Lua::new();

    register_types(&lua).expect("failed to register scripting types");
    globals::register(&lua).expect("failed to register scripting globals");

    // Capture the raw lua_State* via exec_raw (pointer is stable for process lifetime).
    // SAFETY: exec_raw requires unsafe. The closure captures a stable pointer that lives for
    // the process lifetime. No UB — we just store the address as a usize.
    unsafe {
        lua.exec_raw::<()>((), |l| {
            let _ = SL_GSTATE.set(l as usize);
        }).expect("exec_raw failed: sl_gstate could not be initialised");
    }

    let _ = SL_STATE.set(LuaWrapper(lua));
    thread_registry::init();

    // Reload scripts (lua_dir comes from config).
    unsafe { sl_reload(); }
}

/// Convert a Lua value (integer id or light userdata pointer) to a C pointer.
/// Integer values that are negative or exceed `usize::MAX` map to null.
fn lua_val_to_ptr(v: mlua::Value) -> *mut std::ffi::c_void {
    match v {
        mlua::Value::Integer(i)         => {
            usize::try_from(i).map_or(std::ptr::null_mut(), |u| u as *mut std::ffi::c_void)
        }
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
    g.set("PC", lua.create_function(|_, v: mlua::Value| {
        let ptr = lua_val_to_ptr(v) as *const crate::game::pc::MapSessionData;
        let id = if ptr.is_null() { 0u32 } else { unsafe { (*ptr).bl.id } };
        Ok(PcObject { id })
    })?)?;
    g.set("MOB", lua.create_function(|_, v: mlua::Value| {
        let ptr = lua_val_to_ptr(v) as *const crate::game::mob::MobSpawnData;
        let id = if ptr.is_null() { 0u32 } else { unsafe { (*ptr).bl.id } };
        Ok(MobObject { id })
    })?)?;
    // NPC(id) — looks up the NPC by ID.
    // The old C constructor called map_id2npc(id) which resolves the integer ID
    // to a real pointer; storing the raw integer as a pointer would cause a
    // misaligned-pointer panic when Rust later tries to dereference it.
    g.set("NPC", lua.create_function(|lua, v: mlua::Value| -> mlua::Result<mlua::Value> {
        let ptr = match v {
            mlua::Value::Integer(i) if i >= 0 && i <= u32::MAX as i64 => {
                ffi::map_id2bl_ref(i as u32)
            }
            mlua::Value::Number(f) if f.is_finite() && f >= 0.0 && f <= u32::MAX as f64 => {
                ffi::map_id2bl_ref(f as u32)
            }
            mlua::Value::String(ref s) => {
                let cs = CString::new(s.as_bytes().to_vec()).map_err(mlua::Error::external)?;
                unsafe { ffi::map_name2npc(cs.as_ptr()) as *mut BlockList }
            }
            _ => std::ptr::null_mut(),
        };
        if ptr.is_null() { return Ok(mlua::Value::Nil); }
        if unsafe { (*ptr).bl_type as i32 } != ffi::BL_NPC {
            return Ok(mlua::Value::Nil);
        }
        Ok(mlua::Value::UserData(lua.create_userdata(NpcObject { ptr: ptr as *mut std::ffi::c_void })?))
    })?)?;

    // Player — callable namespace table for PC scripts.
    // Player(id)   → map_id2sd(id), nil if not found.
    // Player(name) → map_name2sd(name), nil if not found.
    let player_tbl = lua.create_table()?;
    let player_mt  = lua.create_table()?;
    player_mt.set("__call", lua.create_function(|lua, (_tbl, v): (mlua::Value, mlua::Value)| -> mlua::Result<mlua::Value> {
        let bl_id = match v {
            mlua::Value::Integer(id) => {
                if id < 0 || id > u32::MAX as i64 { return Ok(mlua::Value::Nil); }
                crate::game::map_server::map_id2sd_pc(id as u32).map(|arc| arc.read().bl.id)
            }
            mlua::Value::Number(f) => {
                if !f.is_finite() || f < 0.0 || f > u32::MAX as f64 { return Ok(mlua::Value::Nil); }
                crate::game::map_server::map_id2sd_pc(f as u32).map(|arc| arc.read().bl.id)
            }
            mlua::Value::String(ref s) => {
                let cs = CString::new(s.as_bytes().to_vec()).map_err(mlua::Error::external)?;
                let ptr = unsafe { ffi::map_name2sd(cs.as_ptr()) };
                if ptr.is_null() { None }
                else { Some(unsafe { (*(ptr as *const crate::game::pc::MapSessionData)).bl.id }) }
            }
            _ => None,
        };
        let id = match bl_id { Some(id) => id, None => return Ok(mlua::Value::Nil) };
        Ok(mlua::Value::UserData(lua.create_userdata(PcObject { id })?))
    })?)?;
    player_tbl.set_metatable(Some(player_mt));
    g.set("Player", player_tbl)?;

    // Mob — callable namespace table for mob scripts.
    // Mob(id) → map_id2mob(id), nil if not found.
    let mob_tbl = lua.create_table()?;
    let mob_mt  = lua.create_table()?;
    mob_mt.set("__call", lua.create_function(|lua, (_tbl, v): (mlua::Value, mlua::Value)| -> mlua::Result<mlua::Value> {
        let id: u32 = match v {
            mlua::Value::Integer(id) => {
                if id < 0 || id > u32::MAX as i64 { return Ok(mlua::Value::Nil); }
                id as u32
            }
            mlua::Value::Number(f) => {
                if !f.is_finite() || f < 0.0 || f > u32::MAX as f64 { return Ok(mlua::Value::Nil); }
                f as u32
            }
            _ => return Ok(mlua::Value::Nil),
        };
        // Verify the mob exists before creating a MobObject
        if crate::game::map_server::map_id2mob_ref(id).is_none() { return Ok(mlua::Value::Nil); }
        Ok(mlua::Value::UserData(lua.create_userdata(MobObject { id })?))
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
        let item: Option<std::sync::Arc<crate::database::item_db::ItemData>> = match v {
            mlua::Value::Integer(id) => {
                if id < 0 || id > u32::MAX as i64 { return Ok(mlua::Value::Nil); }
                Some(crate::database::item_db::search(id as u32))
            }
            mlua::Value::Number(f) => {
                if !f.is_finite() || f < 0.0 || f > u32::MAX as f64 { return Ok(mlua::Value::Nil); }
                Some(crate::database::item_db::search(f as u32))
            }
            mlua::Value::String(ref s) => {
                let text = s.to_str()?;
                crate::database::item_db::searchname(&text)
            }
            _ => None,
        };
        let Some(item) = item else { return Ok(mlua::Value::Nil); };
        let ptr = std::sync::Arc::into_raw(item) as *mut std::ffi::c_void;
        Ok(mlua::Value::UserData(lua.create_userdata(ItemObject { ptr })?))
    })?)?;
    g.set("BITEM",    ctor!(lua, BItemObject))?;
    g.set("BANKITEM", ctor!(lua, BankItemObject))?;
    g.set("PARCEL",   ctor!(lua, ParcelObject))?;
    g.set("RECIPE", lua.create_function(|lua, v: mlua::Value| -> mlua::Result<mlua::Value> {
        let ptr: *mut std::ffi::c_void = match v {
            mlua::Value::Integer(id) => {
                if id < 0 || id > u32::MAX as i64 { return Ok(mlua::Value::Nil); }
                crate::database::recipe_db::rust_recipedb_search(id as u32) as *mut std::ffi::c_void
            }
            mlua::Value::Number(f) => {
                if !f.is_finite() || f < 0.0 || f > u32::MAX as f64 { return Ok(mlua::Value::Nil); }
                crate::database::recipe_db::rust_recipedb_search(f as u32) as *mut std::ffi::c_void
            }
            mlua::Value::String(ref s) => {
                let text = s.to_str()?;
                let cs = CString::new(text.as_bytes()).map_err(mlua::Error::external)?;
                unsafe { crate::database::recipe_db::rust_recipedb_searchname(cs.as_ptr()) as *mut std::ffi::c_void }
            }
            _ => std::ptr::null_mut(),
        };
        if ptr.is_null() { return Ok(mlua::Value::Nil); }
        Ok(mlua::Value::UserData(lua.create_userdata(RecipeObject { ptr })?))
    })?)?;
    let fl_ctor = lua.create_function(|lua, id: u32| -> mlua::Result<mlua::Value> {
        let ptr = unsafe { ffi::map_id2fl(id) };
        if ptr.is_null() { return Ok(mlua::Value::Nil); }
        Ok(mlua::Value::UserData(lua.create_userdata(FloorListObject::new(ptr))?))
    })?;
    g.set("FL",        fl_ctor.clone())?;
    g.set("FloorItem", fl_ctor)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// sl_reload
// ---------------------------------------------------------------------------
pub unsafe fn sl_reload() -> i32 {
    let lua = sl_state();
    let cfg = crate::config::config();
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
    let sys = std::path::Path::new(dir).join("sys.lua");
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
            if path.to_str().is_none() {
                tracing::warn!("[scripting] skipping non-UTF8 directory path: {}", path.display());
            }
            load_dir_recursive(lua, &path.to_string_lossy())?;
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

pub unsafe fn sl_luasize() -> i32 {
    sl_state().globals()
        .get::<mlua::Function>("collectgarbage")
        .and_then(|f| f.call::<f64>("count"))
        .map(|kb| kb as i32)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

pub(crate) unsafe fn bl_to_lua(lua: &Lua, bl: *mut std::ffi::c_void) -> mlua::Result<mlua::Value> {
    debug_assert!(!bl.is_null(), "bl_to_lua: caller must not pass a null pointer");
    if bl.is_null() { return Ok(mlua::Value::Nil); }
    let bl_ref = &*(bl as *const BlockList);
    let bl_type = bl_ref.bl_type as i32;
    match bl_type {
        ffi::BL_PC   => lua.pack(PcObject  { id: bl_ref.id }),
        ffi::BL_MOB  => lua.pack(MobObject { id: bl_ref.id }),
        ffi::BL_NPC  => lua.pack(NpcObject { ptr: bl }),
        ffi::BL_ITEM => lua.pack(FloorListObject::new(bl)),
        other => {
            tracing::warn!("[scripting] bl_to_lua: unhandled bl_type={other:#04x}, returning nil");
            Ok(mlua::Value::Nil)
        }
    }
}

/// Safe dispatch: look up entity by id and wrap in appropriate Lua userdata.
/// Returns Nil if the entity no longer exists (killed, picked up, etc.).
pub fn entity_to_lua(lua: &mlua::Lua, id: u32) -> mlua::Result<mlua::Value> {
    use crate::game::map_server::GameEntity;
    match crate::game::map_server::map_id2entity(id) {
        Some(GameEntity::Player(arc)) => lua.pack(PcObject  { id: arc.read().bl.id }),
        Some(GameEntity::Mob(arc))   => lua.pack(MobObject { id: arc.read().bl.id }),
        Some(GameEntity::Npc(arc)) => lua.pack(
            NpcObject { ptr: &*arc.write() as *const _ as *mut std::ffi::c_void }
        ),
        Some(GameEntity::Item(arc)) => lua.pack(
            FloorListObject::new(&*arc.write() as *const _ as *mut std::ffi::c_void)
        ),
        None => Ok(mlua::Value::Nil),
    }
}

unsafe fn call_lua(
    root: *const i8,
    method: *const i8,
    args: mlua::MultiValue,
) -> bool {
    if root.is_null() { return false; }
    let lua = sl_state();
    let root_s = match CStr::from_ptr(root).to_str() { Ok(s) => s, Err(_) => return false };

    if method.is_null() {
        // Direct function call: sl_doscript_blargs("startup", NULL, 0) → startup()
        let func: mlua::Function = match lua.globals().get(root_s) { Ok(f) => f, Err(_) => return false };
        if let Err(e) = func.call::<mlua::MultiValue>(args) {
            tracing::warn!("[scripting] {root_s}: {e}");
        }
        return true;
    }

    let method_s = match CStr::from_ptr(method).to_str() { Ok(s) => s, Err(_) => return false };
    let tbl: mlua::Table  = match lua.globals().get(root_s)  { Ok(t) => t, Err(_) => return false };
    let func: mlua::Function = match tbl.get(method_s)       { Ok(f) => f, Err(_) => return false };

    if let Err(e) = func.call::<mlua::MultiValue>(args) {
        tracing::warn!("[scripting] {root_s}.{method_s}: {e}");
    }
    true
}

/// Dispatch a Lua script call with block_list pointer arguments (slice API).
///
/// Each pointer in `args` may be null (mapped to Lua nil) or a valid `*mut BlockList`.
///
/// # Safety
/// Every non-null pointer in `args` must be a valid `*mut BlockList` for the
/// duration of this call.  Null pointers are accepted and mapped to Lua nil.
pub unsafe fn doscript_blargs(
    root: *const i8,
    method: *const i8,
    args: &[*mut BlockList],
) -> i32 {
    if args.is_empty() {
        return call_lua(root, method, mlua::MultiValue::new()) as i32;
    }
    let lua = sl_state();
    let mut mv = mlua::MultiValue::new();
    for &bl in args {
        let val = if bl.is_null() {
            mlua::Value::Nil
        } else {
            bl_to_lua(lua, bl as *mut std::ffi::c_void).unwrap_or(mlua::Value::Nil)
        };
        mv.push_back(val);
    }
    call_lua(root, method, mv) as i32
}

/// Coroutine dispatch for NPC interactions.
///
/// Creates a Lua coroutine thread, wraps the first BL_PC argument through
/// `_wrap_player()` so yielding methods (dialog, menu, input, etc.) work,
/// and resumes the thread.  If the script yields (waiting for client
/// response), the thread is stored in `thread_registry` keyed by the
/// player's `MapSessionData` pointer.
///
/// # Safety
/// Same requirements as `doscript_blargs`.
pub unsafe fn doscript_coro(
    root: *const i8,
    method: *const i8,
    args: &[*mut BlockList],
) -> i32 {
    if root.is_null() { return 0; }
    let lua = sl_state();
    let root_s = match CStr::from_ptr(root).to_str() { Ok(s) => s, Err(_) => return 0 };

    let func = if method.is_null() {
        match lua.globals().get::<mlua::Function>(root_s) { Ok(f) => f, Err(_) => return 0 }
    } else {
        let method_s = match CStr::from_ptr(method).to_str() { Ok(s) => s, Err(_) => return 0 };
        let tbl: mlua::Table = match lua.globals().get(root_s) { Ok(t) => t, Err(_) => return 0 };
        match tbl.get::<mlua::Function>(method_s) { Ok(f) => f, Err(_) => return 0 }
    };

    let thread = match lua.create_thread(func) {
        Ok(t) => t,
        Err(e) => { tracing::warn!("[scripting] create_thread failed: {e}"); return 0; }
    };

    // Build args, wrapping the first BL_PC argument through _wrap_player.
    let wrap_fn: Option<mlua::Function> = lua.globals().get("_wrap_player").ok();
    let mut mv = mlua::MultiValue::new();
    let mut user_key: Option<usize> = None;
    for (i, &bl) in args.iter().enumerate() {
        let val = if bl.is_null() {
            mlua::Value::Nil
        } else {
            let bl_ref = &*(bl as *const BlockList);
            // Wrap the first player arg through _wrap_player for yielding method support.
            if i == 0 && bl_ref.bl_type as i32 == ffi::BL_PC {
                // Derive the user key (MapSessionData pointer) for thread registry.
                if let Some(arc) = crate::game::map_server::map_id2sd_pc(bl_ref.id) {
                    user_key = Some(&*arc.read() as *const crate::game::pc::MapSessionData as usize);
                }
                let pc_val = bl_to_lua(lua, bl as *mut std::ffi::c_void).unwrap_or(mlua::Value::Nil);
                if let Some(ref wf) = wrap_fn {
                    wf.call::<mlua::Value>(pc_val).unwrap_or(mlua::Value::Nil)
                } else {
                    pc_val
                }
            } else {
                bl_to_lua(lua, bl as *mut std::ffi::c_void).unwrap_or(mlua::Value::Nil)
            }
        };
        mv.push_back(val);
    }

    match thread.resume::<mlua::MultiValue>(mv) {
        Ok(_) => {
            if thread.status() == mlua::ThreadStatus::Resumable {
                if let Some(uk) = user_key {
                    thread_registry::store(lua, uk, &thread);
                } else {
                    tracing::warn!("[scripting] {root_s}: thread yielded but no user_key to store it");
                }
            }
        }
        Err(e) => {
            tracing::warn!("[scripting] {root_s}: {e}");
        }
    }
    1
}

/// Dispatch a Lua script call with C string arguments (slice API).
///
/// Each pointer in `args` may be null (mapped to Lua nil) or a valid nul-terminated
/// C string.
///
/// # Safety
/// Every non-null pointer in `args` must be a valid nul-terminated C string for
/// the duration of this call.
pub unsafe fn doscript_strings(
    root: *const i8,
    method: *const i8,
    args: &[*const i8],
) -> i32 {
    if args.is_empty() {
        return call_lua(root, method, mlua::MultiValue::new()) as i32;
    }
    let lua = sl_state();
    let mut mv = mlua::MultiValue::new();
    for &p in args {
        let val = if p.is_null() {
            mlua::Value::Nil
        } else {
            let s = CStr::from_ptr(p).to_string_lossy().into_owned();
            lua.pack(s).unwrap_or(mlua::Value::Nil)
        };
        mv.push_back(val);
    }
    call_lua(root, method, mv) as i32
}

/// # Safety
/// `args` must point to an array of at least `nargs` valid (or null) block-list
/// pointers.  `nargs` must be non-negative and accurate; the caller owns the
/// array for the duration of this call.
pub unsafe fn sl_doscript_blargs_vec(
    root: *const i8, method: *const i8,
    nargs: i32, args: *const *mut std::ffi::c_void,
) -> i32 {
    debug_assert!(nargs >= 0, "sl_doscript_blargs_vec: nargs must be non-negative");
    debug_assert!(nargs <= 64, "sl_doscript_blargs_vec: nargs={nargs} exceeds sanity limit");
    if nargs <= 0 || args.is_null() {
        return call_lua(root, method, mlua::MultiValue::new()) as i32;
    }
    let slice = std::slice::from_raw_parts(args as *const *mut BlockList, nargs as usize);
    doscript_blargs(root, method, slice)
}

pub unsafe fn sl_doscript_strings_vec(
    root: *const i8, method: *const i8,
    nargs: i32, args: *const *const i8,
) -> i32 {
    if nargs <= 0 || args.is_null() {
        return call_lua(root, method, mlua::MultiValue::new()) as i32;
    }
    let slice = std::slice::from_raw_parts(args, nargs as usize);
    doscript_strings(root, method, slice)
}

pub unsafe fn sl_doscript_stackargs(
    root: *const i8, method: *const i8, _nargs: i32,
) -> i32 {
    // Args-on-stack path requires direct stack access not exposed by mlua.
    // Return 0 (not found) until map_parse.c is ported and this path is audited.
    tracing::warn!("[scripting] sl_doscript_stackargs not yet implemented");
    0
}

pub unsafe fn sl_exec_str(user: *mut std::ffi::c_void, code: *const i8) {
    let s = CStr::from_ptr(code).to_string_lossy();
    let lua = sl_state();
    if let Err(e) = lua.load(s.as_ref()).eval::<()>() {
        tracing::warn!("[scripting] sl_exec error: {e}");
    }
}

pub unsafe fn sl_updatepeople_impl(_bl: *mut std::ffi::c_void, _ap: *mut std::ffi::c_void) -> i32 {
    // Implement when map_foreachinarea is ported to Rust.
    0
}


pub unsafe fn rust_sl_init() {
    ffi_catch!((), sl_init())
}

pub unsafe fn rust_sl_fixmem() {
    ffi_catch!((), sl_fixmem())
}

pub unsafe fn rust_sl_reload() -> i32 {
    ffi_catch!(-1, sl_reload())
}

pub unsafe fn rust_sl_luasize(_user: *mut crate::game::pc::MapSessionData) -> i32 {
    ffi_catch!(0, sl_luasize())
}

pub unsafe fn rust_sl_doscript_blargs_vec(
    root:   *const i8,
    method: *const i8,
    nargs:  i32,
    args:   *const *mut std::ffi::c_void,
) -> i32 {
    ffi_catch!(0, sl_doscript_blargs_vec(root, method, nargs, args))
}

pub unsafe fn rust_sl_doscript_strings_vec(
    root:   *const i8,
    method: *const i8,
    nargs:  i32,
    args:   *const *const i8,
) -> i32 {
    ffi_catch!(0, sl_doscript_strings_vec(root, method, nargs, args))
}

pub unsafe fn rust_sl_doscript_stackargs(
    root:   *const i8,
    method: *const i8,
    nargs:  i32,
) -> i32 {
    ffi_catch!(0, sl_doscript_stackargs(root, method, nargs))
}

pub unsafe fn rust_sl_updatepeople(
    bl: *mut std::ffi::c_void,
    ap: *mut std::ffi::c_void,
) -> i32 {
    ffi_catch!(0, sl_updatepeople_impl(bl, ap))
}

/// Direct symbol used as a function pointer callback in map_foreachinarea.
pub unsafe fn sl_updatepeople(
    bl: *mut std::ffi::c_void,
    ap: *mut std::ffi::c_void,
) -> i32 {
    ffi_catch!(0, sl_updatepeople_impl(bl, ap))
}

pub unsafe fn rust_sl_resumemenu(selection: u32, sd: *mut crate::game::pc::MapSessionData) {
    ffi_catch!((), async_coro::resume_menu(selection, sd as *mut std::ffi::c_void))
}

pub unsafe fn rust_sl_resumemenuseq(selection: u32, choice: i32, sd: *mut crate::game::pc::MapSessionData) {
    ffi_catch!((), async_coro::resume_menuseq(selection, choice, sd as *mut std::ffi::c_void))
}

pub unsafe fn rust_sl_resumeinputseq(
    choice: u32,
    input:  *mut i8,
    sd:     *mut crate::game::pc::MapSessionData,
) {
    ffi_catch!((), async_coro::resume_inputseq(choice, input, sd as *mut std::ffi::c_void))
}

pub unsafe fn rust_sl_resumedialog(choice: u32, sd: *mut crate::game::pc::MapSessionData) {
    ffi_catch!((), async_coro::resume_dialog(choice, sd as *mut std::ffi::c_void))
}

pub unsafe fn rust_sl_resumebuy(items: *mut i8, sd: *mut crate::game::pc::MapSessionData) {
    ffi_catch!((), async_coro::resume_buy(items, sd as *mut std::ffi::c_void))
}

pub unsafe fn rust_sl_resumeinput(
    tag:   *mut i8,
    input: *mut i8,
    sd:    *mut crate::game::pc::MapSessionData,
) {
    ffi_catch!((), async_coro::resume_input(tag, input, sd as *mut std::ffi::c_void))
}

pub unsafe fn rust_sl_resumesell(choice: u32, sd: *mut crate::game::pc::MapSessionData) {
    ffi_catch!((), async_coro::resume_sell(choice, sd as *mut std::ffi::c_void))
}

pub unsafe fn rust_sl_exec(user: *mut crate::game::pc::MapSessionData, code: *mut i8) {
    ffi_catch!((), sl_exec_str(user as *mut std::ffi::c_void, code))
}

pub unsafe fn rust_sl_async_freeco(user: *mut crate::game::pc::MapSessionData) {
    thread_registry::cancel(user as usize);
    async_coro::clear_menu_opts(user as *mut std::ffi::c_void);
}
