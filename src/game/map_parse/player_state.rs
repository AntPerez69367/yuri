//! Covers the initial login packet sequence and periodic state updates sent
//! to a single player's own socket (as opposed to area-broadcast packets).
//!

#![allow(non_snake_case, clippy::wildcard_imports)]

use std::ptr;

use crate::common::player::legends::MAX_LEGENDS;
use crate::database::map_db::raw_map_ptr;
use crate::game::pc::{
    MapSessionData,
    // Setting-flags constants (from mmo.h)
    FLAG_ADVICE,
    FLAG_EXCHANGE,
    FLAG_FASTMOVE,
    FLAG_GROUP,
    FLAG_HELM,
    FLAG_MAGIC,
    FLAG_REALM,
    FLAG_SOUND,
    FLAG_WEATHER,
    // SFLAG_* constants (from map_server.h)
    SFLAG_ALWAYSON,
    SFLAG_FULLSTATS,
    SFLAG_GMON,
    SFLAG_HPMP,
    SFLAG_XPMONEY,
};
use crate::game::player::entity::PlayerEntity;
use crate::game::player::prelude::*;
use crate::session::{session_exists, SessionId};

use super::packet::{encrypt, wfifob, wfifohead, wfifol, wfifop, wfifoset, wfifow};

// Constants not in packet.rs — defined locally (from map_server.h / map_parse.h).
const OUT_STATUS: u8 = 0x08; // packet id for clif_sendstatus

// ─── Local helpers ────────────────────────────────────────────────────────────

/// Replace the first occurrence of `orig` (NUL-terminated) in `src` with
/// `rep` (NUL-terminated).  Writes into the caller-provided 4096-byte buffer.
///
/// Returns a pointer into `buf` containing the result, or `src` unchanged if
/// `orig` is not found.
unsafe fn replace_str_local(
    src: *const i8,
    orig: &[u8],
    rep: *const i8,
    buf: &mut [u8; 4096],
) -> *const i8 {
    let orig_bytes = match orig.iter().position(|&b| b == 0) {
        Some(n) => &orig[..n],
        None => orig,
    };
    let p = libc::strstr(src, orig_bytes.as_ptr() as *const i8);
    if p.is_null() {
        return src;
    }
    let prefix_len = (p as usize).saturating_sub(src as usize);
    let rep_len = libc::strlen(rep);
    let tail = p.add(orig_bytes.len());
    std::ptr::copy_nonoverlapping(src as *const u8, buf.as_mut_ptr(), prefix_len.min(4095));
    let after_prefix = prefix_len.min(4095);
    let copy_rep = rep_len.min(4095 - after_prefix);
    std::ptr::copy_nonoverlapping(
        rep as *const u8,
        buf.as_mut_ptr().add(after_prefix),
        copy_rep,
    );
    let after_rep = after_prefix + copy_rep;
    let tail_len = libc::strlen(tail).min(4095 - after_rep);
    std::ptr::copy_nonoverlapping(tail as *const u8, buf.as_mut_ptr().add(after_rep), tail_len);
    buf[after_rep + tail_len] = 0;
    buf.as_ptr() as *const i8
}

use crate::database::clan_db;
use crate::database::class_db::name as classdb_name;
use crate::database::item_db;
use crate::game::client::handlers::clif_getName;
use crate::game::client::visual::{
    clif_destroyold, clif_getLevelTNL, clif_getXPBarPercent, clif_sendweather,
};
use crate::game::map_parse::chat::clif_sendminitext;
use crate::game::map_parse::groups::{clif_grouphealth_update, clif_leavegroup};
use crate::game::map_parse::movement::clif_sendchararea;
use crate::game::map_server::{CURRENT_TIME, CURRENT_YEAR};
use crate::network::crypt::crypt_set_packet_indexes;

use crate::game::block::AreaType;
use crate::game::block_grid;
use crate::game::map_parse::visual::{
    clif_mob_look_close_func_inner, clif_mob_look_start_func_inner, clif_object_look_by_id,
    load_visible_entities,
};

// ─── Constants ────────────────────────────────────────────────────────────────

use crate::common::constants::entity::player::OPT_FLAG_WALKTHROUGH;

// ─── clif_sendack ─────────────────────────────────────────────────────────────

/// Send the initial login ACK packet.
///
/// Packet layout (7 bytes after header):
///   [0]      = 0xAA
///   [1..2]   = BE size 0x0006
///   [3]      = 0x1E  (packet id)
///   [5]      = 0x06
///   [6]      = 0x00
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendack(pe: &PlayerEntity) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }
    let fd = pe.fd;
    wfifohead(fd, 255);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x1E);
    wfifob(fd, 5, 0x06);
    wfifob(fd, 6, 0x00);
    // Write big-endian size 0x0006 at [1..2]
    {
        let p = wfifop(fd, 1) as *mut u16;
        if !p.is_null() {
            p.write_unaligned(0x0006_u16.to_be());
        }
    }
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_retrieveprofile ─────────────────────────────────────────────────────

