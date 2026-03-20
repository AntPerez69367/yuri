//!
//! callable from any remaining C code that has not yet been ported.

#![allow(non_snake_case, clippy::wildcard_imports)]


use crate::common::traits::LegacyEntity;
use crate::game::scripting::carray_to_str;
use crate::game::player::PlayerEntity;
use crate::database::mob_db::MobDbData;
use crate::database::map_db::raw_map_ptr;
use crate::session::{SessionId, session_exists, session_set_eof};
use crate::game::mob::{MobSpawnData, MOB_DEAD, MAX_MAGIC_TIMERS, MAX_THREATCOUNT};
use crate::game::pc::{
    MapSessionData,
    BL_PC, BL_MOB,
    EQ_WEAP, EQ_ARMOR, EQ_SHIELD, EQ_HELM, EQ_LEFT, EQ_RIGHT,
    EQ_SUBLEFT, EQ_SUBRIGHT, EQ_FACEACC, EQ_CROWN, EQ_MANTLE, EQ_NECKLACE, EQ_BOOTS, EQ_COAT,
    SFLAG_HPMP, SFLAG_FULLSTATS,
    FLAG_MAGIC,
};
use crate::common::player::spells::MAX_SPELLS;

use super::packet::{
    encrypt, wfifob, wfifohead, wfifol, wfifop, wfifoset, wfifow, wfifoheader,
    clif_send,
    AREA, SELF, SAMEAREA,
};
use crate::game::block::AreaType;
use crate::game::block_grid;

use crate::game::map_parse::player_state::clif_sendstatus;
use crate::game::map_parse::groups::{clif_grouphealth_update, clif_isingroup};
use crate::game::map_parse::chat::{clif_sendmsg, clif_sendminitext, clif_playsound_entity};
use crate::game::map_parse::items::clif_unequipit;
use crate::game::client::visual::{clif_getequiptype, broadcast_update_state};
use crate::game::client::BroadcastSrc;
use crate::game::map_server::groups;
use crate::game::pc::{addtokillreg, pc_calcstat, pc_checklevel, pc_isequip};
use crate::game::client::handlers::clif_addtokillreg;
use crate::database::item_db;
use crate::database::magic_db;
use crate::game::mob::mob_flushmagic;
use crate::game::scripting::sl_async_freeco;

use std::sync::Arc;
use parking_lot::RwLock;

/// Arc-based player lookup.
#[inline]
fn map_id2sd_arc(id: u32) -> Option<Arc<crate::game::pc::PlayerEntity>> {
    crate::game::map_server::map_id2sd_pc(id)
}

/// Arc-based mob lookup.
#[inline]
fn map_id2mob_arc(id: u32) -> Option<Arc<RwLock<MobSpawnData>>> {
    crate::game::map_server::map_id2mob_ref(id)
}

/// Legacy raw-pointer player lookup for deeply unsafe code paths.
/// Returns a raw pointer by write-locking the Arc. The pointer is valid as long
/// as the Arc (held internally in the global map) is not removed.
/// Callers MUST hold the returned `Arc` alive (or the global map keeps it alive).
#[inline]
fn map_id2sd_local(id: u32) -> *mut MapSessionData {
    match crate::game::map_server::map_id2sd_pc(id) {
        Some(arc) => {
            let ptr = &mut *arc.write() as *mut MapSessionData;
            // SAFETY: The Arc in the global map keeps the allocation alive.
            // The RwLock write guard is dropped here, but the underlying data
            // persists in the Arc. This is a transitional pattern.
            ptr
        }
        None => std::ptr::null_mut(),
    }
}

/// Legacy raw-pointer mob lookup for deeply unsafe code paths.
#[inline]
fn map_id2mob_local(id: u32) -> *mut MobSpawnData {
    match crate::game::map_server::map_id2mob_ref(id) {
        Some(arc) => {
            let ptr = &mut *arc.write() as *mut MobSpawnData;
            ptr
        }
        None => std::ptr::null_mut(),
    }
}

/// Dispatch a Lua event with a single entity ID argument.
fn sl_doscript_simple(root: &str, method: Option<&str>, id: u32) -> i32 {
    crate::game::scripting::doscript_blargs_id(root, method, &[id])
}

/// Dispatch a Lua event with two entity ID arguments.
fn sl_doscript_2(root: &str, method: Option<&str>, id1: u32, id2: u32) -> i32 {
    crate::game::scripting::doscript_blargs_id(root, method, &[id1, id2])
}

// rnd(x) macro: ((int)(randomMT() & 0xFFFFFF) % (x))
#[inline]
fn rnd(x: i32) -> i32 {
    ((rand::random::<u32>() & 0x00FF_FFFF) as i32).wrapping_rem(x)
}

// ─── clif_pc_damage ──────────────────────────────────────────────────────────

/// Apply a critical hit: run scripts and send health packet.
///
pub fn clif_pc_damage(sd: &mut MapSessionData, src: &mut MapSessionData) -> i32 {
    if src.player.combat.state == 1 { return 0; }

    sl_doscript_2("hitCritChance", None, sd.id, src.id);

    if sd.critchance > 0 {
        sl_doscript_2("swingDamage", None, sd.id, src.id);
        sd.damage += 0.5f32;
        let damage = sd.damage as i32;

        if sd.player.inventory.equip[EQ_WEAP as usize].id > 0 {
            unsafe {
                clif_playsound_entity(
                    src.id, src.m, src.x, src.y, BL_PC as u8,
                    item_db::search(sd.player.inventory.equip[EQ_WEAP as usize].id).sound_hit as i32,
                );
            }
        }

        for x in 0..14usize {
            if sd.player.inventory.equip[x].id > 0 {
                sl_doscript_2(carray_to_str(&item_db::search(sd.player.inventory.equip[x].id).yname), Some("on_hit"), sd.id, src.id);
            }
        }

        for x in 0..MAX_SPELLS {
            if sd.player.spells.skills[x] > 0 {
                sl_doscript_2(carray_to_str(&magic_db::search(sd.player.spells.skills[x] as i32).yname), Some("passive_on_hit"), sd.id, src.id);
            }
        }

        for x in 0..MAX_MAGIC_TIMERS {
            if sd.player.spells.dura_aether[x].id > 0 && sd.player.spells.dura_aether[x].duration > 0 {
                let caster_id = sd.player.spells.dura_aether[x].caster_id;
                let spell_entry = magic_db::search(sd.player.spells.dura_aether[x].id as i32);
                let spell_root = carray_to_str(&spell_entry.yname);
                if caster_id > 0 && crate::game::map_server::map_id2sd_pc(caster_id).is_some() {
                    crate::game::scripting::doscript_blargs_id(spell_root, Some("on_hit_while_cast"), &[sd.id, src.id, caster_id]);
                } else {
                    sl_doscript_2(spell_root, Some("on_hit_while_cast"), sd.id, src.id);
                }
            }
        }

        if sd.critchance == 1 {
            clif_send_pc_health(src, damage, 33);
        } else if sd.critchance == 2 {
            clif_send_pc_health(src, damage, 255);
        }

        if let Some(src_arc) = map_id2sd_arc(src.id) {
            unsafe { clif_sendstatus(&src_arc, SFLAG_HPMP); }
        }
    }

    0
}

// ─── clif_send_pc_health ─────────────────────────────────────────────────────

/// Trigger player combat scripts when attacked.
///
pub fn clif_send_pc_health(src: &mut MapSessionData, damage: i32, critical: i32) -> i32 {
    let _ = (damage, critical);
    let attacker_id = if crate::game::map_server::entity_position(src.attacker).is_some() {
        src.attacker
    } else {
        src.id
    };

    sl_doscript_2("player_combat", Some("on_attacked"), src.id, attacker_id);
    0
}

// ─── clif_send_pc_healthscript ───────────────────────────────────────────────

