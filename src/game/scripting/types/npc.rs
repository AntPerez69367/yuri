use std::ffi::CString;
use mlua::{MetaMethod, UserData, UserDataMethods};

use crate::common::types::Point;
use crate::database::map_db::MapData;
use crate::database::map_db::get_map_ptr;
use crate::common::traits::LegacyEntity;
use crate::game::npc::{NpcData, npc_move, npc_warp};
use crate::game::scripting::map_globals::{
    sl_g_sendside, sl_g_sendanimxy, sl_g_talk, sl_g_deliddb, sl_g_addpermanentspawn,
    sl_g_getusers_ids, sl_g_addnpc, NpcSpawnConfig,
};
use crate::game::mob::{mobspawn_onetime, SpawnConfig};
use crate::game::scripting::types::mob::MobObject;
use crate::game::scripting::types::registry::{GameRegObject, MapRegObject, NpcRegObject};
use crate::game::scripting::types::shared;
use crate::common::player::inventory::MAX_EQUIP;

pub struct NpcObject { pub id: u32 }
// u32 is Send — no unsafe impl needed.

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

unsafe fn npc_map(nd: *const NpcData) -> *mut MapData {
    if nd.is_null() { return std::ptr::null_mut(); }
    if !(nd as usize).is_multiple_of(std::mem::align_of::<NpcData>()) { return std::ptr::null_mut(); }
    get_map_ptr((*nd).m)
}

unsafe fn cstr_to_string(p: *const i8) -> String {
    if p.is_null() { return String::new(); }
    std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
}