/// Send the profile retrieval trigger packet.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_retrieveprofile(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 1, 0x00);
    wfifob(fd, 2, 0x04);
    wfifob(fd, 3, 0x49);
    wfifob(fd, 4, 0x03);
    wfifow(fd, 5, 0);
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_screensaver ─────────────────────────────────────────────────────────

/// Send the AFK / screensaver state packet.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_screensaver(pe: &PlayerEntity, screen: i32) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }
    let fd = pe.fd;
    wfifohead(fd, 4 + 3);
    wfifob(fd, 0, 0xAA);
    // big-endian size 0x0004
    {
        let p = wfifop(fd, 1) as *mut u16;
        if !p.is_null() {
            p.write_unaligned(0x0004_u16.to_be());
        }
    }
    wfifob(fd, 3, 0x5A);
    wfifob(fd, 4, 0x03);
    wfifob(fd, 5, 0x00);
    wfifob(fd, 6, screen as u8);
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_sendtime ────────────────────────────────────────────────────────────

/// Send the server-time packet.
///
/// Packet layout:
///   [0]    = 0xAA
///   [1..2] = 0x00 0x04  (size = 4)
///   [3]    = 0x20  (packet id)
///   [4]    = 0x03
///   [5]    = CURRENT_TIME
///   [6]    = CURRENT_YEAR
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendtime(pe: &PlayerEntity) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }
    let fd = pe.fd;
    wfifohead(fd, 7);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 1, 0x00);
    wfifob(fd, 2, 0x04);
    wfifob(fd, 3, 0x20);
    wfifob(fd, 4, 0x03);
    wfifob(
        fd,
        5,
        CURRENT_TIME.load(std::sync::atomic::Ordering::Relaxed) as u8,
    );
    wfifob(
        fd,
        6,
        CURRENT_YEAR.load(std::sync::atomic::Ordering::Relaxed) as u8,
    );
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_sendid ──────────────────────────────────────────────────────────────

/// Send the character ID packet.
///
/// Packet layout (17 bytes + 3-byte header = 20 total):
///   [0]      = 0xAA
///   [1..2]   = 0x00 0x0E  (size = 14)
///   [3]      = 0x05  (packet id)
///   [5..8]   = BE u32 sd->status.id
///   [9..10]  = 0x0000
///   [11]     = 0x00
///   [12]     = 0x02
///   [13]     = 0x03
///   [14..15] = BE u16 0x0000
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendid(pe: &PlayerEntity) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }
    let fd = pe.fd;
    let player_id = pe.read().player.identity.id;
    wfifohead(fd, 17);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 1, 0x00);
    wfifob(fd, 2, 0x0E);
    wfifob(fd, 3, 0x05);
    wfifol(fd, 5, player_id.swap_bytes()); // SWAP32
    wfifow(fd, 9, 0);
    wfifob(fd, 11, 0);
    wfifob(fd, 12, 2);
    wfifob(fd, 13, 3);
    wfifow(fd, 14, 0u16.swap_bytes()); // SWAP16(0)
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_sendmapinfo ─────────────────────────────────────────────────────────

