//! Port of the combat/magic/animation/durability helpers from `c_src/map_parse.c`.
//!
//! Functions declared `#[no_mangle] pub unsafe extern "C"` so they remain
//! callable from any remaining C code that has not yet been ported.

#![allow(non_snake_case, clippy::wildcard_imports)]

use std::ffi::{c_char, c_int, c_uint};

use crate::database::map_db::BlockList;
use crate::database::mob_db::MobDbData;
use crate::ffi::map_db::map;
use crate::ffi::session::{rust_session_exists, rust_session_set_eof};
use crate::game::mob::{MobSpawnData, MOB_DEAD, MAX_MAGIC_TIMERS, MAX_THREATCOUNT};
use crate::game::pc::{
    MapSessionData,
    BL_PC, BL_MOB,
    EQ_WEAP, EQ_ARMOR, EQ_SHIELD, EQ_HELM, EQ_LEFT, EQ_RIGHT,
    EQ_SUBLEFT, EQ_SUBRIGHT, EQ_FACEACC, EQ_CROWN, EQ_MANTLE, EQ_NECKLACE, EQ_BOOTS, EQ_COAT,
    SFLAG_HPMP, SFLAG_FULLSTATS,
    FLAG_MAGIC,
};
use crate::servers::char::charstatus::MAX_SPELLS;

use super::packet::{
    encrypt, wfifob, wfifohead, wfifol, wfifoset, wfifow, wfifoheader,
    clif_send, map_foreachinarea,
    AREA, SELF, SAMEAREA,
};

// enum { LOOK_GET = 0, LOOK_SEND = 1 } from map_parse.h
const LOOK_GET: c_int = 0;

// ─── C FFI: functions remaining in C ─────────────────────────────────────────

extern "C" {
    fn clif_sendstatus(sd: *mut MapSessionData, flags: c_int) -> c_int;
    fn clif_grouphealth_update(sd: *mut MapSessionData) -> c_int;
    fn clif_sendmsg(sd: *mut MapSessionData, t: c_int, msg: *const c_char) -> c_int;
    fn clif_sendminitext(sd: *mut MapSessionData, msg: *const c_char) -> c_int;
    fn clif_unequipit(sd: *mut MapSessionData, t: c_int) -> c_int;
    fn clif_getequiptype(val: c_int) -> c_int;
    fn clif_updatestate(bl: *mut BlockList, ...) -> c_int;
    fn clif_playsound(bl: *mut BlockList, sound: c_int) -> c_int;
    fn clif_isingroup(sd: *mut MapSessionData, tsd: *mut MapSessionData) -> c_int;
    fn map_lastdeath_mob(mob: *mut MobSpawnData) -> c_int;
    fn addtokillreg(sd: *mut MapSessionData, mob: c_int) -> c_int;
    fn clif_addtokillreg(sd: *mut MapSessionData, mob: c_int) -> c_int;
    fn map_id2bl(id: c_uint) -> *mut BlockList;
    fn map_id2sd(id: c_uint) -> *mut MapSessionData;
    fn sl_doscript_blargs(script: *const c_char, func: *const c_char, n: c_int, ...) -> c_int;
    fn sl_doscript_simple(script: *const c_char, func: *const c_char, bl: *mut BlockList) -> c_int;
    fn rust_pc_calcstat(sd: *mut MapSessionData) -> c_int;
    fn rust_pc_checklevel(sd: *mut MapSessionData) -> c_int;
    fn rust_pc_isequip(sd: *mut MapSessionData, t: c_int) -> c_int;
    fn rust_itemdb_name(id: c_uint) -> *mut c_char;
    fn rust_itemdb_yname(id: c_uint) -> *mut c_char;
    fn rust_itemdb_sound(id: c_uint) -> c_uint;
    fn rust_itemdb_soundhit(id: c_uint) -> c_uint;
    fn rust_itemdb_ethereal(id: c_uint) -> c_int;
    fn rust_itemdb_dura(id: c_uint) -> c_int;
    fn rust_itemdb_protected(id: c_uint) -> c_int;
    fn rust_itemdb_breakondeath(id: c_uint) -> c_int;
    fn rust_itemdb_look(id: c_uint) -> c_int;
    fn rust_magicdb_name(id: c_int) -> *mut c_char;
    fn rust_magicdb_yname(id: c_int) -> *mut c_char;
    fn rust_magicdb_question(id: c_int) -> *mut c_char;
    fn rust_magicdb_type(id: c_int) -> c_int;
    fn rust_magicdb_mute(id: c_int) -> c_int;
    fn rust_magicdb_ticker(id: c_int) -> c_int;
    fn rust_mob_flushmagic(mob: *mut MobSpawnData) -> c_int;
    fn randomMT() -> c_uint;
    fn rust_sl_async_freeco(user: *mut std::ffi::c_void);
    fn rust_magicdb_canfail(id: c_int) -> c_int;
    fn map_id2mob(id: c_uint) -> *mut MobSpawnData;
    static groups: [c_uint; 65536]; // 256 * 256 flat
}

// rnd(x) macro: ((int)(randomMT() & 0xFFFFFF) % (x))
#[inline]
unsafe fn rnd(x: c_int) -> c_int {
    ((randomMT() & 0x00FF_FFFF) as c_int).wrapping_rem(x)
}

// ─── clif_pc_damage ──────────────────────────────────────────────────────────

/// Apply a critical hit: run scripts and send health packet.
///
/// Mirrors `clif_pc_damage` from `c_src/map_parse.c` ~line 1009.
#[no_mangle]
pub unsafe extern "C" fn clif_pc_damage(sd: *mut MapSessionData, src: *mut MapSessionData) -> c_int {
    if sd.is_null() || src.is_null() { return 0; }

    if (*src).status.state == 1 { return 0; }

    sl_doscript_blargs(
        b"hitCritChance\0".as_ptr() as *const c_char,
        std::ptr::null(),
        2,
        &raw mut (*sd).bl,
        &raw mut (*src).bl,
    );

    if (*sd).critchance > 0 {
        sl_doscript_blargs(
            b"swingDamage\0".as_ptr() as *const c_char,
            std::ptr::null(),
            2,
            &raw mut (*sd).bl,
            &raw mut (*src).bl,
        );
        (*sd).damage += 0.5f32;
        let damage = (*sd).damage as c_int;

        if (*sd).status.equip[EQ_WEAP as usize].id > 0 {
            clif_playsound(
                &raw mut (*src).bl,
                rust_itemdb_soundhit((*sd).status.equip[EQ_WEAP as usize].id) as c_int,
            );
        }

        for x in 0..14usize {
            if (*sd).status.equip[x].id > 0 {
                sl_doscript_blargs(
                    rust_itemdb_yname((*sd).status.equip[x].id),
                    b"on_hit\0".as_ptr() as *const c_char,
                    2,
                    &raw mut (*sd).bl,
                    &raw mut (*src).bl,
                );
            }
        }

        for x in 0..MAX_SPELLS {
            if (*sd).status.skill[x] > 0 {
                sl_doscript_blargs(
                    rust_magicdb_yname((*sd).status.skill[x] as c_int),
                    b"passive_on_hit\0".as_ptr() as *const c_char,
                    2,
                    &raw mut (*sd).bl,
                    &raw mut (*src).bl,
                );
            }
        }

        for x in 0..MAX_MAGIC_TIMERS {
            if (*sd).status.dura_aether[x].id > 0 && (*sd).status.dura_aether[x].duration > 0 {
                let tsd = map_id2sd((*sd).status.dura_aether[x].caster_id);
                if !tsd.is_null() {
                    sl_doscript_blargs(
                        rust_magicdb_yname((*sd).status.dura_aether[x].id as c_int),
                        b"on_hit_while_cast\0".as_ptr() as *const c_char,
                        3,
                        &raw mut (*sd).bl,
                        &raw mut (*src).bl,
                        &raw mut (*tsd).bl,
                    );
                } else {
                    sl_doscript_blargs(
                        rust_magicdb_yname((*sd).status.dura_aether[x].id as c_int),
                        b"on_hit_while_cast\0".as_ptr() as *const c_char,
                        2,
                        &raw mut (*sd).bl,
                        &raw mut (*src).bl,
                    );
                }
            }
        }

        if (*sd).critchance == 1 {
            clif_send_pc_health(src, damage, 33);
        } else if (*sd).critchance == 2 {
            clif_send_pc_health(src, damage, 255);
        }

        clif_sendstatus(src, SFLAG_HPMP);
    }

    0
}

// ─── clif_send_pc_health ─────────────────────────────────────────────────────

/// Trigger player combat scripts when attacked.
///
/// Mirrors `clif_send_pc_health` from `c_src/map_parse.c` ~line 1071.
#[no_mangle]
pub unsafe extern "C" fn clif_send_pc_health(src: *mut MapSessionData, damage: c_int, critical: c_int) -> c_int {
    let _ = (damage, critical);
    let mut bl = map_id2bl((*src).attacker);
    if bl.is_null() {
        bl = map_id2bl((*src).bl.id);
    }

    sl_doscript_blargs(
        b"player_combat\0".as_ptr() as *const c_char,
        b"on_attacked\0".as_ptr() as *const c_char,
        2,
        &raw mut (*src).bl,
        bl,
    );
    0
}

