use mlua::{MetaMethod, UserData, UserDataMethods};
use std::ffi::{c_char, c_float, c_int, c_uint, CString};
use std::os::raw::c_void;

use crate::database::map_db::{BlockList, MapData};
use crate::ffi::map_db::get_map_ptr;
use crate::database::mob_db::MobDbData;
use crate::game::mob::{
    mob_calcstat, mob_warp, move_mob, move_mob_ignore_object, move_mob_intent, moveghost_mob,
    MobSpawnData, BL_MOB, BL_PC, MAX_MAGIC_TIMERS, MAX_THREATCOUNT,
};
use crate::game::scripting::ffi as sffi;
use crate::game::scripting::types::item::fixed_str;
use crate::game::scripting::types::npc::NpcObject;
use crate::game::scripting::types::pc::PcObject;
use crate::game::scripting::types::registry::{GameRegObject, MapRegObject, MobRegObject};
use crate::game::scripting::types::shared;

pub struct MobObject {
    pub ptr: *mut c_void,
}
unsafe impl Send for MobObject {}

// ---------------------------------------------------------------------------
// C functions not yet in game/mob.rs extern block
// ---------------------------------------------------------------------------
extern "C" {
    fn mob_attack(mob: *mut MobSpawnData, id: c_int) -> c_int;
    fn clif_send_mob_healthscript(mob: *mut c_void, damage: c_int, critical: c_int);
    fn rust_magicdb_id(s: *const c_char) -> c_int;

    // Mob scripting helpers defined in sl_compat.c
    fn sl_mob_addhealth(mob: *mut c_void, damage: c_int);
    fn sl_mob_removehealth(mob: *mut c_void, damage: c_int, caster_id: c_uint);
    fn sl_mob_checkthreat(mob: *mut c_void, player_id: c_uint) -> c_int;
    fn sl_mob_setinddmg(mob: *mut c_void, player_id: c_uint, dmg: c_float) -> c_int;
    fn sl_mob_setgrpdmg(mob: *mut c_void, player_id: c_uint, dmg: c_float) -> c_int;
    fn sl_mob_checkmove(mob: *mut c_void) -> c_int;
    fn sl_mob_setduration(
        mob: *mut c_void,
        name: *const c_char,
        time: c_int,
        caster_id: c_uint,
        recast: c_int,
    );
    fn sl_mob_flushduration(mob: *mut c_void, dis: c_int, minid: c_int, maxid: c_int);
    fn sl_mob_flushdurationnouncast(mob: *mut c_void, dis: c_int, minid: c_int, maxid: c_int);
    fn sl_mob_callbase(mob: *mut c_void, script: *const c_char) -> c_int;
    #[link_name = "map_id2bl"]
    fn map_id2bl_mob(id: c_uint) -> *mut BlockList;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

unsafe fn mob_map(mob: *const MobSpawnData) -> *mut MapData {
    get_map_ptr((*mob).bl.m)
}

fn val_to_int(v: &mlua::Value) -> c_int {
    match v {
        mlua::Value::Integer(i) => *i as c_int,
        mlua::Value::Number(f) => *f as c_int,
        _ => 0,
    }
}

fn val_to_uint(v: &mlua::Value) -> c_uint {
    match v {
        mlua::Value::Integer(i) => *i as c_uint,
        mlua::Value::Number(f) => *f as c_uint,
        _ => 0,
    }
}

fn val_to_float(v: &mlua::Value) -> c_float {
    match v {
        mlua::Value::Integer(i) => *i as c_float,
        mlua::Value::Number(f) => *f as c_float,
        _ => 0.0,
    }
}

// ---------------------------------------------------------------------------
// UserData implementation
// ---------------------------------------------------------------------------
impl UserData for MobObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // ── __index ─────────────────────────────────────────────────────────
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            if this.ptr.is_null() {
                return Ok(mlua::Value::Nil);
            }
            let mob = unsafe { &*(this.ptr as *const MobSpawnData) };
            let bl = &mob.bl;
            let ptr = this.ptr;

            macro_rules! int {
                ($e:expr) => {
                    Ok(mlua::Value::Integer($e as i64))
                };
            }
            macro_rules! bool {
                ($e:expr) => {
                    Ok(mlua::Value::Boolean($e != 0))
                };
            }
            macro_rules! cstr {
                ($arr:expr) => {{
                    let s = unsafe { fixed_str($arr) };
                    Ok(mlua::Value::String(lua.create_string(s)?))
                }};
            }
            macro_rules! map_int {
                ($field:ident) => {{
                    let mp = unsafe { mob_map(mob as *const MobSpawnData) };
                    if mp.is_null() {
                        return Ok(mlua::Value::Nil);
                    }
                    int!(unsafe { (*mp).$field })
                }};
            }
            macro_rules! data_int {
                ($field:ident) => {{
                    if mob.data.is_null() {
                        return Ok(mlua::Value::Nil);
                    }
                    int!(unsafe { (*mob.data).$field })
                }};
            }
            macro_rules! data_cstr {
                ($field:ident) => {{
                    if mob.data.is_null() {
                        return Ok(mlua::Value::Nil);
                    }
                    cstr!(unsafe { &(*mob.data).$field })
                }};
            }

            // ── named methods ────────────────────────────────────────────────
            match key.as_str() {
                "attack" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (_, id): (mlua::Value, c_int)| {
                            if ptr.is_null() {
                                return Ok(0i32);
                            }
                            Ok(unsafe { mob_attack(ptr as *mut MobSpawnData, id) })
                        },
                    )?))
                }
                "addHealth" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (_, damage): (mlua::Value, c_int)| {
                            if ptr.is_null() {
                                return Ok(());
                            }
                            unsafe {
                                sl_mob_addhealth(ptr, damage);
                            }
                            Ok(())
                        },
                    )?))
                }
                "removeHealth" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (_, damage, caster_id): (mlua::Value, c_int, c_uint)| {
                            if ptr.is_null() {
                                return Ok(());
                            }
                            unsafe {
                                sl_mob_removehealth(ptr, damage, caster_id);
                            }
                            Ok(())
                        },
                    )?))
                }
                "move" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, _: mlua::MultiValue| {
                            if ptr.is_null() {
                                return Ok(false);
                            }
                            Ok(unsafe { move_mob(ptr as *mut MobSpawnData) } != 0)
                        },
                    )?))
                }
                "moveIgnoreObject" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, _: mlua::MultiValue| {
                            if ptr.is_null() {
                                return Ok(false);
                            }
                            Ok(unsafe { move_mob_ignore_object(ptr as *mut MobSpawnData) } != 0)
                        },
                    )?))
                }
                "moveGhost" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, _: mlua::MultiValue| {
                            if ptr.is_null() {
                                return Ok(0i32);
                            }
                            Ok(unsafe { moveghost_mob(ptr as *mut MobSpawnData) })
                        },
                    )?))
                }
                "moveIntent" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (_, target_id): (mlua::Value, c_uint)| {
                            if ptr.is_null() {
                                return Ok(0i32);
                            }
                            let bl = unsafe { map_id2bl_mob(target_id) };
                            if bl.is_null() {
                                return Ok(0i32);
                            }
                            Ok(unsafe { move_mob_intent(ptr as *mut MobSpawnData, bl) })
                        },
                    )?))
                }
                "warp" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (_, m, x, y): (mlua::Value, c_int, c_int, c_int)| {
                            if ptr.is_null() {
                                return Ok(());
                            }
                            unsafe {
                                mob_warp(ptr as *mut MobSpawnData, m, x, y);
                            }
                            Ok(())
                        },
                    )?))
                }
                "sendHealth" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (_, dmg, critical): (mlua::Value, c_float, c_int)| {
                            if ptr.is_null() {
                                return Ok(());
                            }
                            let damage = if dmg > 0.0 {
                                (dmg + 0.5) as c_int
                            } else if dmg < 0.0 {
                                (dmg - 0.5) as c_int
                            } else {
                                0
                            };
                            let crit = match critical {
                                1 => 33,
                                2 => 255,
                                c => c,
                            };
                            unsafe {
                                clif_send_mob_healthscript(ptr, damage, crit);
                            }
                            Ok(())
                        },
                    )?))
                }
                "setDuration" => {
                    return Ok(mlua::Value::Function(
                        lua.create_function(
                            move |_,
                                  (_, name, time, caster_id, recast): (
                                mlua::Value,
                                String,
                                c_int,
                                c_uint,
                                c_int,
                            )| {
                                if ptr.is_null() {
                                    return Ok(());
                                }
                                let cs =
                                    CString::new(name.as_bytes()).map_err(mlua::Error::external)?;
                                unsafe {
                                    sl_mob_setduration(ptr, cs.as_ptr(), time, caster_id, recast);
                                }
                                Ok(())
                            },
                        )?,
                    ))
                }
                "flushDuration" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (_, dis, minid, maxid): (mlua::Value, c_int, c_int, c_int)| {
                            if ptr.is_null() {
                                return Ok(());
                            }
                            unsafe {
                                sl_mob_flushduration(ptr, dis, minid, maxid);
                            }
                            Ok(())
                        },
                    )?))
                }
                "flushDurationNoUncast" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (_, dis, minid, maxid): (mlua::Value, c_int, c_int, c_int)| {
                            if ptr.is_null() {
                                return Ok(());
                            }
                            unsafe {
                                sl_mob_flushdurationnouncast(ptr, dis, minid, maxid);
                            }
                            Ok(())
                        },
                    )?))
                }
                "hasDuration" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (_, name): (mlua::Value, String)| -> mlua::Result<bool> {
                            if ptr.is_null() {
                                return Ok(false);
                            }
                            let cs =
                                CString::new(name.as_bytes()).map_err(mlua::Error::external)?;
                            let id = unsafe { rust_magicdb_id(cs.as_ptr()) };
                            let mob = unsafe { &*(ptr as *const MobSpawnData) };
                            Ok((0..MAX_MAGIC_TIMERS)
                                .any(|x| mob.da[x].id as c_int == id && mob.da[x].duration > 0))
                        },
                    )?))
                }
                "hasDurationID" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (_, name, caster_id): (mlua::Value, String, c_uint)| -> mlua::Result<bool> {
                            if ptr.is_null() {
                                return Ok(false);
                            }
                            let cs =
                                CString::new(name.as_bytes()).map_err(mlua::Error::external)?;
                            let id = unsafe { rust_magicdb_id(cs.as_ptr()) };
                            let mob = unsafe { &*(ptr as *const MobSpawnData) };
                            Ok((0..MAX_MAGIC_TIMERS).any(|x| {
                                mob.da[x].id as c_int == id
                                    && mob.da[x].caster_id == caster_id
                                    && mob.da[x].duration > 0
                            }))
                        },
                    )?))
                }
                "getDuration" | "durationAmount" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (_, name): (mlua::Value, String)| -> mlua::Result<c_int> {
                            if ptr.is_null() {
                                return Ok(0);
                            }
                            let cs =
                                CString::new(name.as_bytes()).map_err(mlua::Error::external)?;
                            let id = unsafe { rust_magicdb_id(cs.as_ptr()) };
                            let mob = unsafe { &*(ptr as *const MobSpawnData) };
                            for x in 0..MAX_MAGIC_TIMERS {
                                if mob.da[x].id as c_int == id && mob.da[x].duration > 0 {
                                    return Ok(mob.da[x].duration);
                                }
                            }
                            Ok(0)
                        },
                    )?))
                }
                "getDurationID" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (_, name, caster_id): (mlua::Value, String, c_uint)| -> mlua::Result<c_int> {
                            if ptr.is_null() {
                                return Ok(0);
                            }
                            let cs =
                                CString::new(name.as_bytes()).map_err(mlua::Error::external)?;
                            let id = unsafe { rust_magicdb_id(cs.as_ptr()) };
                            let mob = unsafe { &*(ptr as *const MobSpawnData) };
                            for x in 0..MAX_MAGIC_TIMERS {
                                if mob.da[x].id as c_int == id
                                    && mob.da[x].caster_id == caster_id
                                    && mob.da[x].duration > 0
                                {
                                    return Ok(mob.da[x].duration);
                                }
                            }
                            Ok(0)
                        },
                    )?))
                }
                "checkThreat" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (_, player_id): (mlua::Value, c_uint)| {
                            if ptr.is_null() {
                                return Ok(0i32);
                            }
                            Ok(unsafe { sl_mob_checkthreat(ptr, player_id) })
                        },
                    )?))
                }
                "callBase" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (_, script): (mlua::Value, String)| -> mlua::Result<bool> {
                            if ptr.is_null() {
                                return Ok(false);
                            }
                            let cs =
                                CString::new(script.as_bytes()).map_err(mlua::Error::external)?;
                            Ok(unsafe { sl_mob_callbase(ptr, cs.as_ptr()) } != 0)
                        },
                    )?))
                }
                "checkMove" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, _: mlua::MultiValue| {
                            if ptr.is_null() {
                                return Ok(false);
                            }
                            Ok(unsafe { sl_mob_checkmove(ptr) } != 0)
                        },
                    )?))
                }
                "setIndDmg" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (_, player_id, dmg): (mlua::Value, c_uint, c_float)| -> mlua::Result<bool> {
                            if ptr.is_null() {
                                return Ok(false);
                            }
                            Ok(unsafe { sl_mob_setinddmg(ptr, player_id, dmg) } != 0)
                        },
                    )?))
                }
                "setGrpDmg" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (_, player_id, dmg): (mlua::Value, c_uint, c_float)| -> mlua::Result<bool> {
                            if ptr.is_null() {
                                return Ok(false);
                            }
                            Ok(unsafe { sl_mob_setgrpdmg(ptr, player_id, dmg) } != 0)
                        },
                    )?))
                }
                "getIndDmg" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |lua, _: mlua::MultiValue| -> mlua::Result<mlua::Value> {
                            let tbl = lua.create_table()?;
                            if ptr.is_null() {
                                return Ok(mlua::Value::Table(tbl));
                            }
                            let mob = unsafe { &*(ptr as *const MobSpawnData) };
                            let mut y = 1i64;
                            for x in 0..MAX_THREATCOUNT {
                                if mob.dmgindtable[x][0] > 0.0 {
                                    tbl.raw_set(y, mob.dmgindtable[x][0])?;
                                    y += 1;
                                    tbl.raw_set(y, mob.dmgindtable[x][1])?;
                                    y += 1;
                                }
                            }
                            Ok(mlua::Value::Table(tbl))
                        },
                    )?))
                }
                "getGrpDmg" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |lua, _: mlua::MultiValue| -> mlua::Result<mlua::Value> {
                            let tbl = lua.create_table()?;
                            if ptr.is_null() {
                                return Ok(mlua::Value::Table(tbl));
                            }
                            let mob = unsafe { &*(ptr as *const MobSpawnData) };
                            let mut y = 1i64;
                            for x in 0..MAX_THREATCOUNT {
                                if mob.dmggrptable[x][0] > 0.0 {
                                    tbl.raw_set(y, mob.dmggrptable[x][0])?;
                                    y += 1;
                                    tbl.raw_set(y, mob.dmggrptable[x][1])?;
                                    y += 1;
                                }
                            }
                            Ok(mlua::Value::Table(tbl))
                        },
                    )?))
                }
                "getEquippedItem" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |lua, (_, num): (mlua::Value, usize)| -> mlua::Result<mlua::Value> {
                            if ptr.is_null() {
                                return Ok(mlua::Value::Nil);
                            }
                            let mob = unsafe { &*(ptr as *const MobSpawnData) };
                            if mob.data.is_null() || num >= 15 {
                                return Ok(mlua::Value::Nil);
                            }
                            let item = unsafe { &(*mob.data).equip[num] };
                            if item.id == 0 {
                                return Ok(mlua::Value::Nil);
                            }
                            let t = lua.create_table()?;
                            t.raw_set(1, item.id)?;
                            t.raw_set(2, item.custom)?;
                            Ok(mlua::Value::Table(t))
                        },
                    )?))
                }
                "calcStat" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, _: mlua::MultiValue| {
                            if ptr.is_null() {
                                return Ok(());
                            }
                            unsafe {
                                mob_calcstat(ptr as *mut MobSpawnData);
                            }
                            Ok(())
                        },
                    )?))
                }
                "sendStatus" => {
                    return Ok(mlua::Value::Function(
                        lua.create_function(move |_, _: mlua::MultiValue| Ok(()))?,
                    ))
                }
                "sendMinitext" => {
                    return Ok(mlua::Value::Function(
                        lua.create_function(move |_, _: mlua::MultiValue| Ok(()))?,
                    ))
                }
                // sendSide() — send a side-update to nearby players.
                "sendSide" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, _: mlua::MultiValue| {
                            unsafe { sffi::sl_g_sendside(ptr); }
                            Ok(())
                        }
                    )?));
                }
                // delete() — remove mob from world and free its memory.
                "delete" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, _: mlua::MultiValue| {
                            unsafe { sffi::sl_g_delete_bl(ptr); }
                            Ok(())
                        }
                    )?));
                }
                // talk(type, msg) — speak in the surrounding area.
                "talk" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, args: mlua::MultiValue| {
                            let a: Vec<mlua::Value> = args.into_iter().collect();
                            let talk_type = a.get(1).map(|v| val_to_int(v)).unwrap_or(0);
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
                // Registry sub-objects (set during mobl_init; exposed via __index here)
                "registry" => return lua.pack(MobRegObject { ptr }),
                "mapRegistry" => return lua.pack(MapRegObject { ptr }),
                "gameRegistry" => {
                    return lua.pack(GameRegObject {
                        ptr: std::ptr::null_mut(),
                    })
                }
                _ => {}
            }

            // ── block_list / map fields ──────────────────────────────────────
            // Shared map properties (pvp, mapTitle, bgm, etc.) — delegate to shared module.
            if let Some(v) = unsafe { shared::map_field(lua, bl.m as c_int, key.as_str()) } {
                return v;
            }
            // Shared GfxViewer properties (gfxFace, gfxWeap, etc.) — delegate to shared module.
            if let Some(v) = unsafe { shared::gfx_read(lua, &mob.gfx, key.as_str()) } {
                return v;
            }

            match key.as_str() {
                "x" => int!(bl.x),
                "y" => int!(bl.y),
                "m" => int!(bl.m),
                "blType" => int!(bl.bl_type),
                "ID" => int!(bl.id),
                "xmax" => {
                    let mp = unsafe { mob_map(mob as *const MobSpawnData) };
                    if mp.is_null() { return Ok(mlua::Value::Nil); }
                    int!(unsafe { (*mp).xs.saturating_sub(1) })
                }
                "ymax" => {
                    let mp = unsafe { mob_map(mob as *const MobSpawnData) };
                    if mp.is_null() { return Ok(mlua::Value::Nil); }
                    int!(unsafe { (*mp).ys.saturating_sub(1) })
                }
                // ── mob-instance fields ──────────────────────────────────────
                "state" => int!(mob.state),
                "startX" => int!(mob.startx),
                "startY" => int!(mob.starty),
                "startM" => int!(mob.startm),
                "mobID" => int!(mob.mobid),
                "id" => int!(mob.id),
                "side" => int!(mob.side),
                "amnesia" => int!(mob.amnesia),
                "paralyzed" => bool!(mob.paralyzed),
                "blind" => bool!(mob.blind),
                "hit" => int!(mob.hit),
                "miss" => int!(mob.miss),
                "minDam" => int!(mob.mindam),
                "maxDam" => int!(mob.maxdam),
                "might" => int!(mob.might),
                "grace" => int!(mob.grace),
                "will" => int!(mob.will),
                "health" => int!(mob.current_vita),
                "maxHealth" => int!(mob.maxvita),
                "lastHealth" => int!(mob.lastvita),
                "magic" => int!(mob.current_mana),
                "maxMagic" => int!(mob.maxmana),
                "armor" => int!(mob.ac),
                "attacker" => int!(mob.attacker),
                "confused" => bool!(mob.confused),
                "owner" => int!(mob.owner),
                "sleep" => Ok(mlua::Value::Number(mob.sleep as f64)),
                "target" => int!(mob.target),
                "confuseTarget" => int!(mob.confused_target),
                "deduction" => Ok(mlua::Value::Number(mob.deduction as f64)),
                "damage" => Ok(mlua::Value::Number(mob.damage as f64)),
                "crit" => int!(mob.crit),
                "critChance" => int!(mob.critchance),
                "critMult" => int!(mob.critmult),
                "rangeTarget" => int!(mob.rangeTarget),
                "newMove" => int!(mob.newmove),
                "newAttack" => int!(mob.newatk),
                "snare" => bool!(mob.snare),
                "lastAction" => int!(mob.lastaction),
                "summon" => bool!(mob.summon),
                "block" => int!(mob.block),
                "protection" => int!(mob.protection),
                "returning" => bool!(mob.returning),
                "dmgShield" => Ok(mlua::Value::Number(mob.dmgshield as f64)),
                "dmgDealt" => Ok(mlua::Value::Number(mob.dmgdealt)),
                "dmgTaken" => Ok(mlua::Value::Number(mob.dmgtaken)),
                "look" => int!(mob.look),
                "lookColor" => int!(mob.look_color),
                "charState" => int!(mob.charstate),
                "invis" => Ok(mlua::Value::Number(mob.invis as f64)),
                "gfxClone" => int!(mob.clone),
                "lastDeath" => int!(mob.last_death),
                "cursed" => int!(mob.cursed),
                // ── mob-data (template) fields ───────────────────────────────
                "behavior" => data_int!(r#type),
                "aiType" => data_int!(subtype),
                "mobType" => data_int!(mobtype),
                "name" => data_cstr!(name),
                "yname" => data_cstr!(yname),
                "experience" => int!(mob.exp),
                "level" => data_int!(level),
                "tier" => data_int!(tier),
                "mark" => data_int!(mark),
                "baseHit" => data_int!(hit),
                "baseMiss" => data_int!(miss),
                "baseMinDam" => data_int!(mindam),
                "baseMaxDam" => data_int!(maxdam),
                "baseMight" => data_int!(might),
                "baseGrace" => data_int!(grace),
                "baseWill" => data_int!(will),
                "baseHealth" => data_int!(vita),
                "baseMagic" => data_int!(mana),
                "baseArmor" => data_int!(baseac),
                "sound" => data_int!(sound),
                "baseMove" => data_int!(movetime),
                "baseAttack" => data_int!(atktime),
                "spawnTime" => data_int!(spawntime),
                "baseBlock" => data_int!(block),
                "baseProtection" => data_int!(protection),
                "retDist" => data_int!(retdist),
                "race" => data_int!(race),
                "seeInvis" => data_int!(seeinvis),
                "isBoss" => data_int!(isboss),
                "getBlock" =>
                    return shared::make_getblock_fn(lua),
                "getObjectsInCell" | "getAliveObjectsInCell" | "getObjectsInCellWithTraps" =>
                    return shared::make_cell_query_fn(lua, key.as_str()),
                "getObjectsInArea" | "getAliveObjectsInArea"
                | "getObjectsInSameMap" | "getAliveObjectsInSameMap" =>
                    return shared::make_area_query_fn(lua, key.as_str(), ptr),
                "getObjectsInMap" =>
                    return shared::make_map_query_fn(lua),
                _ => {
                    if let Ok(tbl) = lua.globals().get::<mlua::Table>("Mob") {
                        if let Ok(v) = tbl.get::<mlua::Value>(key.as_str()) {
                            if !matches!(v, mlua::Value::Nil) {
                                return Ok(v);
                            }
                        }
                    }
                    tracing::debug!("[scripting] MobObject: unimplemented __index key={key:?}");
                    Ok(mlua::Value::Nil)
                }
            }
        });

        // ── __newindex ───────────────────────────────────────────────────────
        methods.add_meta_method(
            MetaMethod::NewIndex,
            |_, this, (key, val): (String, mlua::Value)| {
                if this.ptr.is_null() {
                    return Ok(());
                }
                let mob = unsafe { &mut *(this.ptr as *mut MobSpawnData) };
                let mp = unsafe { mob_map(mob as *const MobSpawnData) };

                macro_rules! map_set {
                    ($field:ident) => {
                        if !mp.is_null() {
                            unsafe {
                                (*mp).$field = val_to_int(&val) as _;
                            }
                        }
                    };
                }

                match key.as_str() {
                    // map writable fields
                    "bgm" => map_set!(bgm),
                    "bgmType" => map_set!(bgmtype),
                    "pvp" => map_set!(pvp),
                    "spell" => map_set!(spell),
                    "light" => map_set!(light),
                    "weather" => map_set!(weather),
                    "sweepTime" => map_set!(sweeptime),
                    "canTalk" => map_set!(cantalk),
                    "showGhosts" => map_set!(show_ghosts),
                    "region" => map_set!(region),
                    "indoor" => map_set!(indoor),
                    "warpOut" => map_set!(warpout),
                    "bind" => map_set!(bind),
                    "reqLvl" => map_set!(reqlvl),
                    "reqVita" => map_set!(reqvita),
                    "reqMana" => map_set!(reqmana),
                    "reqPath" => map_set!(reqpath),
                    "reqMark" => map_set!(reqmark),
                    "maxLvl" => map_set!(lvlmax),
                    "maxVita" => map_set!(vitamax),
                    "maxMana" => map_set!(manamax),
                    "canSummon" => map_set!(summon),
                    "canUse" => map_set!(can_use),
                    "canEat" => map_set!(can_eat),
                    "canSmoke" => map_set!(can_smoke),
                    "canMount" => map_set!(can_mount),
                    "canGroup" => map_set!(can_group),
                    "health" => {
                        mob.current_vita = val_to_int(&val) as _;
                    }
                    "maxHealth" => {
                        mob.maxvita = val_to_int(&val) as _;
                    }
                    "magic" => {
                        mob.current_mana = val_to_int(&val) as _;
                    }
                    "maxMagic" => {
                        mob.maxmana = val_to_int(&val) as _;
                    }
                    "side"          => { mob.side            = val_to_int(&val) as _; }
                    // combat stats
                    "time"          => { mob.time_           = val_to_int(&val) as _; }
                    "amnesia"       => { mob.amnesia         = val_to_int(&val) as _; }
                    "paralyzed"     => { mob.paralyzed       = val_to_int(&val) as _; }
                    "blind"         => { mob.blind           = val_to_int(&val) as _; }
                    "hit"           => { mob.hit             = val_to_int(&val) as _; }
                    "miss"          => { mob.miss            = val_to_int(&val) as _; }
                    "minDam"        => { mob.mindam          = val_to_int(&val) as _; }
                    "maxDam"        => { mob.maxdam          = val_to_int(&val) as _; }
                    "might"         => { mob.might           = val_to_int(&val) as _; }
                    "grace"         => { mob.grace           = val_to_int(&val) as _; }
                    "will"          => { mob.will            = val_to_int(&val) as _; }
                    "armor"         => { mob.ac              = val_to_int(&val) as _; }
                    "attacker"      => { mob.attacker        = val_to_int(&val) as _; }
                    "confused"      => { mob.confused        = val_to_int(&val) as _; }
                    "owner"         => { mob.owner           = val_to_int(&val) as _; }
                    "experience"    => { mob.exp             = val_to_int(&val) as _; }
                    "sleep"         => { mob.sleep           = val_to_int(&val) as _; }
                    "target"        => { mob.target          = val_to_int(&val) as _; }
                    "confusedTarget"=> { mob.confused_target = val_to_int(&val) as _; }
                    "deduction"     => { mob.deduction       = val_to_int(&val) as _; }
                    "state"         => { mob.state           = val_to_int(&val) as _; }
                    "rangeTarget"   => { mob.rangeTarget     = val_to_int(&val) as _; }
                    "newMove"       => { mob.newmove         = val_to_int(&val) as _; }
                    "newAttack"     => { mob.newatk          = val_to_int(&val) as _; }
                    "snare"         => { mob.snare           = val_to_int(&val) as _; }
                    "lastAction"    => { mob.lastaction      = val_to_int(&val) as _; }
                    "crit"          => { mob.crit            = val_to_int(&val) as _; }
                    "critChance"    => { mob.critchance      = val_to_int(&val) as _; }
                    "critMult"      => { mob.critmult        = val_to_int(&val) as _; }
                    "damage"        => { mob.damage          = val_to_int(&val) as _; }
                    "summon"        => { mob.summon          = val_to_int(&val) as _; }
                    "block"         => { mob.block           = val_to_int(&val) as _; }
                    "protection"    => { mob.protection      = val_to_int(&val) as _; }
                    "returning"     => { mob.returning       = val_to_int(&val) as _; }
                    "dmgShield"     => { mob.dmgshield       = val_to_int(&val) as _; }
                    "dmgDealt"      => { mob.dmgdealt        = val_to_int(&val) as _; }
                    "dmgTaken"      => { mob.dmgtaken        = val_to_int(&val) as _; }
                    "look"          => { mob.look            = val_to_int(&val) as _; }
                    "lookColor"     => { mob.look_color      = val_to_int(&val) as _; }
                    "charState"     => { mob.charstate       = val_to_int(&val) as _; }
                    "invis"         => { mob.invis           = val_to_int(&val) as _; }
                    "lastDeath"     => { mob.last_death      = val_to_int(&val) as _; }
                    "cursed"        => { mob.cursed          = val_to_int(&val) as _; }
                    "gfxClone"      => { mob.clone           = val_to_int(&val) as _; }
                    // mob.data fields
                    "baseMagic"     => if !mob.data.is_null() { unsafe { (*mob.data).mana   = val_to_int(&val) as _; } }
                    "isBoss"        => if !mob.data.is_null() { unsafe { (*mob.data).isboss = val_to_int(&val) as _; } }
                    // GfxViewer fields — delegated to shared module.
                    key if key.starts_with("gfx") && key != "gfxClone" => {
                        let str_owned = if let mlua::Value::String(ref s) = val {
                            s.to_str().ok().map(|x| x.to_string())
                        } else { None };
                        unsafe { shared::gfx_write(&mut mob.gfx, key, val_to_int(&val), str_owned.as_deref()); }
                    }
                    _ => {
                        tracing::debug!("[scripting] MobObject: unimplemented __newindex key={key:?}");
                    }
                }
                Ok(())
            },
        );
    }
}
