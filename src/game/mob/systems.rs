//! Mob system functions: timers, AI state machine, stat calc, registries.

#![allow(non_snake_case, dead_code)]

use std::sync::atomic::Ordering;

use super::entity::{
    broadcast_animation_to_pcs, free_onetime, magicdb_yname_str, sl_doscript_2,
    sl_doscript_simple, MobSpawnData, MOB_ONETIME_MAX, MOB_ONETIME_START, MOB_SPAWN_MAX,
    MOB_SPAWN_START, TIMERCHECK,
};
use crate::common::constants::entity::mob::{
    MAX_GLOBALMOBREG, MAX_MAGIC_TIMERS, MOB_ALIVE, MOB_DEAD, MOB_ESCAPE, MOB_HIT,
};
use crate::common::constants::entity::SUBTYPE_FLOOR;
use crate::common::traits::LegacyEntity;
use crate::common::constants::entity::{BL_MOB, BL_PC};
use crate::database::map_db::get_map_ptr as ffi_get_map_ptr;
use crate::database::map_db::map_is_loaded as ffi_map_is_loaded;
use crate::database::mob_db;
use crate::game::block::map_delblock_id;
use crate::game::block::AreaType;
use crate::game::block_grid;
use crate::game::map_server::{map_deliddb, map_id2mob_ref, CURRENT_TIME};
use crate::game::pc::MapSessionData;
use std::sync::Arc;

// ---- Spawn window check -------------------------------------------------------

unsafe fn in_spawn_window(mob: *const MobSpawnData) -> bool {
    let s = (*mob).start as i32;
    let e = (*mob).end as i32;
    let ct = CURRENT_TIME.load(Ordering::Relaxed);
    (s < e && ct >= s && ct <= e)
        || (s > e && (ct >= s || ct <= e))
        || (s == e && ct == s && ct == e)
        || (s == 25 && e == 25)
}

// ---- Stat / respawn functions --------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_respawn_getstats(mob: *mut MobSpawnData) -> i32 {
    if mob.is_null() {
        return 0;
    }
    (*mob).data = if in_spawn_window(mob) {
        Arc::as_ptr(&mob_db::search((*mob).mobid)) as *mut _
    } else if (*mob).replace != 0 {
        Arc::as_ptr(&mob_db::search((*mob).replace)) as *mut _
    } else {
        Arc::as_ptr(&mob_db::search((*mob).mobid)) as *mut _
    };
    if (*mob).data.is_null() {
        return 0;
    }
    let d = &*(*mob).data;
    (*mob).maxvita = d.vita as u32;
    (*mob).maxmana = d.mana as u32;
    (*mob).ac = d.baseac;
    if (*mob).ac < -95 {
        (*mob).ac = -95;
    }
    if (*mob).exp == 0 {
        (*mob).exp = mob_db::experience((*mob).mobid);
    }
    (*mob).miss = d.miss;
    (*mob).newmove = d.movetime as u32;
    (*mob).newatk = d.atktime as u32;
    (*mob).current_vita = (*mob).maxvita;
    (*mob).current_mana = (*mob).maxmana;
    (*mob).maxdmg = (*mob).current_vita as f64;
    (*mob).hit = d.hit;
    (*mob).mindam = d.mindam;
    (*mob).maxdam = d.maxdam;
    (*mob).might = d.might;
    (*mob).grace = d.grace;
    (*mob).will = d.will;
    (*mob).block = d.block;
    (*mob).protection = d.protection;
    (*mob).look = d.look as u16;
    (*mob).look_color = d.look_color as u8;
    (*mob).charstate = d.state;
    (*mob).clone = 0;
    (*mob).time_ = 0;
    (*mob).paralyzed = 0;
    (*mob).blind = 0;
    (*mob).confused = 0;
    (*mob).snare = 0;
    (*mob).target = 0;
    (*mob).attacker = 0;
    (*mob).confused_target = 0;
    (*mob).rangeTarget = 0;
    (*mob).dmgshield = 0.0;
    (*mob).sleep = 1.0;
    (*mob).deduction = 1.0;
    (*mob).damage = 0.0;
    (*mob).critchance = 0;
    (*mob).crit = 0;
    (*mob).critmult = 0;
    (*mob).invis = 1.0;
    0
}

