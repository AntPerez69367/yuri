//! Lua scripting engine.
#![allow(non_snake_case, dead_code, unused_variables, non_upper_case_globals)]

pub mod async_coro;
pub mod globals;
pub mod map_globals;
pub mod object_collect;
pub mod pc_accessors;
pub mod pending;
pub mod thread_registry;
pub mod types;

use mlua::Lua;
use std::ffi::{CStr, CString};

use crate::common::traits::LegacyEntity;
use crate::game::lua::register as register_new_lua_globals;

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
pub fn sl_state() -> &'static Lua {
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

    // register_types(&lua).expect("failed to register scripting types");
    // globals::register(&lua).expect("failed to register scripting globals");

    register_new_lua_globals(&lua).expect("failed to register scripting globals");
    let _ = SL_STATE.set(LuaWrapper(lua));
    thread_registry::init();
    sl_reload();
}

/// Convert a Lua value (integer id or light userdata pointer) to a C pointer.
/// Integer values that are negative or exceed `usize::MAX` map to null.
fn lua_val_to_ptr(v: mlua::Value) -> *mut std::ffi::c_void {
    match v {
        mlua::Value::Integer(i) => {
            usize::try_from(i).map_or(std::ptr::null_mut(), |u| u as *mut std::ffi::c_void)
        }
        mlua::Value::LightUserData(ud) => ud.0,
        _ => std::ptr::null_mut(),
    }
}

macro_rules! ctor {
    ($lua:expr, $T:ident) => {
        $lua.create_function(|_, v: mlua::Value| {
            Ok($T {
                ptr: lua_val_to_ptr(v),
            })
        })?
    };
}

