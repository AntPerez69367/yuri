//! Shared __index helpers for all BL-typed scripting objects (PC, MOB, NPC).
//!
//! In the original C scripting.c, these were registered on the base `bll_type`
//! via `typel_extendproto` and inherited by pcl_type, mobl_type, npcl_type.
//! Here we achieve the same with free functions called from each type's __index.


use crate::database::map_db::{BlockList, MapData};
use crate::game::types::GfxViewer;
use crate::database::map_db::get_map_ptr;
use crate::game::map_server::map_id2bl_ref;
use crate::game::scripting::object_collect::{
    sl_g_getobjectscell, sl_g_getobjectscellwithtraps, sl_g_getaliveobjectscell,
    sl_g_getobjectsarea, sl_g_getaliveobjectsarea,
    sl_g_getobjectssamemap, sl_g_getaliveobjectssamemap,
    sl_g_getobjectsinmap,
};
use crate::game::scripting::map_globals::{
    sl_g_sendanimation, sl_g_playsound, sl_g_sendaction, sl_g_msg,
    sl_g_dropitem, sl_g_dropitemxy,
    sl_g_objectcanmove, sl_g_objectcanmovefrom,
    sl_g_repeatanimation, sl_g_selfanimation, sl_g_selfanimationxy,
    sl_g_sendparcel, sl_g_throwblock,
};
use crate::game::scripting::types::item::fixed_str;

// ── Object collection methods ─────────────────────────────────────────────────

/// Create a `getObjectsInCell` / `getAliveObjectsInCell` / `getObjectsInCellWithTraps`
/// Lua function. `variant` is the method name string that selects the FFI call.
pub fn make_cell_query_fn(lua: &mlua::Lua, variant: &str) -> mlua::Result<mlua::Value> {
    let variant = variant.to_string();
    lua.create_function(
        move |lua, (_self, m, x, y, bl_type): (mlua::Value, i32, i32, i32, i32)| {
            const MAX: usize = 256;
            let mut ptrs = vec![std::ptr::null_mut::<std::ffi::c_void>(); MAX];
            let raw_count = unsafe {
                match variant.as_str() {
                    "getAliveObjectsInCell" =>
                        sl_g_getaliveobjectscell(m, x, y, bl_type, ptrs.as_mut_ptr(), MAX as i32),
                    "getObjectsInCellWithTraps" =>
                        sl_g_getobjectscellwithtraps(m, x, y, bl_type, ptrs.as_mut_ptr(), MAX as i32),
                    _ =>
                        sl_g_getobjectscell(m, x, y, bl_type, ptrs.as_mut_ptr(), MAX as i32),
                }
            };
            let count = (raw_count.max(0) as usize).min(MAX);
            let tbl = lua.create_table()?;
            for (i, &bl) in ptrs[..count].iter().enumerate() {
                let val = unsafe {
                    crate::game::scripting::bl_to_lua(lua, bl).unwrap_or(mlua::Value::Nil)
                };
                tbl.raw_set(i + 1, val)?;
            }
            Ok(tbl)
        },
    )
    .map(mlua::Value::Function)
}