// ---- Magic timer functions -----------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_duratimer(mob: *mut MobSpawnData) -> i32 {
    if mob.is_null() {
        return 0;
    }
    for x in 0..MAX_MAGIC_TIMERS {
        let id = (*mob).da[x].id as i32;
        if id <= 0 {
            continue;
        }

        let caster_id = (*mob).da[x].caster_id;
        // Resolve caster: check if it's a living mob or a PC.
        let caster_info = if caster_id > 0 {
            crate::game::map_server::entity_position(caster_id)
        } else {
            None
        };

        if (*mob).da[x].duration > 0 {
            (*mob).da[x].duration -= 1000;

            let yname = magicdb_yname_str(id);
            if let Some((_pos, bl_type)) = caster_info {
                let health: i64 = if bl_type == BL_MOB as u8 {
                    crate::game::map_server::map_id2mob_ref(caster_id)
                        .map(|arc| arc.read().current_vita as i64)
                        .unwrap_or(0)
                } else {
                    0
                };
                if health > 0 || bl_type == BL_PC as u8 {
                    sl_doscript_2(&yname, Some("while_cast"), (*mob).id, caster_id);
                }
            } else {
                sl_doscript_simple(&yname, Some("while_cast"), (*mob).id);
            }

            if (*mob).da[x].duration <= 0 {
                (*mob).da[x].duration = 0;
                (*mob).da[x].id = 0;
                (*mob).da[x].caster_id = 0;
                broadcast_animation_to_pcs(&*mob, (*mob).da[x].animation as i32);
                (*mob).da[x].animation = 0;
                if caster_info.is_some() {
                    sl_doscript_2(&yname, Some("uncast"), (*mob).id, caster_id);
                } else {
                    sl_doscript_simple(&yname, Some("uncast"), (*mob).id);
                }
            }
        }
    }
    0
}