/// Apply damage to the player, compute health percentage, broadcast health
/// packet to the area, and fire all combat scripts.
///
pub fn clif_send_pc_healthscript(
    sd: &mut MapSessionData,
    damage: i32,
    critical: i32,
) -> i32 {
    let maxvita = sd.max_hp;
    let mut currentvita = sd.player.combat.hp;

    let mut tsd: *mut MapSessionData = std::ptr::null_mut();
    let attacker_id = sd.attacker;

    if let Some(arc) = crate::game::map_server::map_id2sd_pc(attacker_id) {
        tsd = &mut *arc.write();
    } else if let Some(arc) = crate::game::map_server::map_id2mob_ref(attacker_id) {
        let mob = unsafe { &*arc.data_ptr() };
        if mob.owner < crate::game::mob::MOB_START_NUM && mob.owner > 0 {
            tsd = map_id2sd_local(mob.owner);
        }
    }

    let bl_id = if crate::game::map_server::entity_position(attacker_id).is_some() { attacker_id } else { 0 };

    if damage > 0 {
        for x in 0..MAX_SPELLS {
            if sd.player.spells.skills[x] > 0 {
                sl_doscript_2(carray_to_str(&magic_db::search(sd.player.spells.skills[x] as i32).yname), Some("passive_on_takingdamage"), sd.id, bl_id);
            }
        }
    }

    sd.lastvita = currentvita;
    if damage < 0 {
        // Healing: damage is negative, so subtracting it increases vita.
        // Use i64 to avoid wrapping when casting negative i32 to u32.
        let new_vita = (currentvita as i64 - damage as i64)
            .max(0)
            .min(maxvita as i64) as u32;
        currentvita = new_vita;
    } else {
        if currentvita < damage as u32 {
            currentvita = 0;
        } else {
            currentvita -= damage as u32;
        }
    }

    if currentvita > maxvita {
        currentvita = maxvita;
    }

    sd.player.combat.hp = currentvita;

    let mut percentage: f32 = if currentvita == 0 {
        0.0f32
    } else {
        (currentvita as f32 / maxvita as f32) * 100.0f32
    };

    if (percentage as i32) == 0 && currentvita != 0 {
        percentage = 1.0f32;
    }

    let mut buf = [0u8; 32];
    buf[0] = 0xAA;
    let sz = 12u16;
    buf[1] = (sz >> 8) as u8;
    buf[2] = sz as u8;
    buf[3] = 0x13;
    let blid = sd.id;
    buf[5] = (blid >> 24) as u8;
    buf[6] = (blid >> 16) as u8;
    buf[7] = (blid >>  8) as u8;
    buf[8] = blid as u8;
    buf[9]  = critical as u8;
    buf[10] = percentage as u8;
    let dmg = damage as u32;
    buf[11] = (dmg >> 24) as u8;
    buf[12] = (dmg >> 16) as u8;
    buf[13] = (dmg >>  8) as u8;
    buf[14] = dmg as u8;

    if sd.player.combat.state == 2 {
        unsafe { clif_send(buf.as_ptr(), 32, BroadcastSrc { id: sd.id, m: sd.m, x: sd.x, y: sd.y, bl_type: BL_PC as u8 }, SELF); }
    } else {
        unsafe { clif_send(buf.as_ptr(), 32, BroadcastSrc { id: sd.id, m: sd.m, x: sd.x, y: sd.y, bl_type: BL_PC as u8 }, AREA); }
    }

    if sd.player.combat.hp != 0 && damage > 0 {
        for x in 0..MAX_SPELLS {
            if sd.player.spells.skills[x] > 0 {
                sl_doscript_2(carray_to_str(&magic_db::search(sd.player.spells.skills[x] as i32).yname), Some("passive_on_takedamage"), sd.id, bl_id);
            }
        }
        for x in 0..MAX_MAGIC_TIMERS {
            if sd.player.spells.dura_aether[x].id > 0 && sd.player.spells.dura_aether[x].duration > 0 {
                sl_doscript_2(carray_to_str(&magic_db::search(sd.player.spells.dura_aether[x].id as i32).yname), Some("on_takedamage_while_cast"), sd.id, bl_id);
            }
        }
        for x in 0..14usize {
            if sd.player.inventory.equip[x].id > 0 {
                sl_doscript_2(carray_to_str(&item_db::search(sd.player.inventory.equip[x].id).yname), Some("on_takedamage"), sd.id, bl_id);
            }
        }
    }

    if sd.player.combat.hp == 0 {
        sl_doscript_simple("onDeathPlayer", None, sd.id);

        if !tsd.is_null() {
            let tsd_bl_id = unsafe { (*tsd).id };
            sl_doscript_2("onKill", None, sd.id, tsd_bl_id);
        }
    }

    if sd.group_count > 0 {
        if let Some(sd_arc) = map_id2sd_arc(sd.id) {
            unsafe { clif_grouphealth_update(&sd_arc); }
        }
    }

    0
}

// ─── clif_send_selfbar ────────────────────────────────────────────────────────

/// Send the player's own health bar to themselves.
///
pub fn clif_send_selfbar(sd: &mut MapSessionData) {
    let mut percentage: f32 = if sd.player.combat.hp == 0 {
        0.0f32
    } else {
        (sd.player.combat.hp as f32 / sd.max_hp as f32) * 100.0f32
    };

    if (percentage as i32) == 0 && sd.player.combat.hp != 0 {
        percentage = 1.0f32;
    }

    if !session_exists(sd.fd) {
        return;
    }

    let fd = sd.fd;
    unsafe {
        wfifohead(fd, 15);
        wfifob(fd, 0, 0xAA);
        wfifow(fd, 1, 12u16.swap_bytes());
        wfifob(fd, 3, 0x13);
        wfifol(fd, 5, sd.id.swap_bytes());
        wfifob(fd, 9, 0);
        wfifob(fd, 10, percentage as u8);
        wfifol(fd, 11, 0u32.swap_bytes());
        wfifoset(fd, encrypt(fd) as usize);
    }
}

// ─── clif_send_groupbars ─────────────────────────────────────────────────────

/// Send another player's health bar to `sd` (group bar update).
///
pub fn clif_send_groupbars(sd: &mut MapSessionData, tsd: &mut MapSessionData) {
    let mut percentage: f32 = if tsd.player.combat.hp == 0 {
        0.0f32
    } else {
        (tsd.player.combat.hp as f32 / tsd.max_hp as f32) * 100.0f32
    };

    if (percentage as i32) == 0 && tsd.player.combat.hp != 0 {
        percentage = 1.0f32;
    }

    if !session_exists(sd.fd) {
        return;
    }

    let fd = sd.fd;
    let tsd_blid = tsd.id;
    unsafe {
        wfifohead(fd, 15);
        wfifob(fd, 0, 0xAA);
        wfifow(fd, 1, 12u16.swap_bytes());
        wfifob(fd, 3, 0x13);
        wfifol(fd, 5, tsd_blid.swap_bytes());
        wfifob(fd, 9, 0);
        wfifob(fd, 10, percentage as u8);
        wfifol(fd, 11, 0u32.swap_bytes());
        wfifoset(fd, encrypt(fd) as usize);
    }
}

// ─── clif_send_mobbars ────────────────────────────────────────────────────────

///
/// Send a mob's health bar to a player.
/// `bl` is the mob, `sd` is the receiving player.
///
/// Send mob health bar to a specific player.
/// `mob` is the mob whose health bar is being sent, `sd` is the receiving player.
pub fn clif_send_mobbars_inner(mob: &MobSpawnData, sd: &MapSessionData) -> i32 {
    if mob.current_vita == 0 && mob.maxvita == 0 { return 1; }

    let mut percentage: f32 = if mob.current_vita == 0 {
        0.0f32
    } else {
        (mob.current_vita as f32 / mob.maxvita as f32) * 100.0f32
    };

    if (percentage as i32) == 0 && mob.current_vita != 0 {
        percentage = 1.0f32;
    }

    if !session_exists(sd.fd) {
        return 1;
    }

    let fd = sd.fd;
    wfifohead(fd, 15);
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, 12u16.swap_bytes());
    wfifob(fd, 3, 0x13);
    wfifol(fd, 5, mob.id.swap_bytes());
    wfifob(fd, 9, 0);
    wfifob(fd, 10, percentage as u8);
    wfifol(fd, 11, 0u32.swap_bytes());
    wfifoset(fd, unsafe { encrypt(fd) } as usize);

    0
}

// ─── clif_findspell_pos ───────────────────────────────────────────────────────

/// Find the spell slot index for a given spell id. Returns -1 if not found.
///
pub fn clif_findspell_pos(sd: &mut MapSessionData, id: i32) -> i32 {
    for x in 0..52usize {
        if sd.player.spells.skills[x] as i32 == id {
            return x as i32;
        }
    }
    -1
}

// ─── clif_calc_critical ───────────────────────────────────────────────────────

/// Calculate whether an attack is a normal hit, critical hit, or miss.
/// Returns 0 (miss), 1 (hit), or 2 (critical).
/// `target_id` is the entity being attacked (PC or MOB).
pub fn clif_calc_critical(sd: &mut MapSessionData, target_id: u32) -> i32 {
    let max_hit = 95;
    let mut equat: i32 = 0;

    if let Some(tsd_arc) = crate::game::map_server::map_id2sd_pc(target_id) {
        let tsd = tsd_arc.read();
        equat = (55 + sd.grace / 2) - tsd.grace / 2
            + (sd.hit as f32 * 1.5f32) as i32
            + (sd.player.progression.level as i32 - tsd.player.progression.level as i32);
    } else if let Some(mob_arc) = crate::game::map_server::map_id2mob_ref(target_id) {
        let mob = mob_arc.read();
        let data: *mut MobDbData = mob.data;
        unsafe {
            equat = (55 + sd.grace / 2) - (*data).grace / 2
                + (sd.hit as f32 * 1.5f32) as i32
                + (sd.player.progression.level as i32 - (*data).level);
        }
    }

    if equat < 5 { equat = 5; }
    if equat > max_hit { equat = max_hit; }

    let chance = rnd(100);
    if chance < equat {
        let mut crit = sd.hit / 3;
        if crit < 1 { crit = 1; }
        if crit >= 100 { crit = 99; }
        if chance < crit {
            return 2;
        } else {
            return 1;
        }
    }
    0
}

