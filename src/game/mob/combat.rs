//! Mob combat functions: attacks, targeting, health, duration effects.

#![allow(non_snake_case, dead_code)]

use super::entity::{
    ai_script_name, broadcast_animation_to_pcs, magicdb_dispel, magicdb_name, magicdb_yname_str,
    map_id2sd_mob, sl_doscript_2, sl_doscript_simple, MobSpawnData,
};
use super::systems::mob_flushmagic;
use crate::common::constants::entity::mob::{MAX_MAGIC_TIMERS, MAX_THREATCOUNT, MOB_DEAD};
use crate::common::traits::LegacyEntity;
use crate::database::magic_db;
use crate::game::map_parse::combat::{
    clif_mob_kill, clif_send_mob_health, clif_send_pc_health, clif_sendmob_action,
};
use crate::game::map_parse::player_state::clif_sendstatus as clif_sendstatus_mob;
use crate::game::map_server::map_id2mob_ref;
use crate::game::pc::MapSessionData;

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn kill_mob(mob: *mut MobSpawnData) -> i32 {
    {
        clif_mob_kill(&mut *mob).await;
        mob_flushmagic(mob);
    }
    0
}

/// Visibility check then optionally retarget the mob to attack the given player.
/// updates `mob->target` based on `sd->status.gm_level` and a random roll.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_find_target_inner(sd: *mut MapSessionData, mob: *mut MobSpawnData) -> i32 {
    use crate::game::pc::PC_DIE;
    if sd.is_null() {
        return 0;
    }
    if mob.is_null() {
        return 0;
    }
    let seeinvis = if (*mob).data.is_null() {
        0i8
    } else {
        (*(*mob).data).seeinvis
    };
    let mut invis: u8 = 0;
    for i in 0..MAX_MAGIC_TIMERS {
        if (&(*sd).player.spells.dura_aether)[i].duration > 0 {
            let name = magicdb_name((&(*sd).player.spells.dura_aether)[i].id as i32);
            if !name.is_null() {
                if libc::strcasecmp(name, c"sneak".as_ptr()) == 0 {
                    invis = 1;
                }
                if libc::strcasecmp(name, c"cloak".as_ptr()) == 0 {
                    invis = 2;
                }
                if libc::strcasecmp(name, c"hide".as_ptr()) == 0 {
                    invis = 3;
                }
            }
        }
    }
    match invis {
        1 => {
            if seeinvis != 1 && seeinvis != 3 && seeinvis != 5 {
                return 0;
            }
        }
        2 => {
            if seeinvis != 2 && seeinvis != 3 && seeinvis != 5 {
                return 0;
            }
        }
        3 => {
            if seeinvis != 4 && seeinvis != 5 {
                return 0;
            }
        }
        _ => {}
    }
    if (*sd).player.combat.state == PC_DIE as i8 {
        return 0;
    }
    if (*mob).confused != 0 && (*mob).confused_target == (*sd).id {
        return 0;
    }
    if (*mob).target != 0 {
        let num = (rand::random::<u32>() & 0x00FF_FFFF) % 1000;
        if num <= 499 && (*sd).player.identity.gm_level < 50 {
            (*mob).target = (*sd).player.identity.id;
        }
    } else if (*sd).player.identity.gm_level < 50 {
        (*mob).target = (*sd).player.identity.id;
    }
    0
}