/// Send map info (name, dimensions, BGM, spell flag) to the player.
///
/// Builds two packets:
///   1. Map header packet (0x15): map id, xs, ys, spell/realm flags, title string, light value.
///   2. BGM packet (0x19): bgm type, bgm id × 2, setting flags.
///
/// Followed by a call to `clif_sendweather` (still in C).
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendmapinfo(pe: &PlayerEntity) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }
    let fd = pe.fd;
    let (m, setting_flags) = {
        let sd = pe.read();
        (sd.m as usize, sd.player.appearance.setting_flags)
    };

    // Safety: map[] is initialised by map_init before any player can reach
    // Accessing map[sd->bl.m]:
    let md = &*raw_map_ptr().add(m);

    // ── Packet 1: map header ─────────────────────────────────────────────────
    // Total payload length = 18 + len(title)
    let title_ptr = md.title.as_ptr();
    // Compute null-terminated title length (≤ 63 bytes).
    let mut title_len: usize = 0;
    while title_len < 63 && *title_ptr.add(title_len) != 0 {
        title_len += 1;
    }
    let len = title_len as u8;

    wfifohead(fd, 100);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x15);
    // sd->bl.m  (big-endian u16) at [5..6]
    wfifow(fd, 5, (m as u16).swap_bytes());
    // xs, ys
    wfifow(fd, 7, md.xs.swap_bytes());
    wfifow(fd, 9, md.ys.swap_bytes());
    // spell/weather flag at [11]
    let spell_flag: u8 = if setting_flags & FLAG_WEATHER != 0 {
        4
    } else {
        5
    };
    wfifob(fd, 11, spell_flag);
    // realm flag at [12]
    let realm_flag: u8 = if setting_flags & FLAG_REALM != 0 {
        0x01
    } else {
        0x00
    };
    wfifob(fd, 12, realm_flag);
    // title length at [13], then title bytes at [14..14+len]
    wfifob(fd, 13, len);
    {
        let dst = wfifop(fd, 14);
        if !dst.is_null() {
            ptr::copy_nonoverlapping(title_ptr as *const u8, dst, title_len);
        }
    }
    // light value at [14+len .. 15+len] (big-endian u16)
    let light_val: u16 = if md.light != 0 { md.light as u16 } else { 232 };
    wfifow(fd, 14 + title_len, light_val.swap_bytes());
    // big-endian packet size at [1..2]: 18 + title_len
    {
        let p = wfifop(fd, 1) as *mut u16;
        if !p.is_null() {
            p.write_unaligned(((18 + title_len) as u16).to_be());
        }
    }
    wfifoset(fd, encrypt(fd) as usize);

    // ── clif_sendweather ─────────────────────────────────────────────────────
    clif_sendweather(pe);

    // ── Packet 2: BGM ────────────────────────────────────────────────────────
    wfifohead(fd, 100);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 1, 0x00);
    wfifob(fd, 2, 0x12);
    wfifob(fd, 3, 0x19);
    wfifob(fd, 5, md.bgmtype as u8);
    wfifow(fd, 7, md.bgm.swap_bytes());
    wfifow(fd, 9, md.bgm.swap_bytes()); // same field written twice (C does the same)
    wfifob(fd, 11, 0x64);
    // SWAP32(sd->status.settingFlags) — C accesses the 4-byte unsigned int field.
    // Rust stores it as u16; zero-extend to u32 for the wire format.
    wfifol(fd, 12, setting_flags.swap_bytes());
    wfifob(fd, 16, 0);
    wfifob(fd, 17, 0);
    wfifoset(fd, encrypt(fd) as usize);

    0
}

// ─── clif_sendxy ──────────────────────────────────────────────────────────────

/// Send the player position packet (click-to-walk variant).
///
/// Writes absolute position and computes the viewport offset depending on
/// whether the map is larger than the 16 × 14 client viewport.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendxy(pe: &PlayerEntity) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }
    let fd = pe.fd;
    let (m, x, y) = {
        let sd = pe.read();
        (sd.m as usize, sd.x as i32, sd.y as i32)
    };
    let md = &*raw_map_ptr().add(m);

    wfifohead(fd, 14);
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, 0x000D_u16.swap_bytes()); // SWAP16(0x0D)
    wfifob(fd, 3, 0x04);
    wfifow(fd, 5, (x as u16).swap_bytes());
    wfifow(fd, 7, (y as u16).swap_bytes());

    // Viewport X offset
    let vx: u16 = if md.xs as i32 >= 16 {
        if x < 8 {
            x as u16
        } else if x >= md.xs as i32 - 8 {
            (x - md.xs as i32 + 17) as u16
        } else {
            8
        }
    } else {
        ((16 - md.xs as i32) / 2 + x) as u16
    };
    wfifow(fd, 9, vx.swap_bytes());

    // Viewport Y offset
    let vy: u16 = if md.ys as i32 >= 14 {
        if y < 7 {
            y as u16
        } else if y >= md.ys as i32 - 7 {
            (y - md.ys as i32 + 15) as u16
        } else {
            7
        }
    } else {
        ((14 - md.ys as i32) / 2 + y) as u16
    };
    wfifow(fd, 11, vy.swap_bytes());

    wfifob(fd, 13, 0x00);
    wfifoset(fd, encrypt(fd) as usize);

    crate::game::pc::pc_runfloor_sub(&mut *pe.write() as *mut MapSessionData);
    0
}

// ─── clif_sendxynoclick ───────────────────────────────────────────────────────