// ─── clif_has_aethers ────────────────────────────────────────────────────────

/// Return the aether value for a given spell id, or 0 if not found.
///
pub fn clif_has_aethers(sd: &mut MapSessionData, spell: i32) -> i32 {
    for x in 0..MAX_MAGIC_TIMERS {
        if sd.player.spells.dura_aether[x].id as i32 == spell {
            return sd.player.spells.dura_aether[x].aether;
        }
    }
    0
}

// ─── clif_send_duration ──────────────────────────────────────────────────────

/// Send a duration/ticker bar packet to the player.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_send_duration(
    sd: &mut MapSessionData,
    id: i32,
    time: i32,
    tsd: *mut MapSessionData,
) -> i32 {
    let name = magic_db::name_ptr(id);

    if magic_db::ticker(id) == 0 { return 0; }

    // Compute label string and its length.
    // label is written directly into the WFIFO via copy.
    let label: &[u8];
    let mut composed_buf = [0u8; 512];
    let label_len: usize;

    if id == 0 {
        label = b"Shield";
        label_len = label.len();
    } else if !tsd.is_null() {
        // sprintf(buf, "%s (%s)", name, tsd->status.name)
        unsafe {
            let name_bytes = cstr_bytes(name as *const u8);
            let char_name_bytes = (*tsd).player.identity.name.as_bytes();
            let total = name_bytes.len() + 3 + char_name_bytes.len();
            let total = if total < composed_buf.len() {
                let mut pos = 0usize;
                composed_buf[pos..pos + name_bytes.len()].copy_from_slice(name_bytes);
                pos += name_bytes.len();
                composed_buf[pos] = b' ';
                composed_buf[pos + 1] = b'(';
                pos += 2;
                composed_buf[pos..pos + char_name_bytes.len()].copy_from_slice(char_name_bytes);
                pos += char_name_bytes.len();
                composed_buf[pos] = b')';
                total
            } else {
                // Label too long for buffer: truncate to last valid index (no null terminator needed,
                // length is tracked explicitly via label_len).
                composed_buf.len() - 1
            };
            label = std::slice::from_raw_parts(composed_buf.as_ptr(), total);
            label_len = total;
        }
    } else {
        unsafe {
            label = cstr_bytes(name as *const u8);
            label_len = label.len();
        }
    }

    if !session_exists(sd.fd) {
        return 0;
    }

    let len = label_len as i32;
    let fd = sd.fd;
    unsafe {
        wfifohead(fd, (len + 10) as usize);
        wfifob(fd, 5, len as u8);

        // copy label bytes to WFIFOP(fd, 6)
        {
            let dst = wfifop(fd, 6);
            if !dst.is_null() {
                std::ptr::copy_nonoverlapping(label.as_ptr(), dst, label_len);
            }
        }

        wfifol(fd, label_len + 6, (time as u32).swap_bytes());
        wfifoheader(fd, 0x3A, (label_len as u16) + 7);
        wfifoset(fd, encrypt(fd) as usize);
    }

    0
}

// ─── clif_send_aether ─────────────────────────────────────────────────────────

/// Send aether (spell cooldown) bar update to the player.
///
pub fn clif_send_aether(sd: &mut MapSessionData, id: i32, time: i32) -> i32 {
    let pos = clif_findspell_pos(sd, id);
    if pos < 0 { return 0; }

    if !session_exists(sd.fd) {
        return 0;
    }

    let fd = sd.fd;
    unsafe {
        wfifohead(fd, 11);
        wfifoheader(fd, 63, 8);
        wfifow(fd, 5, ((pos + 1) as u16).swap_bytes());
        wfifol(fd, 7, (time as u32).swap_bytes());
        wfifoset(fd, encrypt(fd) as usize);
    }
    0
}

// ─── clif_mob_damage ─────────────────────────────────────────────────────────

/// Apply a melee hit to a mob: fire scripts, update threat, send health packet.
///
pub fn clif_mob_damage(sd: &mut MapSessionData, mob: &mut MobSpawnData) -> i32 {
    if mob.state == MOB_DEAD { return 0; }

    sl_doscript_2("hitCritChance", None, sd.id, mob.id);

    if sd.critchance > 0 {
        sl_doscript_2("swingDamage", None, sd.id, mob.id);

        if sd.player.inventory.equip[EQ_WEAP as usize].id > 0 {
            unsafe {
                clif_playsound_entity(
                    mob.id, mob.m, mob.x, mob.y, BL_MOB as u8,
                    item_db::search(sd.player.inventory.equip[EQ_WEAP as usize].id).sound_hit as i32,
                );
            }
        }

        if rnd(100) > 75 {
            clif_deductdura(sd, EQ_WEAP, 1);
        }

        sd.damage += 0.5f32;
        let damage = sd.damage as i32; // (int)(sd->damage += 0.5f)
        mob.lastaction = unsafe { libc_time() } as i32;

        for x in 0..MAX_THREATCOUNT {
            if mob.threat[x].user == sd.id {
                mob.threat[x].amount = mob.threat[x].amount.saturating_add(damage as u32);
                break;
            } else if mob.threat[x].user == 0 {
                mob.threat[x].user = sd.id;
                mob.threat[x].amount = damage as u32;
                break;
            }
        }

        for x in 0..14usize {
            if sd.player.inventory.equip[x].id > 0 {
                sl_doscript_2(carray_to_str(&item_db::search(sd.player.inventory.equip[x].id).yname), Some("on_hit"), sd.id, mob.id);
            }
        }

        for x in 0..MAX_SPELLS {
            if sd.player.spells.skills[x] > 0 {
                sl_doscript_2(carray_to_str(&magic_db::search(sd.player.spells.skills[x] as i32).yname), Some("passive_on_hit"), sd.id, mob.id);
            }
        }

        for x in 0..MAX_MAGIC_TIMERS {
            if sd.player.spells.dura_aether[x].id > 0 && sd.player.spells.dura_aether[x].duration > 0 {
                sl_doscript_2(carray_to_str(&magic_db::search(sd.player.spells.dura_aether[x].id as i32).yname), Some("on_hit_while_cast"), sd.id, mob.id);
            }
        }

        if sd.critchance == 1 {
            clif_send_mob_health(mob, damage, 33);
        } else if sd.critchance == 2 {
            clif_send_mob_health(mob, damage, 255);
        }
    }

    0
}

// ─── clif_send_mob_health_sub ─────────────────────────────────────────────────

///
/// Send mob health bar to a player in the area (group-filtered).
/// `bl` is the receiving player.
///
/// # Safety of the internal cast
///
/// SAFETY: These functions are called as foreach_in_area callbacks which dispatch
/// Send mob health/damage to a viewing player.
/// `sd_viewer` is the receiving player, `sd` is the attacker, `mob` is the target mob.
pub fn clif_send_mob_health_sub_inner(
    sd_viewer: &mut MapSessionData,
    sd: &mut MapSessionData,
    mob: &mut MobSpawnData,
    critical: i32,
    percentage: i32,
    damage: i32,
) -> i32 {
    unsafe {
        let viewer_in_group = if let Some(viewer_arc) = map_id2sd_arc(sd_viewer.id) {
            clif_isingroup(&viewer_arc, sd as *mut MapSessionData) != 0
        } else {
            false
        };
        if !viewer_in_group
            && sd.id != sd_viewer.id {
                return 0;
            }

        if !session_exists(sd.fd) {
            return 0;
        }

        use crate::session::session_get_eof;
        if !session_exists(sd_viewer.fd) || session_get_eof(sd_viewer.fd) != 0 {
            session_set_eof(sd_viewer.fd, 8);
            return 0;
        }

        let fd = sd_viewer.fd;
        wfifohead(fd, 15);
        wfifoheader(fd, 0x13, 12);
        wfifol(fd, 5, mob.id.swap_bytes());
        wfifob(fd, 9, critical as u8);
        wfifob(fd, 10, percentage as u8);
        wfifol(fd, 11, (damage as u32).swap_bytes());
        wfifoset(fd, encrypt(fd) as usize);
    }
    0
}

// ─── clif_send_mob_health_sub_nosd ────────────────────────────────────────────

/// Send mob health bar to a player in the area (no-sd variant).
/// `sd_viewer` is the receiving player.
pub fn clif_send_mob_health_sub_nosd_inner(
    sd_viewer: &MapSessionData,
    mob: &MobSpawnData,
    critical: i32,
    percentage: i32,
    damage: i32,
) -> i32 {
    if !session_exists(sd_viewer.fd) {
        return 0;
    }

    let fd = sd_viewer.fd;
    wfifohead(fd, 15);
    wfifoheader(fd, 0x13, 12);
    wfifol(fd, 5, mob.id.swap_bytes());
    wfifob(fd, 9, critical as u8);
    wfifob(fd, 10, percentage as u8);
    wfifol(fd, 11, (damage as u32).swap_bytes());
    wfifoset(fd, unsafe { encrypt(fd) } as usize);
    0
}