/// Create a `getObjectsInArea` / `getAliveObjectsInArea` / `getObjectsInSameMap` /
/// `getAliveObjectsInSameMap` Lua method. Unlike `make_cell_query_fn`, these use
/// the entity's own position (bl->m, bl->x, bl->y) rather than explicit coords.
/// The entity's stable numeric ID is captured at __index lookup time and re-resolved
/// to a live pointer via `map_id2bl` on each invocation, so the closure never holds
/// a dangling pointer if the C entity is freed between calls.
pub fn make_area_query_fn(lua: &mlua::Lua, variant: &str, self_ptr: *mut std::ffi::c_void) -> mlua::Result<mlua::Value> {
    if self_ptr.is_null() {
        return Err(mlua::Error::external("null self_ptr in make_area_query_fn"));
    }
    let variant = variant.to_string();
    // Extract the stable BL id now; the raw pointer may dangle after the entity is freed.
    let entity_id: u32 = unsafe { (*(self_ptr as *mut BlockList)).id };
    lua.create_function(
        move |lua, (_self, bl_type): (mlua::Value, i32)| {
            const MAX: usize = 512;
            // Re-resolve the entity pointer on every call so we never use a dangling ptr.
            let bl_ptr = map_id2bl_ref(entity_id) as *mut std::ffi::c_void;
            if bl_ptr.is_null() {
                return Ok(lua.create_table()?);
            }
            let mut ptrs = vec![std::ptr::null_mut::<std::ffi::c_void>(); MAX];
            let raw_count = unsafe {
                match variant.as_str() {
                    "getAliveObjectsInArea" =>
                        sl_g_getaliveobjectsarea(bl_ptr, bl_type, ptrs.as_mut_ptr(), MAX as i32),
                    "getObjectsInSameMap" =>
                        sl_g_getobjectssamemap(bl_ptr, bl_type, ptrs.as_mut_ptr(), MAX as i32),
                    "getAliveObjectsInSameMap" =>
                        sl_g_getaliveobjectssamemap(bl_ptr, bl_type, ptrs.as_mut_ptr(), MAX as i32),
                    _ =>
                        sl_g_getobjectsarea(bl_ptr, bl_type, ptrs.as_mut_ptr(), MAX as i32),
                }
            };
            let count = (raw_count.max(0) as usize).min(MAX);
            let tbl = lua.create_table()?;
            for (i, &bl) in ptrs[..count].iter().enumerate() {
                let val = unsafe {
                    crate::game::scripting::bl_to_lua(lua, bl).unwrap_or(mlua::Value::Nil)
                };
                tbl.raw_set(i + 1, val)?;
            }
            Ok(tbl)
        },
    )
    .map(mlua::Value::Function)
}

/// Create a `getObjectsInMap` Lua function.
pub fn make_map_query_fn(lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
    lua.create_function(|lua, (_self, m, bl_type): (mlua::Value, i32, i32)| {
        const MAX: usize = 4096;
        let mut ptrs = vec![std::ptr::null_mut::<std::ffi::c_void>(); MAX];
        let raw_count = unsafe {
            sl_g_getobjectsinmap(m, bl_type, ptrs.as_mut_ptr(), MAX as i32)
        };
        let count = (raw_count.max(0) as usize).min(MAX);
        let tbl = lua.create_table()?;
        for (i, &bl) in ptrs[..count].iter().enumerate() {
            let val = unsafe {
                crate::game::scripting::bl_to_lua(lua, bl).unwrap_or(mlua::Value::Nil)
            };
            tbl.raw_set(i + 1, val)?;
        }
        Ok(tbl)
    })
    .map(mlua::Value::Function)
}

/// Create a `getBlock` Lua method.
/// Takes a BL id (integer), calls `map_id2bl`, and returns the typed object or nil.
pub fn make_getblock_fn(lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
    lua.create_function(|lua, (_self, id): (mlua::Value, u32)| {
        let bl = map_id2bl_ref(id) as *mut std::ffi::c_void;
        if bl.is_null() {
            return Ok(mlua::Value::Nil);
        }
        unsafe { crate::game::scripting::bl_to_lua(lua, bl) }
    })
    .map(mlua::Value::Function)
}

// ── Shared block-object method factories — Task 6 ─────────────────────────────

fn val_to_int(v: &mlua::Value) -> i32 {
    match v {
        mlua::Value::Integer(i) => *i as i32,
        mlua::Value::Number(f)  => *f as i32,
        _ => 0,
    }
}

/// Resolve a Lua value to an item database ID.
/// Integers pass through directly; strings are resolved via itemdb_searchname.
fn val_to_item_id(v: &mlua::Value) -> i32 {
    match v {
        mlua::Value::Integer(i) => *i as i32,
        mlua::Value::Number(f)  => *f as i32,
        mlua::Value::String(s)  => {
            if let Ok(r) = s.to_str() {
                if let Some(item) = crate::database::item_db::searchname(&r) {
                    return item.id as i32;
                }
            }
            0
        }
        _ => 0,
    }
}

