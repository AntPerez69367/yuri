//! Shared __index helpers for all BL-typed scripting objects (PC, MOB, NPC).
//!
//! In the original C scripting.c, these were registered on the base `bll_type`
//! via `typel_extendproto` and inherited by pcl_type, mobl_type, npcl_type.
//! Here we achieve the same with free functions called from each type's __index.

use std::ffi::{c_char, c_int, c_uint};
use std::os::raw::c_void;

use crate::database::map_db::{BlockList, MapData};
use crate::game::types::GfxViewer;
use crate::ffi::map_db::get_map_ptr;
use crate::game::scripting::ffi as sffi;
use crate::game::scripting::types::item::fixed_str;
use crate::game::scripting::types::mob::MobObject;
use crate::game::scripting::types::npc::NpcObject;
use crate::game::scripting::types::pc::PcObject;

// ── Object collection methods ─────────────────────────────────────────────────

/// Create a `getObjectsInCell` / `getAliveObjectsInCell` / `getObjectsInCellWithTraps`
/// Lua function. `variant` is the method name string that selects the FFI call.
/// Mirrors `bll_getobjects_cell` / `bll_getaliveobjects_cell` from scripting.c.
pub fn make_cell_query_fn(lua: &mlua::Lua, variant: &str) -> mlua::Result<mlua::Value> {
    let variant = variant.to_string();
    lua.create_function(
        move |lua, (_self, m, x, y, bl_type): (mlua::Value, c_int, c_int, c_int, c_int)| {
            const MAX: usize = 256;
            let mut ptrs = vec![std::ptr::null_mut::<c_void>(); MAX];
            let count = unsafe {
                match variant.as_str() {
                    "getAliveObjectsInCell" =>
                        sffi::sl_g_getaliveobjectscell(m, x, y, bl_type, ptrs.as_mut_ptr(), MAX as c_int),
                    "getObjectsInCellWithTraps" =>
                        sffi::sl_g_getobjectscellwithtraps(m, x, y, bl_type, ptrs.as_mut_ptr(), MAX as c_int),
                    _ =>
                        sffi::sl_g_getobjectscell(m, x, y, bl_type, ptrs.as_mut_ptr(), MAX as c_int),
                }
            } as usize;
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
/// The `self_ptr` is the raw entity pointer captured at __index lookup time.
/// Mirrors `bll_getobjects_area` / `bll_getaliveobjects_area` from scripting.c.
pub fn make_area_query_fn(lua: &mlua::Lua, variant: &str, self_ptr: *mut c_void) -> mlua::Result<mlua::Value> {
    let variant = variant.to_string();
    lua.create_function(
        move |lua, (_self, bl_type): (mlua::Value, c_int)| {
            const MAX: usize = 512;
            let mut ptrs = vec![std::ptr::null_mut::<c_void>(); MAX];
            let count = unsafe {
                match variant.as_str() {
                    "getAliveObjectsInArea" =>
                        sffi::sl_g_getaliveobjectsarea(self_ptr, bl_type, ptrs.as_mut_ptr(), MAX as c_int),
                    "getObjectsInSameMap" =>
                        sffi::sl_g_getobjectssamemap(self_ptr, bl_type, ptrs.as_mut_ptr(), MAX as c_int),
                    "getAliveObjectsInSameMap" =>
                        sffi::sl_g_getaliveobjectssamemap(self_ptr, bl_type, ptrs.as_mut_ptr(), MAX as c_int),
                    _ =>
                        sffi::sl_g_getobjectsarea(self_ptr, bl_type, ptrs.as_mut_ptr(), MAX as c_int),
                }
            } as usize;
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
/// Mirrors `bll_getobjects_map` from scripting.c.
pub fn make_map_query_fn(lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
    lua.create_function(|lua, (_self, m, bl_type): (mlua::Value, c_int, c_int)| {
        const MAX: usize = 4096;
        let mut ptrs = vec![std::ptr::null_mut::<c_void>(); MAX];
        let count = unsafe {
            sffi::sl_g_getobjectsinmap(m, bl_type, ptrs.as_mut_ptr(), MAX as c_int)
        } as usize;
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
/// Mirrors `bll_getblock` from scripting.c.
pub fn make_getblock_fn(lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
    lua.create_function(|lua, (_self, id): (mlua::Value, c_uint)| {
        let bl = unsafe { sffi::map_id2bl(id) };
        if bl.is_null() {
            return Ok(mlua::Value::Nil);
        }
        unsafe { crate::game::scripting::bl_to_lua(lua, bl) }
    })
    .map(mlua::Value::Function)
}

// ── Shared block-object method factories — Task 6 ─────────────────────────────

fn val_to_int(v: &mlua::Value) -> c_int {
    match v {
        mlua::Value::Integer(i) => *i as c_int,
        mlua::Value::Number(f)  => *f as c_int,
        _ => 0,
    }
}

/// Resolve a Lua value to an item database ID.
/// Integers pass through directly; strings are resolved via itemdb_searchname.
fn val_to_item_id(v: &mlua::Value) -> c_int {
    match v {
        mlua::Value::Integer(i) => *i as c_int,
        mlua::Value::Number(f)  => *f as c_int,
        mlua::Value::String(s)  => {
            if let Some(cs) = s.to_str().ok().and_then(|r| std::ffi::CString::new(r.as_bytes()).ok()) {
                let data = unsafe { crate::ffi::item_db::rust_itemdb_searchname(cs.as_ptr()) };
                if !data.is_null() { return unsafe { (*data).id } as c_int; }
            }
            0
        }
        _ => 0,
    }
}

pub fn make_sendanimation_fn(lua: &mlua::Lua, self_ptr: *mut c_void) -> mlua::Result<mlua::Value> {
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let anim  = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let times = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        unsafe { sffi::sl_g_sendanimation(self_ptr, anim, times); }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_playsound_fn(lua: &mlua::Lua, self_ptr: *mut c_void) -> mlua::Result<mlua::Value> {
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let sound = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        unsafe { sffi::sl_g_playsound(self_ptr, sound); }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_sendaction_fn(lua: &mlua::Lua, self_ptr: *mut c_void) -> mlua::Result<mlua::Value> {
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let action = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let speed  = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        unsafe { sffi::sl_g_sendaction(self_ptr, action, speed); }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_msg_fn(lua: &mlua::Lua, self_ptr: *mut c_void) -> mlua::Result<mlua::Value> {
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let color  = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let msg    = match a.get(2) {
            Some(mlua::Value::String(s)) => String::from_utf8_lossy(&*s.as_bytes()).into_owned(),
            _ => String::new(),
        };
        let target = a.get(3).map(|v| val_to_int(v)).unwrap_or(-1);
        if let Ok(cs) = std::ffi::CString::new(msg.as_bytes()) {
            unsafe { sffi::sl_g_msg(self_ptr, color, cs.as_ptr(), target); }
        }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_dropitem_fn(lua: &mlua::Lua, self_ptr: *mut c_void) -> mlua::Result<mlua::Value> {
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let item   = a.get(1).map(|v| val_to_item_id(v)).unwrap_or(0);
        let amount = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        let owner  = a.get(3).map(|v| val_to_int(v)).unwrap_or(0);
        unsafe { sffi::sl_g_dropitem(self_ptr, item, amount, owner); }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_dropitemxy_fn(lua: &mlua::Lua, self_ptr: *mut c_void) -> mlua::Result<mlua::Value> {
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let item   = a.get(1).map(|v| val_to_item_id(v)).unwrap_or(0);
        let amount = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        let m      = a.get(3).map(|v| val_to_int(v)).unwrap_or(0);
        let x      = a.get(4).map(|v| val_to_int(v)).unwrap_or(0);
        let y      = a.get(5).map(|v| val_to_int(v)).unwrap_or(0);
        let owner  = a.get(6).map(|v| val_to_int(v)).unwrap_or(0);
        unsafe { sffi::sl_g_dropitemxy(self_ptr, item, amount, m, x, y, owner); }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_objectcanmove_fn(lua: &mlua::Lua, self_ptr: *mut c_void) -> mlua::Result<mlua::Value> {
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let x    = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let y    = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        let side = a.get(3).map(|v| val_to_int(v)).unwrap_or(0);
        Ok(unsafe { sffi::sl_g_objectcanmove(self_ptr, x, y, side) } != 0)
    }).map(mlua::Value::Function)
}

pub fn make_objectcanmovefrom_fn(lua: &mlua::Lua, self_ptr: *mut c_void) -> mlua::Result<mlua::Value> {
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let x    = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let y    = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        let side = a.get(3).map(|v| val_to_int(v)).unwrap_or(0);
        Ok(unsafe { sffi::sl_g_objectcanmovefrom(self_ptr, x, y, side) } != 0)
    }).map(mlua::Value::Function)
}

pub fn make_repeatanimation_fn(lua: &mlua::Lua, self_ptr: *mut c_void) -> mlua::Result<mlua::Value> {
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let anim     = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let duration = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        unsafe { sffi::sl_g_repeatanimation(self_ptr, anim, duration); }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_selfanimation_fn(lua: &mlua::Lua, self_ptr: *mut c_void) -> mlua::Result<mlua::Value> {
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let target = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let anim   = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        let times  = a.get(3).map(|v| val_to_int(v)).unwrap_or(0);
        unsafe { sffi::sl_g_selfanimation(self_ptr, target, anim, times); }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_selfanimationxy_fn(lua: &mlua::Lua, self_ptr: *mut c_void) -> mlua::Result<mlua::Value> {
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let target = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let anim   = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        let x      = a.get(3).map(|v| val_to_int(v)).unwrap_or(0);
        let y      = a.get(4).map(|v| val_to_int(v)).unwrap_or(0);
        let times  = a.get(5).map(|v| val_to_int(v)).unwrap_or(0);
        unsafe { sffi::sl_g_selfanimationxy(self_ptr, target, anim, x, y, times); }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_sendparcel_fn(lua: &mlua::Lua, self_ptr: *mut c_void) -> mlua::Result<mlua::Value> {
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
        if let Ok(cs) = std::ffi::CString::new(engrave.as_bytes()) {
            unsafe { sffi::sl_g_sendparcel(self_ptr, receiver, sender, item, amount, owner, cs.as_ptr(), npcflag); }
        }
        Ok(())
    }).map(mlua::Value::Function)
}

pub fn make_throwblock_fn(lua: &mlua::Lua, self_ptr: *mut c_void) -> mlua::Result<mlua::Value> {
    lua.create_function(move |_, args: mlua::MultiValue| {
        let a: Vec<mlua::Value> = args.into_iter().collect();
        let x      = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
        let y      = a.get(2).map(|v| val_to_int(v)).unwrap_or(0);
        let icon   = a.get(3).map(|v| val_to_int(v)).unwrap_or(0);
        let color  = a.get(4).map(|v| val_to_int(v)).unwrap_or(0);
        let action = a.get(5).map(|v| val_to_int(v)).unwrap_or(0);
        unsafe { sffi::sl_g_throwblock(self_ptr, x, y, icon, color, action); }
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
    m: c_int,
    key: &str,
) -> Option<mlua::Result<mlua::Value>> {
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
pub unsafe fn gfx_write(gfx: *mut GfxViewer, key: &str, val: c_int, str_val: Option<&str>) -> bool {
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
        "gfxShieldC"   => { (*gfx).cshield     = val as u8;  true }
        "gfxNeck"      => { (*gfx).necklace    = val as u16; true }
        "gfxNeckC"     => { (*gfx).cnecklace   = val as u8;  true }
        "gfxMantle"    => { (*gfx).mantle      = val as u16; true }
        "gfxMantleC"   => { (*gfx).cmantle     = val as u8;  true }
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
            if let Some(s) = str_val {
                let dst = (*gfx).name.as_mut_ptr();
                let bytes = s.as_bytes();
                let n = bytes.len().min((*gfx).name.len() - 1);
                for i in 0..n {
                    *dst.add(i) = bytes[i] as c_char;
                }
                *dst.add(n) = 0;
            }
            true
        }
        _ => false,
    }
}
