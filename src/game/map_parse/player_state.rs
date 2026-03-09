//! Covers the initial login packet sequence and periodic state updates sent
//! to a single player's own socket (as opposed to area-broadcast packets).
//!

#![allow(non_snake_case, clippy::wildcard_imports)]

use std::ptr;

use crate::database::map_db::BlockList;
use crate::database::map_db::map;
use crate::session::{
    rust_session_exists, rust_session_set_eof, rust_session_wdata_ptr,
};
use crate::game::pc::{
    MapSessionData,
    // Setting-flags constants (from mmo.h)
    FLAG_ADVICE, FLAG_EXCHANGE, FLAG_FASTMOVE, FLAG_GROUP, FLAG_HELM,
    FLAG_MAGIC, FLAG_REALM, FLAG_SOUND, FLAG_WEATHER,
    // SFLAG_* constants (from map_server.h)
    SFLAG_ALWAYSON, SFLAG_FULLSTATS, SFLAG_GMON, SFLAG_HPMP, SFLAG_XPMONEY,
};
use crate::servers::char::charstatus::MAX_LEGENDS;

use super::packet::{
    encrypt, wfifob, wfifohead, wfifol, wfifoset, wfifow,
};

// Constants not in packet.rs — defined locally (from map_server.h / map_parse.h).
const OUT_STATUS: u8 = 0x08; // packet id for clif_sendstatus
const BL_ALL:  i32 = 0x0F;  // all block-list types

// ─── Local helpers ────────────────────────────────────────────────────────────

/// Replace the first occurrence of `orig` (NUL-terminated) in `src` with
/// `rep` (NUL-terminated).  Uses a 4096-byte module-local static buffer —

/// Not thread-safe (single-threaded map server loop).
unsafe fn replace_str_local(src: *const i8, orig: &[u8], rep: *const i8) -> *const i8 {
    let orig_bytes = match orig.iter().position(|&b| b == 0) {
        Some(n) => &orig[..n],
        None => orig,
    };
    let p = libc::strstr(src, orig_bytes.as_ptr() as *const i8);
    if p.is_null() { return src; }
    static mut REPL_BUF: [u8; 4096] = [0u8; 4096];
    let prefix_len = (p as usize).saturating_sub(src as usize);
    let rep_len = libc::strlen(rep);
    let tail = p.add(orig_bytes.len());
    std::ptr::copy_nonoverlapping(src as *const u8, REPL_BUF.as_mut_ptr(), prefix_len.min(4095));
    let after_prefix = prefix_len.min(4095);
    let copy_rep = rep_len.min(4095 - after_prefix);
    std::ptr::copy_nonoverlapping(rep as *const u8, REPL_BUF.as_mut_ptr().add(after_prefix), copy_rep);
    let after_rep = after_prefix + copy_rep;
    let tail_len = libc::strlen(tail).min(4095 - after_rep);
    std::ptr::copy_nonoverlapping(tail as *const u8, REPL_BUF.as_mut_ptr().add(after_rep), tail_len);
    REPL_BUF[after_rep + tail_len] = 0;
    REPL_BUF.as_ptr() as *const i8
}


use crate::game::map_server::{cur_time, cur_year};
use crate::database::class_db::rust_classdb_name;
use crate::database::clan_db::rust_clandb_name;
use crate::database::item_db::{
    rust_itemdb_name, rust_itemdb_icon, rust_itemdb_iconcolor, rust_itemdb_protected,
};
use crate::game::map_server::map_id2name;
use crate::game::client::handlers::clif_getName;
use crate::game::client::visual::{
    clif_sendweather, clif_destroyold, clif_getLevelTNL, clif_getXPBarPercent,
};
use crate::game::map_parse::visual::{clif_mob_look_start, clif_mob_look_close};
use crate::game::map_parse::movement::clif_sendchararea;
use crate::game::map_parse::groups::{clif_grouphealth_update, clif_leavegroup};
use crate::game::map_parse::chat::clif_sendminitext;
use crate::network::crypt::rust_crypt_set_packet_indexes;

use crate::game::block::{foreach_in_area, AreaType};
use crate::game::map_parse::visual::{
    clif_object_look_sub_inner, clif_charlook_inner, clif_cnpclook_inner, clif_cmoblook_inner,
};

// ─── Constants ────────────────────────────────────────────────────────────────