// ─── clif_send_pc_healthscript ───────────────────────────────────────────────

/// Apply damage to the player, compute health percentage, broadcast health
/// packet to the area, and fire all combat scripts.
///
/// Mirrors `clif_send_pc_healthscript` from `c_src/map_parse.c` ~line 1089.
#[no_mangle]
pub unsafe extern "C" fn clif_send_pc_healthscript(
    sd: *mut MapSessionData,
    damage: c_int,
    critical: c_int,
) -> c_int {
    if sd.is_null() { return 0; }

    let maxvita = (*sd).max_hp;
    let mut currentvita = (*sd).status.hp;

    let bl = map_id2bl((*sd).attacker);
    let mut tsd: *mut MapSessionData = std::ptr::null_mut();

    if !bl.is_null() {
        if (*bl).bl_type == BL_MOB as u8 {
            let tmob = bl as *mut MobSpawnData;
            if (*tmob).owner < crate::game::mob::MOB_START_NUM && (*tmob).owner > 0 {
                tsd = map_id2sd((*tmob).owner);
            }
        } else if (*bl).bl_type == BL_PC as u8 {
            tsd = bl as *mut MapSessionData;
        }
    }

    if damage > 0 {
        for x in 0..MAX_SPELLS {
            if (*sd).status.skill[x] > 0 {
                sl_doscript_blargs(
                    rust_magicdb_yname((*sd).status.skill[x] as c_int),
                    b"passive_on_takingdamage\0".as_ptr() as *const c_char,
                    2,
                    &raw mut (*sd).bl,
                    bl,
                );
            }
        }
    }

    (*sd).lastvita = currentvita;
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

    (*sd).status.hp = currentvita;

    let mut percentage: f32 = if currentvita == 0 {
        0.0f32
    } else {
        (currentvita as f32 / maxvita as f32) * 100.0f32
    };

    if (percentage as c_int) == 0 && currentvita != 0 {
        percentage = 1.0f32;
    }

    let mut buf = [0u8; 32];
    buf[0] = 0xAA;
    let sz = (12u16).to_be();
    buf[1] = (sz >> 8) as u8;
    buf[2] = sz as u8;
    buf[3] = 0x13;
    let blid = (*sd).bl.id.to_be();
    buf[5] = (blid >> 24) as u8;
    buf[6] = (blid >> 16) as u8;
    buf[7] = (blid >>  8) as u8;
    buf[8] = blid as u8;
    buf[9]  = critical as u8;
    buf[10] = percentage as u8;
    let dmg = (damage as u32).to_be();
    buf[11] = (dmg >> 24) as u8;
    buf[12] = (dmg >> 16) as u8;
    buf[13] = (dmg >>  8) as u8;
    buf[14] = dmg as u8;

    if (*sd).status.state == 2 {
        clif_send(buf.as_ptr(), 32, &raw mut (*sd).bl, SELF);
    } else {
        clif_send(buf.as_ptr(), 32, &raw mut (*sd).bl, AREA);
    }

    if (*sd).status.hp != 0 && damage > 0 {
        for x in 0..MAX_SPELLS {
            if (*sd).status.skill[x] > 0 {
                sl_doscript_blargs(
                    rust_magicdb_yname((*sd).status.skill[x] as c_int),
                    b"passive_on_takedamage\0".as_ptr() as *const c_char,
                    2,
                    &raw mut (*sd).bl,
                    bl,
                );
            }
        }
        for x in 0..MAX_MAGIC_TIMERS {
            if (*sd).status.dura_aether[x].id > 0 && (*sd).status.dura_aether[x].duration > 0 {
                sl_doscript_blargs(
                    rust_magicdb_yname((*sd).status.dura_aether[x].id as c_int),
                    b"on_takedamage_while_cast\0".as_ptr() as *const c_char,
                    2,
                    &raw mut (*sd).bl,
                    bl,
                );
            }
        }
        for x in 0..14usize {
            if (*sd).status.equip[x].id > 0 {
                sl_doscript_blargs(
                    rust_itemdb_yname((*sd).status.equip[x].id),
                    b"on_takedamage\0".as_ptr() as *const c_char,
                    2,
                    &raw mut (*sd).bl,
                    bl,
                );
            }
        }
    }

    if (*sd).status.hp == 0 {
        sl_doscript_blargs(
            b"onDeathPlayer\0".as_ptr() as *const c_char,
            std::ptr::null(),
            1,
            &raw mut (*sd).bl,
        );

        if !tsd.is_null() {
            sl_doscript_blargs(
                b"onKill\0".as_ptr() as *const c_char,
                std::ptr::null(),
                2,
                &raw mut (*sd).bl,
                &raw mut (*tsd).bl,
            );
        }
    }

    if (*sd).group_count > 0 {
        clif_grouphealth_update(sd);
    }

    0
}

// ─── clif_send_selfbar ────────────────────────────────────────────────────────

/// Send the player's own health bar to themselves.
///
/// Mirrors `clif_send_selfbar` from `c_src/map_parse.c` ~line 1262.
#[no_mangle]
pub unsafe extern "C" fn clif_send_selfbar(sd: *mut MapSessionData) {
    let mut percentage: f32 = if (*sd).status.hp == 0 {
        0.0f32
    } else {
        ((*sd).status.hp as f32 / (*sd).max_hp as f32) * 100.0f32
    };

    if (percentage as c_int) == 0 && (*sd).status.hp != 0 {
        percentage = 1.0f32;
    }

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return;
    }

    let fd = (*sd).fd;
    wfifohead(fd, 15);
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, 12u16.swap_bytes());
    wfifob(fd, 3, 0x13);
    wfifol(fd, 5, (*sd).bl.id.swap_bytes());
    wfifob(fd, 9, 0);
    wfifob(fd, 10, percentage as u8);
    wfifol(fd, 11, 0u32.swap_bytes());
    wfifoset(fd, encrypt(fd) as usize);
}

// ─── clif_send_groupbars ─────────────────────────────────────────────────────

/// Send another player's health bar to `sd` (group bar update).
///
/// Mirrors `clif_send_groupbars` from `c_src/map_parse.c` ~line 1290.
#[no_mangle]
pub unsafe extern "C" fn clif_send_groupbars(sd: *mut MapSessionData, tsd: *mut MapSessionData) {
    if sd.is_null() || tsd.is_null() { return; }

    let mut percentage: f32 = if (*tsd).status.hp == 0 {
        0.0f32
    } else {
        ((*tsd).status.hp as f32 / (*tsd).max_hp as f32) * 100.0f32
    };

    if (percentage as c_int) == 0 && (*tsd).status.hp != 0 {
        percentage = 1.0f32;
    }

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return;
    }

    let fd = (*sd).fd;
    wfifohead(fd, 15);
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, 12u16.swap_bytes());
    wfifob(fd, 3, 0x13);
    wfifol(fd, 5, (*tsd).bl.id.swap_bytes());
    wfifob(fd, 9, 0);
    wfifob(fd, 10, percentage as u8);
    wfifol(fd, 11, 0u32.swap_bytes());
    wfifoset(fd, encrypt(fd) as usize);
}

// ─── clif_send_mobbars ────────────────────────────────────────────────────────

/// Send a mob's health bar to a player (foreachinarea callback).
///
/// va_list: USER *sd
/// Mirrors `clif_send_mobbars` from `c_src/map_parse.c` ~line 1322.
#[no_mangle]
pub unsafe extern "C" fn clif_send_mobbars(bl: *mut BlockList, mut ap: ...) -> c_int {
    let sd: *mut MapSessionData = ap.arg::<*mut MapSessionData>();
    let mob = bl as *mut MobSpawnData;

    if sd.is_null() || mob.is_null() { return 1; }

    let mut percentage: f32 = if (*mob).current_vita == 0 {
        0.0f32
    } else {
        ((*mob).current_vita as f32 / (*mob).maxvita as f32) * 100.0f32
    };

    if (percentage as c_int) == 0 && (*mob).current_vita != 0 {
        percentage = 1.0f32;
    }

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 1;
    }

    let fd = (*sd).fd;
    wfifohead(fd, 15);
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, 12u16.swap_bytes());
    wfifob(fd, 3, 0x13);
    wfifol(fd, 5, (*mob).bl.id.swap_bytes());
    wfifob(fd, 9, 0);
    wfifob(fd, 10, percentage as u8);
    wfifol(fd, 11, 0u32.swap_bytes());
    wfifoset(fd, encrypt(fd) as usize);

    0
}

// ─── clif_findspell_pos ───────────────────────────────────────────────────────

/// Find the spell slot index for a given spell id. Returns -1 if not found.
///
/// Mirrors `clif_findspell_pos` from `c_src/map_parse.c` ~line 1361.
#[no_mangle]
pub unsafe extern "C" fn clif_findspell_pos(sd: *mut MapSessionData, id: c_int) -> c_int {
    for x in 0..52usize {
        if (*sd).status.skill[x] as c_int == id {
            return x as c_int;
        }
    }
    -1
}

// ─── clif_calc_critical ───────────────────────────────────────────────────────