/// Mob attacks a player (or another mob) by ID.
/// Reads `sd->uFlags` and `sd->optFlags` to check immortal/stealth before attacking.
/// Calls scripting hooks `hitCritChance` and `swingDamage`, then sends network damage.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_attack(mob: *mut MobSpawnData, id: i32) -> i32 {
    use crate::game::pc::{OPT_FLAG_STEALTH, SFLAG_HPMP, U_FLAG_IMMORTAL};
    if id < 0 {
        return 0;
    }
    let target = id as u32;
    // Try typed lookups -- target is either a PC or another mob.
    let sd_arc = crate::game::map_server::map_id2sd_pc(target);
    let sd: *mut MapSessionData = sd_arc
        .as_deref()
        .map(|pe| &mut *pe.write() as *mut MapSessionData)
        .unwrap_or(std::ptr::null_mut());
    let tmob: *mut MobSpawnData = if sd.is_null() {
        map_id2mob_ref(target)
            .map(|arc| arc.legacy.data_ptr())
            .unwrap_or(std::ptr::null_mut())
    } else {
        std::ptr::null_mut()
    };
    if sd.is_null() && tmob.is_null() {
        return 0;
    }
    if !sd.is_null()
        && (((*sd).uFlags & U_FLAG_IMMORTAL != 0) || ((*sd).optFlags & OPT_FLAG_STEALTH != 0))
    {
        (*mob).target = 0;
        (*mob).attacker = 0;
        return 0;
    }
    let target_id = id as u32;
    if !sd.is_null() || !tmob.is_null() {
        sl_doscript_2("hitCritChance", None, (*mob).id, target_id);
    }
    if (*mob).critchance != 0 {
        let sound = if !(*mob).data.is_null() {
            (*(*mob).data).sound
        } else {
            0
        };
        clif_sendmob_action(&mut *mob, 1, 20, sound);
        if !sd.is_null() || !tmob.is_null() {
            sl_doscript_2("swingDamage", None, (*mob).id, target_id);
            for x in 0..MAX_MAGIC_TIMERS {
                if (*mob).da[x].id > 0 && (*mob).da[x].duration > 0 {
                    let yname = magicdb_yname_str((*mob).da[x].id as i32);
                    sl_doscript_2(&yname, Some("on_hit_while_cast"), (*mob).id, target_id);
                }
            }
        }
        let dmg = ((*mob).damage + 0.5f32) as i32;
        if !sd.is_null() {
            if (*mob).critchance == 1 {
                clif_send_pc_health(&mut *sd, dmg, 33);
            } else {
                clif_send_pc_health(&mut *sd, dmg, 255);
            }
            if let Some(ref pe) = sd_arc {
                clif_sendstatus_mob(pe, SFLAG_HPMP);
            }
        } else if !tmob.is_null() {
            if (*mob).critchance == 1 {
                clif_send_mob_health(&mut *tmob, dmg, 33);
            } else {
                clif_send_mob_health(&mut *tmob, dmg, 255);
            }
        }
    }
    0
}

/// Calculate and set `mob->critchance` based on mob stats vs player stats.
/// Returns 0 (miss), 1 (normal hit), or 2 (critical hit).
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_calc_critical(mob: *mut MobSpawnData, sd: *mut MapSessionData) -> i32 {
    if mob.is_null() || sd.is_null() {
        return 0;
    }
    let db = (*mob).data;
    if db.is_null() {
        return 0;
    }
    let equat = ((*db).hit + (*db).level + ((*db).might / 5) + 20)
        - ((*sd).player.progression.level as i32 + ((*sd).grace / 2));
    let mut equat = equat - ((*sd).grace / 4) + (*sd).player.progression.level as i32;
    let chance = ((rand::random::<u32>() & 0x00FF_FFFF) % 100) as i32;
    equat = equat.clamp(5, 95);
    if chance < equat {
        let crit = equat as f32 * 0.33f32;
        if (chance as f32) < crit {
            2
        } else {
            1
        }
    } else {
        0
    }
}

/// Heal mob: fire on_healed Lua event then send the negative-damage health packet.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn sl_mob_addhealth(mob: *mut MobSpawnData, damage: i32) {
    use crate::game::map_parse::combat::clif_send_mob_healthscript;
    if mob.is_null() {
        return;
    }
    let attacker = (*mob).attacker;
    let has_attacker =
        attacker > 0 && crate::game::map_server::entity_position(attacker).is_some();
    let data = (*mob).data;
    if !data.is_null() && damage > 0 {
        let yname = ai_script_name(&*data);
        if has_attacker {
            sl_doscript_2(yname, Some("on_healed"), (*mob).id, attacker);
        } else {
            sl_doscript_simple(yname, Some("on_healed"), (*mob).id);
        }
    }
    clif_send_mob_healthscript(&mut *mob, -damage, 0).await;
}