// ─── clif_send_mob_health ─────────────────────────────────────────────────────

/// Trigger mob combat AI scripts when the mob is attacked.
///
pub fn clif_send_mob_health(mob: &mut MobSpawnData, damage: i32, critical: i32) -> i32 {
    let _ = (damage, critical);
    if mob.bl_type != BL_MOB as u8 { return 0; }

    let attacker_id = if crate::game::map_server::entity_position(mob.attacker).is_some() {
        mob.attacker
    } else {
        mob.id
    };

    let data: *mut MobDbData = mob.data;
    let subtype = unsafe { (*data).subtype };

    if subtype == 0 {
        sl_doscript_2("mob_ai_basic", Some("on_attacked"), mob.id, attacker_id);
    } else if subtype == 1 {
        sl_doscript_2("mob_ai_normal", Some("on_attacked"), mob.id, attacker_id);
    } else if subtype == 2 {
        sl_doscript_2("mob_ai_hard", Some("on_attacked"), mob.id, attacker_id);
    } else if subtype == 3 {
        sl_doscript_2("mob_ai_boss", Some("on_attacked"), mob.id, attacker_id);
    } else if subtype == 4 {
        sl_doscript_2(carray_to_str(unsafe { &(*data).yname }), Some("on_attacked"), mob.id, attacker_id);
    } else if subtype == 5 {
        sl_doscript_2("mob_ai_ghost", Some("on_attacked"), mob.id, attacker_id);
    }

    0
}

// ─── clif_send_mob_healthscript ───────────────────────────────────────────────

/// Apply damage to a mob, compute percentage, broadcast health bars, run scripts.
///
pub async fn clif_send_mob_healthscript(mob: &mut MobSpawnData, damage: i32, critical: i32) -> i32 {
    let _ = critical;

    let mut sd: *mut MapSessionData = std::ptr::null_mut();
    let mut tmob: *mut MobSpawnData = std::ptr::null_mut();

    if mob.attacker > 0 {
        if let Some(arc) = crate::game::map_server::map_id2sd_pc(mob.attacker) {
            sd = &mut *arc.write();
        } else if let Some(arc) = crate::game::map_server::map_id2mob_ref(mob.attacker) {
            tmob = unsafe { &mut *arc.data_ptr() };
            unsafe {
                if (*tmob).owner < crate::game::mob::MOB_START_NUM && (*tmob).owner > 0 {
                    sd = map_id2sd_local((*tmob).owner);
                }
            }
        }
    }

    if mob.state == MOB_DEAD { return 0; }

    let maxvita = mob.maxvita as i32;
    let mut currentvita = mob.current_vita as i32;

    if damage < 0 {
        if currentvita - damage > maxvita {
            mob.maxdmg += (maxvita - currentvita) as f64;
        } else {
            mob.maxdmg -= damage as f64;
        }
        mob.lastvita = currentvita as u32;
        currentvita -= damage;
    } else {
        mob.lastvita = currentvita as u32;
        if currentvita < damage {
            currentvita = 0;
        } else {
            currentvita -= damage;
        }
    }

    if currentvita > maxvita {
        currentvita = maxvita;
    }

    let percentage: f32 = if currentvita <= 0 {
        0.0f32
    } else {
        let p = (currentvita as f32 / maxvita as f32) * 100.0f32;
        if p < 1.0f32 && currentvita > 0 { 1.0f32 } else { p }
    };

    let bl_id = mob.attacker;

    if currentvita > 0 && damage > 0 {
        for x in 0..MAX_MAGIC_TIMERS {
            let p = &mob.da[x];
            if p.id > 0 && p.duration > 0 {
                sl_doscript_2(carray_to_str(&magic_db::search(p.id as i32).yname), Some("on_takedamage_while_cast"), mob.id, bl_id);
            }
        }
    }

    let pct_int = percentage as i32;

    if !sd.is_null() {
        unsafe {
            if let Some(grid) = block_grid::get_grid(mob.m as usize) {
                let slot = &*crate::database::map_db::raw_map_ptr().add(mob.m as usize);
                let ids = block_grid::ids_in_area(grid, mob.x as i32, mob.y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
                for id in ids {
                    if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                        let mut pc = pc_arc.write();
                        clif_send_mob_health_sub_inner(&mut pc, &mut *sd, mob, critical, pct_int, damage);
                    }
                }
            }
        }
    } else {
        unsafe {
            if let Some(grid) = block_grid::get_grid(mob.m as usize) {
                let slot = &*crate::database::map_db::raw_map_ptr().add(mob.m as usize);
                let ids = block_grid::ids_in_area(grid, mob.x as i32, mob.y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
                for id in ids {
                    if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                        let pc = pc_arc.read();
                        clif_send_mob_health_sub_nosd_inner(&pc, mob, critical, pct_int, damage);
                    }
                }
            }
        }
    }

    mob.current_vita = currentvita as u32;

    let sd_bl_id = if !sd.is_null() { unsafe { (*sd).id } } else { 0 };

    if mob.current_vita == 0 {
        let data: *mut MobDbData = mob.data;

        sl_doscript_2(carray_to_str(unsafe { &(*data).yname }), Some("before_death"), mob.id, sd_bl_id);
        sl_doscript_2("before_death", None, mob.id, sd_bl_id);

        for x in 0..MAX_MAGIC_TIMERS {
            if mob.da[x].id > 0 && mob.da[x].duration > 0 {
                sl_doscript_2(carray_to_str(&magic_db::search(mob.da[x].id as i32).yname), Some("before_death_while_cast"), mob.id, bl_id);
            }
        }
    }

    if mob.current_vita == 0 {
        unsafe { mob_flushmagic(mob as *mut MobSpawnData); }
        clif_mob_kill(mob).await;

        if !tmob.is_null() && mob.summon == 0 {
            unsafe {
                let tmob_bl_id = (*tmob).id;
                for x in 0..MAX_MAGIC_TIMERS {
                    if (*tmob).da[x].id > 0 && (*tmob).da[x].duration > 0 {
                        sl_doscript_2(carray_to_str(&magic_db::search((*tmob).da[x].id as i32).yname), Some("on_kill_while_cast"), tmob_bl_id, mob.id);
                    }
                }
            }
        }

        if !sd.is_null() && mob.summon == 0 {
            unsafe {
                if tmob.is_null() {
                    let sd_ref = &*sd;
                    for x in 0..MAX_MAGIC_TIMERS {
                        if sd_ref.player.spells.dura_aether[x].id > 0 && sd_ref.player.spells.dura_aether[x].duration > 0 {
                            sl_doscript_2(carray_to_str(&magic_db::search(sd_ref.player.spells.dura_aether[x].id as i32).yname), Some("on_kill_while_cast"), sd_bl_id, mob.id);
                        }
                    }

                    for x in 0..MAX_SPELLS {
                        if sd_ref.player.spells.skills[x] > 0 {
                            sl_doscript_2(carray_to_str(&magic_db::search(sd_ref.player.spells.skills[x] as i32).yname), Some("passive_on_kill"), sd_bl_id, mob.id);
                        }
                    }
                }

                for x in 0..MAX_THREATCOUNT {
                    if mob.dmggrptable[x][1] / mob.maxdmg > mob.dmgindtable[x][1] / mob.maxdmg {
                        // handled by addtokillreg selection below
                    }
                }

                // find dominant damage dealer for drops
                let mut dropid: u32 = 0;
                let mut dmgpct: f64 = 0.0;
                let mut droptype: u8 = 0;

                for x in 0..MAX_THREATCOUNT {
                    if mob.dmggrptable[x][1] / mob.maxdmg > dmgpct {
                        dropid = mob.dmggrptable[x][0] as u32;
                        dmgpct = mob.dmggrptable[x][1] / mob.maxdmg;
                    }
                    if mob.dmgindtable[x][1] / mob.maxdmg > dmgpct {
                        dropid = mob.dmgindtable[x][0] as u32;
                        dmgpct = mob.dmgindtable[x][1] / mob.maxdmg;
                        droptype = 1;
                    }
                }

                let tsd2: *mut MapSessionData = if droptype == 1 {
                    map_id2sd_local(dropid)
                } else {
                    map_id2sd_local(groups()[dropid as usize * 256])
                };

                if !tsd2.is_null() {
                    crate::game::mob::mobdb_drops(mob as *mut MobSpawnData, tsd2);
                } else {
                    crate::game::mob::mobdb_drops(mob as *mut MobSpawnData, sd);
                }

                if (*sd).group_count == 0 {
                    if (*mob.data).exp > 0 {
                        addtokillreg(sd, mob.mobid as i32);
                    }
                } else {
                    let sd_id = (*sd).id;
                    if let Some(sd_arc) = map_id2sd_arc(sd_id) {
                        clif_addtokillreg(&sd_arc, mob.mobid as i32);
                    }
                }

                sl_doscript_2("onGetExp", None, (*sd).id, mob.id);

                if (*sd).group_count == 0 {
                    pc_checklevel(sd);
                } else {
                    for x in 0..(*sd).group_count as usize {
                        let tsdg = map_id2sd_local(groups()[(*sd).groupid as usize * 256 + x]);
                        if tsdg.is_null() { continue; }
                        if (*tsdg).m == (*sd).m && (*tsdg).player.combat.state != 1 {
                            pc_checklevel(tsdg);
                        }
                    }
                }

                sl_doscript_2("onKill", None, mob.id, sd_bl_id);
            }
        }

        for x in 0..MAX_MAGIC_TIMERS {
            if mob.da[x].id > 0 {
                sl_doscript_2(carray_to_str(&magic_db::search(mob.da[x].id as i32).yname), Some("after_death_while_cast"), mob.id, bl_id);
            }
        }

        {
            let data: *mut MobDbData = mob.data;
            sl_doscript_2(carray_to_str(unsafe { &(*data).yname }), Some("after_death"), mob.id, bl_id);
            sl_doscript_2("after_death", None, mob.id, sd_bl_id);
        }
    }

    0
}

