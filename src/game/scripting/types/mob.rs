use mlua::{MetaMethod, UserData, UserDataMethods};
use std::ffi::{c_char, c_float, c_int, c_uint, CString};
use std::os::raw::c_void;

use crate::database::map_db::{BlockList, MapData};
use crate::database::mob_db::MobDbData;
use crate::ffi::map_db::get_map_ptr;
use crate::game::mob::{
    mob_calcstat, mob_warp, move_mob, move_mob_ignore_object, move_mob_intent, moveghost_mob,
    MobSpawnData, BL_MOB, BL_PC, MAX_MAGIC_TIMERS, MAX_THREATCOUNT,
};
use crate::game::scripting::types::item::fixed_str;
use crate::game::scripting::types::registry::{GameRegObject, MapRegObject, MobRegObject};

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
                        move |_, id: c_int| {
                            if ptr.is_null() {
                                return Ok(0i32);
                            }
                            Ok(unsafe { mob_attack(ptr as *mut MobSpawnData, id) })
                        },
                    )?))
                }
                "addHealth" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, damage: c_int| {
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
                        move |_, (damage, caster_id): (c_int, c_uint)| {
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
                        move |_, target_id: c_uint| {
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
                        move |_, (m, x, y): (c_int, c_int, c_int)| {
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
                        move |_, (dmg, critical): (c_float, c_int)| {
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
                                  (name, time, caster_id, recast): (
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
                        move |_, (dis, minid, maxid): (c_int, c_int, c_int)| {
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
                        move |_, (dis, minid, maxid): (c_int, c_int, c_int)| {
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
                        move |_, name: String| -> mlua::Result<bool> {
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
                        move |_, (name, caster_id): (String, c_uint)| -> mlua::Result<bool> {
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
                        move |_, name: String| -> mlua::Result<c_int> {
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
                        move |_, (name, caster_id): (String, c_uint)| -> mlua::Result<c_int> {
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
                        move |_, player_id: c_uint| {
                            if ptr.is_null() {
                                return Ok(0i32);
                            }
                            Ok(unsafe { sl_mob_checkthreat(ptr, player_id) })
                        },
                    )?))
                }
                "callBase" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, script: String| -> mlua::Result<bool> {
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
                        move |_, (player_id, dmg): (c_uint, c_float)| -> mlua::Result<bool> {
                            if ptr.is_null() {
                                return Ok(false);
                            }
                            Ok(unsafe { sl_mob_setinddmg(ptr, player_id, dmg) } != 0)
                        },
                    )?))
                }
                "setGrpDmg" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (player_id, dmg): (c_uint, c_float)| -> mlua::Result<bool> {
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
                        move |lua, num: usize| -> mlua::Result<mlua::Value> {
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
            match key.as_str() {
                "x" => int!(bl.x),
                "y" => int!(bl.y),
                "m" => int!(bl.m),
                "blType" => int!(bl.bl_type),
                "ID" => int!(bl.id),
                "xmax" => {
                    let mp = unsafe { mob_map(mob as *const MobSpawnData) };
                    if mp.is_null() {
                        return Ok(mlua::Value::Nil);
                    }
                    int!(unsafe { (*mp).xs.saturating_sub(1) })
                }
                "ymax" => {
                    let mp = unsafe { mob_map(mob as *const MobSpawnData) };
                    if mp.is_null() {
                        return Ok(mlua::Value::Nil);
                    }
                    int!(unsafe { (*mp).ys.saturating_sub(1) })
                }
                "mapId" => map_int!(id),
                "mapTitle" => {
                    let mp = unsafe { mob_map(mob as *const MobSpawnData) };
                    if mp.is_null() {
                        return Ok(mlua::Value::Nil);
                    }
                    cstr!(unsafe { &(*mp).title })
                }
                "mapFile" => {
                    let mp = unsafe { mob_map(mob as *const MobSpawnData) };
                    if mp.is_null() {
                        return Ok(mlua::Value::Nil);
                    }
                    cstr!(unsafe { &(*mp).mapfile })
                }
                "bgm" => map_int!(bgm),
                "bgmType" => map_int!(bgmtype),
                "pvp" => map_int!(pvp),
                "spell" => map_int!(spell),
                "light" => map_int!(light),
                "weather" => map_int!(weather),
                "sweepTime" => map_int!(sweeptime),
                "canTalk" => map_int!(cantalk),
                "showGhosts" => map_int!(show_ghosts),
                "region" => map_int!(region),
                "indoor" => map_int!(indoor),
                "warpOut" => map_int!(warpout),
                "bind" => map_int!(bind),
                "reqLvl" => map_int!(reqlvl),
                "reqVita" => map_int!(reqvita),
                "reqMana" => map_int!(reqmana),
                "maxLvl" => map_int!(lvlmax),
                "maxVita" => map_int!(vitamax),
                "maxMana" => map_int!(manamax),
                "reqPath" => map_int!(reqpath),
                "reqMark" => map_int!(reqmark),
                "canSummon" => map_int!(summon),
                "canUse" => map_int!(can_use),
                "canEat" => map_int!(can_eat),
                "canSmoke" => map_int!(can_smoke),
                "canMount" => map_int!(can_mount),
                "canGroup" => map_int!(can_group),
                // ── mob-instance fields ──────────────────────────────────────
                "state" => int!(mob.state),
                "startX" => int!(mob.startx),
                "startY" => int!(mob.starty),
                "startM" => int!(mob.startm),
                "mobID" => int!(mob.mobid),
                "id" => int!(mob.id),
                "side" => int!(mob.side),
                "amnesia" => int!(mob.amnesia),
                "paralyzed" => int!(mob.paralyzed),
                "blind" => int!(mob.blind),
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
                "confused" => int!(mob.confused),
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
                "snare" => int!(mob.snare),
                "lastAction" => int!(mob.lastaction),
                "summon" => int!(mob.summon),
                "block" => int!(mob.block),
                "protection" => int!(mob.protection),
                "returning" => int!(mob.returning),
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
                // gfxViewer fields
                "gfxFace" => int!(mob.gfx.face),
                "gfxFaceC" => int!(mob.gfx.cface),
                "gfxHair" => int!(mob.gfx.hair),
                "gfxHairC" => int!(mob.gfx.chair),
                "gfxSkinC" => int!(mob.gfx.cskin),
                "gfxDye" => int!(mob.gfx.dye),
                "gfxTitleColor" => int!(mob.gfx.title_color),
                "gfxWeap" => int!(mob.gfx.weapon),
                "gfxWeapC" => int!(mob.gfx.cweapon),
                "gfxArmor" => int!(mob.gfx.armor),
                "gfxArmorC" => int!(mob.gfx.carmor),
                "gfxShield" => int!(mob.gfx.shield),
                "gfxShieldC" | "gfxShiedlC" => int!(mob.gfx.cshield),
                "gfxHelm" => int!(mob.gfx.helm),
                "gfxHelmC" => int!(mob.gfx.chelm),
                "gfxMantle" => int!(mob.gfx.mantle),
                "gfxMantleC" => int!(mob.gfx.cmantle),
                "gfxCrown" => int!(mob.gfx.crown),
                "gfxCrownC" => int!(mob.gfx.ccrown),
                "gfxFaceA" => int!(mob.gfx.face_acc),
                "gfxFaceAC" => int!(mob.gfx.cface_acc),
                "gfxFaceAT" => int!(mob.gfx.face_acc_t),
                "gfxFaceATC" => int!(mob.gfx.cface_acc_t),
                "gfxBoots" => int!(mob.gfx.boots),
                "gfxBootsC" => int!(mob.gfx.cboots),
                "gfxNeck" => int!(mob.gfx.necklace),
                "gfxNeckC" => int!(mob.gfx.cnecklace),
                "gfxName" => cstr!(&mob.gfx.name),
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
                _ => Ok(mlua::Value::Nil),
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
                    // gfx appearance
                    "gfxFace"       => { mob.gfx.face        = val_to_int(&val) as _; }
                    "gfxHair"       => { mob.gfx.hair        = val_to_int(&val) as _; }
                    "gfxHairC"      => { mob.gfx.chair       = val_to_int(&val) as _; }
                    "gfxFaceC"      => { mob.gfx.cface       = val_to_int(&val) as _; }
                    "gfxSkinC"      => { mob.gfx.cskin       = val_to_int(&val) as _; }
                    "gfxDye"        => { mob.gfx.dye         = val_to_int(&val) as _; }
                    "gfxTitleColor" => { mob.gfx.title_color = val_to_int(&val) as _; }
                    "gfxWeap"       => { mob.gfx.weapon      = val_to_int(&val) as _; }
                    "gfxWeapC"      => { mob.gfx.cweapon     = val_to_int(&val) as _; }
                    "gfxArmor"      => { mob.gfx.armor       = val_to_int(&val) as _; }
                    "gfxArmorC"     => { mob.gfx.carmor      = val_to_int(&val) as _; }
                    "gfxShield"     => { mob.gfx.shield      = val_to_int(&val) as _; }
                    "gfxShieldC"    => { mob.gfx.cshield     = val_to_int(&val) as _; }
                    "gfxHelm"       => { mob.gfx.helm        = val_to_int(&val) as _; }
                    "gfxHelmC"      => { mob.gfx.chelm       = val_to_int(&val) as _; }
                    "gfxMantle"     => { mob.gfx.mantle      = val_to_int(&val) as _; }
                    "gfxMantleC"    => { mob.gfx.cmantle     = val_to_int(&val) as _; }
                    "gfxCrown"      => { mob.gfx.crown       = val_to_int(&val) as _; }
                    "gfxCrownC"     => { mob.gfx.ccrown      = val_to_int(&val) as _; }
                    "gfxFaceA"      => { mob.gfx.face_acc    = val_to_int(&val) as _; }
                    "gfxFaceAC"     => { mob.gfx.cface_acc   = val_to_int(&val) as _; }
                    "gfxFaceAT"     => { mob.gfx.face_acc_t  = val_to_int(&val) as _; }
                    "gfxFaceATC"    => { mob.gfx.cface_acc_t = val_to_int(&val) as _; }
                    "gfxBoots"      => { mob.gfx.boots       = val_to_int(&val) as _; }
                    "gfxBootsC"     => { mob.gfx.cboots      = val_to_int(&val) as _; }
                    "gfxNeck"       => { mob.gfx.necklace    = val_to_int(&val) as _; }
                    "gfxNeckC"      => { mob.gfx.cnecklace   = val_to_int(&val) as _; }
                    "gfxName"       => if let mlua::Value::String(ref s) = val {
                        let bytes = s.as_bytes();
                        let n = bytes.len().min(33);
                        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, mob.gfx.name.as_mut_ptr(), n); }
                        mob.gfx.name[n] = 0;
                    }
                    _ => {}
                }
                Ok(())
            },
        );
    }
}