fn val_to_int(v: &mlua::Value) -> i32 {
    match v {
        mlua::Value::Integer(i) => *i as i32,
        mlua::Value::Number(f)  => *f as i32,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// UserData implementation
// ---------------------------------------------------------------------------
impl UserData for NpcObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {

        // ── __index ─────────────────────────────────────────────────────────
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            let entity_id = this.id;

            // Named methods — return Lua functions capturing the entity ID.
            match key.as_str() {
                "move" => {
                    let id = entity_id;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, _: mlua::MultiValue| {
                            let Some(arc) = crate::game::map_server::map_id2npc_ref(id) else { return Ok(0); };
                            // SAFETY: npc_move may dispatch Lua callbacks that access this NPC.
                            // Do not hold the write lock; use the stable raw pointer from the RwLock.
                            Ok(unsafe { npc_move(arc.legacy.data_ptr()) })
                        }
                    )?));
                }
                "warp" => {
                    let id = entity_id;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (_, m, x, y): (mlua::Value, i32, i32, i32)| {
                            let Some(arc) = crate::game::map_server::map_id2npc_ref(id) else { return Ok(()); };
                            // SAFETY: npc_warp may dispatch Lua callbacks; do not hold write lock.
                            unsafe { npc_warp(arc.legacy.data_ptr(), m, x, y); }
                            Ok(())
                        }
                    )?));
                }
                "getEquippedItem" => {
                    let id = entity_id;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |lua, (_, num): (mlua::Value, usize)| -> mlua::Result<mlua::Value> {
                            if num >= MAX_EQUIP { return Ok(mlua::Value::Nil); }
                            let Some(arc) = crate::game::map_server::map_id2npc_ref(id) else { return Ok(mlua::Value::Nil); };
                            let (item_id, item_custom) = {
                                let nd = arc.read();
                                (nd.equip[num].id, nd.equip[num].custom)
                            };
                            if item_id == 0 { return Ok(mlua::Value::Nil); }
                            let t = lua.create_table()?;
                            t.raw_set(1, item_id)?;
                            t.raw_set(2, item_custom)?;
                            Ok(mlua::Value::Table(t))
                        }
                    )?));
                }
                // Registry sub-objects — constructed lazily from the NPC ID.
                "registry"     => {
                    let ptr = crate::game::map_server::map_id2npc_ref(entity_id)
                        .map(|a| a.legacy.data_ptr() as *mut std::ffi::c_void)
                        .unwrap_or(std::ptr::null_mut());
                    return lua.pack(NpcRegObject { ptr });
                }
                "mapRegistry"  => {
                    let ptr = crate::game::map_server::map_id2npc_ref(entity_id)
                        .map(|a| a.legacy.data_ptr() as *mut std::ffi::c_void)
                        .unwrap_or(std::ptr::null_mut());
                    return lua.pack(MapRegObject { ptr });
                }
                "gameRegistry" => return lua.pack(GameRegObject { ptr: std::ptr::null_mut() }),

                // sendSide() — send a side-update packet to nearby players.
                "sendSide" => {
                    let id = entity_id;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, _: mlua::MultiValue| {
                            sl_g_sendside(id);
                            Ok(())
                        }
                    )?));
                }
                // sendAnimationXY(anim, x, y, times) — broadcast animation at (x,y).
                "sendAnimationXY" => {
                    let id = entity_id;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, args: mlua::MultiValue| {
                            let a: Vec<mlua::Value> = args.into_iter().collect();
                            let anim  = a.get(1).map(val_to_int).unwrap_or(0);
                            let x     = a.get(2).map(val_to_int).unwrap_or(0);
                            let y     = a.get(3).map(val_to_int).unwrap_or(0);
                            let times = a.get(4).map(val_to_int).unwrap_or(0);
                            unsafe { sl_g_sendanimxy(id, anim, x, y, times); }
                            Ok(())
                        }
                    )?));
                }
                // talk(type, msg) — speak in the surrounding area.
                "talk" => {
                    let id = entity_id;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, args: mlua::MultiValue| {
                            let a: Vec<mlua::Value> = args.into_iter().collect();
                            let talk_type = a.get(1).map(val_to_int).unwrap_or(0);
                            let msg = match a.get(2) {
                                Some(mlua::Value::String(s)) => {
                                    String::from_utf8_lossy(&s.as_bytes()).into_owned()
                                }
                                _ => String::new(),
                            };
                            match CString::new(msg.as_bytes()) {
                                Ok(cs) => unsafe { sl_g_talk(id, talk_type, cs.as_ptr()); },
                                Err(e) => tracing::debug!(
                                    "[scripting] NpcObject::talk: msg contains embedded null \
                                     (id={id}, talk_type={talk_type}, err={e})"
                                ),
                            }
                            Ok(())
                        }
                    )?));
                }

                "getBlock" =>
                    return shared::make_getblock_fn(lua),
                "getObjectsInCell" | "getAliveObjectsInCell" | "getObjectsInCellWithTraps" =>
                    return shared::make_cell_query_fn(lua, key.as_str()),
                "getObjectsInArea" | "getAliveObjectsInArea"
                | "getObjectsInSameMap" | "getAliveObjectsInSameMap" =>
                    return shared::make_area_query_fn(lua, key.as_str(), entity_id),
                "getObjectsInMap" =>
                    return shared::make_map_query_fn(lua),
                "sendAnimation"     => return shared::make_sendanimation_fn(lua, entity_id),
                "playSound"         => return shared::make_playsound_fn(lua, entity_id),
                "sendAction"        => return shared::make_sendaction_fn(lua, entity_id),
                "msg"               => return shared::make_msg_fn(lua, entity_id),
                "dropItem"          => return shared::make_dropitem_fn(lua, entity_id),
                "dropItemXY"        => return shared::make_dropitemxy_fn(lua, entity_id),
                "objectCanMove"     => return shared::make_objectcanmove_fn(lua, entity_id),
                "objectCanMoveFrom" => return shared::make_objectcanmovefrom_fn(lua, entity_id),
                "repeatAnimation"   => return shared::make_repeatanimation_fn(lua, entity_id),
                "selfAnimation"     => return shared::make_selfanimation_fn(lua, entity_id),
                "selfAnimationXY"   => return shared::make_selfanimationxy_fn(lua, entity_id),
                "sendParcel"        => return shared::make_sendparcel_fn(lua, entity_id),
                "throw"             => return shared::make_throwblock_fn(lua, entity_id),
                "delFromIDDB" => {
                    let id = entity_id;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, _: mlua::MultiValue| {
                            sl_g_deliddb(id);
                            Ok(())
                        }
                    )?));
                }
                "addPermanentSpawn" => {
                    let id = entity_id;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, _: mlua::MultiValue| {
                            sl_g_addpermanentspawn(id);
                            Ok(())
                        }
                    )?));
                }
                // getUsers() — returns all online players.
                "getUsers" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        |lua, _: mlua::MultiValue| {
                            let ids = sl_g_getusers_ids();
                            let tbl = lua.create_table()?;
                            for (i, &id) in ids.iter().enumerate() {
                                let val = crate::game::scripting::id_to_lua(lua, id)?;
                                tbl.raw_set(i + 1, val)?;
                            }
                            Ok(tbl)
                        },
                    )?));
                }
                // spawn(mob_name_or_id, x, y, amount [,m [,owner]])
                "spawn" => {
                    let id = entity_id;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |lua, args: mlua::MultiValue| -> mlua::Result<mlua::Value> {
                            let args: Vec<mlua::Value> = args.into_iter().collect();
                            let mob_id: u32 = match args.get(1) {
                                Some(mlua::Value::String(s)) => {
                                    let name_str = s.to_str().map_err(mlua::Error::external)?;
                                    crate::database::mob_db::find_id(&name_str) as u32
                                }
                                Some(mlua::Value::Integer(n)) => *n as u32,
                                Some(mlua::Value::Number(f))  => *f as u32,
                                _ => return Ok(mlua::Value::Table(lua.create_table()?)),
                            };
                            let vi = |i: usize| -> i32 { match args.get(i) {
                                Some(mlua::Value::Integer(n)) => *n as i32,
                                Some(mlua::Value::Number(f))  => *f as i32,
                                _ => 0,
                            }};
                            let x = vi(2);
                            let y = vi(3);
                            let amount = vi(4);
                            let owner  = vi(6) as u32;
                            let mut m  = vi(5);
                            if m == 0 {
                                let Some(arc) = crate::game::map_server::map_id2npc_ref(id) else {
                                    return Ok(mlua::Value::Table(lua.create_table()?));
                                };
                                m = arc.read().m as i32;
                            }
                            let tbl = lua.create_table()?;
                            if amount <= 0 { return Ok(mlua::Value::Table(tbl)); }
                            let spawned = unsafe {
                                mobspawn_onetime(mob_id, m, x, y, SpawnConfig { times: amount, start: 0, end: 0, replace: 0, owner })
                            };
                            if spawned.is_empty() { return Ok(mlua::Value::Table(tbl)); }
                            for (i, spawn_id) in spawned.into_iter().enumerate() {
                                tbl.set(i + 1, lua.create_userdata(MobObject { id: spawn_id })?)?;
                            }
                            Ok(mlua::Value::Table(tbl))
                        }
                    )?));
                }

                // addNPC — does not dereference self — works with the sentinel core NPC.
                "addNPC" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        |_, args: mlua::MultiValue| {
                            let args: Vec<mlua::Value> = args.into_iter().collect();
                            let name = match args.get(1) {
                                Some(mlua::Value::String(s)) => s.to_str()?.to_owned(),
                                _ => return Ok(()),
                            };
                            let vi = |i: usize| -> i32 { match args.get(i) {
                                Some(mlua::Value::Integer(n)) => *n as i32,
                                Some(mlua::Value::Number(f))  => *f as i32,
                                _ => 0,
                            }};
                            let vs = |i: usize| -> Option<String> { match args.get(i) {
                                Some(mlua::Value::String(s)) => s.to_str().ok().map(|s| s.to_owned()),
                                _ => None,
                            }};
                            sl_g_addnpc(
                                &name,
                                Point { m: vi(2) as u16, x: vi(3) as u16, y: vi(4) as u16 },
                                NpcSpawnConfig {
                                    subtype: vi(5),
                                    timer: vi(6),
                                    duration: vi(7),
                                    owner: vi(8),
                                    movetime: vi(9),
                                    npc_yname: vs(10),
                                },
                            );
                            Ok(())
                        }
                    )?));
                }
                _ => {}
            }

            // ── Field getters ─────────────────────────────────────────────
            // Sentinel ID (e.g. core = NPC(4294967295)) — no actual NPC data.
            let Some(arc) = crate::game::map_server::map_id2npc_ref(entity_id) else {
                return Ok(mlua::Value::Nil);
            };
            let nd = arc.read();

            macro_rules! int  { ($e:expr) => { Ok(mlua::Value::Integer($e as i64)) }; }
            macro_rules! str_ { ($e:expr) => {
                Ok(mlua::Value::String(lua.create_string(
                    unsafe { cstr_to_string($e) }
                )?))
            }; }
            // Shared map properties (pvp, mapTitle, bgm, etc.) — delegate to shared module.
            if let Some(v) = unsafe { shared::map_field(lua, nd.m as i32, key.as_str()) } {
                return v;
            }
            // Shared GfxViewer properties (gfxFace, gfxWeap, etc.) — delegate to shared module.
            if let Some(v) = unsafe { shared::gfx_read(lua, &nd.gfx, key.as_str()) } {
                return v;
            }

            match key.as_str() {
                // block_list fields
                "x"          => int!(nd.x),
                "y"          => int!(nd.y),
                "m"          => int!(nd.m),
                "blType"     => int!(nd.bl_type),
                "ID"         => int!(nd.id),
                "xmax" => {
                    let mp = unsafe { npc_map(&*nd as *const NpcData) };
                    if mp.is_null() { return Ok(mlua::Value::Nil); }
                    int!(unsafe { (*mp).xs.saturating_sub(1) })
                }
                "ymax" => {
                    let mp = unsafe { npc_map(&*nd as *const NpcData) };
                    if mp.is_null() { return Ok(mlua::Value::Nil); }
                    int!(unsafe { (*mp).ys.saturating_sub(1) })
                }
                // NPC-specific fields
                "id"          => int!(nd.id),
                "look"        => int!(nd.graphic_id),
                "lookColor"   => int!(nd.graphic_color),
                "name"        => str_!(nd.name.as_ptr()),
                "yname"       => str_!(nd.npc_name.as_ptr()),
                "subType"     => int!(nd.subtype),
                "npcType"     => int!(nd.npctype),
                "side"        => int!(nd.side),
                "state"       => int!(nd.state),
                "sex"         => int!(nd.sex),
                "face"        => int!(nd.face),
                "faceColor"   => int!(nd.face_color),
                "hair"        => int!(nd.hair),
                "hairColor"   => int!(nd.hair_color),
                "skinColor"   => int!(nd.skin_color),
                "armorColor"  => int!(nd.armor_color),
                "lastAction"  => int!(nd.lastaction),
                "actionTime"  => int!(nd.actiontime),
                "duration"    => int!(nd.duration),
                "owner"       => int!(nd.owner),
                "startM"      => int!(nd.startm),
                "startX"      => int!(nd.startx),
                "startY"      => int!(nd.starty),
                "shopNPC"     => int!(nd.shop_npc),
                "repairNPC"   => int!(nd.repair_npc),
                "retDist"     => int!(nd.retdist),
                "returning"   => Ok(mlua::Value::Boolean(nd.returning != 0)),
                "bankNPC"     => int!(nd.bank_npc),
                "gfxClone"    => int!(nd.clone),
                _ => {
                    if let Ok(tbl) = lua.globals().get::<mlua::Table>("NPC") {
                        if let Ok(v) = tbl.get::<mlua::Value>(key.as_str()) {
                            if !matches!(v, mlua::Value::Nil) {
                                return Ok(v);
                            }
                        }
                    }
                    tracing::debug!("[scripting] NpcObject: unimplemented __index key={key:?}");
                    Ok(mlua::Value::Nil)
                }
            }
        });

        // ── __newindex ───────────────────────────────────────────────────────
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            let Some(arc) = crate::game::map_server::map_id2npc_ref(this.id) else { return Ok(()); };
            let mut nd = arc.write();
            let mp = unsafe { npc_map(&*nd as *const NpcData) };

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
                                std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const i8,
                                    (*mp).title.as_mut_ptr(), len);
                                (*mp).title[len] = 0;
                            }
                        }
                    }
                }
                "mapFile" => {
                    if let mlua::Value::String(ref s) = val {
                        if !mp.is_null() {
                            let bytes = s.as_bytes();
                            let len = bytes.len().min(1023);
                            unsafe {
                                std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const i8,
                                    (*mp).mapfile.as_mut_ptr(), len);
                                (*mp).mapfile[len] = 0;
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
                // NPC-specific writable fields
                "side"        => nd.side        = val_to_int(&val) as i8,
                "subType"     => nd.subtype     = val_to_int(&val) as u8,
                "look"        => nd.graphic_id  = val_to_int(&val) as u32,
                "lookColor"   => nd.graphic_color = val_to_int(&val) as u32,
                "state"       => nd.state       = val_to_int(&val) as i8,
                "sex"         => nd.sex         = val_to_int(&val) as u16,
                "face"        => nd.face        = val_to_int(&val) as u16,
                "faceColor"   => nd.face_color  = val_to_int(&val) as u16,
                "hair"        => nd.hair        = val_to_int(&val) as u16,
                "hairColor"   => nd.hair_color  = val_to_int(&val) as u16,
                "skinColor"   => nd.skin_color  = val_to_int(&val) as u16,
                "armorColor"  => nd.armor_color = val_to_int(&val) as u16,
                "lastAction"  => nd.lastaction  = val_to_int(&val) as u32,
                "actionTime"  => nd.actiontime  = val_to_int(&val) as u32,
                "duration"    => nd.duration    = val_to_int(&val) as u32,
                "returning"   => nd.returning   = val_to_int(&val) as _,
                // GfxViewer fields — delegated to shared module.
                key if key.starts_with("gfx") && key != "gfxClone" => {
                    let bytes_owned: Option<Vec<u8>> = if let mlua::Value::String(ref s) = val {
                        Some(s.as_bytes().to_vec())
                    } else { None };
                    unsafe { shared::gfx_write(&mut nd.gfx, key, val_to_int(&val), bytes_owned.as_deref()); }
                }
                "gfxClone"    => nd.clone = val_to_int(&val) as i8,
                _ => {
                    tracing::debug!("[scripting] NpcObject: unimplemented __newindex key={key:?}");
                }
            }
            Ok(())
        });
    }
}