// ─── clif_mob_kill ────────────────────────────────────────────────────────────

/// Mark a mob as dead, clear threat tables, broadcast despawn packets.
///
pub async fn clif_mob_kill(mob: &mut MobSpawnData) -> i32 {
    for x in 0..MAX_THREATCOUNT {
        mob.threat[x].user   = 0;
        mob.threat[x].amount = 0;
        mob.dmggrptable[x][0] = 0.0;
        mob.dmggrptable[x][1] = 0.0;
        mob.dmgindtable[x][0] = 0.0;
        mob.dmgindtable[x][1] = 0.0;
    }

    mob.dmgdealt = 0.0;
    mob.dmgtaken = 0.0;
    unsafe { mob.maxdmg = (*mob.data).vita as f64; }
    mob.state = MOB_DEAD;
    mob.last_death = unsafe { libc_time() } as u32;

    if mob.onetime == 0 {
        unsafe {
            crate::game::map_server::map_lastdeath_mob(mob as *mut MobSpawnData).await;
        }
    }

    let mob_ptr = mob as *mut MobSpawnData;
    unsafe {
        if let Some(grid) = block_grid::get_grid(mob.m as usize) {
            let slot = &*crate::database::map_db::raw_map_ptr().add(mob.m as usize);
            let ids = block_grid::ids_in_area(grid, mob.x as i32, mob.y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
            for id in ids {
                if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                    let pc = pc_arc.read();
                    clif_send_destroy_inner(&pc, mob_ptr);
                }
            }
        }
    }

    0
}

// ─── clif_send_destroy_inner ──────────────────────────────────────────────────

/// Send despawn packet for a mob to one player.
/// `sd` is the receiving player, `mob` is the mob being despawned.
///
/// Note: `mob` is kept as `*const MobSpawnData` rather than `&MobSpawnData`
/// because the call site captures it as `mob_ptr` from `foreach_in_area`. The
/// borrow checker cannot simultaneously allow `&mut *mob` in the closure AND
/// use `mob.m/x/y` as the area arguments to `foreach_in_area` in the same
/// expression — both would require a mutable borrow of `mob`.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_send_destroy_inner(sd: &MapSessionData, mob: *const MobSpawnData) -> i32 {
    if !session_exists(sd.fd) {
        return 0;
    }

    unsafe {
        let fd = sd.fd;
        let data: *mut MobDbData = (*mob).data;
        let packet_id: u8 = if (*data).mobtype == 1 { 0x0E } else { 0x5F };

        wfifohead(fd, 9);
        wfifob(fd, 0, 0xAA);
        wfifow(fd, 1, 6u16.swap_bytes());
        wfifob(fd, 3, packet_id);
        wfifob(fd, 4, 0x03);
        wfifol(fd, 5, (*mob).id.swap_bytes());
        wfifoset(fd, encrypt(fd) as usize);
    }

    0
}

// ─── clif_sendmagic ───────────────────────────────────────────────────────────

/// Send a spell slot packet to the player.
///
pub fn clif_sendmagic(sd: &mut MapSessionData, pos: i32) -> i32 {
    unsafe {
        let id   = sd.player.spells.skills[pos as usize] as i32;
        let spell = magic_db::search(id);
        let name = spell.name.as_ptr();
        let question = spell.question.as_ptr();
        let spell_type = spell.typ;

        if !session_exists(sd.fd) {
            return 0;
        }

        let name_len     = cstr_len(name as *const u8);
        let question_len = cstr_len(question as *const u8);

        let fd = sd.fd;
        wfifohead(fd, 255);
        wfifob(fd, 0, 0xAA);
        wfifob(fd, 3, 0x17);
        wfifob(fd, 5, (pos + 1) as u8);
        wfifob(fd, 6, spell_type as u8);
        wfifob(fd, 7, name_len as u8);
        {
            let dst = wfifop(fd, 8);
            if !dst.is_null() {
                std::ptr::copy_nonoverlapping(name as *const u8, dst, name_len);
            }
            let dst2 = wfifop(fd, 8 + name_len);
            if !dst2.is_null() { *dst2 = question_len as u8; }
            let dst3 = wfifop(fd, 9 + name_len);
            if !dst3.is_null() {
                std::ptr::copy_nonoverlapping(question as *const u8, dst3, question_len);
            }
        }

        let total_len = name_len + question_len + 1;
        wfifow(fd, 1, ((total_len + 5) as u16).swap_bytes());
        wfifoset(fd, encrypt(fd) as usize);
    }
    0
}

// ─── clif_parsemagic ──────────────────────────────────────────────────────────

/// Handle incoming spell cast packet from client.
///
pub fn clif_parsemagic(sd: &mut MapSessionData) -> i32 {
    use crate::game::map_parse::packet::{rfifob, rfifol, rfifop};

    let pos = (rfifob(sd.fd, 5) as i32) - 1;
    let spell = magic_db::search(sd.player.spells.skills[pos as usize] as i32);

    let i = clif_has_aethers(sd, sd.player.spells.skills[pos as usize] as i32);
    if i > 0 {
        let time = i / 1000;
        sl_doscript_simple(carray_to_str(&spell.yname), Some("on_aethers"), sd.id);
        let mut msg = [0u8; 64];
        let s = format!("Wait {} second(s) for aethers to settle.", time);
        let sb = s.as_bytes();
        let copy_len = sb.len().min(63);
        msg[..copy_len].copy_from_slice(&sb[..copy_len]);
        if let Some(sd_arc) = map_id2sd_arc(sd.id) {
            unsafe { clif_sendminitext(&sd_arc, msg.as_ptr() as *const i8); }
        }
        return 0;
    }

    if sd.silence > 0 && spell.mute as i32 <= sd.silence {
        sl_doscript_simple(carray_to_str(&spell.yname), Some("on_mute"), sd.id);
        if let Some(sd_arc) = map_id2sd_arc(sd.id) {
            unsafe { clif_sendminitext(&sd_arc, c"You have been silenced.".as_ptr()); }
        }
        return 0;
    }

    sd.target   = 0;
    sd.attacker = 0;

    match spell.typ {
        1 => {
            // question type
            let dst = sd.question.as_mut_ptr() as *mut u8;
            unsafe {
                std::ptr::write_bytes(dst, 0, 64);
                let src_ptr = rfifop(sd.fd, 6);
                if !src_ptr.is_null() {
                    let mut n = 0usize;
                    while n < 63 && *src_ptr.add(n) != 0 {
                        *dst.add(n) = *src_ptr.add(n);
                        n += 1;
                    }
                }
            }
        }
        2 => {
            // target type
            let raw_id = rfifol(sd.fd, 6);
            let target_id = u32::from_be(raw_id); // SWAP32
            sd.target   = target_id as i32;
            sd.attacker = target_id;
        }
        5 => {
            // self type — no extra data
        }
        _ => {
            return 0;
        }
    }

    sl_doscript_simple("onCast", None, sd.id);

    if sd.target != 0 {
        let target_id = sd.target as u32;
        let Some((tpos, tbl_type)) = crate::game::map_server::entity_position(target_id) else { return 0; };
        let tbl_type = tbl_type as i32;

        // Check stealth for PC targets
        if tbl_type == BL_PC {
            let tsd2 = map_id2sd_local(target_id);
            if !tsd2.is_null() && unsafe { (*tsd2).optFlags } & crate::game::pc::OPT_FLAG_STEALTH != 0 {
                return 0;
            }
        }

        let one = (tpos.m as i32, tpos.x as i32, tpos.y as i32);
        let two = (sd.m as i32, sd.x as i32, sd.y as i32);

        if crate::game::util::check_proximity(one, two, 21) {
            let mut health: i64 = 0;
            let mut twill: i32 = 0;
            let mut tprotection: i32 = 0;

            if tbl_type == BL_PC {
                let tsd2 = map_id2sd_local(target_id);
                if !tsd2.is_null() {
                    unsafe {
                        health = (*tsd2).player.combat.hp as i64;
                        twill = (*tsd2).will;
                        tprotection = (*tsd2).protection as i32;
                    }
                }
            } else if tbl_type == BL_MOB {
                let tmob = map_id2mob_local(target_id);
                if !tmob.is_null() {
                    unsafe {
                        health = (*tmob).current_vita as i64;
                        twill = (*tmob).will;
                        tprotection = (*tmob).protection as i32;
                    }
                }
            }

            if spell.canfail as i32 == 1 {
                let will_diff = (twill - sd.will).max(0);
                let prot = (tprotection + (will_diff + 5) / 10).max(0);
                let fail_chance = (100.0f64 - (0.9f64.powi(prot) * 100.0f64) + 0.5f64) as i32;
                let cast_test = rnd(100);
                if cast_test < fail_chance {
                    if let Some(sd_arc) = map_id2sd_arc(sd.id) {
                        unsafe { clif_sendminitext(&sd_arc, c"The magic has been deflected.".as_ptr()); }
                    }
                    return 0;
                }
            }

            if health > 0 || tbl_type == BL_PC {
                unsafe {
                    sl_async_freeco(sd as *mut MapSessionData);
                    sl_doscript_2(carray_to_str(&spell.yname), Some("cast"), sd.id, target_id);
                }
            }
        }
    } else {
        unsafe { sl_async_freeco(sd as *mut MapSessionData); }
        sl_doscript_2(carray_to_str(&spell.yname), Some("cast"), sd.id, 0);
    }

    0
}