/// Damage mob: set attacker/damage fields then send the health packet.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn sl_mob_removehealth(mob: *mut MobSpawnData, damage: i32, caster_id: u32) {
    use crate::game::map_parse::combat::clif_send_mob_healthscript;
    if mob.is_null() {
        return;
    }
    let resolved_id = if caster_id > 0 {
        (*mob).attacker = caster_id;
        caster_id
    } else {
        (*mob).attacker
    };
    // Set damage/critchance on the resolved attacker entity.
    let mut set_on_attacker = false;
    if resolved_id > 0 {
        if let Some(arc) = crate::game::map_server::map_id2sd_pc(resolved_id) {
            let mut sd = arc.write();
            sd.damage = damage as f32;
            sd.critchance = 0;
            set_on_attacker = true;
        } else if let Some(arc) = map_id2mob_ref(resolved_id) {
            let mut tmob = arc.write();
            tmob.damage = damage as f32;
            tmob.critchance = 0;
            set_on_attacker = true;
        }
    }
    if !set_on_attacker {
        (*mob).damage = damage as f32;
        (*mob).critchance = 0;
    }
    if (*mob).state != MOB_DEAD {
        clif_send_mob_healthscript(&mut *mob, damage, 0).await;
    }
}

/// Return accumulated threat amount from a specific player on this mob.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_mob_checkthreat(mob: *mut MobSpawnData, player_id: u32) -> i32 {
    if mob.is_null() {
        return 0;
    }
    let tsd = map_id2sd_mob(player_id);
    if tsd.is_null() {
        return 0;
    }
    let uid = (*tsd).id;
    for x in 0..MAX_THREATCOUNT {
        if (*mob).threat[x].user == uid {
            return (*mob).threat[x].amount as i32;
        }
    }
    0
}

/// Add individual damage from player to mob's dmgindtable.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_mob_setinddmg(mob: *mut MobSpawnData, player_id: u32, dmg: f32) -> i32 {
    if mob.is_null() {
        return 0;
    }
    let sd = map_id2sd_mob(player_id);
    if sd.is_null() {
        return 0;
    }
    let cid = (*sd).player.identity.id;
    for x in 0..MAX_THREATCOUNT {
        if (*mob).dmgindtable[x][0] as u32 == cid || (*mob).dmgindtable[x][0] == 0.0 {
            (*mob).dmgindtable[x][0] = cid as f64;
            (*mob).dmgindtable[x][1] += dmg as f64;
            return 1;
        }
    }
    0
}

/// Add group damage from player to mob's dmggrptable.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_mob_setgrpdmg(mob: *mut MobSpawnData, player_id: u32, dmg: f32) -> i32 {
    if mob.is_null() {
        return 0;
    }
    let sd = map_id2sd_mob(player_id);
    if sd.is_null() {
        return 0;
    }
    let gid = (*sd).groupid;
    for x in 0..MAX_THREATCOUNT {
        if (*mob).dmggrptable[x][0] as u32 == gid || (*mob).dmggrptable[x][0] == 0.0 {
            (*mob).dmggrptable[x][0] = gid as f64;
            (*mob).dmggrptable[x][1] += dmg as f64;
            return 1;
        }
    }
    0
}

/// Call a named event on this mob's custom AI script.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_mob_callbase(mob: *mut MobSpawnData, script: &str) -> i32 {
    if mob.is_null() {
        return 0;
    }
    let attacker = (*mob).attacker;
    let yname = crate::game::scripting::carray_to_str(&(*(*mob).data).yname);
    let attacker_id =
        if attacker > 0 && crate::game::map_server::entity_position(attacker).is_some() {
            attacker
        } else {
            (*mob).id
        };
    sl_doscript_2(yname, Some(script), (*mob).id, attacker_id);
    1
}