/// Common body for the 250 / 500 / 1500 ms timers (no expire logic).
unsafe fn dura_tick(mob: *mut MobSpawnData, event: &str) {
    for x in 0..MAX_MAGIC_TIMERS {
        let id = (*mob).da[x].id as i32;
        if id <= 0 {
            continue;
        }
        let caster_id = (*mob).da[x].caster_id;
        let caster_info = if caster_id > 0 {
            crate::game::map_server::entity_position(caster_id)
        } else {
            None
        };
        if (*mob).da[x].duration > 0 {
            let yname = magicdb_yname_str(id);
            if let Some((_pos, bl_type)) = caster_info {
                let health: i64 = if bl_type == BL_MOB as u8 {
                    crate::game::map_server::map_id2mob_ref(caster_id)
                        .map(|arc| arc.read().current_vita as i64)
                        .unwrap_or(0)
                } else {
                    0
                };
                if health > 0 || bl_type == BL_PC as u8 {
                    sl_doscript_2(&yname, Some(event), (*mob).id, caster_id);
                }
            } else {
                sl_doscript_simple(&yname, Some(event), (*mob).id);
            }
        }
    }
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_secondduratimer(mob: *mut MobSpawnData) -> i32 {
    if mob.is_null() {
        return 0;
    }
    dura_tick(mob, "while_cast_250");
    0
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_thirdduratimer(mob: *mut MobSpawnData) -> i32 {
    if mob.is_null() {
        return 0;
    }
    dura_tick(mob, "while_cast_500");
    0
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_fourthduratimer(mob: *mut MobSpawnData) -> i32 {
    if mob.is_null() {
        return 0;
    }
    dura_tick(mob, "while_cast_1500");
    0
}

// ---- Flush magic ---------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_flushmagic(mob: *mut MobSpawnData) -> i32 {
    for x in 0..MAX_MAGIC_TIMERS {
        let id = (*mob).da[x].id as i32;
        if id <= 0 {
            continue;
        }
        (*mob).da[x].duration = 0;
        (*mob).da[x].id = 0;
        (*mob).da[x].caster_id = 0;
        broadcast_animation_to_pcs(&*mob, (*mob).da[x].animation as i32);
        (*mob).da[x].animation = 0;
        // Note: caster_id is already 0 here (cleared above).
        // Porting C behavior faithfully (C bug: checks stale zeroed field).
        let cid = (*mob).da[x].caster_id;
        let has_caster =
            cid != (*mob).id && cid > 0 && crate::game::map_server::entity_position(cid).is_some();
        let yname = magicdb_yname_str(id);
        if has_caster {
            sl_doscript_2(&yname, Some("uncast"), (*mob).id, cid);
        } else {
            sl_doscript_simple(&yname, Some("uncast"), (*mob).id);
        }
    }
    0
}

// ---- Stat recalculation --------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_calcstat(mob: *mut MobSpawnData) -> i32 {
    if mob.is_null() || (*mob).data.is_null() {
        return 0;
    }
    let d = &*(*mob).data;
    (*mob).maxvita = d.vita as u32;
    (*mob).maxmana = d.mana as u32;
    (*mob).ac = d.baseac;
    if (*mob).ac < -95 {
        (*mob).ac = -95;
    }
    (*mob).miss = d.miss;
    (*mob).newmove = d.movetime as u32;
    (*mob).newatk = d.atktime as u32;
    (*mob).hit = d.hit;
    (*mob).mindam = d.mindam;
    (*mob).maxdam = d.maxdam;
    (*mob).might = d.might;
    (*mob).grace = d.grace;
    (*mob).will = d.will;
    (*mob).block = d.block;
    (*mob).protection = d.protection;
    (*mob).charstate = d.state;
    (*mob).clone = 0;
    (*mob).paralyzed = 0;
    (*mob).blind = 0;
    (*mob).confused = 0;
    (*mob).snare = 0;
    (*mob).sleep = 1.0;
    (*mob).deduction = 1.0;
    (*mob).crit = 0;
    (*mob).critmult = 0;
    (*mob).invis = 1.0;
    (*mob).amnesia = 0;

    if (*mob).state != MOB_DEAD {
        for x in 0..MAX_MAGIC_TIMERS {
            let p = &(*mob).da[x];
            let id = p.id as i32;
            if id > 0 && p.duration > 0 {
                let caster_id = p.caster_id;
                let has_caster =
                    caster_id > 0 && crate::game::map_server::entity_position(caster_id).is_some();
                let yname = magicdb_yname_str(id);
                if has_caster {
                    sl_doscript_2(&yname, Some("recast"), (*mob).id, caster_id);
                } else {
                    sl_doscript_simple(&yname, Some("recast"), (*mob).id);
                }
            }
        }
    }
    0
}

// ---- AI state machine ----------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_handle_sub(mob: *mut MobSpawnData) {
    if mob.is_null() {
        return;
    }
    let sptime = libc::time(std::ptr::null_mut()) as u32;

    if in_spawn_window(mob) {
        let data = (*mob).data.as_ref();
        let spawn_delay = data.map_or(0, |d| d.spawntime as u32);
        if (*mob).last_death + spawn_delay <= sptime {
            (*mob).spawncheck = 0;
            if (*mob).state == MOB_DEAD && (*mob).onetime == 0 {
                (*mob).target = 0;
                (*mob).attacker = 0;
                let has_users = ffi_map_is_loaded((*mob).m)
                    && crate::game::block::map_user_count((*mob).m as usize) > 0;
                if has_users {
                    crate::game::mob::mob_respawn(mob);
                } else {
                    crate::game::mob::mob_respawn_nousers(mob);
                }
            }
        }
    }

    if (*mob).data.as_ref().map_or(0, |d| d.r#type) >= 2 {
        return;
    }

    let has_users =
        ffi_map_is_loaded((*mob).m) && crate::game::block::map_user_count((*mob).m as usize) > 0;
    let subtype2 = (*mob).data.as_ref().map_or(0, |d| d.subtype);

    if !has_users && (*mob).onetime != 0 && subtype2 != 2 && (*mob).state != MOB_DEAD {
        return;
    }
    if !has_users && (*mob).onetime == 0 && subtype2 != 4 && (*mob).state != MOB_DEAD {
        return;
    }

    (*mob).time_ = (*mob).time_.wrapping_add(50);

    match (*mob).state {
        MOB_DEAD => {
            if (*mob).onetime != 0 {
                map_delblock_id((*mob).id, (*mob).m);
                map_deliddb((*mob).id);
                free_onetime(mob);
            }
        }
        MOB_ALIVE => {
            let data = if (*mob).data.is_null() {
                return;
            } else {
                &*(*mob).data
            };
            if ((*mob).newmove > 0 || (*mob).time_ >= data.movetime)
                && (*mob).time_ >= (*mob).newmove as i32
            {
                if data.r#type >= 2 {
                    return;
                }
                if data.r#type == 1 && (*mob).target == 0 {
                    if let Some(grid) = block_grid::get_grid((*mob).m as usize) {
                        let slot = &*ffi_get_map_ptr((*mob).m);
                        let ids = block_grid::ids_in_area(
                            grid,
                            (*mob).x as i32,
                            (*mob).y as i32,
                            AreaType::Area,
                            slot.xs as i32,
                            slot.ys as i32,
                        );
                        for id in ids {
                            if let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                                crate::game::mob::mob_find_target_inner(
                                    &mut *sd_arc.write() as *mut MapSessionData,
                                    mob,
                                );
                            }
                        }
                    }
                }
                let target_id = mob_resolve_target(mob);
                let pre_x = (*mob).x;
                let pre_y = (*mob).y;
                (*mob).time_ = 0;
                dispatch_ai(mob, target_id, "move");
                // If the mob didn't actually move but Lua left newmove faster
                // than the base speed (e.g. return-to-start mode while blocked),
                // reset newmove so the mob doesn't rapid-fire move attempts.
                if (*mob).x == pre_x
                    && (*mob).y == pre_y
                    && !(*mob).data.is_null()
                    && (*mob).newmove < (*(*mob).data).movetime as u32
                {
                    (*mob).newmove = (*(*mob).data).movetime as u32;
                }
            }
        }
        MOB_HIT => {
            let data = if (*mob).data.is_null() {
                return;
            } else {
                &*(*mob).data
            };
            if ((*mob).newatk > 0 || (*mob).time_ >= data.atktime)
                && (*mob).time_ >= (*mob).newatk as i32
            {
                if data.r#type >= 2 {
                    return;
                }
                let target_id = mob_resolve_target(mob);
                if target_id == 0 {
                    // mob_resolve_target already cleared target/attacker
                    (*mob).state = MOB_ALIVE;
                    return;
                }
                (*mob).time_ = 0;
                dispatch_ai(mob, target_id, "attack");
            }
        }
        MOB_ESCAPE => {
            let data = if (*mob).data.is_null() {
                return;
            } else {
                &*(*mob).data
            };
            if ((*mob).newmove > 0 || (*mob).time_ >= data.movetime)
                && (*mob).time_ >= (*mob).newmove as i32
            {
                if data.r#type >= 2 {
                    return;
                }
                if data.r#type == 1 && (*mob).target == 0 {
                    if let Some(grid) = block_grid::get_grid((*mob).m as usize) {
                        let slot = &*ffi_get_map_ptr((*mob).m);
                        let ids = block_grid::ids_in_area(
                            grid,
                            (*mob).x as i32,
                            (*mob).y as i32,
                            AreaType::Area,
                            slot.xs as i32,
                            slot.ys as i32,
                        );
                        for id in ids {
                            if let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                                crate::game::mob::mob_find_target_inner(
                                    &mut *sd_arc.write() as *mut MapSessionData,
                                    mob,
                                );
                            }
                        }
                    }
                }
                let target_id = mob_resolve_target(mob);
                (*mob).time_ = 0;
                dispatch_ai(mob, target_id, "escape");
            }
        }
        _ => {}
    }
}

