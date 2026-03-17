use mlua::{MetaMethod, UserData, UserDataMethods};

use crate::database::map_db::{MapData, get_map_ptr};
use crate::game::scripting::types::item::{
    BoundItem, fixed_str, item_data_getattr, write_str_field,
};

// MAX_GROUP_MEMBERS from map_server.h
const MAX_GROUP_MEMBERS: usize = 256;

#[repr(C)]
pub struct FloorItemData {
    pub id:            u32,
    pub graphic_id:    u32,
    pub graphic_color: u32,
    pub m:             u16,
    pub x:             u16,
    pub y:             u16,
    pub bl_type:       u8,
    pub subtype:       u8,
    pub data:       BoundItem,
    pub lastamount: u32,
    pub timer:      u32,
    pub looters:    [u32; MAX_GROUP_MEMBERS],
}
// SAFETY: FloorItemData contains raw pointers (next/prev) that are legacy block-list links.
// All access is gated behind unsafe blocks; no Rust code aliases these pointers.
unsafe impl Send for FloorItemData {}
unsafe impl Sync for FloorItemData {}


/// Lua userdata for floor items. Stores entity ID; lookups via `map_id2fl_ref`.
/// After `delete()`, the entity is removed from the map, so subsequent lookups
/// return `None` and all field accesses return nil.
pub struct FloorListObject {
    pub id: u32,
}
// u32 is Send — no unsafe impl needed.

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

unsafe fn fl_map(fl: *const FloorItemData) -> *mut MapData {
    get_map_ptr((*fl).m)
}

fn val_to_int(v: &mlua::Value) -> i32 {
    match v {
        mlua::Value::Integer(i) => *i as i32,
        mlua::Value::Number(f)  => *f as i32,
        _ => 0,
    }
}