/// Calculate whether an attack is a normal hit, critical hit, or miss.
/// Returns 0 (miss), 1 (hit), or 2 (critical).
///
/// Mirrors `clif_calc_critical` from `c_src/map_parse.c` ~line 1372.
#[no_mangle]
pub unsafe extern "C" fn clif_calc_critical(sd: *mut MapSessionData, bl: *mut BlockList) -> c_int {
    let max_hit = 95;
    let mut equat: c_int = 0;

    if (*bl).bl_type == BL_PC as u8 {
        let tsd = bl as *mut MapSessionData;
        equat = (55 + (*sd).grace / 2) - (*tsd).grace / 2
            + ((*sd).hit as f32 * 1.5f32) as c_int
            + ((*sd).status.level as c_int - (*tsd).status.level as c_int);
    } else if (*bl).bl_type == BL_MOB as u8 {
        let mob = bl as *mut MobSpawnData;
        let data: *mut MobDbData = (*mob).data;
        equat = (55 + (*sd).grace / 2) - (*data).grace / 2
            + ((*sd).hit as f32 * 1.5f32) as c_int
            + ((*sd).status.level as c_int - (*data).level);
    }

    if equat < 5 { equat = 5; }
    if equat > max_hit { equat = max_hit; }

    let chance = rnd(100);
    if chance < equat {
        let mut crit = (*sd).hit / 3;
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
/// Mirrors `clif_has_aethers` from `c_src/map_parse.c` ~line 1414.
#[no_mangle]
pub unsafe extern "C" fn clif_has_aethers(sd: *mut MapSessionData, spell: c_int) -> c_int {
    for x in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[x].id as c_int == spell {
            return (*sd).status.dura_aether[x].aether;
        }
    }
    0
}

// ─── clif_send_duration ──────────────────────────────────────────────────────

/// Send a duration/ticker bar packet to the player.
///
/// Mirrors `clif_send_duration` from `c_src/map_parse.c` ~line 1430.
#[no_mangle]
pub unsafe extern "C" fn clif_send_duration(
    sd: *mut MapSessionData,
    id: c_int,
    time: c_int,
    tsd: *mut MapSessionData,
) -> c_int {
    if sd.is_null() { return 0; }

    let name = rust_magicdb_name(id);

    if rust_magicdb_ticker(id) == 0 { return 0; }

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
        let name_bytes = cstr_bytes(name as *const u8);
        let char_name_bytes = cstr_bytes((*tsd).status.name.as_ptr() as *const u8);
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
        label = &composed_buf[..total];
        label_len = total;
    } else {
        label = cstr_bytes(name as *const u8);
        label_len = label.len();
    }

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    let len = label_len as c_int;
    let fd = (*sd).fd;
    wfifohead(fd, (len + 10) as usize);
    wfifob(fd, 5, len as u8);

    // copy label bytes to WFIFOP(fd, 6)
    {
        use crate::ffi::session::rust_session_wdata_ptr;
        let dst = rust_session_wdata_ptr(fd, 6);
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(label.as_ptr(), dst, label_len);
        }
    }

    wfifol(fd, label_len + 6, (time as u32).swap_bytes());
    wfifoheader(fd, 0x3A, (label_len as u16) + 7);
    wfifoset(fd, encrypt(fd) as usize);

    0
}

// ─── clif_send_aether ─────────────────────────────────────────────────────────