// enum { LOOK_GET = 0, LOOK_SEND = 1 } from map_parse.h
const LOOK_GET: i32 = 0;

// BL_* type constants (from map_server.h)
const BL_PC:  i32 = 0x01;
const BL_MOB: i32 = 0x02;
const BL_NPC: i32 = 0x04;

// optFlag_walkthrough = 128 (from map_server.h)
const OPT_WALKTHROUGH: u64 = 128;

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
pub unsafe fn clif_sendack(sd: *mut MapSessionData) -> i32 {
    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }
    let fd = (*sd).fd;
    wfifohead(fd, 255);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x1E);
    wfifob(fd, 5, 0x06);
    wfifob(fd, 6, 0x00);
    // Write big-endian size 0x0006 at [1..2]
    {
        let p = rust_session_wdata_ptr(fd, 1) as *mut u16;
        if !p.is_null() { p.write_unaligned(0x0006_u16.to_be()); }
    }
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_retrieveprofile ─────────────────────────────────────────────────────

/// Send the profile retrieval trigger packet.
///
pub unsafe fn clif_retrieveprofile(sd: *mut MapSessionData) -> i32 {
    let fd = (*sd).fd;
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
pub unsafe fn clif_screensaver(sd: *mut MapSessionData, screen: i32) -> i32 {
    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }
    let fd = (*sd).fd;
    wfifohead(fd, 4 + 3);
    wfifob(fd, 0, 0xAA);
    // big-endian size 0x0004
    {
        let p = rust_session_wdata_ptr(fd, 1) as *mut u16;
        if !p.is_null() { p.write_unaligned(0x0004_u16.to_be()); }
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
///   [5]    = cur_time
///   [6]    = cur_year
///
pub unsafe fn clif_sendtime(sd: *mut MapSessionData) -> i32 {
    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }
    let fd = (*sd).fd;
    wfifohead(fd, 7);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 1, 0x00);
    wfifob(fd, 2, 0x04);
    wfifob(fd, 3, 0x20);
    wfifob(fd, 4, 0x03);
    wfifob(fd, 5, cur_time.load(std::sync::atomic::Ordering::Relaxed) as u8);
    wfifob(fd, 6, cur_year.load(std::sync::atomic::Ordering::Relaxed) as u8);
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
pub unsafe fn clif_sendid(sd: *mut MapSessionData) -> i32 {
    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }
    let fd = (*sd).fd;
    wfifohead(fd, 17);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 1, 0x00);
    wfifob(fd, 2, 0x0E);
    wfifob(fd, 3, 0x05);
    wfifol(fd, 5, (*sd).status.id.swap_bytes()); // SWAP32
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
/// Followed by a call to `clif_sendweather` (still in C).
///
pub unsafe fn clif_sendmapinfo(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }
    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }
    let fd = (*sd).fd;
    let m  = (*sd).bl.m as usize;

    // Safety: map[] is initialised by rust_map_init before any player can reach
    // Accessing map[sd->bl.m]:
    let md = &*map.add(m);

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
    wfifow(fd, 5, ((*sd).bl.m as u16).swap_bytes());
    // xs, ys
    wfifow(fd, 7, md.xs.swap_bytes());
    wfifow(fd, 9, md.ys.swap_bytes());
    // spell/weather flag at [11]
    let spell_flag: u8 = if (*sd).status.setting_flags as u32 & FLAG_WEATHER != 0 { 4 } else { 5 };
    wfifob(fd, 11, spell_flag);
    // realm flag at [12]
    let realm_flag: u8 = if (*sd).status.setting_flags as u32 & FLAG_REALM != 0 { 0x01 } else { 0x00 };
    wfifob(fd, 12, realm_flag);
    // title length at [13], then title bytes at [14..14+len]
    wfifob(fd, 13, len);
    {
        let dst = rust_session_wdata_ptr(fd, 14);
        if !dst.is_null() {
            ptr::copy_nonoverlapping(title_ptr as *const u8, dst, title_len);
        }
    }
    // light value at [14+len .. 15+len] (big-endian u16)
    let light_val: u16 = if md.light != 0 { md.light as u16 } else { 232 };
    wfifow(fd, 14 + title_len, light_val.swap_bytes());
    // big-endian packet size at [1..2]: 18 + title_len
    {
        let p = rust_session_wdata_ptr(fd, 1) as *mut u16;
        if !p.is_null() { p.write_unaligned(((18 + title_len) as u16).to_be()); }
    }
    wfifoset(fd, encrypt(fd) as usize);

    // ── clif_sendweather (still in C) ────────────────────────────────────────
    clif_sendweather(sd);

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
    wfifol(fd, 12, ((*sd).status.setting_flags as u32).swap_bytes());
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
pub unsafe fn clif_sendxy(sd: *mut MapSessionData) -> i32 {
    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }
    let fd = (*sd).fd;
    let m  = (*sd).bl.m as usize;
    let md = &*map.add(m);
    let x  = (*sd).bl.x as i32;
    let y  = (*sd).bl.y as i32;

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

    crate::game::pc::rust_pc_runfloor_sub(sd);
    0
}