/// Resolves mob->target to a valid target ID. Clears target if dead/invalid.
/// Returns the target entity ID, or 0 if no valid target.
unsafe fn mob_resolve_target(mob: *mut MobSpawnData) -> u32 {
    let target_id = (*mob).target;
    let pos_info = crate::game::map_server::entity_position(target_id);
    let (pos, bl_type) = match pos_info {
        Some(v) => v,
        None => {
            (*mob).target = 0;
            (*mob).attacker = 0;
            return 0;
        }
    };
    if pos.m != (*mob).m {
        (*mob).target = 0;
        (*mob).attacker = 0;
        return 0;
    }
    if bl_type == BL_MOB as u8 {
        if let Some(arc) = crate::game::map_server::map_id2mob_ref(target_id) {
            if arc.read().state == MOB_DEAD {
                (*mob).target = 0;
                (*mob).attacker = 0;
                return 0;
            }
        }
    } else if bl_type == BL_PC as u8 {
        use crate::game::pc::PC_DIE;
        if let Some(arc) = crate::game::map_server::map_id2sd_pc(target_id) {
            if arc.read().player.combat.state == PC_DIE as i8 {
                (*mob).target = 0;
                (*mob).attacker = 0;
                return 0;
            }
        }
    }
    target_id
}

/// Dispatches to the correct Lua AI script based on mob subtype.
unsafe fn dispatch_ai(mob: *mut MobSpawnData, bl_id: u32, event: &str) {
    let data = if (*mob).data.is_null() {
        return;
    } else {
        &*(*mob).data
    };
    match data.subtype {
        0 => {
            sl_doscript_2("mob_ai_basic", Some(event), (*mob).id, bl_id);
        }
        1 => {
            sl_doscript_2("mob_ai_normal", Some(event), (*mob).id, bl_id);
        }
        2 => {
            sl_doscript_2("mob_ai_hard", Some(event), (*mob).id, bl_id);
        }
        3 => {
            sl_doscript_2("mob_ai_boss", Some(event), (*mob).id, bl_id);
        }
        4 => {
            let yname = crate::game::scripting::carray_to_str(&data.yname);
            sl_doscript_2(yname, Some(event), (*mob).id, bl_id);
        }
        5 => {
            sl_doscript_2("mob_ai_ghost", Some(event), (*mob).id, bl_id);
        }
        _ => {}
    };
}