/// Send the player position packet (no-click variant).
///
/// Identical wire format to `clif_sendxy`; the distinction is only
/// meaningful to the caller — no "click" flag is present in either packet
/// variant (both write 0x00 at [13]).
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendxynoclick(pe: &PlayerEntity) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }
    let fd = pe.fd;
    let (m, x, y) = {
        let sd = pe.read();
        (sd.m as usize, sd.x as i32, sd.y as i32)
    };
    let md = &*raw_map_ptr().add(m);

    wfifohead(fd, 14);
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, 0x000D_u16.swap_bytes());
    wfifob(fd, 3, 0x04);
    wfifow(fd, 5, (x as u16).swap_bytes());
    wfifow(fd, 7, (y as u16).swap_bytes());

    let vx: u16 = if md.xs as i32 >= 16 {
        if x < 8 {
            x as u16
        } else if x >= md.xs as i32 - 8 {
            (x - md.xs as i32 + 17) as u16
        } else {
            8
        }
    } else {
        ((16 - md.xs as i32) / 2 + x) as u16
    };
    wfifow(fd, 9, vx.swap_bytes());

    let vy: u16 = if md.ys as i32 >= 14 {
        if y < 7 {
            y as u16
        } else if y >= md.ys as i32 - 7 {
            (y - md.ys as i32 + 15) as u16
        } else {
            7
        }
    } else {
        ((14 - md.ys as i32) / 2 + y) as u16
    };
    wfifow(fd, 11, vy.swap_bytes());

    wfifob(fd, 13, 0x00);
    wfifoset(fd, encrypt(fd) as usize);

    crate::game::pc::pc_runfloor_sub(&mut *pe.write() as *mut MapSessionData);
    0
}

// ─── clif_sendxychange ────────────────────────────────────────────────────────

/// Send a delta-movement position update.
///
/// Adjusts `dx`/`dy` to prevent the viewport from scrolling off the map edge,
/// then stores the resulting offsets in `sd->viewx`/`sd->viewy`.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendxychange(pe: &PlayerEntity, dx: i32, dy: i32) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }
    let fd = pe.fd;
    let (m, bx, by) = {
        let sd = pe.read();
        (sd.m as usize, sd.x as i32, sd.y as i32)
    };
    let md = &*raw_map_ptr().add(m);

    wfifohead(fd, 14);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 1, 0x00);
    wfifob(fd, 2, 0x0A);
    wfifob(fd, 3, 0x04);
    wfifow(fd, 5, (bx as u16).swap_bytes());
    wfifow(fd, 7, (by as u16).swap_bytes());

    // Clamp dx to prevent viewport from going off the left or right edge.
    let mut dx = dx;
    if bx - dx < 0 {
        dx -= 1;
    } else if bx + (16 - dx) >= md.xs as i32 {
        dx += 1;
    }
    wfifow(fd, 9, (dx as u16).swap_bytes());
    pe.write().viewx = dx as u16;

    // Clamp dy to prevent viewport from going off the top or bottom edge.
    let mut dy = dy;
    if by - dy < 0 {
        dy -= 1;
    } else if by + (14 - dy) >= md.ys as i32 {
        dy += 1;
    }
    wfifow(fd, 11, (dy as u16).swap_bytes());
    pe.write().viewy = dy as u16;

    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_sendstatus ─────────────────────────────────────────────────────────

/// Send the full character status packet.
///
/// `flags` is a bitmask of `SFLAG_*` values.  `SFLAG_ALWAYSON` is always
/// added; `SFLAG_GMON` is added for GMs who are walking-through.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendstatus(pe: &PlayerEntity, flags: i32) -> i32 {
    let mut f = flags | SFLAG_ALWAYSON;

    // XP percentage — delegate to visual (map_parse.c) which computes the percentage
    // within the current level band using classdb_level DB lookups.
    let percentage: f32 = clif_getXPBarPercent(pe);

    {
        let sd = pe.read();
        if sd.player.identity.gm_level != 0 && sd.optFlags & OPT_FLAG_WALKTHROUGH != 0 {
            f |= SFLAG_GMON;
        }
    }

    if !session_exists(pe.fd) {
        return 0;
    }
    let fd = pe.fd;

    wfifohead(fd, 63);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, OUT_STATUS);
    wfifob(fd, 5, f as u8);

    let mut len: usize = 0;

    if f & SFLAG_FULLSTATS != 0 {
        let sd = pe.read();
        wfifob(fd, 6, 0); // Unknown
        wfifob(fd, 7, sd.player.progression.country as u8); // Nation
        wfifob(fd, 8, sd.player.progression.totem); // Totem
        wfifob(fd, 9, 0); // Unknown
        wfifob(fd, 10, sd.player.progression.level);
        wfifol(fd, 11, sd.max_hp.swap_bytes());
        wfifol(fd, 15, sd.max_mp.swap_bytes());
        wfifob(fd, 19, sd.might as u8);
        wfifob(fd, 20, sd.will as u8);
        wfifob(fd, 21, 0x03);
        wfifob(fd, 22, 0x03);
        wfifob(fd, 23, sd.grace as u8);
        wfifob(fd, 24, 0);
        wfifob(fd, 25, 0);
        wfifob(fd, 26, sd.armor as u8); // AC
        wfifob(fd, 27, 0);
        wfifob(fd, 28, 0);
        wfifob(fd, 29, 0);
        wfifob(fd, 30, 0);
        wfifob(fd, 31, 0);
        wfifob(fd, 32, 0);
        wfifob(fd, 33, 0);
        wfifob(fd, 34, sd.player.inventory.max_inv);
        len += 29;
    }

    if f & SFLAG_HPMP != 0 {
        let sd = pe.read();
        wfifol(fd, len + 6, sd.player.combat.hp.swap_bytes());
        wfifol(fd, len + 10, sd.player.combat.mp.swap_bytes());
        len += 8;
    }

    if f & SFLAG_XPMONEY != 0 {
        let sd = pe.read();
        wfifol(fd, len + 6, sd.player.progression.exp.swap_bytes());
        wfifol(fd, len + 10, sd.player.inventory.money.swap_bytes());
        wfifob(fd, len + 14, percentage as u8);
        len += 9;
    }

    {
        let sd = pe.read();
        wfifob(fd, len + 6, sd.drunk as u8);
        wfifob(fd, len + 7, sd.blind as u8);
        wfifob(fd, len + 8, 0);
        wfifob(fd, len + 9, 0); // hear self/others
        wfifob(fd, len + 10, 0);
        wfifob(fd, len + 11, sd.flags as u8); // 1=New parcel, 16=new Message
        wfifob(fd, len + 12, 0); // nothing
        wfifol(
            fd,
            len + 13,
            sd.player.appearance.setting_flags.swap_bytes(),
        );
    }
    len += 11;

    // Write big-endian packet size at [1..2]: len + 3
    {
        let p = wfifop(fd, 1) as *mut u16;
        if !p.is_null() {
            p.write_unaligned(((len + 3) as u16).to_be());
        }
    }
    wfifoset(fd, encrypt(fd) as usize);

    if pe.read().group_count > 0 {
        clif_grouphealth_update(pe);
    }
    0
}