// ─── clif_sendaction ──────────────────────────────────────────────────────────

/// Broadcast a PC action animation to the area, optionally play a sound.
///
pub fn clif_sendaction_pc(sd: &mut MapSessionData, action_type: i32, time: i32, sound: i32) -> i32 {
    let mut buf = [0u8; 32];
    buf[0] = 0xAA;
    buf[1] = 0x00;
    buf[2] = 0x0B;
    buf[3] = 0x1A;
    let blid = sd.id;
    buf[5] = (blid >> 24) as u8;
    buf[6] = (blid >> 16) as u8;
    buf[7] = (blid >>  8) as u8;
    buf[8] = blid as u8;
    buf[9]  = action_type as u8;
    buf[10] = 0x00;
    buf[11] = time as u8;
    buf[12] = 0x00;

    tracing::debug!("[attack] clif_sendaction: id={} action={} time={} sound={} m={} x={} y={}",
        sd.id, action_type, time, sound, sd.m, sd.x, sd.y);
    unsafe { clif_send(buf.as_ptr(), 32, BroadcastSrc { id: sd.id, m: sd.m, x: sd.x, y: sd.y, bl_type: BL_PC as u8 }, SAMEAREA); }

    if sound > 0 {
        unsafe { clif_playsound_entity(sd.id, sd.m, sd.x, sd.y, BL_PC as u8, sound); }
    }

    sd.action = action_type as i8;
    sl_doscript_simple("onAction", None, sd.id);

    0
}

// ─── clif_sendmob_action ──────────────────────────────────────────────────────

/// Broadcast a mob action animation to the area, optionally play a sound.
///
pub fn clif_sendmob_action(mob: &mut MobSpawnData, action_type: i32, time: i32, sound: i32) -> i32 {
    let mut buf = [0u8; 32];
    buf[0] = 0xAA;
    buf[1] = 0x00;
    buf[2] = 0x0B;
    buf[3] = 0x1A;
    buf[4] = 0x03;
    let blid = mob.id;
    buf[5] = (blid >> 24) as u8;
    buf[6] = (blid >> 16) as u8;
    buf[7] = (blid >>  8) as u8;
    buf[8] = blid as u8;
    buf[9]  = action_type as u8;
    buf[10] = 0x00;
    buf[11] = time as u8;
    buf[12] = 0x00;

    unsafe {
        clif_send(buf.as_ptr(), 32, BroadcastSrc { id: mob.id, m: mob.m, x: mob.x, y: mob.y, bl_type: BL_MOB as u8 }, SAMEAREA);
    }

    if sound > 0 {
        unsafe { clif_playsound_entity(mob.id, mob.m, mob.x, mob.y, BL_MOB as u8, sound); }
    }

    0
}

// ─── clif_sendanimation_xy ────────────────────────────────────────────────────

///
/// Send a positional animation packet to one player.
/// `bl` is the receiving player.
///
/// Send an XY animation packet to a single player.
/// `sd` is the receiving player.
pub fn clif_sendanimation_xy_inner(sd: &MapSessionData, anim: i32, times: i32, x: i32, y: i32) -> i32 {
    if !session_exists(sd.fd) {
        return 0;
    }

    let fd = sd.fd;
    wfifohead(fd, 0x30);
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, 14u16.swap_bytes());
    wfifob(fd, 3, 0x29);
    wfifol(fd, 5, 0);
    wfifow(fd, 9,  (anim  as u16).swap_bytes());
    wfifow(fd, 11, (times as u16).swap_bytes());
    wfifow(fd, 13, (x     as u16).swap_bytes());
    wfifow(fd, 15, (y     as u16).swap_bytes());
    wfifoset(fd, unsafe { encrypt(fd) } as usize);
    0
}

// ─── clif_sendanimation ───────────────────────────────────────────────────────

/// Send animation for a target to one player.
/// `fd` is the receiving player's session, `anim` is the animation ID,
/// `times` is the loop count (pass -1 for duration-based).
pub fn clif_sendanimation_inner(fd: SessionId, setting_flags: u32, anim: i32, target_id: u32, times: i32) -> i32 {
    if target_id == 0 { return 0; }

    unsafe {
        if setting_flags & FLAG_MAGIC != 0 {
            if !session_exists(fd) {
                return 0;
            }

            wfifohead(fd, 13);
            wfifob(fd, 0, 0xAA);
            wfifow(fd, 1, 0x000Au16.swap_bytes());
            wfifob(fd, 3, 0x29);
            wfifol(fd, 5, target_id.swap_bytes());
            wfifow(fd, 9,  (anim  as u16).swap_bytes());
            wfifow(fd, 11, (times as u16).swap_bytes());
            wfifoset(fd, encrypt(fd) as usize);
        }
    }

    0
}

// ─── clif_animation ───────────────────────────────────────────────────────────

/// Send animation for `sd`'s block_list to `src`'s socket.
///
/// Send a spell animation packet for `entity` to `viewer`.
pub fn clif_animation(
    viewer: &PlayerEntity,
    entity: &PlayerEntity,
    animation: i32,
    duration: i32,
) -> i32 {
    if !session_exists(entity.fd) {
        return 0;
    }

    let setting_flags = viewer.read().player.appearance.setting_flags;
    unsafe {
        let fd = viewer.fd;
        wfifohead(fd, 0x0A + 3);
        if setting_flags & FLAG_MAGIC != 0 {
            wfifob(fd, 0, 0xAA);
            wfifow(fd, 1, 0x000Au16.swap_bytes());
            wfifob(fd, 3, 0x29);
            wfifob(fd, 4, 0x03);
            wfifol(fd, 5, entity.id.swap_bytes());
            wfifow(fd, 9,  (animation as u16).swap_bytes());
            wfifow(fd, 11, ((duration / 1000) as u16).swap_bytes());
            wfifoset(fd, encrypt(fd) as usize);
        }
    }
    0
}

// ─── clif_sendanimations ──────────────────────────────────────────────────────

/// Send all active aether spell animations from `entity` to `viewer`.
pub fn clif_sendanimations(viewer: &PlayerEntity, entity: &PlayerEntity) -> i32 {
    let guard = entity.read();
    for x in 0..MAX_MAGIC_TIMERS {
        let aether = &guard.player.spells.dura_aether[x];
        if aether.duration > 0 && aether.animation != 0 {
            clif_animation(viewer, entity, aether.animation as i32, aether.duration);
        }
    }
    0
}

// ─── clif_parseattack ─────────────────────────────────────────────────────────

/// Handle a melee attack swing from the client.
///
pub fn clif_parseattack(sd: &mut MapSessionData) -> i32 {
    let attackspeed = sd.attack_speed as i32;

    if sd.paralyzed != 0 || sd.sleep != 1.0f32 {
        tracing::warn!("[attack] clif_parseattack BLOCKED: paralyzed={} sleep={}", sd.paralyzed, sd.sleep);
        return 0;
    }

    if sd.player.combat.state == 1 || sd.player.combat.state == 3 {
        tracing::warn!("[attack] clif_parseattack BLOCKED: state={}", sd.player.combat.state);
        return 0;
    }
    tracing::debug!("[attack] clif_parseattack PASS: id={} atkspd={} state={}", sd.id, attackspeed, sd.player.combat.state);

    let weap_id = sd.player.inventory.equip[EQ_WEAP as usize].id;
    let weap_item = item_db::search(weap_id);
    let sound = weap_item.sound as i32;

    if sound == 0 {
        clif_sendaction_pc(sd, 1, attackspeed, 9);
    } else {
        clif_sendaction_pc(sd, 1, attackspeed, sound);
    }

    sl_doscript_simple("swingDamage", None, sd.id);
    sl_doscript_simple("swing", None, sd.id);
    sl_doscript_simple("onSwing", None, sd.id);

    let weap_look = weap_item.look;
    if (20000..30000).contains(&weap_look) {
        sl_doscript_simple(carray_to_str(&weap_item.yname), Some("shootArrow"), sd.id);
        sl_doscript_simple("shootArrow", None, sd.id);
    }

    for x in 0..14usize {
        if sd.player.inventory.equip[x].id > 0 {
            sl_doscript_simple(carray_to_str(&item_db::search(sd.player.inventory.equip[x].id).yname), Some("on_swing"), sd.id);
        }
    }

    for x in 0..MAX_SPELLS {
        if sd.player.spells.skills[x] > 0 {
            sl_doscript_simple(carray_to_str(&magic_db::search(sd.player.spells.skills[x] as i32).yname), Some("passive_on_swing"), sd.id);
        }
    }

    for x in 0..MAX_MAGIC_TIMERS {
        if sd.player.spells.dura_aether[x].id > 0 && sd.player.spells.dura_aether[x].duration > 0 {
            sl_doscript_simple(carray_to_str(&magic_db::search(sd.player.spells.dura_aether[x].id as i32).yname), Some("on_swing_while_cast"), sd.id);
        }
    }

    0
}