// ---- mob_trap_look (typed inner callback) --------------------------------------

/// Typed inner: activates NPC trap if mob steps on its cell.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_trap_look_inner(
    nd: *mut crate::game::npc::NpcData,
    mob: *mut MobSpawnData,
    type_: i32,
    def: *mut i32,
) -> i32 {
    if nd.is_null() {
        return 0;
    }
    // Only SUBTYPE_FLOOR (subtype==1) or sub-2 NPCs are traps
    if (*nd).subtype != SUBTYPE_FLOOR && (*nd).subtype != 2 {
        return 0;
    }
    if !def.is_null() && *def != 0 {
        return 0;
    }
    if type_ != 0 && (*nd).subtype == 2 {
        // skip sub-2 NPCs when type_ is non-zero
    } else {
        if !def.is_null() {
            *def = 1;
        }
        let nd_name = crate::game::scripting::carray_to_str(&(*nd).name);
        sl_doscript_2(nd_name, Some("click"), (*mob).id, (*nd).id);
    }
    0
}

// ---- Timer spawns (50ms game loop entry) ----------------------------------------

/// Called every 50ms by the game loop.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_timer_spawns() {
    TIMERCHECK.fetch_add(1, Ordering::Relaxed);

    let spawn_start = MOB_SPAWN_START.load(Ordering::Relaxed);
    let spawn_max = MOB_SPAWN_MAX.load(Ordering::Relaxed);
    if spawn_start != spawn_max {
        let mut x = spawn_start;
        while x < spawn_max {
            if let Some(mob_arc) = map_id2mob_ref(x) {
                // SAFETY: single-threaded game loop, Arc keeps allocation alive.
                // tick_mob -> Lua -> MobObject.__index acquires its own lock,
                // so we must NOT hold any guard across this call.
                let ptr: *mut MobSpawnData = mob_arc.legacy.data_ptr();
                tick_mob(&mut *ptr);
            }
            x += 1;
        }
    }

    let onetime_start = MOB_ONETIME_START.load(Ordering::Relaxed);
    let onetime_max = MOB_ONETIME_MAX.load(Ordering::Relaxed);
    if onetime_start != onetime_max {
        let mut x = onetime_start;
        while x < onetime_max {
            if let Some(mob_arc) = map_id2mob_ref(x) {
                // SAFETY: same as above -- no guard held across tick_mob/Lua.
                let ptr: *mut MobSpawnData = mob_arc.legacy.data_ptr();
                tick_mob(&mut *ptr);
            }
            x += 1;
        }
    }

    if TIMERCHECK.load(Ordering::Relaxed) >= 30 {
        TIMERCHECK.store(0, Ordering::Relaxed);
    }
}