fn register_types(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    g.set(
        "PC",
        lua.create_function(|_, v: mlua::Value| {
            let ptr = lua_val_to_ptr(v) as *const crate::game::pc::MapSessionData;
            let id = if ptr.is_null() {
                0u32
            } else {
                unsafe { (*ptr).id }
            };
            Ok(PcObject { id })
        })?,
    )?;
    g.set(
        "MOB",
        lua.create_function(|_, v: mlua::Value| {
            let ptr = lua_val_to_ptr(v) as *const crate::game::mob::MobSpawnData;
            let id = if ptr.is_null() {
                0u32
            } else {
                unsafe { (*ptr).id }
            };
            Ok(MobObject { id })
        })?,
    )?;
    // NPC(id) — looks up the NPC by ID.
    // The old C constructor called map_id2npc(id) which resolves the integer ID
    // to a real pointer; storing the raw integer as a pointer would cause a
    // misaligned-pointer panic when Rust later tries to dereference it.
    g.set(
        "NPC",
        lua.create_function(|lua, v: mlua::Value| -> mlua::Result<mlua::Value> {
            let npc_id: Option<u32> = match v {
                mlua::Value::Integer(i) if i >= 0 && i <= u32::MAX as i64 => {
                    let id = i as u32;
                    if crate::game::map_server::map_id2npc_ref(id).is_some() {
                        Some(id)
                    } else {
                        None
                    }
                }
                mlua::Value::Number(f) if f.is_finite() && f >= 0.0 && f <= u32::MAX as f64 => {
                    let id = f as u32;
                    if crate::game::map_server::map_id2npc_ref(id).is_some() {
                        Some(id)
                    } else {
                        None
                    }
                }
                mlua::Value::String(ref s) => {
                    let cs = CString::new(s.as_bytes().to_vec()).map_err(mlua::Error::external)?;
                    let ptr = unsafe { crate::game::map_server::map_name2npc(cs.as_ptr()) };
                    if ptr.is_null() {
                        None
                    } else {
                        Some(unsafe { (*(ptr as *const crate::game::npc::NpcData)).id })
                    }
                }
                _ => None,
            };
            match npc_id {
                Some(id) => Ok(mlua::Value::UserData(
                    lua.create_userdata(NpcObject { id })?,
                )),
                None => Ok(mlua::Value::Nil),
            }
        })?,
    )?;

    // Player — callable namespace table for PC scripts.
    // Player(id)   → map_id2sd(id), nil if not found.
    // Player(name) → map_name2sd(name), nil if not found.
    let player_tbl = lua.create_table()?;
    let player_mt = lua.create_table()?;
    player_mt.set(
        "__call",
        lua.create_function(
            |lua, (_tbl, v): (mlua::Value, mlua::Value)| -> mlua::Result<mlua::Value> {
                let bl_id = match v {
                    mlua::Value::Integer(id) => {
                        if id < 0 || id > u32::MAX as i64 {
                            return Ok(mlua::Value::Nil);
                        }
                        crate::game::map_server::map_id2sd_pc(id as u32).map(|arc| arc.read().id)
                    }
                    mlua::Value::Number(f) => {
                        if !f.is_finite() || f < 0.0 || f > u32::MAX as f64 {
                            return Ok(mlua::Value::Nil);
                        }
                        crate::game::map_server::map_id2sd_pc(f as u32).map(|arc| arc.read().id)
                    }
                    mlua::Value::String(ref s) => {
                        let cs =
                            CString::new(s.as_bytes().to_vec()).map_err(mlua::Error::external)?;
                        let ptr = unsafe { crate::game::map_server::map_name2sd(cs.as_ptr()) };
                        if ptr.is_null() {
                            None
                        } else {
                            Some(unsafe { (*ptr).id })
                        }
                    }
                    _ => None,
                };
                let id = match bl_id {
                    Some(id) => id,
                    None => return Ok(mlua::Value::Nil),
                };
                Ok(mlua::Value::UserData(lua.create_userdata(PcObject { id })?))
            },
        )?,
    )?;
    player_tbl.set_metatable(Some(player_mt));
    g.set("Player", player_tbl)?;

    // Mob — callable namespace table for mob scripts.
    // Mob(id) → map_id2mob(id), nil if not found.
    let mob_tbl = lua.create_table()?;
    let mob_mt = lua.create_table()?;
    mob_mt.set(
        "__call",
        lua.create_function(
            |lua, (_tbl, v): (mlua::Value, mlua::Value)| -> mlua::Result<mlua::Value> {
                let id: u32 = match v {
                    mlua::Value::Integer(id) => {
                        if id < 0 || id > u32::MAX as i64 {
                            return Ok(mlua::Value::Nil);
                        }
                        id as u32
                    }
                    mlua::Value::Number(f) => {
                        if !f.is_finite() || f < 0.0 || f > u32::MAX as f64 {
                            return Ok(mlua::Value::Nil);
                        }
                        f as u32
                    }
                    _ => return Ok(mlua::Value::Nil),
                };
                // Verify the mob exists before creating a MobObject
                if crate::game::map_server::map_id2mob_ref(id).is_none() {
                    return Ok(mlua::Value::Nil);
                }
                Ok(mlua::Value::UserData(
                    lua.create_userdata(MobObject { id })?,
                ))
            },
        )?,
    )?;
    mob_tbl.set_metatable(Some(mob_mt));
    g.set("Mob", mob_tbl)?;
    g.set("REG", ctor!(lua, RegObject))?;
    g.set("REGS", ctor!(lua, RegStringObject))?;
    g.set("NPCREG", ctor!(lua, NpcRegObject))?;
    g.set("MOBREG", ctor!(lua, MobRegObject))?;
    g.set("MAPREG", ctor!(lua, MapRegObject))?;
    g.set("GAMEREG", ctor!(lua, GameRegObject))?;
    g.set("QUESTREG", ctor!(lua, QuestRegObject))?;
    // ITEM/RECIPE/FL need custom ctors that perform DB/id-db lookups.
    g.set(
        "ITEM",
        lua.create_function(|lua, v: mlua::Value| -> mlua::Result<mlua::Value> {
            let item: Option<std::sync::Arc<crate::database::item_db::ItemData>> = match v {
                mlua::Value::Integer(id) => {
                    if id < 0 || id > u32::MAX as i64 {
                        return Ok(mlua::Value::Nil);
                    }
                    Some(crate::database::item_db::search(id as u32))
                }
                mlua::Value::Number(f) => {
                    if !f.is_finite() || f < 0.0 || f > u32::MAX as f64 {
                        return Ok(mlua::Value::Nil);
                    }
                    Some(crate::database::item_db::search(f as u32))
                }
                mlua::Value::String(ref s) => {
                    let text = s.to_str()?;
                    crate::database::item_db::searchname(&text)
                }
                _ => None,
            };
            let Some(item) = item else {
                return Ok(mlua::Value::Nil);
            };
            let ptr = std::sync::Arc::into_raw(item) as *mut std::ffi::c_void;
            Ok(mlua::Value::UserData(
                lua.create_userdata(ItemObject { ptr })?,
            ))
        })?,
    )?;
    g.set("BITEM", ctor!(lua, BItemObject))?;
    g.set("BANKITEM", ctor!(lua, BankItemObject))?;
    g.set("PARCEL", ctor!(lua, ParcelObject))?;
    g.set(
        "RECIPE",
        lua.create_function(|lua, v: mlua::Value| -> mlua::Result<mlua::Value> {
            let ptr: *mut std::ffi::c_void = match v {
                mlua::Value::Integer(id) => {
                    if id < 0 || id > u32::MAX as i64 {
                        return Ok(mlua::Value::Nil);
                    }
                    std::sync::Arc::into_raw(crate::database::recipe_db::search(id as u32))
                        as *mut std::ffi::c_void
                }
                mlua::Value::Number(f) => {
                    if !f.is_finite() || f < 0.0 || f > u32::MAX as f64 {
                        return Ok(mlua::Value::Nil);
                    }
                    std::sync::Arc::into_raw(crate::database::recipe_db::search(f as u32))
                        as *mut std::ffi::c_void
                }
                mlua::Value::String(ref s) => {
                    let text = s.to_str()?;
                    match crate::database::recipe_db::searchname(&text) {
                        Some(arc) => std::sync::Arc::into_raw(arc) as *mut std::ffi::c_void,
                        None => std::ptr::null_mut(),
                    }
                }
                _ => std::ptr::null_mut(),
            };
            if ptr.is_null() {
                return Ok(mlua::Value::Nil);
            }
            Ok(mlua::Value::UserData(
                lua.create_userdata(RecipeObject { ptr })?,
            ))
        })?,
    )?;
    let fl_ctor = lua.create_function(|lua, id: u32| -> mlua::Result<mlua::Value> {
        if crate::game::map_server::map_id2fl_ref(id).is_none() {
            return Ok(mlua::Value::Nil);
        }
        Ok(mlua::Value::UserData(
            lua.create_userdata(FloorListObject { id })?,
        ))
    })?;
    g.set("FL", fl_ctor.clone())?;
    g.set("FloorItem", fl_ctor)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// sl_reload
// ---------------------------------------------------------------------------
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub fn sl_reload() -> i32 {
    let lua = sl_state();
    let cfg = crate::config::config();
    match load_lua_dir(lua, &cfg.lua_dir) {
        Ok(_) => 0,
        Err(e) => {
            tracing::error!("[scripting] sl_reload failed: {e:#}");
            -1
        }
    }
}

fn load_lua_file(lua: &Lua, path: &std::path::Path) -> mlua::Result<()> {
    let src = std::fs::read(path).map_err(mlua::Error::external)?;
    let name = path.to_string_lossy();
    lua.load(src.as_slice())
        .set_name(name.as_ref())
        .eval::<()>()
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
        Ok(r) => r,
        Err(_) => return Ok(()),
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || name == "sys.lua" {
            continue;
        }
        if path.is_dir() {
            if path.to_str().is_none() {
                tracing::warn!(
                    "[scripting] skipping non-UTF8 directory path: {}",
                    path.display()
                );
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
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_fixmem() {
    if let Ok(gc) = sl_state().globals().get::<mlua::Function>("collectgarbage") {
        let _ = gc.call::<()>("collect");
    }
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_luasize() -> i32 {
    sl_state()
        .globals()
        .get::<mlua::Function>("collectgarbage")
        .and_then(|f| f.call::<f64>("count"))
        .map(|kb| kb as i32)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Safe dispatch: look up entity by id and wrap in appropriate Lua userdata.
/// Returns Nil if the entity no longer exists (killed, picked up, etc.).
pub fn entity_to_lua(lua: &mlua::Lua, id: u32) -> mlua::Result<mlua::Value> {
    id_to_lua(lua, id)
}

/// Convert an entity ID to the appropriate Lua userdata value.
/// Uses ID ranges to determine the entity type — no map lookup needed.
pub fn id_to_lua(lua: &mlua::Lua, id: u32) -> mlua::Result<mlua::Value> {
    use crate::game::mob::{FLOORITEM_START_NUM, MOB_START_NUM, NPC_START_NUM};

    if id == 0 {
        return Ok(mlua::Value::Nil);
    }
    if id < MOB_START_NUM {
        if crate::game::map_server::map_id2sd_pc(id).is_some() {
            return lua.pack(PcObject { id });
        }
    } else if id >= NPC_START_NUM {
        if crate::game::map_server::map_id2npc_ref(id).is_some() {
            return lua.pack(NpcObject { id });
        }
    } else if id >= FLOORITEM_START_NUM {
        if crate::game::map_server::map_id2fl_ref(id).is_some() {
            return lua.pack(FloorListObject { id });
        }
    } else {
        if crate::game::map_server::map_id2mob_ref(id).is_some() {
            return lua.pack(MobObject { id });
        }
    }
    Ok(mlua::Value::Nil)
}

/// Safe Lua dispatch: resolve `root` (and optionally `root.method`) from globals,
/// then call the function with the given arguments.
fn call_lua_str(root: &str, method: Option<&str>, args: mlua::MultiValue) -> bool {
    let lua = unsafe { sl_state() };

    match method {
        None => {
            let func: mlua::Function = match lua.globals().get(root) {
                Ok(f) => f,
                Err(_) => {
                    tracing::warn!("[scripting] {root}: function not found");
                    return false;
                }
            };
            tracing::info!("[scripting] calling {root}");
            match func.call::<mlua::MultiValue>(args) {
                Ok(_) => tracing::info!("[scripting] {root} returned ok"),
                Err(e) => tracing::warn!("[scripting] {root}: {e}"),
            }
            true
        }
        Some(method_s) => {
            let tbl: mlua::Table = match lua.globals().get(root) {
                Ok(t) => t,
                Err(_) => return false,
            };
            let func: mlua::Function = match tbl.get(method_s) {
                Ok(f) => f,
                Err(_) => return false,
            };
            if let Err(e) = func.call::<mlua::MultiValue>(args) {
                tracing::warn!("[scripting] {root}.{method_s}: {e}");
            }
            true
        }
    }
}

/// Legacy C-string wrapper — delegates to `call_lua_str`.
unsafe fn call_lua(root: *const i8, method: *const i8, args: mlua::MultiValue) -> bool {
    if root.is_null() {
        return false;
    }
    let root_s = match CStr::from_ptr(root).to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };
    let method_s = if method.is_null() {
        None
    } else {
        match CStr::from_ptr(method).to_str() {
            Ok(s) => Some(s),
            Err(_) => return false,
        }
    };
    call_lua_str(root_s, method_s, args)
}

/// Convert a fixed `[i8]` C-style string array to `&str`.
/// Finds the first null byte and decodes as UTF-8, returning `""` on invalid data.
#[inline]
pub fn carray_to_str(arr: &[i8]) -> &str {
    let bytes = unsafe { &*(arr as *const [i8] as *const [u8]) };
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end]).unwrap_or("")
}

/// ID-based Lua script dispatch — safe replacement for doscript_blargs.
///
/// Converts entity IDs to Lua userdata via `id_to_lua` (0 → Nil),
/// then dispatches to `call_lua_str`.
pub fn doscript_blargs_id(root: &str, method: Option<&str>, entity_ids: &[u32]) -> i32 {
    if entity_ids.is_empty() {
        return call_lua_str(root, method, mlua::MultiValue::new()) as i32;
    }
    let lua = unsafe { sl_state() };
    let mut mv = mlua::MultiValue::new();
    for &id in entity_ids {
        let val = if id == 0 {
            mlua::Value::Nil
        } else {
            id_to_lua(lua, id).unwrap_or(mlua::Value::Nil)
        };
        mv.push_back(val);
    }
    call_lua_str(root, method, mv) as i32
}

/// ID-based coroutine dispatch — safe replacement for doscript_coro.
///
/// Creates a Lua coroutine thread, wraps the first PC argument through
/// `_wrap_player()` for yielding method support, and resumes the thread.
pub fn doscript_coro_id(root: &str, method: Option<&str>, entity_ids: &[u32]) -> i32 {
    use crate::game::mob::MOB_START_NUM;

    let lua = unsafe { sl_state() };

    let func = match method {
        None => match lua.globals().get::<mlua::Function>(root) {
            Ok(f) => f,
            Err(_) => return 0,
        },
        Some(m) => {
            let tbl: mlua::Table = match lua.globals().get(root) {
                Ok(t) => t,
                Err(_) => return 0,
            };
            match tbl.get::<mlua::Function>(m) {
                Ok(f) => f,
                Err(_) => return 0,
            }
        }
    };

    let thread = match lua.create_thread(func) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("[scripting] create_thread failed: {e}");
            return 0;
        }
    };

    let wrap_fn: Option<mlua::Function> = lua.globals().get("_wrap_player").ok();
    let mut mv = mlua::MultiValue::new();
    let mut user_key: Option<usize> = None;
    for (i, &id) in entity_ids.iter().enumerate() {
        let val = if id == 0 {
            mlua::Value::Nil
        } else if i == 0 && id < MOB_START_NUM {
            // First arg is a PC — wrap through _wrap_player for yielding support.
            if let Some(arc) = crate::game::map_server::map_id2sd_pc(id) {
                user_key = Some(&*arc.read() as *const crate::game::pc::MapSessionData as usize);
            }
            let pc_val = id_to_lua(lua, id).unwrap_or(mlua::Value::Nil);
            if let Some(ref wf) = wrap_fn {
                wf.call::<mlua::Value>(pc_val).unwrap_or(mlua::Value::Nil)
            } else {
                pc_val
            }
        } else {
            id_to_lua(lua, id).unwrap_or(mlua::Value::Nil)
        };
        mv.push_back(val);
    }

    match thread.resume::<mlua::MultiValue>(mv) {
        Ok(_) => {
            if thread.status() == mlua::ThreadStatus::Resumable {
                if let Some(uk) = user_key {
                    thread_registry::store(lua, uk, &thread);
                } else {
                    tracing::warn!(
                        "[scripting] {root}: thread yielded but no user_key to store it"
                    );
                }
            }
        }
        Err(e) => {
            tracing::warn!("[scripting] {root}: {e}");
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
pub unsafe fn doscript_strings(root: *const i8, method: *const i8, args: &[*const i8]) -> i32 {
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
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_doscript_strings_vec(
    root: *const i8,
    method: *const i8,
    nargs: i32,
    args: *const *const i8,
) -> i32 {
    if nargs <= 0 || args.is_null() {
        return call_lua(root, method, mlua::MultiValue::new()) as i32;
    }
    let slice = std::slice::from_raw_parts(args, nargs as usize);
    doscript_strings(root, method, slice)
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_doscript_stackargs(root: *const i8, method: *const i8, _nargs: i32) -> i32 {
    // Args-on-stack path requires direct stack access not exposed by mlua.
    // Return 0 (not found) until map_parse.c is ported and this path is audited.
    tracing::warn!("[scripting] sl_doscript_stackargs not yet implemented");
    0
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_exec_str(user: *mut std::ffi::c_void, code: *const i8) {
    let s = CStr::from_ptr(code).to_string_lossy();
    let lua = sl_state();
    if let Err(e) = lua.load(s.as_ref()).eval::<()>() {
        tracing::warn!("[scripting] sl_exec error: {e}");
    }
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_resumemenu(selection: u32, sd: *mut crate::game::pc::MapSessionData) {
    async_coro::resume_menu(selection, sd as *mut std::ffi::c_void)
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_resumemenuseq(
    selection: u32,
    choice: i32,
    sd: *mut crate::game::pc::MapSessionData,
) {
    async_coro::resume_menuseq(selection, choice, sd as *mut std::ffi::c_void)
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_resumeinputseq(
    choice: u32,
    input: *mut i8,
    sd: *mut crate::game::pc::MapSessionData,
) {
    async_coro::resume_inputseq(choice, input, sd as *mut std::ffi::c_void)
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_resumedialog(choice: u32, sd: *mut crate::game::pc::MapSessionData) {
    async_coro::resume_dialog(choice, sd as *mut std::ffi::c_void)
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_resumebuy(items: *mut i8, sd: *mut crate::game::pc::MapSessionData) {
    async_coro::resume_buy(items, sd as *mut std::ffi::c_void)
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_resumeinput(
    tag: *mut i8,
    input: *mut i8,
    sd: *mut crate::game::pc::MapSessionData,
) {
    async_coro::resume_input(tag, input, sd as *mut std::ffi::c_void)
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_resumesell(choice: u32, sd: *mut crate::game::pc::MapSessionData) {
    async_coro::resume_sell(choice, sd as *mut std::ffi::c_void)
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_exec(_user: *mut crate::game::pc::MapSessionData, code: *mut i8) {
    sl_exec_str(_user as *mut std::ffi::c_void, code)
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_async_freeco(user: *mut crate::game::pc::MapSessionData) {
    thread_registry::cancel(user as usize);
    async_coro::clear_menu_opts(user as *mut std::ffi::c_void);
}