// ─── clif_sendoptions ────────────────────────────────────────────────────────

/// Send the client option flags (weather, magic, advice, fastmove, sound,
/// helm, realm) to the player.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendoptions(pe: &PlayerEntity) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }
    let fd = pe.fd;
    let sf = pe.read().player.appearance.setting_flags;

    wfifohead(fd, 12);
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, 9_u16.swap_bytes()); // SWAP16(9)
    wfifob(fd, 3, 0x23);
    wfifob(fd, 4, 0x03);
    wfifob(fd, 5, if sf & FLAG_WEATHER != 0 { 1 } else { 0 }); // Weather
    wfifob(fd, 6, if sf & FLAG_MAGIC != 0 { 1 } else { 0 }); // Magic
    wfifob(fd, 7, if sf & FLAG_ADVICE != 0 { 1 } else { 0 }); // Advice
    wfifob(fd, 8, if sf & FLAG_FASTMOVE != 0 { 1 } else { 0 });
    wfifob(fd, 9, if sf & FLAG_SOUND != 0 { 1 } else { 0 }); // Sound
    wfifob(fd, 10, if sf & FLAG_HELM != 0 { 1 } else { 0 }); // Helm
    wfifob(fd, 11, if sf & FLAG_REALM != 0 { 1 } else { 0 }); // Realm
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_mystatus ──────────────────────────────────────────────────────────