/// Send aether (spell cooldown) bar update to the player.
///
/// Mirrors `clif_send_aether` from `c_src/map_parse.c` ~line 1474.
#[no_mangle]
pub unsafe extern "C" fn clif_send_aether(sd: *mut MapSessionData, id: c_int, time: c_int) -> c_int {
    if sd.is_null() { return 0; }

    let pos = clif_findspell_pos(sd, id);
    if pos < 0 { return 0; }

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    let fd = (*sd).fd;
    wfifohead(fd, 11);
    wfifoheader(fd, 63, 8);
    wfifow(fd, 5, ((pos + 1) as u16).swap_bytes());
    wfifol(fd, 7, (time as u32).swap_bytes());
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_mob_damage ─────────────────────────────────────────────────────────

/// Apply a melee hit to a mob: fire scripts, update threat, send health packet.
///
/// Mirrors `clif_mob_damage` from `c_src/map_parse.c` ~line 1560.
#[no_mangle]
pub unsafe extern "C" fn clif_mob_damage(sd: *mut MapSessionData, mob: *mut MobSpawnData) -> c_int {
    if sd.is_null() || mob.is_null() { return 0; }

    if (*mob).state == MOB_DEAD { return 0; }

    sl_doscript_blargs(
        b"hitCritChance\0".as_ptr() as *const c_char,
        std::ptr::null(),
        2,
        &raw mut (*sd).bl,
        &raw mut (*mob).bl,
    );

    if (*sd).critchance > 0 {
        sl_doscript_blargs(
            b"swingDamage\0".as_ptr() as *const c_char,
            std::ptr::null(),
            2,
            &raw mut (*sd).bl,
            &raw mut (*mob).bl,
        );

        if (*sd).status.equip[EQ_WEAP as usize].id > 0 {
            clif_playsound(
                &raw mut (*mob).bl,
                rust_itemdb_soundhit((*sd).status.equip[EQ_WEAP as usize].id) as c_int,
            );
        }

        if rnd(100) > 75 {
            clif_deductdura(sd, EQ_WEAP, 1);
        }

        (*sd).damage += 0.5f32;
        let damage = (*sd).damage as c_int; // (int)(sd->damage += 0.5f)
        (*mob).lastaction = libc_time() as c_int;

        for x in 0..MAX_THREATCOUNT {
            if (*mob).threat[x].user == (*sd).bl.id {
                (*mob).threat[x].amount = (*mob).threat[x].amount.saturating_add(damage as u32);
                break;
            } else if (*mob).threat[x].user == 0 {
                (*mob).threat[x].user = (*sd).bl.id;
                (*mob).threat[x].amount = damage as u32;
                break;
            }
        }

        for x in 0..14usize {
            if (*sd).status.equip[x].id > 0 {
                sl_doscript_blargs(
                    rust_itemdb_yname((*sd).status.equip[x].id),
                    b"on_hit\0".as_ptr() as *const c_char,
                    2,
                    &raw mut (*sd).bl,
                    &raw mut (*mob).bl,
                );
            }
        }

        for x in 0..MAX_SPELLS {
            if (*sd).status.skill[x] > 0 {
                sl_doscript_blargs(
                    rust_magicdb_yname((*sd).status.skill[x] as c_int),
                    b"passive_on_hit\0".as_ptr() as *const c_char,
                    2,
                    &raw mut (*sd).bl,
                    &raw mut (*mob).bl,
                );
            }
        }

        for x in 0..MAX_MAGIC_TIMERS {
            if (*sd).status.dura_aether[x].id > 0 && (*sd).status.dura_aether[x].duration > 0 {
                sl_doscript_blargs(
                    rust_magicdb_yname((*sd).status.dura_aether[x].id as c_int),
                    b"on_hit_while_cast\0".as_ptr() as *const c_char,
                    2,
                    &raw mut (*sd).bl,
                    &raw mut (*mob).bl,
                );
            }
        }

        if (*sd).critchance == 1 {
            clif_send_mob_health(mob, damage, 33);
        } else if (*sd).critchance == 2 {
            clif_send_mob_health(mob, damage, 255);
        }
    }

    0
}

// ─── clif_send_mob_health_sub ─────────────────────────────────────────────────

/// Send mob health bar to a player in the area (group-filtered, foreachinarea callback).
///
/// va_list: USER *sd, MOB *mob, int critical, int percentage, int damage
/// Mirrors `clif_send_mob_health_sub` from `c_src/map_parse.c` ~line 1627.
#[no_mangle]
pub unsafe extern "C" fn clif_send_mob_health_sub(bl: *mut BlockList, mut ap: ...) -> c_int {
    let sd:         *mut MapSessionData = ap.arg::<*mut MapSessionData>();
    let mob:        *mut MobSpawnData   = ap.arg::<*mut MobSpawnData>();
    let critical:   c_int               = ap.arg::<c_int>();
    let percentage: c_int               = ap.arg::<c_int>();
    let damage:     c_int               = ap.arg::<c_int>();
    let tsd = bl as *mut MapSessionData;

    if sd.is_null() || mob.is_null() || tsd.is_null() { return 0; }

    if clif_isingroup(tsd, sd) == 0 {
        if (*sd).bl.id != (*bl).id {
            return 0;
        }
    }

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    use crate::ffi::session::{rust_session_get_eof};
    if rust_session_exists((*tsd).fd) == 0 || rust_session_get_eof((*tsd).fd) != 0 {
        rust_session_set_eof((*tsd).fd, 8);
        return 0;
    }

    let fd = (*tsd).fd;
    wfifohead(fd, 15);
    wfifoheader(fd, 0x13, 12);
    wfifol(fd, 5, (*mob).bl.id.swap_bytes());
    wfifob(fd, 9, critical as u8);
    wfifob(fd, 10, percentage as u8);
    wfifol(fd, 11, (damage as u32).swap_bytes());
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_send_mob_health_sub_nosd ────────────────────────────────────────────

/// Send mob health bar to a player in the area (no-sd variant, foreachinarea callback).
///
/// va_list: MOB *mob, int critical, int percentage, int damage
/// Mirrors `clif_send_mob_health_sub_nosd` from `c_src/map_parse.c` ~line 1667.
#[no_mangle]
pub unsafe extern "C" fn clif_send_mob_health_sub_nosd(bl: *mut BlockList, mut ap: ...) -> c_int {
    let mob:        *mut MobSpawnData = ap.arg::<*mut MobSpawnData>();
    let critical:   c_int             = ap.arg::<c_int>();
    let percentage: c_int             = ap.arg::<c_int>();
    let damage:     c_int             = ap.arg::<c_int>();
    let sd = bl as *mut MapSessionData;

    if mob.is_null() || sd.is_null() { return 0; }

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    let fd = (*sd).fd;
    wfifohead(fd, 15);
    wfifoheader(fd, 0x13, 12);
    wfifol(fd, 5, (*mob).bl.id.swap_bytes());
    wfifob(fd, 9, critical as u8);
    wfifob(fd, 10, percentage as u8);
    wfifol(fd, 11, (damage as u32).swap_bytes());
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_send_mob_health ─────────────────────────────────────────────────────

/// Trigger mob combat AI scripts when the mob is attacked.
///
/// Mirrors `clif_send_mob_health` from `c_src/map_parse.c` ~line 1695.
#[no_mangle]
pub unsafe extern "C" fn clif_send_mob_health(mob: *mut MobSpawnData, damage: c_int, critical: c_int) -> c_int {
    let _ = (damage, critical);
    if (*mob).bl.bl_type != BL_MOB as u8 { return 0; }

    let mut bl = map_id2bl((*mob).attacker);
    if bl.is_null() {
        bl = map_id2bl((*mob).bl.id);
    }

    let data: *mut MobDbData = (*mob).data;
    let subtype = (*data).subtype;

    if subtype == 0 {
        sl_doscript_blargs(b"mob_ai_basic\0".as_ptr() as *const c_char, b"on_attacked\0".as_ptr() as *const c_char, 2, &raw mut (*mob).bl, bl);
    } else if subtype == 1 {
        sl_doscript_blargs(b"mob_ai_normal\0".as_ptr() as *const c_char, b"on_attacked\0".as_ptr() as *const c_char, 2, &raw mut (*mob).bl, bl);
    } else if subtype == 2 {
        sl_doscript_blargs(b"mob_ai_hard\0".as_ptr() as *const c_char, b"on_attacked\0".as_ptr() as *const c_char, 2, &raw mut (*mob).bl, bl);
    } else if subtype == 3 {
        sl_doscript_blargs(b"mob_ai_boss\0".as_ptr() as *const c_char, b"on_attacked\0".as_ptr() as *const c_char, 2, &raw mut (*mob).bl, bl);
    } else if subtype == 4 {
        sl_doscript_blargs((*data).yname.as_ptr(), b"on_attacked\0".as_ptr() as *const c_char, 2, &raw mut (*mob).bl, bl);
    } else if subtype == 5 {
        sl_doscript_blargs(b"mob_ai_ghost\0".as_ptr() as *const c_char, b"on_attacked\0".as_ptr() as *const c_char, 2, &raw mut (*mob).bl, bl);
    }

    0
}

// ─── clif_send_mob_healthscript ───────────────────────────────────────────────

/// Apply damage to a mob, compute percentage, broadcast health bars, run scripts.
///
/// Mirrors `clif_send_mob_healthscript` from `c_src/map_parse.c` ~line 1721.
#[no_mangle]
pub unsafe extern "C" fn clif_send_mob_healthscript(mob: *mut MobSpawnData, damage: c_int, critical: c_int) -> c_int {
    let _ = critical;
    if mob.is_null() { return 0; }

    let mut bl: *mut BlockList = std::ptr::null_mut();
    if (*mob).attacker > 0 {
        bl = map_id2bl((*mob).attacker);
    }

    let mut sd: *mut MapSessionData = std::ptr::null_mut();
    let mut tmob: *mut MobSpawnData = std::ptr::null_mut();

    if !bl.is_null() {
        if (*bl).bl_type == BL_PC as u8 {
            sd = bl as *mut MapSessionData;
        } else if (*bl).bl_type == BL_MOB as u8 {
            tmob = bl as *mut MobSpawnData;
            if (*tmob).owner < crate::game::mob::MOB_START_NUM && (*tmob).owner > 0 {
                sd = map_id2sd((*tmob).owner);
            }
        }
    }

    if (*mob).state == MOB_DEAD { return 0; }

    let maxvita = (*mob).maxvita as i32;
    let mut currentvita = (*mob).current_vita as i32;

    if damage < 0 {
        if currentvita - damage > maxvita {
            (*mob).maxdmg += (maxvita - currentvita) as f64;
        } else {
            (*mob).maxdmg -= damage as f64;
        }
        (*mob).lastvita = currentvita as u32;
        currentvita -= damage;
    } else {
        (*mob).lastvita = currentvita as u32;
        if currentvita < damage {
            currentvita = 0;
        } else {
            currentvita -= damage;
        }
    }

    if currentvita > maxvita {
        currentvita = maxvita;
    }

    let mut percentage: f32 = if currentvita <= 0 {
        0.0f32
    } else {
        let p = (currentvita as f32 / maxvita as f32) * 100.0f32;
        if p < 1.0f32 && currentvita > 0 { 1.0f32 } else { p }
    };

    if currentvita > 0 && damage > 0 {
        for x in 0..MAX_MAGIC_TIMERS {
            let p = &(*mob).da[x];
            if p.id > 0 && p.duration > 0 {
                sl_doscript_blargs(
                    rust_magicdb_yname(p.id as c_int),
                    b"on_takedamage_while_cast\0".as_ptr() as *const c_char,
                    2,
                    &raw mut (*mob).bl,
                    bl,
                );
            }
        }
    }

    let pct_int = percentage as c_int;

    if !sd.is_null() {
        map_foreachinarea(
            clif_send_mob_health_sub,
            (*mob).bl.m as c_int, (*mob).bl.x as c_int, (*mob).bl.y as c_int,
            AREA, BL_PC,
            sd, mob, critical, pct_int, damage,
        );
    } else {
        map_foreachinarea(
            clif_send_mob_health_sub_nosd,
            (*mob).bl.m as c_int, (*mob).bl.x as c_int, (*mob).bl.y as c_int,
            AREA, BL_PC,
            mob, critical, pct_int, damage,
        );
    }

    (*mob).current_vita = currentvita as u32;

    if (*mob).current_vita == 0 {
        let sd_bl_ref: *mut BlockList = if !sd.is_null() { &raw mut (*sd).bl } else { std::ptr::null_mut() };
        let data: *mut MobDbData = (*mob).data;

        sl_doscript_blargs(
            (*data).yname.as_ptr(),
            b"before_death\0".as_ptr() as *const c_char,
            2,
            &raw mut (*mob).bl,
            sd_bl_ref,
        );
        sl_doscript_blargs(
            b"before_death\0".as_ptr() as *const c_char,
            std::ptr::null(),
            2,
            &raw mut (*mob).bl,
            sd_bl_ref,
        );

        for x in 0..MAX_MAGIC_TIMERS {
            if (*mob).da[x].id > 0 && (*mob).da[x].duration > 0 {
                sl_doscript_blargs(
                    rust_magicdb_yname((*mob).da[x].id as c_int),
                    b"before_death_while_cast\0".as_ptr() as *const c_char,
                    2,
                    &raw mut (*mob).bl,
                    bl,
                );
            }
        }
    }

    if (*mob).current_vita == 0 {
        rust_mob_flushmagic(mob);
        clif_mob_kill(mob);

        if !tmob.is_null() && (*mob).summon == 0 {
            for x in 0..MAX_MAGIC_TIMERS {
                if (*tmob).da[x].id > 0 && (*tmob).da[x].duration > 0 {
                    sl_doscript_blargs(
                        rust_magicdb_yname((*tmob).da[x].id as c_int),
                        b"on_kill_while_cast\0".as_ptr() as *const c_char,
                        2,
                        &raw mut (*tmob).bl,
                        &raw mut (*mob).bl,
                    );
                }
            }
        }

        if !sd.is_null() && (*mob).summon == 0 {
            if tmob.is_null() {
                for x in 0..MAX_MAGIC_TIMERS {
                    if (*sd).status.dura_aether[x].id > 0 && (*sd).status.dura_aether[x].duration > 0 {
                        sl_doscript_blargs(
                            rust_magicdb_yname((*sd).status.dura_aether[x].id as c_int),
                            b"on_kill_while_cast\0".as_ptr() as *const c_char,
                            2,
                            &raw mut (*sd).bl,
                            &raw mut (*mob).bl,
                        );
                    }
                }

                for x in 0..MAX_SPELLS {
                    if (*sd).status.skill[x] > 0 {
                        sl_doscript_blargs(
                            rust_magicdb_yname((*sd).status.skill[x] as c_int),
                            b"passive_on_kill\0".as_ptr() as *const c_char,
                            2,
                            &raw mut (*sd).bl,
                            &raw mut (*mob).bl,
                        );
                    }
                }
            }

            for x in 0..MAX_THREATCOUNT {
                if (*mob).dmggrptable[x][1] / (*mob).maxdmg > (*mob).dmgindtable[x][1] / (*mob).maxdmg {
                    // handled by addtokillreg selection below
                }
            }

            // find dominant damage dealer for drops
            let mut dropid: c_uint = 0;
            let mut dmgpct: f64 = 0.0;
            let mut droptype: u8 = 0;

            for x in 0..MAX_THREATCOUNT {
                if (*mob).dmggrptable[x][1] / (*mob).maxdmg > dmgpct {
                    dropid = (*mob).dmggrptable[x][0] as c_uint;
                    dmgpct = (*mob).dmggrptable[x][1] / (*mob).maxdmg;
                }
                if (*mob).dmgindtable[x][1] / (*mob).maxdmg > dmgpct {
                    dropid = (*mob).dmgindtable[x][0] as c_uint;
                    dmgpct = (*mob).dmgindtable[x][1] / (*mob).maxdmg;
                    droptype = 1;
                }
            }

            let tsd2: *mut MapSessionData = if droptype == 1 {
                map_id2sd(dropid)
            } else {
                map_id2sd(groups[dropid as usize * 256])
            };

            extern "C" { fn rust_mob_drops(mob: *mut MobSpawnData, sd: *mut std::ffi::c_void) -> c_int; }
            if !tsd2.is_null() {
                rust_mob_drops(mob, tsd2 as *mut std::ffi::c_void);
            } else {
                rust_mob_drops(mob, sd as *mut std::ffi::c_void);
            }

            if (*sd).group_count == 0 {
                if (*(*mob).data).exp > 0 {
                    addtokillreg(sd, (*mob).mobid as c_int);
                }
            } else {
                clif_addtokillreg(sd, (*mob).mobid as c_int);
            }

            sl_doscript_blargs(
                b"onGetExp\0".as_ptr() as *const c_char,
                std::ptr::null(),
                2,
                &raw mut (*sd).bl,
                &raw mut (*mob).bl,
            );

            if (*sd).group_count == 0 {
                rust_pc_checklevel(sd);
            } else {
                for x in 0..(*sd).group_count as usize {
                    let tsdg = map_id2sd(groups[(*sd).groupid as usize * 256 + x]);
                    if tsdg.is_null() { continue; }
                    if (*tsdg).bl.m == (*sd).bl.m && (*tsdg).status.state != 1 {
                        rust_pc_checklevel(tsdg);
                    }
                }
            }

            sl_doscript_blargs(
                b"onKill\0".as_ptr() as *const c_char,
                std::ptr::null(),
                2,
                &raw mut (*mob).bl,
                &raw mut (*sd).bl,
            );
        }

        for x in 0..MAX_MAGIC_TIMERS {
            if (*mob).da[x].id > 0 {
                sl_doscript_blargs(
                    rust_magicdb_yname((*mob).da[x].id as c_int),
                    b"after_death_while_cast\0".as_ptr() as *const c_char,
                    2,
                    &raw mut (*mob).bl,
                    bl,
                );
            }
        }

        let data: *mut MobDbData = (*mob).data;
        sl_doscript_blargs(
            (*data).yname.as_ptr(),
            b"after_death\0".as_ptr() as *const c_char,
            2,
            &raw mut (*mob).bl,
            bl,
        );
        let sd_bl_ref2: *mut BlockList = if !sd.is_null() { &raw mut (*sd).bl } else { std::ptr::null_mut() };
        sl_doscript_blargs(
            b"after_death\0".as_ptr() as *const c_char,
            std::ptr::null(),
            2,
            &raw mut (*mob).bl,
            sd_bl_ref2,
        );
    }

    0
}

// ─── clif_mob_kill ────────────────────────────────────────────────────────────

/// Mark a mob as dead, clear threat tables, broadcast despawn packets.
///
/// Mirrors `clif_mob_kill` from `c_src/map_parse.c` ~line 1964.
#[no_mangle]
pub unsafe extern "C" fn clif_mob_kill(mob: *mut MobSpawnData) -> c_int {
    for x in 0..MAX_THREATCOUNT {
        (*mob).threat[x].user   = 0;
        (*mob).threat[x].amount = 0;
        (*mob).dmggrptable[x][0] = 0.0;
        (*mob).dmggrptable[x][1] = 0.0;
        (*mob).dmgindtable[x][0] = 0.0;
        (*mob).dmgindtable[x][1] = 0.0;
    }

    (*mob).dmgdealt = 0.0;
    (*mob).dmgtaken = 0.0;
    (*mob).maxdmg = (*(*mob).data).vita as f64;
    (*mob).state = MOB_DEAD;
    (*mob).last_death = libc_time() as u32;

    if (*mob).onetime == 0 {
        map_lastdeath_mob(mob);
    }

    map_foreachinarea(
        clif_send_destroy,
        (*mob).bl.m as c_int, (*mob).bl.x as c_int, (*mob).bl.y as c_int,
        AREA, BL_PC,
        LOOK_GET, &raw mut (*mob).bl,
    );

    0
}

// ─── clif_send_destroy ────────────────────────────────────────────────────────

/// Send despawn packet for a mob to one player (foreachinarea callback).
///
/// va_list: int type, MOB *mob
/// Mirrors `clif_send_destroy` from `c_src/map_parse.c` ~line 1990.
#[no_mangle]
pub unsafe extern "C" fn clif_send_destroy(bl: *mut BlockList, mut ap: ...) -> c_int {
    let _type: c_int             = ap.arg::<c_int>();
    let mob:   *mut MobSpawnData = ap.arg::<*mut MobSpawnData>();
    let sd = bl as *mut MapSessionData;

    if sd.is_null() || mob.is_null() { return 0; }

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    let fd = (*sd).fd;
    let data: *mut MobDbData = (*mob).data;
    let packet_id: u8 = if (*data).mobtype == 1 { 0x0E } else { 0x5F };

    wfifohead(fd, 9);
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, 6u16.swap_bytes());
    wfifob(fd, 3, packet_id);
    wfifob(fd, 4, 0x03);
    wfifol(fd, 5, (*mob).bl.id.swap_bytes());
    wfifoset(fd, encrypt(fd) as usize);

    0
}

// ─── clif_sendmagic ───────────────────────────────────────────────────────────

/// Send a spell slot packet to the player.
///
/// Mirrors `clif_sendmagic` from `c_src/map_parse.c` ~line 5987.
#[no_mangle]
pub unsafe extern "C" fn clif_sendmagic(sd: *mut MapSessionData, pos: c_int) -> c_int {
    let id   = (*sd).status.skill[pos as usize] as c_int;
    let name = rust_magicdb_name(id);
    let question = rust_magicdb_question(id);
    let spell_type = rust_magicdb_type(id);

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    let name_len     = cstr_len(name as *const u8);
    let question_len = cstr_len(question as *const u8);

    let fd = (*sd).fd;
    wfifohead(fd, 255);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x17);
    wfifob(fd, 5, (pos + 1) as u8);
    wfifob(fd, 6, spell_type as u8);
    wfifob(fd, 7, name_len as u8);
    {
        use crate::ffi::session::rust_session_wdata_ptr;
        let dst = rust_session_wdata_ptr(fd, 8);
        if !dst.is_null() && !name.is_null() {
            std::ptr::copy_nonoverlapping(name as *const u8, dst, name_len);
        }
        let dst2 = rust_session_wdata_ptr(fd, 8 + name_len);
        if !dst2.is_null() { *dst2 = question_len as u8; }
        let dst3 = rust_session_wdata_ptr(fd, 9 + name_len);
        if !dst3.is_null() && !question.is_null() {
            std::ptr::copy_nonoverlapping(question as *const u8, dst3, question_len);
        }
    }

    let total_len = name_len + question_len + 1;
    wfifow(fd, 1, ((total_len + 5) as u16).swap_bytes());
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_parsemagic ──────────────────────────────────────────────────────────

/// Handle incoming spell cast packet from client.
///
/// Mirrors `clif_parsemagic` from `c_src/map_parse.c` ~line 6022.
#[no_mangle]
pub unsafe extern "C" fn clif_parsemagic(sd: *mut MapSessionData) -> c_int {
    use crate::game::map_parse::packet::{rfifob, rfifol, rfifop};

    // struct point { int m, x, y; } from mmo.h
    #[repr(C)]
    struct Point { m: c_int, x: c_int, y: c_int }

    extern "C" {
        fn CheckProximity(one: Point, two: Point, radius: c_int) -> c_int;
        fn rand() -> c_int;
    }

    let pos = (rfifob((*sd).fd, 5) as c_int) - 1;

    let i = clif_has_aethers(sd, (*sd).status.skill[pos as usize] as c_int);
    if i > 0 {
        let time = i / 1000;
        sl_doscript_blargs(
            rust_magicdb_yname((*sd).status.skill[pos as usize] as c_int),
            b"on_aethers\0".as_ptr() as *const c_char,
            1,
            &raw mut (*sd).bl,
        );
        let mut msg = [0u8; 64];
        let s = format!("Wait {} second(s) for aethers to settle.", time);
        let sb = s.as_bytes();
        let copy_len = sb.len().min(63);
        msg[..copy_len].copy_from_slice(&sb[..copy_len]);
        clif_sendminitext(sd, msg.as_ptr() as *const c_char);
        return 0;
    }

    if (*sd).silence > 0 && rust_magicdb_mute((*sd).status.skill[pos as usize] as c_int) <= (*sd).silence {
        sl_doscript_blargs(
            rust_magicdb_yname((*sd).status.skill[pos as usize] as c_int),
            b"on_mute\0".as_ptr() as *const c_char,
            1,
            &raw mut (*sd).bl,
        );
        clif_sendminitext(sd, b"You have been silenced.\0".as_ptr() as *const c_char);
        return 0;
    }

    (*sd).target   = 0;
    (*sd).attacker = 0;

    match rust_magicdb_type((*sd).status.skill[pos as usize] as c_int) {
        1 => {
            // question type
            let dst = (*sd).question.as_mut_ptr() as *mut u8;
            std::ptr::write_bytes(dst, 0, 64);
            let src_ptr = rfifop((*sd).fd, 6);
            if !src_ptr.is_null() {
                let mut n = 0usize;
                while n < 63 && *src_ptr.add(n) != 0 {
                    *dst.add(n) = *src_ptr.add(n);
                    n += 1;
                }
            }
        }
        2 => {
            // target type
            let raw_id = rfifol((*sd).fd, 6);
            let target_id = u32::from_be(raw_id); // SWAP32
            (*sd).target   = target_id as c_int;
            (*sd).attacker = target_id;
        }
        5 => {
            // self type — no extra data
        }
        _ => {
            return 0;
        }
    }

    sl_doscript_blargs(
        b"onCast\0".as_ptr() as *const c_char,
        std::ptr::null(),
        1,
        &raw mut (*sd).bl,
    );

    if (*sd).target != 0 {
        let tbl = map_id2bl((*sd).target as c_uint);
        if tbl.is_null() { return 0; }

        let tsd2 = map_id2sd((*tbl).id);

        if (*tbl).bl_type == BL_PC as u8 {
            if !tsd2.is_null() && (*tsd2).optFlags & crate::game::pc::OPT_FLAG_STEALTH != 0 {
                return 0;
            }
        }

        let one = Point { m: (*tbl).m as c_int, x: (*tbl).x as c_int, y: (*tbl).y as c_int };
        let two = Point { m: (*sd).bl.m as c_int, x: (*sd).bl.x as c_int, y: (*sd).bl.y as c_int };

        if CheckProximity(one, two, 21) == 1 {
            let mut health: i64 = 0;
            let mut twill: c_int = 0;
            let mut tprotection: c_int = 0;

            if (*tbl).bl_type == BL_PC as u8 && !tsd2.is_null() {
                health = (*tsd2).status.hp as i64;
                twill = (*tsd2).will;
                tprotection = (*tsd2).protection as c_int;
            } else if (*tbl).bl_type == BL_MOB as u8 {
                let tmob = map_id2mob((*tbl).id);
                if !tmob.is_null() {
                    health = (*tmob).current_vita as i64;
                    twill = (*tmob).will;
                    tprotection = (*tmob).protection as c_int;
                }
            }

            if rust_magicdb_canfail((*sd).status.skill[pos as usize] as c_int) == 1 {
                let will_diff = (twill - (*sd).will).max(0);
                // C: (int)((willDiff / 10) + 0.5) — integer division then round-half-up via +0.5.
                // Pure-integer equivalent: (will_diff + 5) / 10 (will_diff >= 0 here).
                let prot = (tprotection + (will_diff + 5) / 10).max(0);
                let fail_chance = (100.0f64 - (0.9f64.powi(prot) * 100.0f64) + 0.5f64) as c_int;
                let cast_test = (rand() % 100) as c_int;
                if cast_test < fail_chance {
                    clif_sendminitext(sd, b"The magic has been deflected.\0".as_ptr() as *const c_char);
                    return 0;
                }
            }

            if health > 0 || (*tbl).bl_type == BL_PC as u8 {
                rust_sl_async_freeco(sd as *mut std::ffi::c_void);
                sl_doscript_blargs(
                    rust_magicdb_yname((*sd).status.skill[pos as usize] as c_int),
                    b"cast\0".as_ptr() as *const c_char,
                    2,
                    &raw mut (*sd).bl,
                    tbl,
                );
            }
        }
    } else {
        rust_sl_async_freeco(sd as *mut std::ffi::c_void);
        sl_doscript_blargs(
            rust_magicdb_yname((*sd).status.skill[pos as usize] as c_int),
            b"cast\0".as_ptr() as *const c_char,
            2,
            &raw mut (*sd).bl,
            std::ptr::null_mut::<BlockList>(),
        );
    }

    0
}

// ─── clif_sendaction ──────────────────────────────────────────────────────────

/// Broadcast an action animation to the area, optionally play a sound.
///
/// Mirrors `clif_sendaction` from `c_src/map_parse.c` ~line 5836.
#[no_mangle]
pub unsafe extern "C" fn clif_sendaction(bl: *mut BlockList, action_type: c_int, time: c_int, sound: c_int) -> c_int {
    let mut buf = [0u8; 32];
    buf[0] = 0xAA;
    buf[1] = 0x00;
    buf[2] = 0x0B;
    buf[3] = 0x1A;
    let blid = (*bl).id.to_be();
    buf[5] = (blid >> 24) as u8;
    buf[6] = (blid >> 16) as u8;
    buf[7] = (blid >>  8) as u8;
    buf[8] = blid as u8;
    buf[9]  = action_type as u8;
    buf[10] = 0x00;
    buf[11] = time as u8;
    buf[12] = 0x00;

    clif_send(buf.as_ptr(), 32, bl, SAMEAREA);

    if sound > 0 {
        clif_playsound(bl, sound);
    }

    if (*bl).bl_type == BL_PC as u8 {
        let sd = bl as *mut MapSessionData;
        (*sd).action = action_type as i8;
        sl_doscript_blargs(
            b"onAction\0".as_ptr() as *const c_char,
            std::ptr::null(),
            1,
            &raw mut (*sd).bl,
        );
    }

    0
}

// ─── clif_sendmob_action ──────────────────────────────────────────────────────

/// Broadcast a mob action animation to the area, optionally play a sound.
///
/// Mirrors `clif_sendmob_action` from `c_src/map_parse.c` ~line 5871.
#[no_mangle]
pub unsafe extern "C" fn clif_sendmob_action(mob: *mut MobSpawnData, action_type: c_int, time: c_int, sound: c_int) -> c_int {
    let mut buf = [0u8; 32];
    buf[0] = 0xAA;
    buf[1] = 0x00;
    buf[2] = 0x0B;
    buf[3] = 0x1A;
    buf[4] = 0x03;
    let blid = (*mob).bl.id.to_be();
    buf[5] = (blid >> 24) as u8;
    buf[6] = (blid >> 16) as u8;
    buf[7] = (blid >>  8) as u8;
    buf[8] = blid as u8;
    buf[9]  = action_type as u8;
    buf[10] = 0x00;
    buf[11] = time as u8;
    buf[12] = 0x00;

    clif_send(buf.as_ptr(), 32, &raw mut (*mob).bl, SAMEAREA);

    if sound > 0 {
        clif_playsound(&raw mut (*mob).bl, sound);
    }

    0
}

// ─── clif_sendanimation_xy ────────────────────────────────────────────────────

/// Send a positional animation packet to one player (foreachinarea callback).
///
/// va_list: int anim, int times, int x, int y
/// Mirrors `clif_sendanimation_xy` from `c_src/map_parse.c` ~line 5898.
#[no_mangle]
pub unsafe extern "C" fn clif_sendanimation_xy(bl: *mut BlockList, mut ap: ...) -> c_int {
    let anim:  c_int = ap.arg::<c_int>();
    let times: c_int = ap.arg::<c_int>();
    let x:     c_int = ap.arg::<c_int>();
    let y:     c_int = ap.arg::<c_int>();
    let src = bl as *mut MapSessionData;

    if rust_session_exists((*src).fd) == 0 {
        rust_session_set_eof((*src).fd, 8);
        return 0;
    }

    let fd = (*src).fd;
    wfifohead(fd, 0x30);
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, 14u16.swap_bytes());
    wfifob(fd, 3, 0x29);
    wfifol(fd, 5, 0);
    wfifow(fd, 9,  (anim  as u16).swap_bytes());
    wfifow(fd, 11, (times as u16).swap_bytes());
    wfifow(fd, 13, (x     as u16).swap_bytes());
    wfifow(fd, 15, (y     as u16).swap_bytes());
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_sendanimation ───────────────────────────────────────────────────────

/// Send animation for a target to one player (foreachinarea callback).
///
/// va_list: int anim, block_list *t, int times
/// Mirrors `clif_sendanimation` from `c_src/map_parse.c` ~line 5926.
#[no_mangle]
pub unsafe extern "C" fn clif_sendanimation(bl: *mut BlockList, mut ap: ...) -> c_int {
    let anim: c_int         = ap.arg::<c_int>();
    let t: *mut BlockList   = ap.arg::<*mut BlockList>();
    let sd = bl as *mut MapSessionData;
    let times: c_int        = ap.arg::<c_int>();

    if t.is_null() || sd.is_null() { return 0; }

    if (*sd).status.setting_flags as u32 & FLAG_MAGIC != 0 {
        if rust_session_exists((*sd).fd) == 0 {
            rust_session_set_eof((*sd).fd, 8);
            return 0;
        }

        let fd = (*sd).fd;
        wfifohead(fd, 13);
        wfifob(fd, 0, 0xAA);
        wfifow(fd, 1, 0x000Au16.swap_bytes());
        wfifob(fd, 3, 0x29);
        wfifol(fd, 5, (*t).id.swap_bytes());
        wfifow(fd, 9,  (anim  as u16).swap_bytes());
        wfifow(fd, 11, (times as u16).swap_bytes());
        wfifoset(fd, encrypt(fd) as usize);
    }

    0
}

// ─── clif_animation ───────────────────────────────────────────────────────────

/// Send animation for `sd`'s block_list to `src`'s socket.
///
/// Mirrors `clif_animation` from `c_src/map_parse.c` ~line 5955.
#[no_mangle]
pub unsafe extern "C" fn clif_animation(
    src: *mut MapSessionData,
    sd: *mut MapSessionData,
    animation: c_int,
    duration: c_int,
) -> c_int {
    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    wfifohead((*src).fd, 0x0A + 3);
    if (*src).status.setting_flags as u32 & FLAG_MAGIC != 0 {
        let fd = (*src).fd;
        wfifob(fd, 0, 0xAA);
        wfifow(fd, 1, 0x000Au16.swap_bytes());
        wfifob(fd, 3, 0x29);
        wfifob(fd, 4, 0x03);
        wfifol(fd, 5, (*sd).bl.id.swap_bytes());
        wfifow(fd, 9,  (animation as u16).swap_bytes());
        wfifow(fd, 11, ((duration / 1000) as u16).swap_bytes());
        wfifoset(fd, encrypt(fd) as usize);
    }
    0
}

// ─── clif_sendanimations ──────────────────────────────────────────────────────

/// Send all active aether animations from `sd` to `src`.
///
/// Mirrors `clif_sendanimations` from `c_src/map_parse.c` ~line 5975.
#[no_mangle]
pub unsafe extern "C" fn clif_sendanimations(src: *mut MapSessionData, sd: *mut MapSessionData) -> c_int {
    for x in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[x].duration > 0 && (*sd).status.dura_aether[x].animation != 0 {
            clif_animation(src, sd, (*sd).status.dura_aether[x].animation as c_int, (*sd).status.dura_aether[x].duration);
        }
    }
    0
}

// ─── clif_parseattack ─────────────────────────────────────────────────────────

/// Handle a melee attack swing from the client.
///
/// Mirrors `clif_parseattack` from `c_src/map_parse.c` ~line 7379.
#[no_mangle]
pub unsafe extern "C" fn clif_parseattack(sd: *mut MapSessionData) -> c_int {
    let attackspeed = (*sd).attack_speed as c_int;

    if (*sd).paralyzed != 0 || (*sd).sleep != 1.0f32 { return 0; }

    if (*sd).status.state == 1 || (*sd).status.state == 3 { return 0; }

    let weap_id = (*sd).status.equip[EQ_WEAP as usize].id;
    let sound = rust_itemdb_sound(weap_id) as c_int;

    if sound == 0 {
        clif_sendaction(&raw mut (*sd).bl, 1, attackspeed, 9);
    } else {
        clif_sendaction(&raw mut (*sd).bl, 1, attackspeed, sound);
    }

    sl_doscript_blargs(b"swingDamage\0".as_ptr() as *const c_char, std::ptr::null(), 1, &raw mut (*sd).bl);
    sl_doscript_blargs(b"swing\0".as_ptr() as *const c_char,       std::ptr::null(), 1, &raw mut (*sd).bl);
    sl_doscript_blargs(b"onSwing\0".as_ptr() as *const c_char,     std::ptr::null(), 1, &raw mut (*sd).bl);

    let weap_look = rust_itemdb_look(weap_id);
    if weap_look >= 20000 && weap_look < 30000 {
        sl_doscript_blargs(rust_itemdb_yname(weap_id), b"shootArrow\0".as_ptr() as *const c_char, 1, &raw mut (*sd).bl);
        sl_doscript_blargs(b"shootArrow\0".as_ptr() as *const c_char, std::ptr::null(), 1, &raw mut (*sd).bl);
    }

    for x in 0..14usize {
        if (*sd).status.equip[x].id > 0 {
            sl_doscript_blargs(rust_itemdb_yname((*sd).status.equip[x].id), b"on_swing\0".as_ptr() as *const c_char, 1, &raw mut (*sd).bl);
        }
    }

    for x in 0..MAX_SPELLS {
        if (*sd).status.skill[x] > 0 {
            sl_doscript_blargs(rust_magicdb_yname((*sd).status.skill[x] as c_int), b"passive_on_swing\0".as_ptr() as *const c_char, 1, &raw mut (*sd).bl);
        }
    }

    for x in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[x].id > 0 && (*sd).status.dura_aether[x].duration > 0 {
            sl_doscript_simple(rust_magicdb_yname((*sd).status.dura_aether[x].id as c_int), b"on_swing_while_cast\0".as_ptr() as *const c_char, &raw mut (*sd).bl);
        }
    }

    0
}