/// Set or clear a magic-effect duration slot on the mob.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_mob_setduration(
    mob: *mut MobSpawnData,
    name: *const i8,
    mut time: i32,
    caster_id: u32,
    recast: i32,
) {
    if mob.is_null() {
        return;
    }
    let id = magic_db::id_by_name(&std::ffi::CStr::from_ptr(name).to_string_lossy());
    if time > 0 && time < 1000 {
        time = 1000;
    }
    let mut alreadycast = 0i32;
    for x in 0..MAX_MAGIC_TIMERS {
        if (*mob).da[x].id as i32 == id
            && (*mob).da[x].caster_id == caster_id
            && (*mob).da[x].duration > 0
        {
            alreadycast = 1;
        }
    }
    for x in 0..MAX_MAGIC_TIMERS {
        let mid = (*mob).da[x].id as i32;
        if mid == id && time <= 0 && (*mob).da[x].caster_id == caster_id && alreadycast == 1 {
            let saved_caster_id = (*mob).da[x].caster_id;
            (*mob).da[x].duration = 0;
            (*mob).da[x].id = 0;
            (*mob).da[x].caster_id = 0;
            broadcast_animation_to_pcs(&*mob, (*mob).da[x].animation as i32);
            (*mob).da[x].animation = 0;
            let has_caster = saved_caster_id != (*mob).id
                && saved_caster_id > 0
                && crate::game::map_server::entity_position(saved_caster_id).is_some();
            let yname = magicdb_yname_str(mid);
            if has_caster {
                sl_doscript_2(&yname, Some("uncast"), (*mob).id, saved_caster_id);
            } else {
                sl_doscript_simple(&yname, Some("uncast"), (*mob).id);
            }
            return;
        } else if (*mob).da[x].id as i32 == id
            && (*mob).da[x].caster_id == caster_id
            && ((*mob).da[x].duration > time || recast == 1)
            && alreadycast == 1
        {
            (*mob).da[x].duration = time;
            return;
        } else if (*mob).da[x].id == 0
            && (*mob).da[x].duration == 0
            && time != 0
            && alreadycast != 1
        {
            (*mob).da[x].id = id as u16;
            (*mob).da[x].duration = time;
            (*mob).da[x].caster_id = caster_id;
            return;
        }
    }
}

/// Clear magic-effect timers in id range [minid..maxid], firing uncast Lua events.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_mob_flushduration(mob: *mut MobSpawnData, dis: i32, minid: i32, maxid: i32) {
    if mob.is_null() {
        return;
    }
    let maxid = if maxid < minid { minid } else { maxid };
    for x in 0..MAX_MAGIC_TIMERS {
        let id = (*mob).da[x].id as i32;
        if id == 0 {
            continue;
        }
        if magicdb_dispel(id) > dis {
            continue;
        }
        let flush = if minid <= 0 {
            true
        } else if maxid <= 0 {
            id == minid
        } else {
            id >= minid && id <= maxid
        };
        if flush {
            (*mob).da[x].duration = 0;
            broadcast_animation_to_pcs(&*mob, (*mob).da[x].animation as i32);
            (*mob).da[x].animation = 0;
            (*mob).da[x].id = 0;
            let cid = (*mob).da[x].caster_id;
            let has_caster = cid > 0 && crate::game::map_server::entity_position(cid).is_some();
            (*mob).da[x].caster_id = 0;
            let yname = magicdb_yname_str(id);
            if has_caster {
                sl_doscript_2(&yname, Some("uncast"), (*mob).id, cid);
            } else {
                sl_doscript_simple(&yname, Some("uncast"), (*mob).id);
            }
        }
    }
}

/// Clear magic-effect timers without firing uncast Lua events.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_mob_flushdurationnouncast(
    mob: *mut MobSpawnData,
    dis: i32,
    minid: i32,
    maxid: i32,
) {
    if mob.is_null() {
        return;
    }
    let maxid = if maxid < minid { minid } else { maxid };
    for x in 0..MAX_MAGIC_TIMERS {
        let id = (*mob).da[x].id as i32;
        if id == 0 {
            continue;
        }
        if magicdb_dispel(id) > dis {
            continue;
        }
        let flush = if minid <= 0 {
            true
        } else if maxid <= 0 {
            id == minid
        } else {
            id >= minid && id <= maxid
        };
        if flush {
            (*mob).da[x].duration = 0;
            (*mob).da[x].caster_id = 0;
            broadcast_animation_to_pcs(&*mob, (*mob).da[x].animation as i32);
            (*mob).da[x].animation = 0;
            (*mob).da[x].id = 0;
        }
    }
}