// ─── clif_sendxynoclick ───────────────────────────────────────────────────────

/// Send the player position packet (no-click variant).
///
/// Identical wire format to `clif_sendxy`; the distinction is only
/// meaningful to the caller — no "click" flag is present in either packet
/// variant (both write 0x00 at [13]).
///
pub unsafe fn clif_sendxynoclick(sd: *mut MapSessionData) -> i32 {
    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }
    let fd = (*sd).fd;
    let m  = (*sd).bl.m as usize;
    let md = &*map.add(m);
    let x  = (*sd).bl.x as i32;
    let y  = (*sd).bl.y as i32;

    wfifohead(fd, 14);
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, 0x000D_u16.swap_bytes());
    wfifob(fd, 3, 0x04);
    wfifow(fd, 5, (x as u16).swap_bytes());
    wfifow(fd, 7, (y as u16).swap_bytes());

    let vx: u16 = if md.xs as i32 >= 16 {
        if x < 8 { x as u16 }
        else if x >= md.xs as i32 - 8 { (x - md.xs as i32 + 17) as u16 }
        else { 8 }
    } else {
        ((16 - md.xs as i32) / 2 + x) as u16
    };
    wfifow(fd, 9, vx.swap_bytes());

    let vy: u16 = if md.ys as i32 >= 14 {
        if y < 7 { y as u16 }
        else if y >= md.ys as i32 - 7 { (y - md.ys as i32 + 15) as u16 }
        else { 7 }
    } else {
        ((14 - md.ys as i32) / 2 + y) as u16
    };
    wfifow(fd, 11, vy.swap_bytes());

    wfifob(fd, 13, 0x00);
    wfifoset(fd, encrypt(fd) as usize);

    crate::game::pc::rust_pc_runfloor_sub(sd);
    0
}

// ─── clif_sendxychange ────────────────────────────────────────────────────────

/// Send a delta-movement position update.
///
/// Adjusts `dx`/`dy` to prevent the viewport from scrolling off the map edge,
/// then stores the resulting offsets in `sd->viewx`/`sd->viewy`.
///
pub unsafe fn clif_sendxychange(sd: *mut MapSessionData, dx: i32, dy: i32) -> i32 {
    if sd.is_null() { return 0; }
    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }
    let fd  = (*sd).fd;
    let m   = (*sd).bl.m as usize;
    let md  = &*map.add(m);
    let bx  = (*sd).bl.x as i32;
    let by  = (*sd).bl.y as i32;

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
    (*sd).viewx = dx as u16;

    // Clamp dy to prevent viewport from going off the top or bottom edge.
    let mut dy = dy;
    if by - dy < 0 {
        dy -= 1;
    } else if by + (14 - dy) >= md.ys as i32 {
        dy += 1;
    }
    wfifow(fd, 11, (dy as u16).swap_bytes());
    (*sd).viewy = dy as u16;

    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_sendstatus ─────────────────────────────────────────────────────────