/// Send the full appearance / status packet visible to the player's own client.
///
/// Builds a variable-length packet (packet id 0x39) containing:
///   - AC, dam, hit values
///   - Clan name, clan title, title strings
///   - Partner string
///   - Group flag + TNL (u32 BE)
///   - Class name string
///   - Up to 14 equipment slots (icon, color, name strings, dura, protection)
///   - Exchange / group flags
///   - Legend entries (icon, color, text — with optional $player substitution)
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_mystatus(pe: &PlayerEntity) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }

    // Clamp armor (write, then drop guard).
    {
        let mut sd = pe.write();
        sd.armor = sd.armor.clamp(-127, 127);
    }

    // Compute TNL (to-next-level) — drop guard before calling external function.
    let tnl: u32 = clif_getLevelTNL(pe) as u32;

    // Get class name (read fields, drop guard before classdb call).
    let class_name = {
        let sd = pe.read();
        classdb_name(
            sd.player.progression.class as i32,
            sd.player.progression.mark as i32,
        )
    };

    if !session_exists(pe.fd) {
        return 0;
    }

    let fd = pe.fd;
    wfifohead(fd, 65535);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x39);
    {
        let sd = pe.read();
        wfifob(fd, 5, sd.armor as u8);
        wfifob(fd, 6, sd.dam as u8);
        wfifob(fd, 7, sd.hit as u8);
    }

    // `len` accumulates the variable portion starting at offset 8.
    let mut len: usize = 0;

    // ── Clan name ────────────────────────────────────────────────────────────
    let clan_id = pe.read().player.social.clan;
    if clan_id == 0 {
        wfifob(fd, 8 + len, 0);
        len += 1;
    } else {
        let cname = clan_db::name(clan_id as i32);
        if cname.is_null() {
            wfifob(fd, 8 + len, 0);
            len += 1;
        } else {
            let cname_len = cstr_len(cname as *const u8);
            wfifob(fd, 8 + len, cname_len as u8);
            copy_cstr_to_wfifo(fd, 9 + len, cname as *const u8, cname_len);
            len += cname_len + 1;
        }
    }

    // ── Clan title ───────────────────────────────────────────────────────────
    let clan_title_len = pe.read().player.social.clan_title.len();
    if clan_title_len > 0 {
        let sd = pe.read();
        wfifob(fd, 8 + len, clan_title_len as u8);
        copy_cstr_to_wfifo(
            fd,
            9 + len,
            sd.player.social.clan_title.as_ptr(),
            clan_title_len,
        );
        len += clan_title_len + 1;
    } else {
        wfifob(fd, 8 + len, 0);
        len += 1;
    }

    // ── Title ─────────────────────────────────────────────────────────────────
    let title_len = pe.read().player.identity.title.len();
    if title_len > 0 {
        let sd = pe.read();
        wfifob(fd, 8 + len, title_len as u8);
        copy_cstr_to_wfifo(fd, 9 + len, sd.player.identity.title.as_ptr(), title_len);
        len += title_len + 1;
    } else {
        wfifob(fd, 8 + len, 0);
        len += 1;
    }

    // ── Partner ───────────────────────────────────────────────────────────────
    let partner_id = pe.read().player.social.partner;
    if partner_id != 0 {
        let pname = pe.read().player.social.partner_name.clone();
        let mut buf = [0i8; 128];
        if !pname.is_empty() {
            // sprintf(buf, "Partner: %s", pname)
            let prefix = b"Partner: ";
            for (i, &b) in prefix.iter().enumerate() {
                buf[i] = b as i8;
            }
            let pname_bytes = pname.as_bytes();
            let pname_len = pname_bytes.len().min(118);
            ptr::copy_nonoverlapping(
                pname_bytes.as_ptr() as *const i8,
                buf.as_mut_ptr().add(prefix.len()),
                pname_len,
            );
        }
        let buf_len = cstr_len(buf.as_ptr() as *const u8);
        wfifob(fd, 8 + len, buf_len as u8);
        copy_cstr_to_wfifo(fd, 9 + len, buf.as_ptr() as *const u8, buf_len);
        len += buf_len + 1;
    } else {
        wfifob(fd, 8 + len, 0);
        len += 1;
    }

    // ── Group flag ────────────────────────────────────────────────────────────
    let sf = pe.read().player.appearance.setting_flags;
    wfifob(fd, 8 + len, if sf & FLAG_GROUP != 0 { 1 } else { 0 });

    // ── TNL (u32 BE) ──────────────────────────────────────────────────────────
    wfifol(fd, 9 + len, tnl.swap_bytes());
    len += 5;

    // ── Class name ────────────────────────────────────────────────────────────
    {
        let cn_bytes = class_name.as_bytes();
        let cn_len = cn_bytes.len();
        if cn_len > 0 {
            wfifob(fd, 8 + len, cn_len as u8);
            let dst = wfifop(fd, 9 + len);
            if !dst.is_null() {
                std::ptr::copy_nonoverlapping(cn_bytes.as_ptr(), dst, cn_len);
            }
            len += cn_len + 1;
        } else {
            wfifob(fd, 8 + len, 0);
            len += 1;
        }
    }

    // ── Equipment (14 slots) ──────────────────────────────────────────────────
    for x in 0..14usize {
        let (
            eq_id,
            eq_custom_icon,
            eq_custom_icon_color,
            eq_real_name_nonempty,
            eq_real_name_ptr,
            eq_dura,
            eq_protected,
        ) = {
            let sd = pe.read();
            let eq = &sd.player.inventory.equip[x];
            let nonempty = !eq.real_name.is_empty() && eq.real_name[0] != 0;
            let real_name_ptr = eq.real_name.as_ptr() as usize; // store as usize to avoid lifetime issue
            (
                eq.id,
                eq.custom_icon,
                eq.custom_icon_color,
                nonempty,
                real_name_ptr,
                eq.dura,
                eq.protected,
            )
        };
        if eq_id > 0 {
            let eq_item = item_db::search(eq_id);
            // Icon
            let icon_w: u16 = if eq_custom_icon != 0 {
                (eq_custom_icon + 49152) as u16
            } else {
                eq_item.icon as u16
            };
            wfifow(fd, 8 + len, icon_w.swap_bytes());

            let icon_color: u8 = if eq_custom_icon != 0 {
                eq_custom_icon_color as u8
            } else {
                eq_item.icon_color
            };
            wfifob(fd, 10 + len, icon_color);
            len += 3;

            // Real name or DB name
            let name_ptr: *const u8 = if eq_real_name_nonempty {
                eq_real_name_ptr as *const u8
            } else {
                eq_item.name.as_ptr() as *const u8
            };
            let name_len = cstr_len(name_ptr);
            wfifob(fd, 8 + len, name_len as u8);
            copy_cstr_to_wfifo(fd, 9 + len, name_ptr, name_len);
            len += name_len + 1;

            // DB name (always from itemdb)
            let dbname_ptr: *const u8 = eq_item.name.as_ptr() as *const u8;
            let dbname_len = cstr_len(dbname_ptr);
            wfifob(fd, 8 + len, dbname_len as u8);
            copy_cstr_to_wfifo(fd, 9 + len, dbname_ptr, dbname_len);
            len += dbname_len + 1;

            // Dura (u32 BE) + protection byte
            wfifol(fd, 8 + len, (eq_dura as u32).swap_bytes());
            let db_prot = eq_item.protected as u32;
            let prot_byte: u8 = if eq_protected >= db_prot {
                eq_protected as u8
            } else {
                db_prot as u8
            };
            wfifob(fd, 12 + len, prot_byte);
            len += 5;
        } else {
            // Empty slot.
            // C writes: wfifow[8]=0, wfifob[10]=0, wfifob[11]=0, wfifob[12]=0,
            //           wfifol[13]=0, wfifob[14]=0 (overlaps the l above — C bug,
            //           writes 0 again), then len += 10.
            // Span used: offsets 8..16 (9 bytes), advance 10 (offset 17 left at 0).
            wfifow(fd, 8 + len, 0);
            wfifob(fd, 10 + len, 0);
            wfifob(fd, 11 + len, 0);
            wfifob(fd, 12 + len, 0);
            wfifol(fd, 13 + len, 0);
            wfifob(fd, 14 + len, 0); // overlap write, harmless
            len += 10;
        }
    }

    // ── Exchange + group flags ────────────────────────────────────────────────
    wfifob(fd, 8 + len, if sf & FLAG_EXCHANGE != 0 { 1 } else { 0 });
    wfifob(fd, 9 + len, if sf & FLAG_GROUP != 0 { 1 } else { 0 });
    len += 1;

    // ── Legends ───────────────────────────────────────────────────────────────
    let mut count: u16 = 0;
    for x in 0..MAX_LEGENDS {
        let sd = pe.read();
        let lg = &sd.player.legends.legends[x];
        if lg.text[0] != 0 && lg.name[0] != 0 {
            count += 1;
        }
    }
    wfifob(fd, 8 + len, 0);
    wfifow(fd, 9 + len, count.swap_bytes());
    len += 3;

    for x in 0..MAX_LEGENDS {
        let (text_first, name_first, tchaid, icon, color, text_ptr_usize, text_len_val) = {
            let sd = pe.read();
            let lg = &sd.player.legends.legends[x];
            (
                lg.text[0],
                lg.name[0],
                lg.tchaid,
                lg.icon,
                lg.color,
                lg.text.as_ptr() as usize,
                cstr_len(lg.text.as_ptr() as *const u8),
            )
        };
        if text_first == 0 || name_first == 0 {
            continue;
        }

        wfifob(fd, 8 + len, icon as u8);
        wfifob(fd, 9 + len, color as u8);

        if tchaid > 0 {
            let char_name = clif_getName(tchaid);
            let text_ptr = text_ptr_usize as *const i8;
            let mut repl_buf = [0u8; 4096];
            let buff = replace_str_local(text_ptr, b"$player\0", char_name, &mut repl_buf);
            let buff_ptr = buff as *const u8;
            let buff_len = if buff.is_null() {
                0
            } else {
                cstr_len(buff_ptr)
            };
            wfifob(fd, 10 + len, buff_len as u8);
            copy_cstr_to_wfifo(fd, 11 + len, buff_ptr, buff_len);
            len += buff_len + 3;
        } else {
            wfifob(fd, 10 + len, text_len_val as u8);
            copy_cstr_to_wfifo(fd, 11 + len, text_ptr_usize as *const u8, text_len_val);
            len += text_len_val + 3;
        }
    }

    // ── Write big-endian packet size at [1..2] ────────────────────────────────
    {
        let p = wfifop(fd, 1) as *mut u16;
        if !p.is_null() {
            p.write_unaligned(((len + 5) as u16).to_be());
        }
    }
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_getchararea ────────────────────────────────────────────────────────