fn val_to_uint(v: &mlua::Value) -> u32 {
    match v {
        mlua::Value::Integer(i) => if *i < 0 { 0 } else { (*i).min(u32::MAX as i64) as u32 },
        mlua::Value::Number(f)  => if *f < 0.0 || f.is_nan() { 0 } else { (*f).min(u32::MAX as f64) as u32 },
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// UserData implementation
// ---------------------------------------------------------------------------
impl UserData for FloorListObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {

        // ── __index ─────────────────────────────────────────────────────────
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            let entity_id = this.id;
            let Some(arc) = crate::game::map_server::map_id2fl_ref(entity_id) else {
                return Ok(mlua::Value::Nil);
            };
            let fl = unsafe { &*arc.data_ptr() };
            let fl_ref = fl;

            macro_rules! int  { ($e:expr) => { Ok(mlua::Value::Integer($e as i64)) }; }
            macro_rules! cstr { ($arr:expr) => {{
                let s = fixed_str($arr);
                Ok(mlua::Value::String(lua.create_string(s)?))
            }}; }
            macro_rules! map_int { ($field:ident) => {{
                let mp = unsafe { fl_map(fl) };
                if mp.is_null() { return Ok(mlua::Value::Nil); }
                int!(unsafe { (*mp).$field })
            }}; }

            // Named method — getTrapSpotters
            if key == "getTrapSpotters" {
                let id = entity_id;
                return Ok(mlua::Value::Function(lua.create_function(
                    move |lua, _: mlua::MultiValue| {
                        let Some(arc) = crate::game::map_server::map_id2fl_ref(id) else {
                            return lua.create_table().map(mlua::Value::Table);
                        };
                        let fl = unsafe { &*arc.data_ptr() };
                        let tbl = lua.create_table()?;
                        let mut idx = 1;
                        for &tid in fl.data.traps_table.iter() {
                            if tid != 0 {
                                tbl.raw_set(idx, tid)?;
                                idx += 1;
                            }
                        }
                        Ok(mlua::Value::Table(tbl))
                    }
                )?));
            }
            if key == "addTrapSpotters" {
                let id = entity_id;
                return Ok(mlua::Value::Function(lua.create_function(
                    move |_, playerid: u32| {
                        let Some(arc) = crate::game::map_server::map_id2fl_ref(id) else { return Ok(()); };
                        let fl = unsafe { &mut *arc.data_ptr() };
                        for slot in fl.data.traps_table.iter_mut() {
                            if *slot == 0 {
                                *slot = playerid;
                                break;
                            }
                        }
                        Ok(())
                    }
                )?));
            }

            // ── block_list / map attributes ──────────────────────────────────
            match key.as_str() {
                "x"          => int!(fl_ref.x),
                "y"          => int!(fl_ref.y),
                "m"          => int!(fl_ref.m),
                "blType"     => int!(fl_ref.bl_type),
                "ID"         => int!(fl_ref.id),
                "xmax" => {
                    let mp = unsafe { fl_map(fl) };
                    if mp.is_null() { return Ok(mlua::Value::Nil); }
                    int!(unsafe { (*mp).xs.saturating_sub(1) })
                }
                "ymax" => {
                    let mp = unsafe { fl_map(fl) };
                    if mp.is_null() { return Ok(mlua::Value::Nil); }
                    int!(unsafe { (*mp).ys.saturating_sub(1) })
                }
                "mapId"      => map_int!(id),
                "mapTitle"   => {
                    let mp = unsafe { fl_map(fl) };
                    if mp.is_null() { return Ok(mlua::Value::Nil); }
                    cstr!(unsafe { &(*mp).title })
                }
                "mapFile"    => {
                    let mp = unsafe { fl_map(fl) };
                    if mp.is_null() { return Ok(mlua::Value::Nil); }
                    cstr!(unsafe { &(*mp).mapfile })
                }
                "bgm"        => map_int!(bgm),
                "bgmType"    => map_int!(bgmtype),
                "pvp"        => map_int!(pvp),
                "spell"      => map_int!(spell),
                "light"      => map_int!(light),
                "weather"    => map_int!(weather),
                "sweepTime"  => map_int!(sweeptime),
                "canTalk"    => map_int!(cantalk),
                "showGhosts" => map_int!(show_ghosts),
                "region"     => map_int!(region),
                "indoor"     => map_int!(indoor),
                "warpOut"    => map_int!(warpout),
                "bind"       => map_int!(bind),
                "reqLvl"     => map_int!(reqlvl),
                "reqVita"    => map_int!(reqvita),
                "reqMana"    => map_int!(reqmana),
                "maxLvl"     => map_int!(lvlmax),
                "maxVita"    => map_int!(vitamax),
                "maxMana"    => map_int!(manamax),
                "reqPath"    => map_int!(reqpath),
                "reqMark"    => map_int!(reqmark),
                "canSummon"  => map_int!(summon),
                "canUse"     => map_int!(can_use),
                "canEat"     => map_int!(can_eat),
                "canSmoke"   => map_int!(can_smoke),
                "canMount"   => map_int!(can_mount),
                "canGroup"   => map_int!(can_group),
                // ── FloorItem-specific attributes ───────────────────────────
                "id"           => int!(fl_ref.data.id),
                "amount"       => int!(fl_ref.data.amount),
                "lastAmount"   => int!(fl_ref.lastamount),
                "owner"        => int!(fl_ref.data.owner),
                "realName"     => cstr!(&fl_ref.data.real_name),
                "dura"         => int!(fl_ref.data.dura),
                "protected"    => int!(fl_ref.data.protected),
                "custom"       => int!(fl_ref.data.custom),
                "customIcon"   => int!(fl_ref.data.custom_icon),
                "customIconC"  => int!(fl_ref.data.custom_icon_color),
                "customLook"   => int!(fl_ref.data.custom_look),
                "customLookC"  => int!(fl_ref.data.custom_look_color),
                "note"         => cstr!(&fl_ref.data.note),
                "timer"        => int!(fl_ref.timer),
                "looters" => {
                    let tbl = lua.create_table()?;
                    for (i, &id) in fl_ref.looters.iter().enumerate() {
                        tbl.raw_set(i + 1, id)?;
                    }
                    Ok(mlua::Value::Table(tbl))
                }
                "delete" => {
                    let id = entity_id;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, _: mlua::MultiValue| {
                            crate::game::scripting::map_globals::sl_fl_delete(id);
                            Ok(())
                        }
                    )?));
                }
                _ => {
                    let db = crate::database::item_db::search(fl_ref.data.id);
                    unsafe { item_data_getattr(lua, &*db as *const crate::database::item_db::ItemData, &key) }
                }
            }
        });

        // ── __newindex ───────────────────────────────────────────────────────
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            let Some(arc) = crate::game::map_server::map_id2fl_ref(this.id) else { return Ok(()); };
            let fl = unsafe { &mut *arc.data_ptr() };
            let mp = unsafe { fl_map(fl as *const FloorItemData) };

            macro_rules! map_set { ($field:ident) => {
                if !mp.is_null() { unsafe { (*mp).$field = val_to_int(&val) as _; } }
            }; }

            match key.as_str() {
                // map writable fields
                "mapTitle" => {
                    if let mlua::Value::String(ref s) = val {
                        if !mp.is_null() {
                            let bytes = s.as_bytes();
                            let len = bytes.len().min(63);
                            unsafe {
                                std::ptr::copy_nonoverlapping(
                                    bytes.as_ptr() as *const i8,
                                    (*mp).title.as_mut_ptr(), len);
                                (*mp).title[len] = 0;
                            }
                        }
                    }
                }
                "bgm"        => map_set!(bgm),
                "bgmType"    => map_set!(bgmtype),
                "pvp"        => map_set!(pvp),
                "spell"      => map_set!(spell),
                "light"      => map_set!(light),
                "weather"    => map_set!(weather),
                "sweepTime"  => map_set!(sweeptime),
                "canTalk"    => map_set!(cantalk),
                "showGhosts" => map_set!(show_ghosts),
                "region"     => map_set!(region),
                "indoor"     => map_set!(indoor),
                "warpOut"    => map_set!(warpout),
                "bind"       => map_set!(bind),
                "reqLvl"     => map_set!(reqlvl),
                "reqVita"    => map_set!(reqvita),
                "reqMana"    => map_set!(reqmana),
                "reqPath"    => map_set!(reqpath),
                "reqMark"    => map_set!(reqmark),
                "maxLvl"     => map_set!(lvlmax),
                "maxVita"    => map_set!(vitamax),
                "maxMana"    => map_set!(manamax),
                "canSummon"  => map_set!(summon),
                "canUse"     => map_set!(can_use),
                "canEat"     => map_set!(can_eat),
                "canSmoke"   => map_set!(can_smoke),
                "canMount"   => map_set!(can_mount),
                "canGroup"   => map_set!(can_group),
                // FloorItem writable fields
                "amount"      => fl.data.amount      = val_to_int(&val),
                "owner"       => fl.data.owner       = val_to_uint(&val),
                "dura"        => fl.data.dura        = val_to_int(&val),
                "protected"   => fl.data.protected   = val_to_uint(&val),
                "custom"      => fl.data.custom      = val_to_uint(&val),
                "customIcon"  => fl.data.custom_icon = val_to_uint(&val),
                "customIconC" => fl.data.custom_icon_color = val_to_uint(&val),
                "customLook"  => fl.data.custom_look = val_to_uint(&val),
                "customLookC" => fl.data.custom_look_color = val_to_uint(&val),
                "timer"       => fl.timer            = val_to_uint(&val),
                "realName" => {
                    if let mlua::Value::String(ref s) = val {
                        write_str_field(&mut fl.data.real_name, s);
                    }
                }
                "note" => {
                    if let mlua::Value::String(ref s) = val {
                        write_str_field(&mut fl.data.note, s);
                    }
                }
                "looters" => {
                    if let mlua::Value::Table(ref tbl) = val {
                        let mut read_count = 0;
                        for i in 0..MAX_GROUP_MEMBERS {
                            if let Ok(v) = tbl.raw_get::<u32>((i + 1) as i64) {
                                fl.looters[i] = v;
                                read_count += 1;
                            } else {
                                break;
                            }
                        }
                        for i in read_count..MAX_GROUP_MEMBERS {
                            fl.looters[i] = 0;
                        }
                    }
                }
                _ => {}
            }
            Ok(())
        });
    }
}