/// Safely extract the stable BL entity id from a raw pointer at factory-creation time.
/// Returns `None` if `ptr` is null, avoiding unsafe dereference of a null pointer.
fn extract_entity_id(ptr: *mut std::ffi::c_void) -> Option<u32> {
    if ptr.is_null() {
        return None;
    }
    Some(unsafe { (*(ptr as *mut BlockList)).id })
}

pub fn make_sendanimation_fn(lua: &mlua::Lua, self_ptr: *mut std::ffi::c_void) -> mlua::Result<mlua::Value> {
    let Some(entity_id) = extract_entity_id(self_ptr) else {
        return lua.create_function(|_, _: mlua::MultiValue| Ok(())).map(mlua::Value::Function);
    };
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let anim  = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let times = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        let bl_ptr = map_id2bl_ref(entity_id) as *mut std::ffi::c_void;
        if bl_ptr.is_null() { return Ok(()); }
        unsafe { sl_g_sendanimation(bl_ptr, anim, times); }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_playsound_fn(lua: &mlua::Lua, self_ptr: *mut std::ffi::c_void) -> mlua::Result<mlua::Value> {
    let Some(entity_id) = extract_entity_id(self_ptr) else {
        return lua.create_function(|_, _: mlua::MultiValue| Ok(())).map(mlua::Value::Function);
    };
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let sound = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let bl_ptr = map_id2bl_ref(entity_id) as *mut std::ffi::c_void;
        if bl_ptr.is_null() { return Ok(()); }
        unsafe { sl_g_playsound(bl_ptr, sound); }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_sendaction_fn(lua: &mlua::Lua, self_ptr: *mut std::ffi::c_void) -> mlua::Result<mlua::Value> {
    let Some(entity_id) = extract_entity_id(self_ptr) else {
        return lua.create_function(|_, _: mlua::MultiValue| Ok(())).map(mlua::Value::Function);
    };
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let action = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let speed  = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        let bl_ptr = map_id2bl_ref(entity_id) as *mut std::ffi::c_void;
        if bl_ptr.is_null() { return Ok(()); }
        unsafe { sl_g_sendaction(bl_ptr, action, speed); }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_msg_fn(lua: &mlua::Lua, self_ptr: *mut std::ffi::c_void) -> mlua::Result<mlua::Value> {
    let Some(entity_id) = extract_entity_id(self_ptr) else {
        return lua.create_function(|_, _: mlua::MultiValue| Ok(())).map(mlua::Value::Function);
    };
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let color  = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let msg    = match a.get(2) {
            Some(mlua::Value::String(s)) => String::from_utf8_lossy(&*s.as_bytes()).into_owned(),
            _ => String::new(),
        };
        let target = a.get(3).map(|v| val_to_int(v)).unwrap_or(-1);
        let cs = std::ffi::CString::new(msg.as_bytes()).map_err(|e| {
            mlua::Error::RuntimeError(format!(
                "msg: string contains interior NUL at byte {}",
                e.nul_position()
            ))
        })?;
        let bl_ptr = map_id2bl_ref(entity_id) as *mut std::ffi::c_void;
        if bl_ptr.is_null() { return Ok(()); }
        unsafe { sl_g_msg(bl_ptr, color, cs.as_ptr(), target); }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_dropitem_fn(lua: &mlua::Lua, self_ptr: *mut std::ffi::c_void) -> mlua::Result<mlua::Value> {
    let Some(entity_id) = extract_entity_id(self_ptr) else {
        return lua.create_function(|_, _: mlua::MultiValue| Ok(())).map(mlua::Value::Function);
    };
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let item   = a.get(1).map(|v| val_to_item_id(v)).unwrap_or(0);
        let amount = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        let owner  = a.get(3).map(|v| val_to_int(v)).unwrap_or(0);
        let bl_ptr = map_id2bl_ref(entity_id) as *mut std::ffi::c_void;
        if bl_ptr.is_null() { return Ok(()); }
        unsafe { sl_g_dropitem(bl_ptr, item, amount, owner); }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_dropitemxy_fn(lua: &mlua::Lua, self_ptr: *mut std::ffi::c_void) -> mlua::Result<mlua::Value> {
    let Some(entity_id) = extract_entity_id(self_ptr) else {
        return lua.create_function(|_, _: mlua::MultiValue| Ok(())).map(mlua::Value::Function);
    };
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let item   = a.get(1).map(|v| val_to_item_id(v)).unwrap_or(0);
        let amount = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        let m      = a.get(3).map(|v| val_to_int(v)).unwrap_or(0);
        let x      = a.get(4).map(|v| val_to_int(v)).unwrap_or(0);
        let y      = a.get(5).map(|v| val_to_int(v)).unwrap_or(0);
        let owner  = a.get(6).map(|v| val_to_int(v)).unwrap_or(0);
        let bl_ptr = map_id2bl_ref(entity_id) as *mut std::ffi::c_void;
        if bl_ptr.is_null() { return Ok(()); }
        unsafe { sl_g_dropitemxy(bl_ptr, item, amount, m, x, y, owner); }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_objectcanmove_fn(lua: &mlua::Lua, self_ptr: *mut std::ffi::c_void) -> mlua::Result<mlua::Value> {
    let Some(entity_id) = extract_entity_id(self_ptr) else {
        return lua.create_function(|_, _: mlua::MultiValue| Ok(false)).map(mlua::Value::Function);
    };
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let x    = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let y    = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        let side = a.get(3).map(|v| val_to_int(v)).unwrap_or(0);
        let bl_ptr = map_id2bl_ref(entity_id) as *mut std::ffi::c_void;
        if bl_ptr.is_null() { return Ok(false); }
        Ok(unsafe { sl_g_objectcanmove(bl_ptr, x, y, side) } != 0)
    }).map(mlua::Value::Function)
}

pub fn make_objectcanmovefrom_fn(lua: &mlua::Lua, self_ptr: *mut std::ffi::c_void) -> mlua::Result<mlua::Value> {
    let Some(entity_id) = extract_entity_id(self_ptr) else {
        return lua.create_function(|_, _: mlua::MultiValue| Ok(false)).map(mlua::Value::Function);
    };
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let x    = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let y    = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        let side = a.get(3).map(|v| val_to_int(v)).unwrap_or(0);
        let bl_ptr = map_id2bl_ref(entity_id) as *mut std::ffi::c_void;
        if bl_ptr.is_null() { return Ok(false); }
        Ok(unsafe { sl_g_objectcanmovefrom(bl_ptr, x, y, side) } != 0)
    }).map(mlua::Value::Function)
}

pub fn make_repeatanimation_fn(lua: &mlua::Lua, self_ptr: *mut std::ffi::c_void) -> mlua::Result<mlua::Value> {
    let Some(entity_id) = extract_entity_id(self_ptr) else {
        return lua.create_function(|_, _: mlua::MultiValue| Ok(())).map(mlua::Value::Function);
    };
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let anim     = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let duration = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        let bl_ptr = map_id2bl_ref(entity_id) as *mut std::ffi::c_void;
        if bl_ptr.is_null() { return Ok(()); }
        unsafe { sl_g_repeatanimation(bl_ptr, anim, duration); }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_selfanimation_fn(lua: &mlua::Lua, self_ptr: *mut std::ffi::c_void) -> mlua::Result<mlua::Value> {
    let Some(entity_id) = extract_entity_id(self_ptr) else {
        return lua.create_function(|_, _: mlua::MultiValue| Ok(())).map(mlua::Value::Function);
    };
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let target = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let anim   = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        let times  = a.get(3).map(|v| val_to_int(v)).unwrap_or(0);
        let bl_ptr = map_id2bl_ref(entity_id) as *mut std::ffi::c_void;
        if bl_ptr.is_null() { return Ok(()); }
        unsafe { sl_g_selfanimation(bl_ptr, target, anim, times); }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_selfanimationxy_fn(lua: &mlua::Lua, self_ptr: *mut std::ffi::c_void) -> mlua::Result<mlua::Value> {
    let Some(entity_id) = extract_entity_id(self_ptr) else {
        return lua.create_function(|_, _: mlua::MultiValue| Ok(())).map(mlua::Value::Function);
    };
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let target = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let anim   = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        let x      = a.get(3).map(|v| val_to_int(v)).unwrap_or(0);
        let y      = a.get(4).map(|v| val_to_int(v)).unwrap_or(0);
        let times  = a.get(5).map(|v| val_to_int(v)).unwrap_or(0);
        let bl_ptr = map_id2bl_ref(entity_id) as *mut std::ffi::c_void;
        if bl_ptr.is_null() { return Ok(()); }
        unsafe { sl_g_selfanimationxy(bl_ptr, target, anim, x, y, times); }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_sendparcel_fn(lua: &mlua::Lua, self_ptr: *mut std::ffi::c_void) -> mlua::Result<mlua::Value> {
    let Some(entity_id) = extract_entity_id(self_ptr) else {
        return lua.create_function(|_, _: mlua::MultiValue| Ok(())).map(mlua::Value::Function);
    };
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let receiver = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let sender   = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        let item     = a.get(3).map(|v| val_to_int(v)).unwrap_or(0);
        let amount   = a.get(4).map(|v| val_to_int(v)).unwrap_or(0);
        let owner    = a.get(5).map(|v| val_to_int(v)).unwrap_or(0);
        let engrave  = match a.get(6) {
            Some(mlua::Value::String(s)) => String::from_utf8_lossy(&*s.as_bytes()).into_owned(),
            _ => String::new(),
        };
        let npcflag  = a.get(7).map(|v| val_to_int(v)).unwrap_or(0);
        let cs = std::ffi::CString::new(engrave.as_bytes()).map_err(|e| {
            mlua::Error::RuntimeError(format!(
                "sendParcel: engrave string contains interior NUL at byte {}",
                e.nul_position()
            ))
        })?;
        let bl_ptr = map_id2bl_ref(entity_id) as *mut std::ffi::c_void;
        if bl_ptr.is_null() { return Ok(()); }
        unsafe { sl_g_sendparcel(bl_ptr, receiver, sender, item, amount, owner, cs.as_ptr(), npcflag); }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_throwblock_fn(lua: &mlua::Lua, self_ptr: *mut std::ffi::c_void) -> mlua::Result<mlua::Value> {
    let Some(entity_id) = extract_entity_id(self_ptr) else {
        return lua.create_function(|_, _: mlua::MultiValue| Ok(())).map(mlua::Value::Function);
    };
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let x      = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let y      = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        let icon   = a.get(3).map(|v| val_to_int(v)).unwrap_or(0);
        let color  = a.get(4).map(|v| val_to_int(v)).unwrap_or(0);
        let action = a.get(5).map(|v| val_to_int(v)).unwrap_or(0);
        let bl_ptr = map_id2bl_ref(entity_id) as *mut std::ffi::c_void;
        if bl_ptr.is_null() { return Ok(()); }
        unsafe { sl_g_throwblock(bl_ptr, x, y, icon, color, action); }
        Ok(())
    }).map(mlua::Value::Function)
}

// ── Map property fields ───────────────────────────────────────────────────────

/// Read a map property by Lua key for the map at index `m`.
///
/// Returns `Some(Ok(value))` for any known map field, `Some(Ok(Nil))` if the
/// map slot is not loaded, and `None` for any key that is not a map field.
///
/// # Safety
/// Dereferences the raw `MapData` pointer returned by `get_map_ptr`.
pub unsafe fn map_field(
    lua: &mlua::Lua,
    m: i32,
    key: &str,
) -> Option<mlua::Result<mlua::Value>> {
    // Reject negative indices and values that wrap when cast to u16 (e.g. -1 → 65535).
    if !(0..=u16::MAX as i32).contains(&m) {
        return Some(Ok(mlua::Value::Nil));
    }
    let mp: *mut MapData = get_map_ptr(m as u16);

    // Quick exit for unknown keys before touching the (possibly null) pointer.
    macro_rules! int {
        ($e:expr) => {
            Some(Ok(mlua::Value::Integer($e as i64)))
        };
    }
    macro_rules! str_field {
        ($arr:expr) => {{
            let s = fixed_str($arr);
            Some(lua.create_string(s.as_str()).map(mlua::Value::String))
        }};
    }

    match key {
        // Integer map fields
        "mapId"      => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).id) },
        "bgm"        => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).bgm) },
        "bgmType"    => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).bgmtype) },
        "pvp"        => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).pvp) },
        "spell"      => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).spell) },
        "light"      => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).light) },
        "weather"    => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).weather) },
        "sweepTime"  => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).sweeptime) },
        "canTalk"    => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).cantalk) },
        "showGhosts" => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).show_ghosts) },
        "region"     => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).region) },
        "indoor"     => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).indoor) },
        "warpOut"    => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).warpout) },
        "bind"       => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).bind) },
        "reqLvl"     => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).reqlvl) },
        "reqVita"    => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).reqvita) },
        "reqMana"    => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).reqmana) },
        "maxLvl"     => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).lvlmax) },
        "maxVita"    => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).vitamax) },
        "maxMana"    => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).manamax) },
        "reqPath"    => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).reqpath) },
        "reqMark"    => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).reqmark) },
        "canSummon"  => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).summon) },
        "canUse"     => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).can_use) },
        "canEat"     => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).can_eat) },
        "canSmoke"   => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).can_smoke) },
        "canMount"   => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).can_mount) },
        "canGroup"   => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { int!((*mp).can_group) },
        // String map fields
        "mapTitle"   => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { str_field!(&(*mp).title) },
        "mapFile"    => if mp.is_null() { Some(Ok(mlua::Value::Nil)) } else { str_field!(&(*mp).mapfile) },
        // Not a map field — caller should check type-specific keys.
        _ => None,
    }
}