// ─── clif_deductdura ─────────────────────────────────────────────────────────

/// Reduce durability of an equipment slot by `val`. Checks pvp map and ethereal flag.
///
/// Mirrors `clif_deductdura` from `c_src/map_parse.c` ~line 3908.
#[no_mangle]
pub unsafe extern "C" fn clif_deductdura(sd: *mut MapSessionData, equip: c_int, val: c_int) -> c_int {
    if sd.is_null() { return 0; }
    let equip_idx = equip as usize;
    if (*sd).status.equip[equip_idx].id == 0 { return 0; }

    let m = (*sd).bl.m as usize;
    if (*map.add(m)).pvp != 0 { return 0; }

    let eth = rust_itemdb_ethereal((*sd).status.equip[equip_idx].id);
    if eth == 0 {
        (*sd).status.equip[equip_idx].dura -= val as i32;
        clif_checkdura(sd, equip);
    }
    0
}

// ─── clif_deductweapon ───────────────────────────────────────────────────────

/// Randomly reduce weapon durability by `hit`.
///
/// Mirrors `clif_deductweapon` from `c_src/map_parse.c` ~line 3922.
#[no_mangle]
pub unsafe extern "C" fn clif_deductweapon(sd: *mut MapSessionData, hit: c_int) -> c_int {
    if rust_pc_isequip(sd, EQ_WEAP) != 0 {
        if rnd(100) > 50 {
            clif_deductdura(sd, EQ_WEAP, hit);
        }
    }
    0
}