// ─── clif_deductdura ─────────────────────────────────────────────────────────

/// Reduce durability of an equipment slot by `val`. Checks pvp map and ethereal flag.
///
pub fn clif_deductdura(sd: &mut MapSessionData, equip: i32, val: i32) -> i32 {
    let equip_idx = equip as usize;
    if sd.player.inventory.equip[equip_idx].id == 0 { return 0; }

    let m = sd.m as usize;
    if unsafe { (*raw_map_ptr().add(m)).pvp } != 0 { return 0; }

    let eth = item_db::search(sd.player.inventory.equip[equip_idx].id).ethereal as i32;
    if eth == 0 {
        sd.player.inventory.equip[equip_idx].dura -= val;
        clif_checkdura(sd, equip);
    }
    0
}

// ─── clif_deductweapon ───────────────────────────────────────────────────────

/// Randomly reduce weapon durability by `hit`.
///
pub fn clif_deductweapon(sd: &mut MapSessionData, hit: i32) -> i32 {
    if unsafe { pc_isequip(sd as *mut MapSessionData, EQ_WEAP) } != 0
        && rnd(100) > 50 {
            clif_deductdura(sd, EQ_WEAP, hit);
        }
    0
}

// ─── clif_deductarmor ────────────────────────────────────────────────────────

/// Randomly reduce durability of all armor slots by `hit`.
///
pub fn clif_deductarmor(sd: &mut MapSessionData, hit: i32) -> i32 {
    macro_rules! maybe_deduct {
        ($slot:expr) => {
            if unsafe { pc_isequip(sd as *mut MapSessionData, $slot) } != 0 && rnd(100) > 50 {
                clif_deductdura(sd, $slot, hit);
            }
        };
    }
    maybe_deduct!(EQ_WEAP);
    maybe_deduct!(EQ_HELM);
    maybe_deduct!(EQ_ARMOR);
    maybe_deduct!(EQ_LEFT);
    maybe_deduct!(EQ_RIGHT);
    maybe_deduct!(EQ_SUBLEFT);
    maybe_deduct!(EQ_SUBRIGHT);
    maybe_deduct!(EQ_BOOTS);
    maybe_deduct!(EQ_MANTLE);
    maybe_deduct!(EQ_COAT);
    maybe_deduct!(EQ_SHIELD);
    maybe_deduct!(EQ_FACEACC);
    maybe_deduct!(EQ_CROWN);
    maybe_deduct!(EQ_NECKLACE);
    0
}

// ─── clif_checkdura ──────────────────────────────────────────────────────────

/// Check durability thresholds and handle item destruction.
///
pub fn clif_checkdura(sd: &mut MapSessionData, equip: i32) -> i32 {
    let equip_idx = equip as usize;
    if sd.player.inventory.equip[equip_idx].id == 0 { return 0; }

    let id = sd.player.inventory.equip[equip_idx].id;
    let item = item_db::search(id);
    sd.equipslot = equip as u8;

    let max_dura = item.dura as f32;
    let cur_dura = sd.player.inventory.equip[equip_idx].dura as f32;
    let percentage = cur_dura / max_dura;

    let mut msg_buf = [0i8; 255];

    let sd_arc = map_id2sd_arc(sd.id);
    if percentage <= 0.5 && sd.player.inventory.equip[equip_idx].repair == 0 {
        unsafe { format_dura_msg(&mut msg_buf, item.name.as_ptr() as *mut i8, "50"); }
        if let Some(ref arc) = sd_arc { unsafe { clif_sendmsg(arc, 5, msg_buf.as_ptr()); } }
        sd.player.inventory.equip[equip_idx].repair = 1;
    }
    if percentage <= 0.25 && sd.player.inventory.equip[equip_idx].repair == 1 {
        unsafe { format_dura_msg(&mut msg_buf, item.name.as_ptr() as *mut i8, "25"); }
        if let Some(ref arc) = sd_arc { unsafe { clif_sendmsg(arc, 5, msg_buf.as_ptr()); } }
        sd.player.inventory.equip[equip_idx].repair = 2;
    }
    if percentage <= 0.1 && sd.player.inventory.equip[equip_idx].repair == 2 {
        unsafe { format_dura_msg(&mut msg_buf, item.name.as_ptr() as *mut i8, "10"); }
        if let Some(ref arc) = sd_arc { unsafe { clif_sendmsg(arc, 5, msg_buf.as_ptr()); } }
        sd.player.inventory.equip[equip_idx].repair = 3;
    }
    if percentage <= 0.05 && sd.player.inventory.equip[equip_idx].repair == 3 {
        unsafe { format_dura_msg(&mut msg_buf, item.name.as_ptr() as *mut i8, "5"); }
        if let Some(ref arc) = sd_arc { unsafe { clif_sendmsg(arc, 5, msg_buf.as_ptr()); } }
        sd.player.inventory.equip[equip_idx].repair = 4;
    }
    if percentage <= 0.01 && sd.player.inventory.equip[equip_idx].repair == 4 {
        unsafe { format_dura_msg(&mut msg_buf, item.name.as_ptr() as *mut i8, "1"); }
        if let Some(ref arc) = sd_arc { unsafe { clif_sendmsg(arc, 5, msg_buf.as_ptr()); } }
        sd.player.inventory.equip[equip_idx].repair = 5;
    }

    let broken = sd.player.inventory.equip[equip_idx].dura <= 0
        || (sd.player.combat.state == 1 && item.bod == 1);

    if broken {
        if item.protected != 0
            || sd.player.inventory.equip[equip_idx].protected >= 1
        {
            sd.player.inventory.equip[equip_idx].protected = sd.player.inventory.equip[equip_idx].protected.saturating_sub(1);
            sd.player.inventory.equip[equip_idx].dura = item.dura;
            unsafe { format_restore_msg(&mut msg_buf, item.name.as_ptr() as *mut i8); }
            if let Some(ref arc) = sd_arc {
                unsafe { clif_sendstatus(arc, SFLAG_FULLSTATS | SFLAG_HPMP); }
                unsafe { clif_sendmsg(arc, 5, msg_buf.as_ptr()); }
            }
            sl_doscript_simple("characterLog", Some("equipRestore"), sd.id);
            return 0;
        }

        sl_doscript_simple("characterLog", Some("equipBreak"), sd.id);
        unsafe { format_destroy_msg(&mut msg_buf, item.name.as_ptr() as *mut i8); }

        sd.breakid = id;
        sl_doscript_simple("onBreak", None, sd.id);
        sl_doscript_simple(carray_to_str(&item.yname), Some("on_break"), sd.id);

        sd.player.inventory.equip[equip_idx].id              = 0;
        sd.player.inventory.equip[equip_idx].dura            = 0;
        sd.player.inventory.equip[equip_idx].amount          = 0;
        sd.player.inventory.equip[equip_idx].protected       = 0;
        sd.player.inventory.equip[equip_idx].owner           = 0;
        sd.player.inventory.equip[equip_idx].custom          = 0;
        sd.player.inventory.equip[equip_idx].custom_look      = 0;
        sd.player.inventory.equip[equip_idx].custom_look_color = 0;
        sd.player.inventory.equip[equip_idx].custom_icon     = 0;
        sd.player.inventory.equip[equip_idx].custom_icon_color = 0;
        sd.player.inventory.equip[equip_idx].time            = 0;
        sd.player.inventory.equip[equip_idx].repair          = 0;
        sd.player.inventory.equip[equip_idx].real_name[0]    = 0;

        if let Some(ref arc) = sd_arc {
            unsafe {
                clif_unequipit(arc, clif_getequiptype(equip));
                broadcast_update_state(arc);
                pc_calcstat(arc);
                clif_sendstatus(arc, SFLAG_FULLSTATS | SFLAG_HPMP);
                clif_sendmsg(arc, 5, msg_buf.as_ptr());
            }
        }
    }

    0
}

// ─── clif_deductduraequip ────────────────────────────────────────────────────