unsafe fn tick_mob(mob: &mut MobSpawnData) {
    let mob = mob as *mut MobSpawnData;
    let tc = TIMERCHECK.load(Ordering::Relaxed);
    if tc.is_multiple_of(5) {
        mob_secondduratimer(mob);
    }
    if tc.is_multiple_of(10) {
        mob_thirdduratimer(mob);
    }
    if tc.is_multiple_of(30) {
        mob_fourthduratimer(mob);
    }
    if tc.is_multiple_of(20) {
        mob_duratimer(mob);
    }
    mob_handle_sub(mob);
}

// ---- Registry ------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_readglobalreg(mob: *mut MobSpawnData, reg: *const i8) -> i32 {
    if mob.is_null() || reg.is_null() {
        return 0;
    }
    for i in 0..MAX_GLOBALMOBREG {
        if libc::strcasecmp((*mob).registry[i].str.as_ptr(), reg) == 0 {
            return (*mob).registry[i].val;
        }
    }
    0
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_setglobalreg(mob: *mut MobSpawnData, reg: *const i8, val: i32) -> i32 {
    if mob.is_null() || reg.is_null() {
        return 1;
    }
    // find existing slot
    for i in 0..MAX_GLOBALMOBREG {
        if libc::strcasecmp((*mob).registry[i].str.as_ptr(), reg) == 0 {
            if val == 0 {
                libc::strcpy((*mob).registry[i].str.as_mut_ptr(), c"".as_ptr());
            }
            (*mob).registry[i].val = val;
            return 0;
        }
    }
    // find empty slot
    for i in 0..MAX_GLOBALMOBREG {
        if libc::strcasecmp((*mob).registry[i].str.as_ptr(), c"".as_ptr()) == 0 {
            let dst = (*mob).registry[i].str.as_mut_ptr();
            let dst_len = core::mem::size_of_val(&(*mob).registry[i].str);
            libc::strncpy(dst, reg, dst_len - 1);
            *dst.add(dst_len - 1) = 0;
            (*mob).registry[i].val = val;
            return 0;
        }
    }
    tracing::warn!(
        "[mob] mob_setglobalreg: couldn't set {:?}",
        std::ffi::CStr::from_ptr(reg)
    );
    1
}