/// Trigger an area scan to send all nearby PC, NPC, and MOB looks to the
/// player.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub fn clif_getchararea(pe: &PlayerEntity) -> i32 {
    let pos = pe.position();
    let (m, x, y) = (pos.m as i32, pos.x as i32, pos.y as i32);
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let slot = unsafe { &*crate::database::map_db::raw_map_ptr().add(m as usize) };
        let ids = block_grid::ids_in_area(
            grid,
            x,
            y,
            AreaType::SameArea,
            slot.xs as i32,
            slot.ys as i32,
        );
        load_visible_entities(pe, &ids);
    }
    0
}

// ─── clif_refresh ────────────────────────────────────────────────────────────

/// Full area refresh: re-sends map info, player position, all visible objects,
/// and the refresh-complete packet (0x22).
///
/// Also enforces the `canGroup` map restriction: if the map does not allow
/// groups and the player has GROUP enabled, it is disabled and the group is
/// disbanded.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_refresh(pe: &PlayerEntity) -> i32 {
    clif_sendmapinfo(pe);
    clif_sendxy(pe);
    {
        let mut net = pe.net.write();
        let fd = pe.fd;
        let (m, x, y, player_id) = {
            let sd = pe.read();
            (
                sd.m as usize,
                sd.x as i32,
                sd.y as i32,
                sd.player.identity.id,
            )
        };
        clif_mob_look_start_func_inner(fd, &mut net.look);
        if let Some(grid) = block_grid::get_grid(m) {
            let slot = &*crate::database::map_db::raw_map_ptr().add(m);
            let ids = block_grid::ids_in_area(
                grid,
                x,
                y,
                AreaType::SameArea,
                slot.xs as i32,
                slot.ys as i32,
            );
            for id in ids {
                clif_object_look_by_id(fd, &mut net.look, player_id, id);
            }
        }
        clif_mob_look_close_func_inner(fd, &mut net.look);
    }
    clif_destroyold(pe);
    clif_sendchararea(pe);
    clif_getchararea(pe);

    if !session_exists(pe.fd) {
        return 0;
    }
    let fd = pe.fd;

    // Refresh-complete packet (0x22): 5-byte fixed-size packet.
    wfifohead(fd, 5);
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, 2_u16.swap_bytes()); // SWAP16(2)
    wfifob(fd, 3, 0x22);
    wfifob(fd, 4, 0x03);
    // set_packet_indexes — shim for crypt_set_packet_indexes
    let pkt_ptr = wfifop(fd, 0);
    if !pkt_ptr.is_null() {
        crypt_set_packet_indexes(pkt_ptr);
    }
    wfifoset(fd, 5 + 3);

    // Enforce canGroup map restriction.
    let m = pe.read().m as usize;
    let can_group = (*raw_map_ptr().add(m)).can_group;
    if can_group == 0 {
        // XOR toggles the flag.
        pe.write().player.appearance.setting_flags ^= FLAG_GROUP;
        let sf_new = pe.read().player.appearance.setting_flags;
        if sf_new & FLAG_GROUP == 0 {
            // Group flag turned off — disband if in a group.
            if pe.read().group_count > 0 {
                clif_leavegroup(pe);
            }
            let msg = b"Join a group     :OFF\0";
            clif_sendstatus(pe, 0);
            clif_sendminitext(pe, msg.as_ptr() as *const i8);
        }
    }
    0
}