/// Send the full character status packet.
///
/// `flags` is a bitmask of `SFLAG_*` values.  `SFLAG_ALWAYSON` is always
/// added; `SFLAG_GMON` is added for GMs who are walking-through.
///
pub unsafe fn clif_sendstatus(sd: *mut MapSessionData, flags: i32) -> i32 {
    if sd.is_null() { return 0; }

    let mut f = flags | SFLAG_ALWAYSON;

    // XP percentage — delegate to C (map_parse.c) which computes the percentage
    // within the current level band using classdb_level DB lookups.
    let percentage: f32 = clif_getXPBarPercent(sd) as f32;

    if (*sd).status.gm_level != 0 && (*sd).optFlags & OPT_WALKTHROUGH != 0 {
        f |= SFLAG_GMON;
    }

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }
    let fd = (*sd).fd;

    wfifohead(fd, 63);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, OUT_STATUS as u8);
    wfifob(fd, 5, f as u8);

    let mut len: usize = 0;

    if f & SFLAG_FULLSTATS != 0 {
        wfifob(fd, 6,  0);                           // Unknown
        wfifob(fd, 7,  (*sd).status.country as u8);  // Nation
        wfifob(fd, 8,  (*sd).status.totem);          // Totem
        wfifob(fd, 9,  0);                           // Unknown
        wfifob(fd, 10, (*sd).status.level);
        wfifol(fd, 11, (*sd).max_hp.swap_bytes());
        wfifol(fd, 15, (*sd).max_mp.swap_bytes());
        wfifob(fd, 19, (*sd).might as u8);
        wfifob(fd, 20, (*sd).will as u8);
        wfifob(fd, 21, 0x03);
        wfifob(fd, 22, 0x03);
        wfifob(fd, 23, (*sd).grace as u8);
        wfifob(fd, 24, 0);
        wfifob(fd, 25, 0);
        wfifob(fd, 26, (*sd).armor as u8); // AC
        wfifob(fd, 27, 0);
        wfifob(fd, 28, 0);
        wfifob(fd, 29, 0);
        wfifob(fd, 30, 0);
        wfifob(fd, 31, 0);
        wfifob(fd, 32, 0);
        wfifob(fd, 33, 0);
        wfifob(fd, 34, (*sd).status.maxinv);
        len += 29;
    }

    if f & SFLAG_HPMP != 0 {
        wfifol(fd, len + 6,  (*sd).status.hp.swap_bytes());
        wfifol(fd, len + 10, (*sd).status.mp.swap_bytes());
        len += 8;
    }

    if f & SFLAG_XPMONEY != 0 {
        wfifol(fd, len + 6,  (*sd).status.exp.swap_bytes());
        wfifol(fd, len + 10, (*sd).status.money.swap_bytes());
        wfifob(fd, len + 14, percentage as u8);
        len += 9;
    }

    wfifob(fd, len + 6,  (*sd).drunk as u8);
    wfifob(fd, len + 7,  (*sd).blind as u8);
    wfifob(fd, len + 8,  0);
    wfifob(fd, len + 9,  0); // hear self/others
    wfifob(fd, len + 10, 0);
    wfifob(fd, len + 11, (*sd).flags as u8); // 1=New parcel, 16=new Message
    wfifob(fd, len + 12, 0);                 // nothing
    wfifol(fd, len + 13, ((*sd).status.setting_flags as u32).swap_bytes());
    len += 11;

    // Write big-endian packet size at [1..2]: len + 3
    {
        let p = rust_session_wdata_ptr(fd, 1) as *mut u16;
        if !p.is_null() { p.write_unaligned(((len + 3) as u16).to_be()); }
    }
    wfifoset(fd, encrypt(fd) as usize);

    if (*sd).group_count > 0 {
        clif_grouphealth_update(sd);
    }
    0
}

// ─── clif_sendoptions ────────────────────────────────────────────────────────