// ── GfxViewer fields (MOB and NPC only — PC uses sl_pc_gfx_* FFI accessors) ──

/// Read a gfx property from a `GfxViewer` by Lua key name.
/// Returns `Some(Ok(value))` for any known gfx field, `None` for unknown keys.
///
/// # Safety
/// Dereferences `gfx`.
pub unsafe fn gfx_read(lua: &mlua::Lua, gfx: *const GfxViewer, key: &str) -> Option<mlua::Result<mlua::Value>> {
    if gfx.is_null() {
        return None;
    }
    macro_rules! int {
        ($e:expr) => { Some(Ok(mlua::Value::Integer($e as i64))) };
    }
    match key {
        "gfxWeap"      => int!((*gfx).weapon),
        "gfxWeapC"     => int!((*gfx).cweapon),
        "gfxArmor"     => int!((*gfx).armor),
        "gfxArmorC"    => int!((*gfx).carmor),
        "gfxHelm"      => int!((*gfx).helm),
        "gfxHelmC"     => int!((*gfx).chelm),
        "gfxFaceA"     => int!((*gfx).face_acc),
        "gfxFaceAC"    => int!((*gfx).cface_acc),
        "gfxCrown"     => int!((*gfx).crown),
        "gfxCrownC"    => int!((*gfx).ccrown),
        "gfxShield"    => int!((*gfx).shield),
        "gfxShieldC" | "gfxShiedlC" => int!((*gfx).cshield),  // gfxShiedlC: C typo preserved
        "gfxNeck"      => int!((*gfx).necklace),
        "gfxNeckC"     => int!((*gfx).cnecklace),
        "gfxMantle"    => int!((*gfx).mantle),
        "gfxMantleC"   => int!((*gfx).cmantle),
        "gfxCape"      => int!((*gfx).mantle),
        "gfxCapeC"     => int!((*gfx).cmantle),
        "gfxBoots"     => int!((*gfx).boots),
        "gfxBootsC"    => int!((*gfx).cboots),
        "gfxFaceAT"    => int!((*gfx).face_acc_t),
        "gfxFaceATC"   => int!((*gfx).cface_acc_t),
        "gfxHair"      => int!((*gfx).hair),
        "gfxHairC"     => int!((*gfx).chair),
        "gfxFace"      => int!((*gfx).face),
        "gfxFaceC"     => int!((*gfx).cface),
        "gfxSkinC"     => int!((*gfx).cskin),
        "gfxDye"       => int!((*gfx).dye),
        "gfxTitleColor" => int!((*gfx).title_color),
        "gfxName" => {
            let s = fixed_str(&(*gfx).name);
            Some(lua.create_string(s.as_str()).map(mlua::Value::String))
        }
        _ => None,
    }
}