/// Reduce durability of all equipped items by 10% of max, checking thresholds.
///
pub fn clif_deductduraequip(sd: &mut MapSessionData) -> i32 {
    let m = sd.m as usize;
    if unsafe { (*raw_map_ptr().add(m)).pvp } != 0 { return 0; }

    for equip in 0..14usize {
        if sd.player.inventory.equip[equip].id == 0 { continue; }
        let id = sd.player.inventory.equip[equip].id;
        let item = item_db::search(id);

        let eth = item.ethereal as i32;
        if eth != 0 { continue; }

        sd.equipslot = equip as u8;

        let deduct = (item.dura as f64 * 0.10).floor() as i32;
        sd.player.inventory.equip[equip].dura -= deduct;

        let max_dura = item.dura as f32;
        let cur_dura = sd.player.inventory.equip[equip].dura as f32;
        let percentage = cur_dura / max_dura;

        let mut msg_buf = [0i8; 255];
        let sd_arc = map_id2sd_arc(sd.id);

        if percentage <= 0.5 && sd.player.inventory.equip[equip].repair == 0 {
            unsafe { format_dura_msg(&mut msg_buf, item.name.as_ptr() as *mut i8, "50"); }
            if let Some(ref arc) = sd_arc { unsafe { clif_sendmsg(arc, 5, msg_buf.as_ptr()); } }
            sd.player.inventory.equip[equip].repair = 1;
        }
        if percentage <= 0.25 && sd.player.inventory.equip[equip].repair == 1 {
            unsafe { format_dura_msg(&mut msg_buf, item.name.as_ptr() as *mut i8, "25"); }
            if let Some(ref arc) = sd_arc { unsafe { clif_sendmsg(arc, 5, msg_buf.as_ptr()); } }
            sd.player.inventory.equip[equip].repair = 2;
        }
        if percentage <= 0.1 && sd.player.inventory.equip[equip].repair == 2 {
            unsafe { format_dura_msg(&mut msg_buf, item.name.as_ptr() as *mut i8, "10"); }
            if let Some(ref arc) = sd_arc { unsafe { clif_sendmsg(arc, 5, msg_buf.as_ptr()); } }
            sd.player.inventory.equip[equip].repair = 3;
        }
        if percentage <= 0.05 && sd.player.inventory.equip[equip].repair == 3 {
            unsafe { format_dura_msg(&mut msg_buf, item.name.as_ptr() as *mut i8, "5"); }
            if let Some(ref arc) = sd_arc { unsafe { clif_sendmsg(arc, 5, msg_buf.as_ptr()); } }
            sd.player.inventory.equip[equip].repair = 4;
        }
        if percentage <= 0.01 && sd.player.inventory.equip[equip].repair == 4 {
            unsafe { format_dura_msg(&mut msg_buf, item.name.as_ptr() as *mut i8, "1"); }
            if let Some(ref arc) = sd_arc { unsafe { clif_sendmsg(arc, 5, msg_buf.as_ptr()); } }
            sd.player.inventory.equip[equip].repair = 5;
        }

        let broken = sd.player.inventory.equip[equip].dura <= 0
            || (sd.player.combat.state == 1 && item.bod == 1);

        if broken {
            if item.protected != 0
                || sd.player.inventory.equip[equip].protected >= 1
            {
                sd.player.inventory.equip[equip].protected = sd.player.inventory.equip[equip].protected.saturating_sub(1);
                sd.player.inventory.equip[equip].dura = item.dura;
                unsafe { format_restore_msg(&mut msg_buf, item.name.as_ptr() as *mut i8); }
                if let Some(ref arc) = sd_arc {
                    unsafe { clif_sendstatus(arc, SFLAG_FULLSTATS | SFLAG_HPMP); }
                    unsafe { clif_sendmsg(arc, 5, msg_buf.as_ptr()); }
                }
                sl_doscript_simple("characterLog", Some("equipRestore"), sd.id);
                continue;
            }

            // copy broken item to boditems
            let bod_count = sd.boditems.bod_count as usize;
            if bod_count < sd.boditems.item.len() {
                sd.boditems.item[bod_count] = sd.player.inventory.equip[equip];
                sd.boditems.bod_count += 1;
            }

            sl_doscript_simple("characterLog", Some("equipBreak"), sd.id);
            unsafe { format_destroy_msg(&mut msg_buf, item.name.as_ptr() as *mut i8); }

            sd.breakid = id;
            sl_doscript_simple("onBreak", None, sd.id);
            sl_doscript_simple(carray_to_str(&item.yname), Some("on_break"), sd.id);

            sd.player.inventory.equip[equip].id              = 0;
            sd.player.inventory.equip[equip].dura            = 0;
            sd.player.inventory.equip[equip].amount          = 0;
            sd.player.inventory.equip[equip].protected       = 0;
            sd.player.inventory.equip[equip].owner           = 0;
            sd.player.inventory.equip[equip].custom          = 0;
            sd.player.inventory.equip[equip].custom_look      = 0;
            sd.player.inventory.equip[equip].custom_look_color = 0;
            sd.player.inventory.equip[equip].custom_icon     = 0;
            sd.player.inventory.equip[equip].custom_icon_color = 0;
            sd.player.inventory.equip[equip].time            = 0;
            sd.player.inventory.equip[equip].repair          = 0;
            sd.player.inventory.equip[equip].real_name[0]    = 0;

            if let Some(ref arc) = sd_arc {
                unsafe {
                    clif_unequipit(arc, clif_getequiptype(equip as i32));
                    broadcast_update_state(arc);
                    pc_calcstat(arc);
                    clif_sendstatus(arc, SFLAG_FULLSTATS | SFLAG_HPMP);
                    clif_sendmsg(arc, 5, msg_buf.as_ptr());
                }
            }
        }
    }

    sl_doscript_simple("characterLog", Some("bodLog"), sd.id);
    sd.boditems.bod_count = 0;

    0
}

// ─── Private helpers ──────────────────────────────────────────────────────────

/// Return the length of a null-terminated byte string (does not count the NUL).
#[inline]
unsafe fn cstr_len(ptr: *const u8) -> usize {
    if ptr.is_null() { return 0; }
    let mut n = 0usize;
    while *ptr.add(n) != 0 { n += 1; }
    n
}

/// Return a byte slice for a null-terminated C string (not including NUL).
#[inline]
unsafe fn cstr_bytes<'a>(ptr: *const u8) -> &'a [u8] {
    if ptr.is_null() { return b""; }
    let len = cstr_len(ptr);
    std::slice::from_raw_parts(ptr, len)
}

/// Thin wrapper around libc `time(NULL)`.
#[inline]
unsafe fn libc_time() -> u64 {
    libc::time(std::ptr::null_mut()) as u64
}

/// Write "Your <name> is at <pct>%." into buf (C sprintf equivalent).
#[inline]
unsafe fn format_dura_msg(buf: &mut [i8; 255], name: *mut i8, pct: &str) {
    let prefix = b"Your ";
    let middle = b" is at ";
    let suffix_pct = pct.as_bytes();
    let suffix_end = b"%.";
    let mut pos = 0usize;
    for &b in prefix { if pos < 254 { buf[pos] = b as i8; pos += 1; } }
    if !name.is_null() {
        let mut i = 0usize;
        while pos < 254 && *(name.add(i)) != 0 { buf[pos] = *(name.add(i)); pos += 1; i += 1; }
    }
    for &b in middle { if pos < 254 { buf[pos] = b as i8; pos += 1; } }
    for &b in suffix_pct { if pos < 254 { buf[pos] = b as i8; pos += 1; } }
    for &b in suffix_end { if pos < 254 { buf[pos] = b as i8; pos += 1; } }
    buf[pos] = 0;
}

/// Write "Your <name> has been restored!" into buf.
#[inline]
unsafe fn format_restore_msg(buf: &mut [i8; 255], name: *mut i8) {
    let prefix = b"Your ";
    let suffix = b" has been restored!";
    let mut pos = 0usize;
    for &b in prefix { if pos < 254 { buf[pos] = b as i8; pos += 1; } }
    if !name.is_null() {
        let mut i = 0usize;
        while pos < 254 && *(name.add(i)) != 0 { buf[pos] = *(name.add(i)); pos += 1; i += 1; }
    }
    for &b in suffix { if pos < 254 { buf[pos] = b as i8; pos += 1; } }
    buf[pos] = 0;
}

/// Write "Your <name> was destroyed!" into buf.
#[inline]
unsafe fn format_destroy_msg(buf: &mut [i8; 255], name: *mut i8) {
    let prefix = b"Your ";
    let suffix = b" was destroyed!";
    let mut pos = 0usize;
    for &b in prefix { if pos < 254 { buf[pos] = b as i8; pos += 1; } }
    if !name.is_null() {
        let mut i = 0usize;
        while pos < 254 && *(name.add(i)) != 0 { buf[pos] = *(name.add(i)); pos += 1; i += 1; }
    }
    for &b in suffix { if pos < 254 { buf[pos] = b as i8; pos += 1; } }
    buf[pos] = 0;
}