// ─── clif_sendminimap ────────────────────────────────────────────────────────

/// Send the minimap packet to the player.
///
/// Note: the C code writes `SWAP16(sd->bl.m)` into a single byte field (WFIFOB),
/// which only captures the low byte of the big-endian value.  This is an
/// existing C bug; we replicate it faithfully.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendminimap(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;
    let m = pe.read().m;
    wfifohead(fd, 0);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 1, 0x00);
    wfifob(fd, 2, 0x06);
    wfifob(fd, 3, 0x70);
    // C writes SWAP16(sd->bl.m) into a u8 slot — captures only the low byte of BE form.
    wfifob(fd, 4, m.swap_bytes() as u8);
    wfifob(fd, 5, 0x00);
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── Private helpers ──────────────────────────────────────────────────────────

/// Return the length of a null-terminated byte string (does not count the NUL).
#[inline]
unsafe fn cstr_len(ptr: *const u8) -> usize {
    if ptr.is_null() {
        return 0;
    }
    let mut n = 0usize;
    while *ptr.add(n) != 0 {
        n += 1;
    }
    n
}

/// Copy `len` bytes from `src` into the WFIFO buffer at `pos`.
#[inline]
unsafe fn copy_cstr_to_wfifo(fd: SessionId, pos: usize, src: *const u8, len: usize) {
    if len == 0 || src.is_null() {
        return;
    }
    let dst = wfifop(fd, pos);
    if !dst.is_null() {
        ptr::copy_nonoverlapping(src, dst, len);
    }
}