/// Write a gfx property into a `GfxViewer` by Lua key name.
/// Returns `true` if the key was handled, `false` if unknown.
///
/// # Safety
/// Dereferences `gfx`.
pub unsafe fn gfx_write(gfx: *mut GfxViewer, key: &str, val: i32, str_val: Option<&[u8]>) -> bool {
    if gfx.is_null() {
        return false;
    }
    match key {
        "gfxWeap"      => { (*gfx).weapon      = val as u16; true }
        "gfxWeapC"     => { (*gfx).cweapon     = val as u8;  true }
        "gfxArmor"     => { (*gfx).armor       = val as u16; true }
        "gfxArmorC"    => { (*gfx).carmor      = val as u8;  true }
        "gfxHelm"      => { (*gfx).helm        = val as u16; true }
        "gfxHelmC"     => { (*gfx).chelm       = val as u8;  true }
        "gfxFaceA"     => { (*gfx).face_acc    = val as u16; true }
        "gfxFaceAC"    => { (*gfx).cface_acc   = val as u8;  true }
        "gfxCrown"     => { (*gfx).crown       = val as u16; true }
        "gfxCrownC"    => { (*gfx).ccrown      = val as u8;  true }
        "gfxShield"    => { (*gfx).shield      = val as u16; true }
        "gfxShieldC" | "gfxShiedlC" => { (*gfx).cshield = val as u8; true }  // gfxShiedlC: C typo preserved
        "gfxNeck"      => { (*gfx).necklace    = val as u16; true }
        "gfxNeckC"     => { (*gfx).cnecklace   = val as u8;  true }
        "gfxMantle"    => { (*gfx).mantle      = val as u16; true }
        "gfxMantleC"   => { (*gfx).cmantle     = val as u8;  true }
        "gfxCape"      => { (*gfx).mantle      = val as u16; true }
        "gfxCapeC"     => { (*gfx).cmantle     = val as u8;  true }
        "gfxBoots"     => { (*gfx).boots       = val as u16; true }
        "gfxBootsC"    => { (*gfx).cboots      = val as u8;  true }
        "gfxFaceAT"    => { (*gfx).face_acc_t  = val as u16; true }
        "gfxFaceATC"   => { (*gfx).cface_acc_t = val as u8;  true }
        "gfxHair"      => { (*gfx).hair        = val as u8;  true }
        "gfxHairC"     => { (*gfx).chair       = val as u8;  true }
        "gfxFace"      => { (*gfx).face        = val as u8;  true }
        "gfxFaceC"     => { (*gfx).cface       = val as u8;  true }
        "gfxSkinC"     => { (*gfx).cskin       = val as u8;  true }
        "gfxDye"       => { (*gfx).dye         = val as u8;  true }
        "gfxTitleColor" => { (*gfx).title_color = val as u8; true }
        "gfxName" => {
            if let Some(bytes) = str_val {
                let dst = (*gfx).name.as_mut_ptr();
                let cap = (*gfx).name.len().saturating_sub(1);
                if cap > 0 {
                    let n = bytes.len().min(cap);
                    for i in 0..n {
                        *dst.add(i) = bytes[i] as i8;
                    }
                    *dst.add(n) = 0;
                }
            }
            true
        }
        _ => false,
    }
}
