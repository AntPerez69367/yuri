use std::ffi::{c_int, c_uint, CString};
use std::os::raw::c_void;
use mlua::{MetaMethod, UserData, UserDataMethods};

use crate::database::map_db::{BlockList, MapData};
use crate::ffi::map_db::get_map_ptr;
use crate::game::npc::{NpcData, npc_move, npc_warp};
use crate::game::scripting::ffi as sffi;
use crate::game::scripting::types::mob::MobObject;
use crate::game::scripting::types::pc::PcObject;
use crate::game::scripting::types::registry::{GameRegObject, MapRegObject, NpcRegObject};
use crate::game::scripting::types::shared;
use crate::servers::char::charstatus::MAX_EQUIP;

pub struct NpcObject { pub ptr: *mut c_void }
unsafe impl Send for NpcObject {}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

unsafe fn npc_map(nd: *const NpcData) -> *mut MapData {
    get_map_ptr((*nd).bl.m)
}

unsafe fn cstr_to_string(p: *const std::ffi::c_char) -> String {
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
            let nd = this.ptr as *mut NpcData;
            if nd.is_null() { return Ok(mlua::Value::Nil); }

            // Named methods — return Lua functions capturing the raw pointer.
            // npc:move() desugars to npc.move(npc); the closure ignores `npc`
            // since it already captured `ptr`.
            match key.as_str() {
                "move" => {
                    let ptr = this.ptr;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, _: mlua::MultiValue| {
                            Ok(unsafe { npc_move(ptr as *mut NpcData) })
                        }
                    )?));
                }
                "warp" => {
                    let ptr = this.ptr;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (_, m, x, y): (mlua::Value, c_int, c_int, c_int)| {
                            unsafe { npc_warp(ptr as *mut NpcData, m, x, y); }
                            Ok(())
                        }
                    )?));
                }
                "getEquippedItem" => {
                    let ptr = this.ptr;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |lua, (_, num): (mlua::Value, usize)| -> mlua::Result<mlua::Value> {
                            if num >= MAX_EQUIP { return Ok(mlua::Value::Nil); }
                            let item = unsafe { &(*(ptr as *const NpcData)).equip[num] };
                            if item.id == 0 { return Ok(mlua::Value::Nil); }
                            let t = lua.create_table()?;
                            t.raw_set(1, item.id)?;
                            t.raw_set(2, item.custom)?;
                            Ok(mlua::Value::Table(t))
                        }
                    )?));
                }
                // Registry sub-objects — constructed lazily from the NPC pointer.
                "registry"     => return lua.pack(NpcRegObject { ptr: this.ptr }),
                "mapRegistry"  => return lua.pack(MapRegObject { ptr: this.ptr }),
                "gameRegistry" => return lua.pack(GameRegObject { ptr: std::ptr::null_mut() }),

                // sendSide() — send a side-update packet to nearby players.
                "sendSide" => {
                    let ptr = this.ptr;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, _: mlua::MultiValue| {
                            unsafe { sffi::sl_g_sendside(ptr); }
                            Ok(())
                        }
                    )?));
                }
                // sendAnimationXY(anim, x, y, times) — broadcast animation at (x,y).
                "sendAnimationXY" => {
                    let ptr = this.ptr;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, args: mlua::MultiValue| {
                            let a: Vec<mlua::Value> = args.into_iter().collect();
                            let anim  = a.get(1).map(val_to_int).unwrap_or(0);
                            let x     = a.get(2).map(val_to_int).unwrap_or(0);
                            let y     = a.get(3).map(val_to_int).unwrap_or(0);
                            let times = a.get(4).map(val_to_int).unwrap_or(0);
                            unsafe { sffi::sl_g_sendanimxy(ptr, anim, x, y, times); }
                            Ok(())
                        }
                    )?));
                }
                // talk(type, msg) — speak in the surrounding area.
                "talk" => {
                    let ptr = this.ptr;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, args: mlua::MultiValue| {
                            let a: Vec<mlua::Value> = args.into_iter().collect();
                            let talk_type = a.get(1).map(val_to_int).unwrap_or(0);
                            let msg = match a.get(2) {
                                Some(mlua::Value::String(s)) => {
                                    String::from_utf8_lossy(&*s.as_bytes()).into_owned()
                                }
                                _ => String::new(),
                            };
                            if let Ok(cs) = CString::new(msg.as_bytes()) {
                                unsafe { sffi::sl_g_talk(ptr, talk_type, cs.as_ptr()); }
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
                    return shared::make_area_query_fn(lua, key.as_str(), this.ptr),
                "getObjectsInMap" =>
                    return shared::make_map_query_fn(lua),
                // spawn(mob_name_or_id, x, y, amount [,m [,owner]])
                // Spawns `amount` mobs at (x,y) on map m (or NPC's own map if m=0).
                // Returns a Lua table of MobObject userdata.
                "spawn" => {
                    let ptr = this.ptr;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |lua, args: mlua::MultiValue| -> mlua::Result<mlua::Value> {
                            let args: Vec<mlua::Value> = args.into_iter().collect();
                            // args[0]=self, [1]=mob, [2]=x, [3]=y, [4]=amount, [5]=m, [6]=owner
                            let mob_id: c_uint = match args.get(1) {
                                Some(mlua::Value::String(s)) => {
                                    let cs = CString::new(&*s.as_bytes()).map_err(mlua::Error::external)?;
                                    unsafe { sffi::rust_mobdb_id(cs.as_ptr()) as c_uint }
                                }
                                Some(mlua::Value::Integer(n)) => *n as c_uint,
                                Some(mlua::Value::Number(f))  => *f as c_uint,
                                _ => return Ok(mlua::Value::Table(lua.create_table()?)),
                            };
                            let vi = |i: usize| -> c_int { match args.get(i) {
                                Some(mlua::Value::Integer(n)) => *n as c_int,
                                Some(mlua::Value::Number(f))  => *f as c_int,
                                _ => 0,
                            }};
                            let x = vi(2);
                            let y = vi(3);
                            let amount = vi(4);
                            let owner  = vi(6) as c_uint;
                            let mut m  = vi(5);
                            if m == 0 {
                                let align = std::mem::align_of::<NpcData>();
                                if ptr.is_null() || ptr as usize % align != 0 {
                                    return Ok(mlua::Value::Table(lua.create_table()?));
                                }
                                m = unsafe { (*(ptr as *const NpcData)).bl.m as c_int };
                            }
                            let tbl = lua.create_table()?;
                            if amount <= 0 { return Ok(mlua::Value::Table(tbl)); }
                            let spawned = unsafe {
                                sffi::rust_mobspawn_onetime(mob_id, m, x, y, amount, 0, 0, 0, owner)
                            };
                            if spawned.is_null() { return Ok(mlua::Value::Table(tbl)); }
                            for i in 0..amount as usize {
                                let id = unsafe { *spawned.add(i) };
                                let bl = unsafe { sffi::map_id2bl(id) };
                                if !bl.is_null() {
                                    tbl.set(i + 1, lua.create_userdata(MobObject { ptr: bl })?)?;
                                }
                            }
                            unsafe { libc::free(spawned as *mut c_void) };
                            Ok(mlua::Value::Table(tbl))
                        }
                    )?));
                }

                // addNPC(name, m, x, y, subtype [,timer, duration, owner, movetime, yname])
                // Mirrors bll_addnpc from scripting.c.
                // Does not dereference self.ptr — works with the sentinel core NPC.
                "addNPC" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        |_, args: mlua::MultiValue| {
                            let args: Vec<mlua::Value> = args.into_iter().collect();
                            let name = match args.get(1) {
                                Some(mlua::Value::String(s)) => s.to_str()?.to_owned(),
                                _ => return Ok(()),
                            };
                            let vi = |i: usize| -> c_int { match args.get(i) {
                                Some(mlua::Value::Integer(n)) => *n as c_int,
                                Some(mlua::Value::Number(f))  => *f as c_int,
                                _ => 0,
                            }};
                            let vs = |i: usize| -> Option<String> { match args.get(i) {
                                Some(mlua::Value::String(s)) => s.to_str().ok().map(|s| s.to_owned()),
                                _ => None,
                            }};
                            let cname  = CString::new(name).map_err(mlua::Error::external)?;
                            let yname  = vs(9);
                            let cyname = yname.as_deref()
                                .and_then(|s| CString::new(s).ok());
                            unsafe {
                                sffi::sl_g_addnpc(
                                    cname.as_ptr(),
                                    vi(2), vi(3), vi(4), vi(5),
                                    vi(6), vi(7), vi(8), vi(9),
                                    cyname.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
                                );
                            }
                            Ok(())
                        }
                    )?));
                }
                _ => {}
            }

            // ── Field getters ─────────────────────────────────────────────
            // Guard against sentinel / misaligned pointers (e.g. core = NPC(4294967295)).
            // 0xFFFFFFFF is not 8-byte aligned so NpcData would fault on deref.
            if (nd as usize) % std::mem::align_of::<NpcData>() != 0 {
                return Ok(mlua::Value::Nil);
            }
            let nd = unsafe { &*nd };
            let bl = &nd.bl;

            macro_rules! int  { ($e:expr) => { Ok(mlua::Value::Integer($e as i64)) }; }
            macro_rules! str_ { ($e:expr) => {
                Ok(mlua::Value::String(lua.create_string(
                    unsafe { cstr_to_string($e) }
                )?))
            }; }
            // Shared map properties (pvp, mapTitle, bgm, etc.) — delegate to shared module.
            if let Some(v) = unsafe { shared::map_field(lua, bl.m as c_int, key.as_str()) } {
                return v;
            }
            // Shared GfxViewer properties (gfxFace, gfxWeap, etc.) — delegate to shared module.
            if let Some(v) = unsafe { shared::gfx_read(lua, &nd.gfx, key.as_str()) } {
                return v;
            }

            match key.as_str() {
                // block_list fields
                "x"          => int!(bl.x),
                "y"          => int!(bl.y),
                "m"          => int!(bl.m),
                "blType"     => int!(bl.bl_type),
                "ID"         => int!(bl.id),
                "xmax" => {
                    let mp = unsafe { npc_map(nd as *const NpcData) };
                    if mp.is_null() { return Ok(mlua::Value::Nil); }
                    int!(unsafe { (*mp).xs.saturating_sub(1) })
                }
                "ymax" => {
                    let mp = unsafe { npc_map(nd as *const NpcData) };
                    if mp.is_null() { return Ok(mlua::Value::Nil); }
                    int!(unsafe { (*mp).ys.saturating_sub(1) })
                }
                // NPC-specific fields
                "id"          => int!(nd.id),
                "look"        => int!(bl.graphic_id),
                "lookColor"   => int!(bl.graphic_color),
                "name"        => str_!(nd.name.as_ptr()),
                "yname"       => str_!(nd.npc_name.as_ptr()),
                "subType"     => int!(bl.subtype),
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
            let nd = this.ptr as *mut NpcData;
            if nd.is_null() { return Ok(()); }
            if (nd as usize) % std::mem::align_of::<NpcData>() != 0 { return Ok(()); }
            let nd = unsafe { &mut *nd };
            let mp = unsafe { npc_map(nd as *const NpcData) };
            let bl = &mut nd.bl;

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
                "subType"     => bl.subtype     = val_to_int(&val) as u8,
                "look"        => bl.graphic_id  = val_to_int(&val) as u32,
                "lookColor"   => bl.graphic_color = val_to_int(&val) as u32,
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
                // GfxViewer fields — delegated to shared module.
                key if key.starts_with("gfx") && key != "gfxClone" => {
                    let str_owned = if let mlua::Value::String(ref s) = val {
                        s.to_str().ok().map(|x| x.to_string())
                    } else { None };
                    unsafe { shared::gfx_write(&mut nd.gfx, key, val_to_int(&val), str_owned.as_deref()); }
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