// ─── clif_deductarmor ────────────────────────────────────────────────────────

/// Randomly reduce durability of all armor slots by `hit`.
///
/// Mirrors `clif_deductarmor` from `c_src/map_parse.c` ~line 3932.
#[no_mangle]
pub unsafe extern "C" fn clif_deductarmor(sd: *mut MapSessionData, hit: c_int) -> c_int {
    macro_rules! maybe_deduct {
        ($slot:expr) => {
            if rust_pc_isequip(sd, $slot) != 0 && rnd(100) > 50 {
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
/// Mirrors `clif_checkdura` from `c_src/map_parse.c` ~line 4006.
#[no_mangle]
pub unsafe extern "C" fn clif_checkdura(sd: *mut MapSessionData, equip: c_int) -> c_int {
    if sd.is_null() { return 0; }
    let equip_idx = equip as usize;
    if (*sd).status.equip[equip_idx].id == 0 { return 0; }

    let id = (*sd).status.equip[equip_idx].id;
    (*sd).equipslot = equip as u8;

    let max_dura = rust_itemdb_dura(id) as f32;
    let cur_dura = (*sd).status.equip[equip_idx].dura as f32;
    let percentage = cur_dura / max_dura;

    let mut msg_buf = [0i8; 255];

    if percentage <= 0.5 && (*sd).status.equip[equip_idx].repair == 0 {
        format_dura_msg(&mut msg_buf, rust_itemdb_name(id), "50");
        clif_sendmsg(sd, 5, msg_buf.as_ptr() as *const c_char);
        (*sd).status.equip[equip_idx].repair = 1;
    }
    if percentage <= 0.25 && (*sd).status.equip[equip_idx].repair == 1 {
        format_dura_msg(&mut msg_buf, rust_itemdb_name(id), "25");
        clif_sendmsg(sd, 5, msg_buf.as_ptr() as *const c_char);
        (*sd).status.equip[equip_idx].repair = 2;
    }
    if percentage <= 0.1 && (*sd).status.equip[equip_idx].repair == 2 {
        format_dura_msg(&mut msg_buf, rust_itemdb_name(id), "10");
        clif_sendmsg(sd, 5, msg_buf.as_ptr() as *const c_char);
        (*sd).status.equip[equip_idx].repair = 3;
    }
    if percentage <= 0.05 && (*sd).status.equip[equip_idx].repair == 3 {
        format_dura_msg(&mut msg_buf, rust_itemdb_name(id), "5");
        clif_sendmsg(sd, 5, msg_buf.as_ptr() as *const c_char);
        (*sd).status.equip[equip_idx].repair = 4;
    }
    if percentage <= 0.01 && (*sd).status.equip[equip_idx].repair == 4 {
        format_dura_msg(&mut msg_buf, rust_itemdb_name(id), "1");
        clif_sendmsg(sd, 5, msg_buf.as_ptr() as *const c_char);
        (*sd).status.equip[equip_idx].repair = 5;
    }

    let broken = (*sd).status.equip[equip_idx].dura <= 0
        || ((*sd).status.state == 1 && rust_itemdb_breakondeath((*sd).status.equip[equip_idx].id) == 1);

    if broken {
        if rust_itemdb_protected((*sd).status.equip[equip_idx].id) != 0
            || (*sd).status.equip[equip_idx].protected >= 1
        {
            (*sd).status.equip[equip_idx].protected = (*sd).status.equip[equip_idx].protected.saturating_sub(1);
            (*sd).status.equip[equip_idx].dura = rust_itemdb_dura((*sd).status.equip[equip_idx].id);
            format_restore_msg(&mut msg_buf, rust_itemdb_name(id));
            clif_sendstatus(sd, SFLAG_FULLSTATS | SFLAG_HPMP);
            clif_sendmsg(sd, 5, msg_buf.as_ptr() as *const c_char);
            sl_doscript_blargs(b"characterLog\0".as_ptr() as *const c_char, b"equipRestore\0".as_ptr() as *const c_char, 1, &raw mut (*sd).bl);
            return 0;
        }

        sl_doscript_blargs(b"characterLog\0".as_ptr() as *const c_char, b"equipBreak\0".as_ptr() as *const c_char, 1, &raw mut (*sd).bl);
        format_destroy_msg(&mut msg_buf, rust_itemdb_name(id));

        (*sd).breakid = id;
        sl_doscript_blargs(b"onBreak\0".as_ptr() as *const c_char,              std::ptr::null(), 1, &raw mut (*sd).bl);
        sl_doscript_blargs(rust_itemdb_yname(id), b"on_break\0".as_ptr() as *const c_char, 1, &raw mut (*sd).bl);

        (*sd).status.equip[equip_idx].id              = 0;
        (*sd).status.equip[equip_idx].dura            = 0;
        (*sd).status.equip[equip_idx].amount          = 0;
        (*sd).status.equip[equip_idx].protected       = 0;
        (*sd).status.equip[equip_idx].owner           = 0;
        (*sd).status.equip[equip_idx].custom          = 0;
        (*sd).status.equip[equip_idx].custom_look      = 0;
        (*sd).status.equip[equip_idx].custom_look_color = 0;
        (*sd).status.equip[equip_idx].custom_icon     = 0;
        (*sd).status.equip[equip_idx].custom_icon_color = 0;
        (*sd).status.equip[equip_idx].time            = 0;
        (*sd).status.equip[equip_idx].repair          = 0;
        (*sd).status.equip[equip_idx].real_name[0]    = 0;

        clif_unequipit(sd, clif_getequiptype(equip));

        map_foreachinarea(
            clif_updatestate,
            (*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int,
            AREA, BL_PC,
            sd,
        );
        rust_pc_calcstat(sd);
        clif_sendstatus(sd, SFLAG_FULLSTATS | SFLAG_HPMP);
        clif_sendmsg(sd, 5, msg_buf.as_ptr() as *const c_char);
    }

    0
}

// ─── clif_deductduraequip ────────────────────────────────────────────────────

/// Reduce durability of all equipped items by 10% of max, checking thresholds.
///
/// Mirrors `clif_deductduraequip` from `c_src/map_parse.c` ~line 4114.
#[no_mangle]
pub unsafe extern "C" fn clif_deductduraequip(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }

    let m = (*sd).bl.m as usize;
    if (*map.add(m)).pvp != 0 { return 0; }

    for equip in 0..14usize {
        if (*sd).status.equip[equip].id == 0 { continue; }
        let id = (*sd).status.equip[equip].id;

        let eth = rust_itemdb_ethereal((*sd).status.equip[equip].id);
        if eth != 0 { continue; }

        (*sd).equipslot = equip as u8;

        let deduct = (rust_itemdb_dura((*sd).status.equip[equip].id) as f64 * 0.10).floor() as i32;
        (*sd).status.equip[equip].dura -= deduct;

        let max_dura = rust_itemdb_dura(id) as f32;
        let cur_dura = (*sd).status.equip[equip].dura as f32;
        let percentage = cur_dura / max_dura;

        let mut msg_buf = [0i8; 255];

        if percentage <= 0.5 && (*sd).status.equip[equip].repair == 0 {
            format_dura_msg(&mut msg_buf, rust_itemdb_name(id), "50");
            clif_sendmsg(sd, 5, msg_buf.as_ptr() as *const c_char);
            (*sd).status.equip[equip].repair = 1;
        }
        if percentage <= 0.25 && (*sd).status.equip[equip].repair == 1 {
            format_dura_msg(&mut msg_buf, rust_itemdb_name(id), "25");
            clif_sendmsg(sd, 5, msg_buf.as_ptr() as *const c_char);
            (*sd).status.equip[equip].repair = 2;
        }
        if percentage <= 0.1 && (*sd).status.equip[equip].repair == 2 {
            format_dura_msg(&mut msg_buf, rust_itemdb_name(id), "10");
            clif_sendmsg(sd, 5, msg_buf.as_ptr() as *const c_char);
            (*sd).status.equip[equip].repair = 3;
        }
        if percentage <= 0.05 && (*sd).status.equip[equip].repair == 3 {
            format_dura_msg(&mut msg_buf, rust_itemdb_name(id), "5");
            clif_sendmsg(sd, 5, msg_buf.as_ptr() as *const c_char);
            (*sd).status.equip[equip].repair = 4;
        }
        if percentage <= 0.01 && (*sd).status.equip[equip].repair == 4 {
            format_dura_msg(&mut msg_buf, rust_itemdb_name(id), "1");
            clif_sendmsg(sd, 5, msg_buf.as_ptr() as *const c_char);
            (*sd).status.equip[equip].repair = 5;
        }

        let broken = (*sd).status.equip[equip].dura <= 0
            || ((*sd).status.state == 1 && rust_itemdb_breakondeath((*sd).status.equip[equip].id) == 1);

        if broken {
            if rust_itemdb_protected((*sd).status.equip[equip].id) != 0
                || (*sd).status.equip[equip].protected >= 1
            {
                (*sd).status.equip[equip].protected = (*sd).status.equip[equip].protected.saturating_sub(1);
                (*sd).status.equip[equip].dura = rust_itemdb_dura((*sd).status.equip[equip].id);
                format_restore_msg(&mut msg_buf, rust_itemdb_name(id));
                clif_sendstatus(sd, SFLAG_FULLSTATS | SFLAG_HPMP);
                clif_sendmsg(sd, 5, msg_buf.as_ptr() as *const c_char);
                sl_doscript_blargs(b"characterLog\0".as_ptr() as *const c_char, b"equipRestore\0".as_ptr() as *const c_char, 1, &raw mut (*sd).bl);
                continue;
            }

            // copy broken item to boditems
            let bod_count = (*sd).boditems.bod_count as usize;
            if bod_count < (*sd).boditems.item.len() {
                (*sd).boditems.item[bod_count] = (*sd).status.equip[equip];
                (*sd).boditems.bod_count += 1;
            }

            sl_doscript_blargs(b"characterLog\0".as_ptr() as *const c_char, b"equipBreak\0".as_ptr() as *const c_char, 1, &raw mut (*sd).bl);
            format_destroy_msg(&mut msg_buf, rust_itemdb_name(id));

            (*sd).breakid = id;
            sl_doscript_blargs(b"onBreak\0".as_ptr() as *const c_char,              std::ptr::null(), 1, &raw mut (*sd).bl);
            sl_doscript_blargs(rust_itemdb_yname(id), b"on_break\0".as_ptr() as *const c_char, 1, &raw mut (*sd).bl);

            (*sd).status.equip[equip].id              = 0;
            (*sd).status.equip[equip].dura            = 0;
            (*sd).status.equip[equip].amount          = 0;
            (*sd).status.equip[equip].protected       = 0;
            (*sd).status.equip[equip].owner           = 0;
            (*sd).status.equip[equip].custom          = 0;
            (*sd).status.equip[equip].custom_look      = 0;
            (*sd).status.equip[equip].custom_look_color = 0;
            (*sd).status.equip[equip].custom_icon     = 0;
            (*sd).status.equip[equip].custom_icon_color = 0;
            (*sd).status.equip[equip].time            = 0;
            (*sd).status.equip[equip].repair          = 0;
            (*sd).status.equip[equip].real_name[0]    = 0;

            clif_unequipit(sd, clif_getequiptype(equip as c_int));

            map_foreachinarea(
                clif_updatestate,
                (*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int,
                AREA, BL_PC,
                sd,
            );
            rust_pc_calcstat(sd);
            clif_sendstatus(sd, SFLAG_FULLSTATS | SFLAG_HPMP);
            clif_sendmsg(sd, 5, msg_buf.as_ptr() as *const c_char);
        }
    }

    sl_doscript_blargs(b"characterLog\0".as_ptr() as *const c_char, b"bodLog\0".as_ptr() as *const c_char, 1, &raw mut (*sd).bl);
    (*sd).boditems.bod_count = 0;

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
    extern "C" { fn time(t: *mut u64) -> u64; }
    time(std::ptr::null_mut())
}

/// Write "Your <name> is at <pct>%." into buf (C sprintf equivalent).
#[inline]
unsafe fn format_dura_msg(buf: &mut [i8; 255], name: *mut c_char, pct: &str) {
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
unsafe fn format_restore_msg(buf: &mut [i8; 255], name: *mut c_char) {
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
unsafe fn format_destroy_msg(buf: &mut [i8; 255], name: *mut c_char) {
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