/// Send the client option flags (weather, magic, advice, fastmove, sound,
/// helm, realm) to the player.
///
pub unsafe fn clif_sendoptions(sd: *mut MapSessionData) -> i32 {
    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }
    let fd  = (*sd).fd;
    let sf  = (*sd).status.setting_flags as u32;

    wfifohead(fd, 12);
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, 9_u16.swap_bytes()); // SWAP16(9)
    wfifob(fd, 3, 0x23);
    wfifob(fd, 4, 0x03);
    wfifob(fd, 5,  if sf & FLAG_WEATHER  != 0 { 1 } else { 0 }); // Weather
    wfifob(fd, 6,  if sf & FLAG_MAGIC    != 0 { 1 } else { 0 }); // Magic
    wfifob(fd, 7,  if sf & FLAG_ADVICE   != 0 { 1 } else { 0 }); // Advice
    wfifob(fd, 8,  if sf & FLAG_FASTMOVE != 0 { 1 } else { 0 });
    wfifob(fd, 9,  if sf & FLAG_SOUND    != 0 { 1 } else { 0 }); // Sound
    wfifob(fd, 10, if sf & FLAG_HELM     != 0 { 1 } else { 0 }); // Helm
    wfifob(fd, 11, if sf & FLAG_REALM    != 0 { 1 } else { 0 }); // Realm
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_mystaytus ──────────────────────────────────────────────────────────

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
pub unsafe fn clif_mystaytus(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    // Clamp armor.
    if (*sd).armor < -127 { (*sd).armor = -127; }
    if (*sd).armor >  127 { (*sd).armor =  127; }

    // Compute TNL (to-next-level) — delegate to C (map_parse.c) which returns 0
    // for level-capped (>=99) characters and does the classdb_level DB lookup.
    let tnl: u32 = clif_getLevelTNL(sd) as u32;

    // Get class name (may return null).
    let class_name = rust_classdb_name(
        (*sd).status.class as i32,
        (*sd).status.mark  as i32,
    );

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    let fd = (*sd).fd;
    wfifohead(fd, 65535);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x39);
    wfifob(fd, 5, (*sd).armor as u8);
    wfifob(fd, 6, (*sd).dam   as u8);
    wfifob(fd, 7, (*sd).hit   as u8);

    // `len` accumulates the variable portion starting at offset 8.
    let mut len: usize = 0;

    // ── Clan name ────────────────────────────────────────────────────────────
    if (*sd).status.clan == 0 {
        wfifob(fd, 8 + len, 0);
        len += 1;
    } else {
        let cname = rust_clandb_name((*sd).status.clan as i32);
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
    let clan_title_len = cstr_len((*sd).status.clan_title.as_ptr() as *const u8);
    if clan_title_len > 0 {
        wfifob(fd, 8 + len, clan_title_len as u8);
        copy_cstr_to_wfifo(fd, 9 + len, (*sd).status.clan_title.as_ptr() as *const u8, clan_title_len);
        len += clan_title_len + 1;
    } else {
        wfifob(fd, 8 + len, 0);
        len += 1;
    }

    // ── Title ─────────────────────────────────────────────────────────────────
    let title_len = cstr_len((*sd).status.title.as_ptr() as *const u8);
    if title_len > 0 {
        wfifob(fd, 8 + len, title_len as u8);
        copy_cstr_to_wfifo(fd, 9 + len, (*sd).status.title.as_ptr() as *const u8, title_len);
        len += title_len + 1;
    } else {
        wfifob(fd, 8 + len, 0);
        len += 1;
    }

    // ── Partner ───────────────────────────────────────────────────────────────
    if (*sd).status.partner != 0 {
        let pname = map_id2name((*sd).status.partner);
        let mut buf = [0i8; 128];
        if !pname.is_null() {
            // sprintf(buf, "Partner: %s", pname)
            let prefix = b"Partner: ";
            for (i, &b) in prefix.iter().enumerate() {
                buf[i] = b as i8;
            }
            let pname_len = cstr_len(pname as *const u8).min(118);
            ptr::copy_nonoverlapping(pname as *const i8, buf.as_mut_ptr().add(prefix.len()), pname_len);
            // map_id2name returns a heap-allocated string in C — we must free it.
            // C uses FREE() macro which is free().  Call libc free.
            libc_free(pname as *mut _);
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
    let sf = (*sd).status.setting_flags as u32;
    wfifob(fd, 8 + len, if sf & FLAG_GROUP != 0 { 1 } else { 0 });

    // ── TNL (u32 BE) ──────────────────────────────────────────────────────────
    wfifol(fd, 9 + len, tnl.swap_bytes());
    len += 5;

    // ── Class name ────────────────────────────────────────────────────────────
    if !class_name.is_null() {
        let cn_len = cstr_len(class_name as *const u8);
        wfifob(fd, 8 + len, cn_len as u8);
        copy_cstr_to_wfifo(fd, 9 + len, class_name as *const u8, cn_len);
        len += cn_len + 1;
    } else {
        wfifob(fd, 8 + len, 0);
        len += 1;
    }

    // ── Equipment (14 slots) ──────────────────────────────────────────────────
    for x in 0..14usize {
        let eq = &(*sd).status.equip[x];
        if eq.id > 0 {
            // Icon
            let icon_w: u16 = if eq.custom_icon != 0 {
                (eq.custom_icon + 49152) as u16
            } else {
                rust_itemdb_icon(eq.id) as u16
            };
            wfifow(fd, 8 + len, icon_w.swap_bytes());

            let icon_color: u8 = if eq.custom_icon != 0 {
                eq.custom_icon_color as u8
            } else {
                rust_itemdb_iconcolor(eq.id) as u8
            };
            wfifob(fd, 10 + len, icon_color);
            len += 3;

            // Real name or DB name
            let name_ptr: *const u8 = if !eq.real_name.is_empty() && eq.real_name[0] != 0 {
                eq.real_name.as_ptr() as *const u8
            } else {
                let n = rust_itemdb_name(eq.id);
                if n.is_null() { b"\0".as_ptr() } else { n as *const u8 }
            };
            let name_len = cstr_len(name_ptr);
            wfifob(fd, 8 + len, name_len as u8);
            copy_cstr_to_wfifo(fd, 9 + len, name_ptr, name_len);
            len += name_len + 1;

            // DB name (always from itemdb)
            let dbname = rust_itemdb_name(eq.id);
            let dbname_ptr: *const u8 = if dbname.is_null() { b"\0".as_ptr() } else { dbname as *const u8 };
            let dbname_len = cstr_len(dbname_ptr);
            wfifob(fd, 8 + len, dbname_len as u8);
            copy_cstr_to_wfifo(fd, 9 + len, dbname_ptr, dbname_len);
            len += dbname_len + 1;

            // Dura (u32 BE) + protection byte
            wfifol(fd, 8 + len, (eq.dura as u32).swap_bytes());
            let db_prot = rust_itemdb_protected(eq.id) as u32;
            let eq_prot = eq.protected;
            let prot_byte: u8 = if eq_prot >= db_prot { eq_prot as u8 } else { db_prot as u8 };
            wfifob(fd, 12 + len, prot_byte);
            len += 5;
        } else {
            // Empty slot.
            // C writes: wfifow[8]=0, wfifob[10]=0, wfifob[11]=0, wfifob[12]=0,
            //           wfifol[13]=0, wfifob[14]=0 (overlaps the l above — C bug,
            //           writes 0 again), then len += 10.
            // Span used: offsets 8..16 (9 bytes), advance 10 (offset 17 left at 0).
            wfifow(fd, 8  + len, 0);
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
    wfifob(fd, 9 + len, if sf & FLAG_GROUP    != 0 { 1 } else { 0 });
    len += 1;

    // ── Legends ───────────────────────────────────────────────────────────────
    let mut count: u16 = 0;
    for x in 0..MAX_LEGENDS {
        let lg = &(*sd).status.legends[x];
        if lg.text[0] != 0 && lg.name[0] != 0 {
            count += 1;
        }
    }
    wfifob(fd, 8  + len, 0);
    wfifow(fd, 9  + len, count.swap_bytes());
    len += 3;

    for x in 0..MAX_LEGENDS {
        let lg = &(*sd).status.legends[x];
        if lg.text[0] == 0 || lg.name[0] == 0 { continue; }

        wfifob(fd, 8 + len, lg.icon as u8);
        wfifob(fd, 9 + len, lg.color as u8);

        if lg.tchaid > 0 {
            let char_name = clif_getName(lg.tchaid);
            let text_ptr  = lg.text.as_ptr() as *const i8;
            let buff      = replace_str_local(text_ptr, b"$player\0", char_name);
            let buff_ptr  = buff as *const u8;
            let buff_len  = if buff.is_null() { 0 } else { cstr_len(buff_ptr) };
            wfifob(fd, 10 + len, buff_len as u8);
            copy_cstr_to_wfifo(fd, 11 + len, buff_ptr, buff_len);
            len += buff_len + 3;
        } else {
            let text_len = cstr_len(lg.text.as_ptr() as *const u8);
            wfifob(fd, 10 + len, text_len as u8);
            copy_cstr_to_wfifo(fd, 11 + len, lg.text.as_ptr() as *const u8, text_len);
            len += text_len + 3;
        }
    }

    // ── Write big-endian packet size at [1..2] ────────────────────────────────
    {
        let p = rust_session_wdata_ptr(fd, 1) as *mut u16;
        if !p.is_null() { p.write_unaligned(((len + 5) as u16).to_be()); }
    }
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_getchararea ────────────────────────────────────────────────────────

/// Trigger an area scan to send all nearby PC, NPC, and MOB looks to the
/// player.
///
pub unsafe fn clif_getchararea(sd: *mut MapSessionData) -> i32 {
    let m = (*sd).bl.m as i32;
    let x = (*sd).bl.x as i32;
    let y = (*sd).bl.y as i32;
    foreach_in_area(m, x, y, AreaType::SameArea, BL_PC,
        |bl| clif_charlook_inner(bl, LOOK_GET, sd));
    foreach_in_area(m, x, y, AreaType::SameArea, BL_NPC,
        |bl| clif_cnpclook_inner(bl, LOOK_GET, sd as *mut BlockList));
    foreach_in_area(m, x, y, AreaType::SameArea, BL_MOB,
        |bl| clif_cmoblook_inner(bl, LOOK_GET, sd as *mut BlockList));
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
pub unsafe fn clif_refresh(sd: *mut MapSessionData) -> i32 {
    clif_sendmapinfo(sd);
    clif_sendxy(sd);
    clif_mob_look_start(sd);
    foreach_in_area(
        (*sd).bl.m as i32, (*sd).bl.x as i32, (*sd).bl.y as i32,
        AreaType::SameArea, BL_ALL,
        |bl| clif_object_look_sub_inner(bl, LOOK_GET, sd as *mut BlockList),
    );
    clif_mob_look_close(sd);
    clif_destroyold(sd);
    clif_sendchararea(sd);
    clif_getchararea(sd);

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }
    let fd = (*sd).fd;

    // Refresh-complete packet (0x22): 5-byte fixed-size packet.
    wfifohead(fd, 5);
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, 2_u16.swap_bytes()); // SWAP16(2)
    wfifob(fd, 3, 0x22);
    wfifob(fd, 4, 0x03);
    // set_packet_indexes — shim for rust_crypt_set_packet_indexes
    let pkt_ptr = rust_session_wdata_ptr(fd, 0);
    if !pkt_ptr.is_null() {
        rust_crypt_set_packet_indexes(pkt_ptr);
    }
    wfifoset(fd, 5 + 3);

    // Enforce canGroup map restriction.
    let m = (*sd).bl.m as usize;
    let can_group = (*map.add(m)).can_group;
    if can_group == 0 {
        let sf = (*sd).status.setting_flags as u32;
        // XOR toggles the flag.
        (*sd).status.setting_flags = (sf ^ FLAG_GROUP) as u16;
        let sf_new = (*sd).status.setting_flags as u32;
        if sf_new & FLAG_GROUP == 0 {
            // Group flag turned off — disband if in a group.
            if (*sd).group_count > 0 {
                clif_leavegroup(sd);
            }
            let msg = b"Join a group     :OFF\0";
            clif_sendstatus(sd, 0);
            clif_sendminitext(sd, msg.as_ptr() as *const i8);
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
pub unsafe fn clif_sendminimap(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }
    let fd = (*sd).fd;
    wfifohead(fd, 0);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 1, 0x00);
    wfifob(fd, 2, 0x06);
    wfifob(fd, 3, 0x70);
    // C writes SWAP16(sd->bl.m) into a u8 slot — captures only the low byte of BE form.
    wfifob(fd, 4, ((*sd).bl.m as u16).swap_bytes() as u8);
    wfifob(fd, 5, 0x00);
    wfifoset(fd, encrypt(fd) as usize);
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

/// Copy `len` bytes from `src` into the WFIFO buffer at `pos`.
#[inline]
unsafe fn copy_cstr_to_wfifo(fd: i32, pos: usize, src: *const u8, len: usize) {
    if len == 0 || src.is_null() { return; }
    let dst = rust_session_wdata_ptr(fd, pos);
    if !dst.is_null() {
        ptr::copy_nonoverlapping(src, dst, len);
    }
}

/// Thin wrapper around libc `free` for pointers returned by C heap allocators.
///
/// Safety: `ptr` must have been allocated by C's `malloc`/`calloc` and must
/// not be used after this call.
#[inline]
unsafe fn libc_free(ptr: *mut std::ffi::c_void) {
    if !ptr.is_null() { libc::free(ptr); }
}
