//! Visual/status packet builders — Rust port of the `clif_sendupdatestatus` group.
//!

//! All functions build and send binary status packets to a player's client.
//!
//! ## Byte-order convention
//! - `WFIFOL(fd, pos) = SWAP32(val)` → big-endian → `val.to_be_bytes()`
//! - `WFIFOL(fd, pos) = val` (no SWAP32) → little-endian → `val.to_le_bytes()`

#![allow(non_snake_case)]

use crate::common::constants::entity::player::FLAG_WEATHER;
use crate::common::player::inventory::MAX_INVENTORY;
use crate::common::traits::LegacyEntity;
use crate::database::{board_db, class_db};
use crate::game::lua::dispatch::dispatch;
use crate::game::map_parse::packet::{rfifop, rfiforest, wfifohead, wfifop, wfifoset};
use crate::game::mob::BL_MOB;
use crate::game::player::entity::PlayerEntity;
use crate::session::{
    session_exists, session_get_client_ip, session_get_data, session_get_eof, session_increment,
    session_set_eof, SessionId,
};
use std::sync::atomic::{AtomicU8, Ordering};

use crate::network::crypt::encrypt;

// ─── Buffer write helpers ─────────────────────────────────────────────────────

/// Write a single byte into the write buffer at `pos`.
///
/// # Safety
/// `p` must be a valid non-null pointer from `wfifop`, and `pos`
/// must lie within the allocated buffer region.
#[inline]
unsafe fn wb(p: *mut u8, pos: usize, val: u8) {
    *p.add(pos) = val;
}

/// Write a 4-byte big-endian integer at `pos`.
///
///
/// # Safety
/// Same requirements as `wb`; `pos..pos+4` must lie within the buffer.
#[inline]
unsafe fn wl_be(p: *mut u8, pos: usize, val: u32) {
    // Stack-local byte array and pre-allocated buffer regions never overlap.
    std::ptr::copy_nonoverlapping(val.to_be_bytes().as_ptr(), p.add(pos), 4);
}

/// Write a 4-byte little-endian integer at `pos`.
///
/// Write little-endian u32 at `pos` in the send-FIFO.
///
/// # Safety
/// Same requirements as `wb`; `pos..pos+4` must lie within the buffer.
#[inline]
unsafe fn wl_le(p: *mut u8, pos: usize, val: u32) {
    // Stack-local byte array and pre-allocated buffer regions never overlap.
    std::ptr::copy_nonoverlapping(val.to_le_bytes().as_ptr(), p.add(pos), 4);
}

/// Write a big-endian u16 at `pos` in write buffer `p`.
///
///
/// # Safety
/// `p` must be a valid write buffer pointer with at least `pos + 2` writable bytes.
#[inline]
unsafe fn ww_be(p: *mut u8, pos: usize, val: u16) {
    std::ptr::copy_nonoverlapping(val.to_be_bytes().as_ptr(), p.add(pos), 2);
}

// ─── Packet helpers ──────────────────────────────────────────────────────────

/// Returns experience needed to reach the next level (TNL = To Next Level).
/// Resolves the class_db path for a given class id.
#[inline]
fn class_path(class: i32) -> i32 {
    if class > 5 {
        class_db::path(class)
    } else {
        class
    }
}

pub fn clif_getLevelTNL(pe: &PlayerEntity) -> u32 {
    let level = pe.level() as i32;
    let path = class_path(pe.class() as i32);

    if level < 99 {
        class_db::level(path, level).saturating_sub(pe.exp())
    } else {
        0
    }
}

/// Returns the current XP bar fill percentage (0.0–100.0).
pub fn clif_getXPBarPercent(pe: &PlayerEntity) -> f32 {
    let level = pe.level() as i32;
    let exp = pe.exp();
    let path = class_path(pe.class() as i32);

    if level >= 99 {
        return exp as f32 / u32::MAX as f32 * 100.0;
    }

    let xp_prev = class_db::level(path, level - 1);
    let xp_cur = class_db::level(path, level);

    if exp < xp_prev {
        exp as f32 / xp_cur as f32 * 100.0
    } else {
        let exp_in_level = xp_cur - xp_prev;
        let progress = exp - xp_prev;
        progress as f32 / exp_in_level as f32 * 100.0
    }
}

// ─── Status packet senders ────────────────────────────────────────────────────

/// Sends a full HP/MP/EXP/money status update packet.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendupdatestatus(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;

    if !session_exists(fd) {
        return 0;
    }

    wfifohead(fd, 33);
    let p = wfifop(fd, 0);
    if p.is_null() {
        return 0;
    }

    let r = pe.read();
    wb(p, 0, 0xAA);
    wb(p, 1, 0x00);
    wb(p, 2, 0x1C);
    wb(p, 3, 0x08);
    // byte 4: not written in C (left as-is after WFIFOHEAD zeroing)
    wb(p, 5, 0x38);
    wl_be(p, 6, r.player.combat.hp);
    wl_be(p, 10, r.player.combat.mp);
    wl_be(p, 14, r.player.progression.exp);
    wl_be(p, 18, r.player.inventory.money);
    wl_be(p, 22, 0x00);
    wb(p, 26, 0x00);
    wb(p, 27, 0x00);
    wb(p, 28, r.blind as u8);
    wb(p, 29, r.drunk as u8);
    wb(p, 30, 0x00);
    wb(p, 31, 0x73);
    wb(p, 32, 0x35);

    // encrypt() returns 1 on error or pkt_len (≥ 3) on success; never negative.
    wfifoset(fd, encrypt(fd) as usize);
    0
}

/// Sends a compact status update (EXP, money, XP%, blind/drunk, flags, settingFlags).
pub fn clif_sendupdatestatus2(pe: &PlayerEntity) -> i32 {
    // Compute percentage before taking the shared ref (clif_getXPBarPercent mutates pe).
    let percentage = clif_getXPBarPercent(pe);

    let fd = pe.fd;

    if !session_exists(fd) {
        return 0;
    }

    unsafe {
        wfifohead(fd, 25);
        let p = wfifop(fd, 0);
        if p.is_null() {
            return 0;
        }

        let r = pe.read();
        wb(p, 0, 0xAA);
        // bytes 1–2: not written in C
        wb(p, 3, 0x08);
        // byte 4: not written in C
        wb(p, 5, 0x18);
        wl_be(p, 6, r.player.progression.exp);
        wl_be(p, 10, r.player.inventory.money);
        wb(p, 14, percentage as u8); // saturates to 0/255 for NaN or out-of-range (Rust 1.45+)
        wb(p, 15, r.drunk as u8);
        wb(p, 16, r.blind as u8);
        wb(p, 17, 0x00);
        wb(p, 18, 0x00);
        wb(p, 19, 0x00);
        // sd->flags is u64 (64-bit on Linux x86-64); C WFIFOB truncates to low byte.
        wb(p, 20, r.flags as u8);
        wb(p, 21, 0x01);
        wl_be(p, 22, r.player.appearance.setting_flags);

        // encrypt() returns 1 on error or pkt_len (≥ 3) on success; never negative.
        wfifoset(fd, encrypt(fd) as usize);
    }
    0
}

/// Sends a status update after a kill (EXP, money, XP%, settingFlags, TNL, armor/dam/hit).
pub fn clif_sendupdatestatus_onkill(pe: &PlayerEntity) -> i32 {
    // Compute before taking the shared ref.
    let tnl = clif_getLevelTNL(pe);
    let percentage = clif_getXPBarPercent(pe);

    let fd = pe.fd;

    if !session_exists(fd) {
        return 0;
    }

    unsafe {
        wfifohead(fd, 33);
        let p = wfifop(fd, 0);
        if p.is_null() {
            return 0;
        }

        let r = pe.read();
        wb(p, 0, 0xAA);
        wb(p, 1, 0x00);
        wb(p, 2, 0x1C);
        wb(p, 3, 0x08);
        // byte 4: not written in C
        wb(p, 5, 0x19);
        wl_be(p, 6, r.player.progression.exp);
        wl_be(p, 10, r.player.inventory.money);
        wb(p, 14, percentage as u8); // saturates to 0/255 for NaN or out-of-range (Rust 1.45+)
        wb(p, 15, r.drunk as u8);
        wb(p, 16, r.blind as u8);
        wb(p, 17, 0);
        wb(p, 18, 0);
        wb(p, 19, 0);
        // sd->flags is u64 (64-bit on Linux x86-64); C WFIFOB truncates to low byte.
        wb(p, 20, r.flags as u8);
        wb(p, 21, 0);
        wl_be(p, 22, r.player.appearance.setting_flags);
        wl_be(p, 26, tnl as u32);
        wb(p, 30, r.armor as u8);
        wb(p, 31, r.dam as u8);
        wb(p, 32, r.hit as u8);

        // encrypt() returns 1 on error or pkt_len (≥ 3) on success; never negative.
        wfifoset(fd, encrypt(fd) as usize);
    }
    0
}

/// Sends a full status update after equipping an item (all stats, XP, TNL, combat stats).
pub fn clif_sendupdatestatus_onequip(pe: &PlayerEntity) -> i32 {
    let tnl = clif_getLevelTNL(pe);
    let percentage = clif_getXPBarPercent(pe);

    let fd = pe.fd;

    if !session_exists(fd) {
        return 0;
    }

    unsafe {
        wfifohead(fd, 62);
        let p = wfifop(fd, 0);
        if p.is_null() {
            return 0;
        }

        let r = pe.read();
        wb(p, 0, 0xAA);
        wb(p, 1, 0x00);
        wb(p, 2, 65);
        wb(p, 3, 0x08);
        // byte 4: not written in C
        wb(p, 5, 89);
        wb(p, 6, 0x00);
        wb(p, 7, r.player.progression.country as u8);
        wb(p, 8, r.player.progression.totem);
        wb(p, 9, 0x00);
        wb(p, 10, r.player.progression.level);
        wl_be(p, 11, r.max_hp);
        wl_be(p, 15, r.max_mp);
        wb(p, 19, r.might as u8);
        wb(p, 20, r.will as u8);
        wb(p, 21, 0x03);
        wb(p, 22, 0x03);
        wb(p, 23, r.grace as u8);
        wb(p, 24, 0);
        wb(p, 25, 0);
        wb(p, 26, 0);
        wb(p, 27, 0);
        wb(p, 28, 0);
        wb(p, 29, 0);
        wb(p, 30, 0);
        wb(p, 31, 0);
        wb(p, 32, 0);
        wb(p, 33, 0);
        wb(p, 34, r.player.inventory.max_inv);
        wl_be(p, 35, r.player.progression.exp);
        wl_be(p, 39, r.player.inventory.money);
        wb(p, 43, percentage as u8); // saturates to 0/255 for NaN or out-of-range (Rust 1.45+)
        wb(p, 44, r.drunk as u8);
        wb(p, 45, r.blind as u8);
        wb(p, 46, 0x00);
        wb(p, 47, 0x00);
        wb(p, 48, 0x00);
        // sd->flags is u64 (64-bit on Linux x86-64); C WFIFOB truncates to low byte.
        wb(p, 49, r.flags as u8);
        wb(p, 50, 0x00);
        wl_be(p, 51, r.player.appearance.setting_flags);
        wl_be(p, 55, tnl as u32);
        wb(p, 59, r.armor as u8);
        wb(p, 60, r.dam as u8);
        wb(p, 61, r.hit as u8);

        // encrypt() returns 1 on error or pkt_len (≥ 3) on success; never negative.
        wfifoset(fd, encrypt(fd) as usize);
    }
    0
}

/// Sends a status update after unequipping an item (HP/MP + armor + XP + TNL).
///
///
/// HP (offset 11) and MP (offset 15) use **little-endian** byte order — the C code
/// writes them without SWAP32. TNL at offset 50 is also little-endian.
pub fn clif_sendupdatestatus_onunequip(pe: &PlayerEntity) -> i32 {
    let tnl = clif_getLevelTNL(pe);
    let percentage = clif_getXPBarPercent(pe);

    let fd = pe.fd;

    if !session_exists(fd) {
        return 0;
    }

    unsafe {
        wfifohead(fd, 52);
        let p = wfifop(fd, 0);
        if p.is_null() {
            return 0;
        }

        let r = pe.read();
        wb(p, 0, 0xAA);
        wb(p, 1, 0x00);
        wb(p, 2, 55);
        wb(p, 3, 0x08);
        // byte 4: not written in C
        wb(p, 5, 88);
        wb(p, 6, 0x00);
        wb(p, 7, 20);
        wb(p, 8, 0x00);
        wb(p, 9, 0x00);
        wb(p, 10, 0x00);
        // No SWAP32 in C → little-endian store.
        wl_le(p, 11, r.player.combat.hp);
        wl_le(p, 15, r.player.combat.mp);
        wb(p, 19, 0);
        wb(p, 20, 0);
        wb(p, 21, 0);
        wb(p, 22, 0);
        wb(p, 23, 0);
        wb(p, 24, 0);
        wb(p, 25, 0);
        wb(p, 26, r.armor as u8);
        wb(p, 27, 0);
        wb(p, 28, 0);
        wb(p, 29, 0);
        wb(p, 30, 0);
        wb(p, 31, 0);
        wb(p, 32, 0);
        wb(p, 33, 0);
        wb(p, 34, 0);
        wl_be(p, 35, r.player.progression.exp);
        wl_be(p, 39, r.player.inventory.money);
        wb(p, 43, percentage as u8); // saturates to 0/255 for NaN or out-of-range (Rust 1.45+)
        wb(p, 44, r.drunk as u8);
        wb(p, 45, r.blind as u8);
        wb(p, 46, 0x00);
        wb(p, 47, 0x00);
        wb(p, 48, 0x00);
        // sd->flags is u64 (64-bit on Linux x86-64); C WFIFOB truncates to low byte.
        wb(p, 49, r.flags as u8);
        // No SWAP32 in C → little-endian store.
        wl_le(p, 50, tnl as u32);

        // encrypt() returns 1 on error or pkt_len (≥ 3) on success; never negative.
        wfifoset(fd, encrypt(fd) as usize);
    }
    0
}

// ─── Utility / packet-builder functions ──────────────────────────────────────

/// Clears AFK state on the player session.
///
/// Sets `pe->afktime = 0` and `pe->afk = 0`. No packet is sent.
pub fn clif_cancelafk(pe: &PlayerEntity) -> i32 {
    let mut w = pe.write();
    w.afktime = 0;
    w.afk = 0;
    0
}

/// Sends a "destroy old objects" packet (opcode 0x58) to the client.
///
/// Fixed 6-byte packet (3-byte header + 3-byte payload).
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_destroyold(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;

    if !session_exists(fd) {
        return 0;
    }

    // Packet: [0xAA][len_hi][len_lo][0x58][0x03][0x00]
    // Length field = 3 (payload bytes after the 3-byte header).
    wfifohead(fd, 6);
    let p = wfifop(fd, 0);
    if p.is_null() {
        return 0;
    }

    wb(p, 0, 0xAA);
    ww_be(p, 1, 3); // WFIFOW(fd,1) = SWAP16(3)
    wb(p, 3, 0x58);
    wb(p, 4, 0x03);
    wb(p, 5, 0x00);

    wfifoset(fd, encrypt(fd) as usize);
    0
}

/// Maps an equipment slot ID to a map-message number.
///
/// Pure switch/case — no packet is sent. Returns `-1` for unknown slot IDs.
///
/// EQ_ slot enum values (0-based):
/// EQ_WEAP=0, EQ_ARMOR=1, EQ_SHIELD=2, EQ_HELM=3, EQ_LEFT=4, EQ_RIGHT=5,
/// EQ_SUBLEFT=6, EQ_SUBRIGHT=7, EQ_FACEACC=8, EQ_CROWN=9,
/// EQ_MANTLE=10, EQ_NECKLACE=11, EQ_BOOTS=12, EQ_COAT=13.
/// EQ_FACEACCTWO=14 exists in `item_db.h` but is not handled by this function
/// (falls through to return -1), consistent with the C original.
///
/// MAP_EQ* enum values:
/// MAP_EQHELM=13, MAP_EQWEAP=14, MAP_EQARMOR=15, MAP_EQSHIELD=16,
/// MAP_EQLEFT=17, MAP_EQRIGHT=18, MAP_EQSUBLEFT=19, MAP_EQSUBRIGHT=20,
/// MAP_EQFACEACC=21, MAP_EQCROWN=22, MAP_EQMANTLE=23, MAP_EQNECKLACE=24,
/// MAP_EQBOOTS=25, MAP_EQCOAT=26.
///
pub fn clif_mapmsgnum(_pe: &PlayerEntity, id: i32) -> i32 {
    match id {
        3 => 13,  // EQ_HELM=3     → MAP_EQHELM=13
        0 => 14,  // EQ_WEAP=0     → MAP_EQWEAP=14
        1 => 15,  // EQ_ARMOR=1    → MAP_EQARMOR=15
        2 => 16,  // EQ_SHIELD=2   → MAP_EQSHIELD=16
        4 => 17,  // EQ_LEFT=4     → MAP_EQLEFT=17
        5 => 18,  // EQ_RIGHT=5    → MAP_EQRIGHT=18
        6 => 19,  // EQ_SUBLEFT=6  → MAP_EQSUBLEFT=19
        7 => 20,  // EQ_SUBRIGHT=7 → MAP_EQSUBRIGHT=20
        8 => 21,  // EQ_FACEACC=8  → MAP_EQFACEACC=21
        9 => 22,  // EQ_CROWN=9    → MAP_EQCROWN=22
        10 => 23, // EQ_MANTLE=10  → MAP_EQMANTLE=23
        11 => 24, // EQ_NECKLACE=11 → MAP_EQNECKLACE=24
        12 => 25, // EQ_BOOTS=12   → MAP_EQBOOTS=25
        13 => 26, // EQ_COAT=13    → MAP_EQCOAT=26
        _ => -1,
    }
}

/// Sends a popup message packet (opcode 0x0A) to the client.
///
///
/// Packet layout (total = `str_len + 8`):
/// ```text
/// [0xAA][len_hi][len_lo][0x0A][0x03][0x08][str_hi][str_lo][...string bytes...]
/// ```
/// where the length field = `str_len + 5`.
///
///
/// # Safety
/// - `buf` must be a valid, null-terminated C string.
pub unsafe fn clif_popup(pe: &PlayerEntity, buf: *const i8) -> i32 {
    let fd = pe.fd;

    if !session_exists(fd) {
        return 0;
    }

    // Measure string length without the null terminator.
    let str_bytes = std::ffi::CStr::from_ptr(buf).to_bytes();
    let str_len = str_bytes.len();

    // C: WFIFOHEAD(sd->fd, strlen(buf) + 5 + 3) — total = str_len + 8.
    wfifohead(fd, str_len + 8);
    let p = wfifop(fd, 0);
    if p.is_null() {
        return 0;
    }

    wb(p, 0, 0xAA);
    // Length field = str_len + 5 (payload bytes not counting the 3-byte header).
    ww_be(p, 1, (str_len + 5) as u16);
    wb(p, 3, 0x0A);
    wb(p, 4, 0x03);
    wb(p, 5, 0x08);
    // String length as big-endian u16.
    ww_be(p, 6, str_len as u16);
    // Copy string bytes (no null terminator needed in packet body).
    std::ptr::copy_nonoverlapping(buf as *const u8, p.add(8), str_len);

    wfifoset(fd, encrypt(fd) as usize);
    0
}

/// Sends the profile page URL to the client (opcode 0x62, subtype 0x04).
///
/// Hardcoded URL: `"https://www.website.com/users"` (29 bytes).
///
/// Packet layout:
/// ```text
/// [0xAA][len_hi][len_lo][0x62][??][0x04][url_len_byte][...url bytes...]
/// ```
/// where `len = url_len + 7`.
///
///
/// # Safety
/// No unsafe pointer dereferences on `pe`.
pub unsafe fn clif_sendprofile(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;

    if !session_exists(fd) {
        return 0;
    }

    let url: &[u8] = b"https://www.website.com/users";
    let url_len = url.len(); // 29 bytes

    // C has no WFIFOHEAD; add for safety.
    // Total packet = url_len + 7 (payload) + 3 (header overhead) = url_len + 10.
    wfifohead(fd, url_len + 10);
    let p = wfifop(fd, 0);
    if p.is_null() {
        return 0;
    }

    wb(p, 0, 0xAA);
    wb(p, 3, 0x62);
    // byte 4: not written in C (left as-is after wfifohead zeroing)
    wb(p, 5, 0x04);
    // C writes strlen(url) as a single byte at offset 6.
    wb(p, 6, url_len as u8);
    std::ptr::copy_nonoverlapping(url.as_ptr(), p.add(7), url_len);
    // Length field = url_len + 7 (payload bytes after the 3-byte header).
    ww_be(p, 1, (url_len + 7) as u16);

    wfifoset(fd, encrypt(fd) as usize);
    0
}

/// Sends the board page URLs to the client (opcode 0x62, subtype 0x00).
///
/// Three hardcoded URLs packed sequentially, each preceded by a length byte:
/// - url1: `"https://www.website.com/boards"` (30 bytes)
/// - url2: `"https://www.website.com/boards"` (30 bytes)
/// - url3: `"?abc123"` (7 bytes)
///
///
/// # Safety
/// No unsafe pointer dereferences on `pe`.
pub unsafe fn clif_sendboard(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;

    let url1: &[u8] = b"https://www.website.com/boards";
    let url2: &[u8] = b"https://www.website.com/boards";
    let url3: &[u8] = b"?abc123";

    // C len accumulates: starts at 6, then += strlen(urlN) + 1 for each url.
    // Total payload = 6 + (url1_len+1) + (url2_len+1) + (url3_len+1).
    let total_payload = 6 + (url1.len() + 1) + (url2.len() + 1) + (url3.len() + 1);
    // C has no WFIFOHEAD; add for safety. Reserve total_payload + 3 bytes.
    wfifohead(fd, total_payload + 3);
    let p = wfifop(fd, 0);
    if p.is_null() {
        return 0;
    }

    wb(p, 0, 0xAA);
    wb(p, 3, 0x62);
    // byte 4: not written in C
    wb(p, 5, 0x00);

    // Write each url: [length_byte][url_bytes...]
    let mut pos: usize = 6;
    for url in &[url1, url2, url3] {
        wb(p, pos, url.len() as u8);
        std::ptr::copy_nonoverlapping(url.as_ptr(), p.add(pos + 1), url.len());
        pos += url.len() + 1;
    }

    // Length field = total_payload (all payload bytes after the 3-byte header).
    ww_be(p, 1, total_payload as u16);

    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── Task 16: utility functions ───────────────────────────────────────────────

/// Maps an equipment slot ID to its item-type integer.
///
/// Pure switch/case — no packet is sent. Returns `0` for unknown slot IDs.
///
/// EQ_ slot enum values (0-based):
/// EQ_WEAP=0, EQ_ARMOR=1, EQ_SHIELD=2, EQ_HELM=3, EQ_LEFT=4, EQ_RIGHT=5,
/// EQ_SUBLEFT=6, EQ_SUBRIGHT=7, EQ_FACEACC=8, EQ_CROWN=9,
/// EQ_MANTLE=10, EQ_NECKLACE=11, EQ_BOOTS=12, EQ_COAT=13.
/// EQ_FACEACCTWO=14 is not handled (falls through to return 0), consistent with C.
///
/// # Safety
/// No pointer dereferences — this function is pure.
pub unsafe fn clif_getequiptype(val: i32) -> i32 {
    match val {
        0 => 1,   // EQ_WEAP=0      → type 1
        1 => 2,   // EQ_ARMOR=1     → type 2
        2 => 3,   // EQ_SHIELD=2    → type 3
        3 => 4,   // EQ_HELM=3      → type 4
        11 => 6,  // EQ_NECKLACE=11 → type 6
        4 => 7,   // EQ_LEFT=4      → type 7
        5 => 8,   // EQ_RIGHT=5     → type 8
        12 => 13, // EQ_BOOTS=12    → type 13
        10 => 14, // EQ_MANTLE=10   → type 14
        13 => 16, // EQ_COAT=13     → type 16
        6 => 20,  // EQ_SUBLEFT=6   → type 20
        7 => 21,  // EQ_SUBRIGHT=7  → type 21
        8 => 22,  // EQ_FACEACC=8   → type 22
        9 => 23,  // EQ_CROWN=9     → type 23
        _ => 0,
    }
}

/// Returns the item area for a player session (stub — always returns 0).
///
///
pub fn clif_getitemarea(_pe: &PlayerEntity) -> i32 {
    0
}

/// Returns the XP required to reach the given level.
///
/// Formula: `(level / 0.2)^2` rounded to nearest integer.
///
/// C original: `pow((level / 0.2), 2)` cast from `float + 0.5` to `unsigned int`.
///
/// # Safety
/// Pure math function — no pointer dereferences.
pub unsafe fn clif_getlvlxp(level: i32) -> u32 {
    let xp = (level as f64 / 0.2_f64).powi(2);
    (xp + 0.5) as u32
}

/// Sends the current map weather to the client (opcode 0x1F).
///
///
/// Packet layout (6 bytes total):
/// ```text
/// [0xAA][len_hi][len_lo][0x1F][seq][weather_byte]
/// ```
/// `len = SWAP16(3)` (big-endian 3) — 3 payload bytes after the 3-byte header.
/// `seq` is the per-session sequence counter from `session_increment`.
///
/// The weather byte is taken from `map[sd->bl.m].weather` only when
/// `sd->status.setting_flags & FLAG_WEATHER` is set; otherwise 0.
/// `FLAG_WEATHER = 32` (bit 5).
///
/// # Safety
/// `crate::database::map_db::raw_map_ptr()` must be initialised before calling this function.
pub unsafe fn clif_sendweather(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;

    if !session_exists(fd) {
        return 0;
    }

    let (setting_flags, m) = {
        let r = pe.read();
        (r.player.appearance.setting_flags, r.m)
    };
    let weather_byte: u8 = if setting_flags & FLAG_WEATHER != 0 {
        let map_ptr = crate::database::map_db::raw_map_ptr();
        if map_ptr.is_null() {
            0
        } else {
            (*map_ptr.add(m as usize)).weather
        }
    } else {
        0
    };

    // WFIFOHEADER(fd, 0x1F, 3) expands to (session.h line 97):
    //   WFIFOB(fd, 0) = 0xAA
    //   WFIFOW(fd, 1) = SWAP16(3)              → big-endian 3
    //   WFIFOB(fd, 3) = 0x1F                   (opcode)
    //   WFIFOB(fd, 4) = session_increment(fd)
    // Then: WFIFOB(fd, 5) = weather_byte
    // Total packet = 6 bytes (header 3 + payload 3).
    wfifohead(fd, 6);
    let p = wfifop(fd, 0);
    if p.is_null() {
        return 0;
    }

    wb(p, 0, 0xAA);
    ww_be(p, 1, 3); // SWAP16(3) — payload length
    wb(p, 3, 0x1F); // opcode
    wb(p, 4, session_increment(fd));
    wb(p, 5, weather_byte);

    wfifoset(fd, encrypt(fd) as usize);
    0
}

/// Returns whether a ghost player (`tsd`) should be visible to `sd`.
///
///
/// Logic:
/// - GMs (`sd->status.gm_level != 0`) always see ghosts → 1.
/// - If `map[sd->bl.m].show_ghosts != 0` → 1 (map forces ghost visibility).
/// - If `tsd->status.state != 1` (tsd is not a ghost) → 1.
/// - If `sd->bl.id == tsd->bl.id` (same entity) → 1.
/// - On PvP maps: 1 only if `sd->status.state == 1` AND `sd->optFlags & 256`
/// - On non-PvP maps: always 1.
///
/// # Safety
/// `crate::database::map_db::raw_map_ptr()` must be initialised before calling this function.
pub unsafe fn clif_show_ghost(pe: &PlayerEntity, tpe: &PlayerEntity) -> i32 {
    let (gm_level, m, sd_id, state, opt_flags) = {
        let r = pe.read();
        (
            r.player.identity.gm_level,
            r.m,
            r.id,
            r.player.combat.state,
            r.optFlags,
        )
    };
    let (t_state, t_id) = {
        let r = tpe.read();
        (r.player.combat.state, r.id)
    };

    if gm_level != 0 {
        return 1;
    }

    let map_ptr = crate::database::map_db::raw_map_ptr();
    if map_ptr.is_null() {
        return 1;
    }
    let map_slot = &*map_ptr.add(m as usize);

    // If map shows ghosts, tpe is not a ghost (state != 1), or same entity → visible.
    if map_slot.show_ghosts != 0 || t_state != 1 || sd_id == t_id {
        return 1;
    }

    if map_slot.pvp != 0 {
        if state == 1 && (opt_flags & OPT_FLAG_GHOSTS) != 0 {
            1
        } else {
            0
        }
    } else {
        1
    }
}

/// Sends a user-list notification to the char server for this player.
///
///
/// Queries the DB for online heroes and sends a user-list packet to the client.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_user_list(pe: &PlayerEntity) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }

    let fd = pe.fd;
    let sd_clan = pe.read().player.social.clan as i32;

    // Query DB directly instead of sending 0x300B to char server.
    let heroes = crate::database::blocking_run_async(async {
        crate::database::boards::list_online_heroes(crate::database::get_pool()).await
    });

    let count = heroes.len();

    // Build client packet (matches 0x380A handler in packet.rs).
    // [0]=0xAA, [1..2]=len(BE), [3]=0x36, [4]=0,
    // [5..6]=total(u16 BE), [7..8]=server(u16 BE), [9]=1,
    // then per entry: path_nation(1)+mark_icon(1)+hunter(1)+color(1)+name(len-prefixed)
    let mut buf = vec![0xAAu8, 0, 0, 0x36, 0];
    buf.extend_from_slice(&(count as u16).to_be_bytes());
    buf.extend_from_slice(&(count as u16).to_be_bytes());
    buf.push(1);

    for hero in &heroes {
        let class = hero.class as i32;
        let mark = hero.mark as i32;
        let clan = hero.clan as i32;
        let hunter = hero.hunter as i32;
        let nation = hero.nation as i32;

        let path = if class > 4 {
            crate::database::class_db::path(class)
        } else {
            class
        };
        let icon = crate::database::class_db::icon(class);

        buf.push((path + 16 * nation) as u8);
        buf.push((16 * mark + icon) as u8);
        buf.push(hunter as u8);

        let color = if crate::database::class_db::path(class) == 5 {
            47
        } else if sd_clan != 0 && sd_clan == clan {
            63
        } else {
            143
        };
        buf.push(color);

        let name_bytes = hero.name.as_bytes();
        buf.push(name_bytes.len() as u8);
        buf.extend_from_slice(name_bytes);
    }

    // Write length at [1..2] (BE) — counts bytes from [3] onward.
    let len = (buf.len() - 3) as u16;
    buf[1] = (len >> 8) as u8;
    buf[2] = (len & 0xFF) as u8;

    // Send via encrypt
    wfifohead(fd, buf.len() + 64);
    let p = wfifop(fd, 0);
    if p.is_null() {
        return 0;
    }
    std::ptr::copy_nonoverlapping(buf.as_ptr(), p, buf.len());
    let enc_len = encrypt(fd);
    if enc_len <= 0 {
        tracing::warn!("[map] [packet] clif_user_list encrypt failed fd={}", fd);
        return 0;
    }
    wfifoset(fd, enc_len as usize);
    0
}

/// Logs a hex dump of a byte buffer for debugging.
///
/// The C original uses `printf` to print two lines: hex bytes and printable chars.
/// This port emits a single `tracing::debug!` line with all hex-formatted bytes.
///
/// # Safety
/// `data` must be a valid pointer to at least `len` readable bytes, or null.
/// If `data` is null or `len <= 0`, this function is a no-op.
pub unsafe fn clif_debug(data: *const u8, len: i32) -> i32 {
    if data.is_null() || len <= 0 {
        return 0;
    }
    let bytes = std::slice::from_raw_parts(data, len as usize);
    let hex: String = bytes.iter().map(|b| format!("{b:02X} ")).collect();
    tracing::debug!("[clif_debug] {}", hex.trim_end());
    0
}

// ─── Task 17: URL / popup / disconnect functions ───────────────────────────────

/// Sends a URL packet (opcode 0x66, subtype 0x03) to the client.
///
///
/// Packet layout (total = `url_len + 11`):
/// ```text
/// [0xAA][len_hi][len_lo][0x66][0x03][type][url_len_hi][url_len_lo][...url bytes...]
/// ```
/// where the length field = `url_len + 8` (bytes after the 3-byte header).
/// The length field is written last, matching the C source ordering.
///
/// # Safety
/// - `url` must be a valid, null-terminated C string.
pub unsafe fn clif_sendurl(pe: &PlayerEntity, ty: i32, url: *const i8) -> i32 {
    let fd = pe.fd;

    // C original had no session guard, but project convention adds one defensively.
    // No set_eof here — we just skip the send if the session is already gone.
    if !session_exists(fd) {
        return 0;
    }

    let url_bytes = std::ffi::CStr::from_ptr(url).to_bytes();
    let url_len = url_bytes.len();

    // C had no WFIFOHEAD — add one for safety.
    // Total packet = 3 (framing header) + url_len + 8 (payload before url) = url_len + 11.
    wfifohead(fd, url_len + 11);
    let p = wfifop(fd, 0);
    if p.is_null() {
        return 0;
    }

    wb(p, 0, 0xAA);
    wb(p, 3, 0x66);
    wb(p, 4, 0x03);
    wb(p, 5, ty as u8);
    // WFIFOW(fd, 6) = SWAP16(strlen(url)) — big-endian url length.
    ww_be(p, 6, url_len as u16);
    // Copy url bytes (no null terminator in packet body).
    std::ptr::copy_nonoverlapping(url as *const u8, p.add(8), url_len);
    // Length field written last, matching C ordering.
    // Length = url_len + 8 (payload bytes after the 3-byte framing header).
    ww_be(p, 1, (url_len + 8) as u16);

    wfifoset(fd, encrypt(fd) as usize);
    0
}

/// Logs a connection timeout and marks the session for closure.
///
///
/// Guard checks (matching C source):
/// - If `fd == char_fd` → return 0 (never time out the char-server link).
/// - If `fd <= 1` → return 0 (reserved / stdin / stdout fds).
/// - If session does not exist → return 0.
/// - If `session_get_data(fd)` is null → set eof=12 then return 0.
///
/// On a valid player session, logs `"<name> (IP: a.b.c.d) timed out!"` via
/// `tracing::info!` and sets eof=1.
///
/// # Safety
/// Safe to call with any fd value. No struct dereferences occur before `sd_ptr` is
/// verified non-null. All pointer dereferences follow their respective null checks.
pub unsafe fn clif_timeout(fd: SessionId) -> i32 {
    if fd.raw() == crate::game::map_server::char_fd.load(std::sync::atomic::Ordering::Relaxed) {
        return 0;
    }
    if fd.raw() <= 1 {
        return 0;
    }
    if !session_exists(fd) {
        return 0;
    }

    // Disconnect if the session is gone.
    // in C — set eof then fall through to the nullpo_ret which returns 0.
    let sd_arc = session_get_data(fd);
    let sd_arc = match sd_arc {
        Some(a) => a,
        None => {
            session_set_eof(fd, 12);
            return 0;
        }
    };

    // Decompose IP into four octets (little-endian byte order in sin_addr).
    let raw_ip = session_get_client_ip(fd);
    let a = raw_ip & 0xff;
    let b = (raw_ip >> 8) & 0xff;
    let c = (raw_ip >> 16) & 0xff;
    let d = (raw_ip >> 24) & 0xff;

    let name_display = sd_arc.read().player.identity.name.clone();

    tracing::info!("{} (IP: {}.{}.{}.{}) timed out!", name_display, a, b, c, d);
    session_set_eof(fd, 1);
    0
}

/// Sends a paper-popup display packet (opcode 0x35) to the client.
///
///
/// Packet layout (total = `str_len + 14`):
/// ```text
/// [0xAA][len_hi][len_lo][0x35][0x00][0x00][width][height][0x00][str_hi][str_lo][...string bytes...]
/// ```
/// where the length field = `str_len + 11` (bytes after the 3-byte framing header).
/// Byte 4 is not written in C (WFIFOHEAD leaves it as zero); written explicitly here for safety.
///
/// # Safety
/// - `buf` must be a valid, null-terminated C string.
pub unsafe fn clif_paperpopup(pe: &PlayerEntity, buf: *const i8, width: i32, height: i32) -> i32 {
    let fd = pe.fd;

    if !session_exists(fd) {
        return 0;
    }

    let str_bytes = std::ffi::CStr::from_ptr(buf).to_bytes();
    let str_len = str_bytes.len();

    // C: WFIFOHEAD(fd, strlen(buf) + 11 + 3) — total = str_len + 14.
    wfifohead(fd, str_len + 14);
    let p = wfifop(fd, 0);
    if p.is_null() {
        return 0;
    }

    wb(p, 0, 0xAA);
    // Length field = str_len + 11 (payload bytes after the 3-byte framing header).
    ww_be(p, 1, (str_len + 11) as u16);
    wb(p, 3, 0x35);
    wb(p, 4, 0x00); // not written in C — zero explicitly for safety
    wb(p, 5, 0x00);
    wb(p, 6, width as u8);
    wb(p, 7, height as u8);
    wb(p, 8, 0x00);
    // WFIFOW(fd, 9) = SWAP16(strlen(buf)) — big-endian string length.
    ww_be(p, 9, str_len as u16);
    // C uses strcpy which copies the null terminator; packet body excludes it.
    std::ptr::copy_nonoverlapping(buf as *const u8, p.add(11), str_len);

    wfifoset(fd, encrypt(fd) as usize);
    0
}

/// Sends a paper-popup write packet (opcode 0x1B) to the client.
///
///
/// Identical layout to [`clif_paperpopup`] except:
/// - opcode at byte 3 is `0x1B` instead of `0x35`.
/// - byte 5 carries `invslot` instead of `0x00`.
/// - byte 6 is `0x00`, byte 7 is `width`, byte 8 is `height`.
///
/// # Safety
/// - `buf` must be a valid, null-terminated C string.
pub unsafe fn clif_paperpopupwrite(
    pe: &PlayerEntity,
    buf: *const i8,
    width: i32,
    height: i32,
    invslot: i32,
) -> i32 {
    let fd = pe.fd;

    if !session_exists(fd) {
        return 0;
    }

    let str_bytes = std::ffi::CStr::from_ptr(buf).to_bytes();
    let str_len = str_bytes.len();

    // C: WFIFOHEAD(fd, strlen(buf) + 11 + 3) — total = str_len + 14.
    wfifohead(fd, str_len + 14);
    let p = wfifop(fd, 0);
    if p.is_null() {
        return 0;
    }

    wb(p, 0, 0xAA);
    // Length field = str_len + 11 (payload bytes after the 3-byte framing header).
    ww_be(p, 1, (str_len + 11) as u16);
    wb(p, 3, 0x1B);
    wb(p, 4, 0x00); // not written in C — zero explicitly for safety
    wb(p, 5, invslot as u8);
    wb(p, 6, 0x00);
    wb(p, 7, width as u8);
    wb(p, 8, height as u8);
    // WFIFOW(fd, 9) = SWAP16(strlen(buf)) — big-endian string length.
    ww_be(p, 9, str_len as u16);
    std::ptr::copy_nonoverlapping(buf as *const u8, p.add(11), str_len);

    wfifoset(fd, encrypt(fd) as usize);
    0
}

/// Sends a fixed 7-byte test packet (opcode 0x63) with a per-call incrementing counter.
///
///
/// The C original uses a `static int number` that increments after each send.
/// This port uses a `static SENDTEST_NUMBER: AtomicU8` with wrapping arithmetic.
///
/// Packet layout (7 bytes fixed):
/// ```text
/// [0xAA][0x00][0x04][0x63][0x03][number][0x00]
/// ```
/// Bytes 1–2 are the big-endian length field (= 4, written literally as 0x00, 0x04).
/// The packet is committed via `encrypt(fd)` matching the C `WFIFOSET(fd, encrypt(...))`.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendtest(pe: &PlayerEntity) -> i32 {
    static SENDTEST_NUMBER: AtomicU8 = AtomicU8::new(0);

    let fd = pe.fd;

    if !session_exists(fd) {
        return 0;
    }

    wfifohead(fd, 7);
    let p = wfifop(fd, 0);
    if p.is_null() {
        return 0;
    }

    wb(p, 0, 0xAA);
    wb(p, 1, 0x00);
    wb(p, 2, 0x04);
    wb(p, 3, 0x63);
    wb(p, 4, 0x03);
    wb(p, 5, SENDTEST_NUMBER.load(Ordering::Relaxed));
    wb(p, 6, 0x00);

    wfifoset(fd, encrypt(fd) as usize);
    // Increment after send, matching C post-increment.
    SENDTEST_NUMBER.fetch_add(1, Ordering::Relaxed);
    0
}

/// Logs the disconnect reason for a session and returns.
///
///
/// Returns early (0) when eof == 4 (`ZERO_RECV_ERROR/NORMAL`) — the C source
/// also returns early in that case without printing.
///
/// # Safety
/// No pointer dereferences — reads only the eof flag via `session_get_eof`.
pub unsafe fn clif_print_disconnect(fd: SessionId) -> i32 {
    let eof = session_get_eof(fd);
    if eof == 4 {
        return 0;
    }

    let reason = match eof {
        0x00 | 0x01 => "NORMAL_EOF",
        0x02 => "SOCKET_SEND_ERROR",
        0x03 => "SOCKET_RECV_ERROR",
        0x04 => "ZERO_RECV_ERROR(NORMAL)",
        0x05 => "MISSING_WDATA",
        0x06 => "WDATA_REALLOC",
        0x07 => "NO_MMO_DATA",
        0x08 => "SESSIONDATA_EXISTS",
        0x09 => "PLAYER_CONNECTING",
        0x0A => "INVALID_EXCHANGE",
        0x0B => "ACCEPT_NAMELEN_ERROR",
        0x0C => "PLAYER_TIMEOUT",
        0x0D => "INVALID_PACKET_HEADER",
        0x0E => "WPE_HACK",
        _ => "UNKNOWN",
    };

    tracing::info!("[map] disconnect fd={} reason={}", fd, reason);
    0
}

// ─── Task 18: RFIFO-reader functions ──────────────────────────────────────────

/// Saves the player's paper-popup note text for the given inventory slot.
///
///
/// Packet layout (incoming):
/// ```text
/// [... header ...][slot byte @ 5][copy_len_hi @ 6][copy_len_lo @ 7][... note @ 8 ...]
/// ```
/// - Byte 5:     inventory slot index (u8).
/// - Bytes 6–7:  big-endian u16 copy length (`SWAP16(RFIFOW(fd, 6))`).
/// - Bytes 8+:   note text (up to 300 bytes; clamped here for safety).
///
/// The copy is skipped when the existing note already equals the incoming bytes.
///
/// # Safety
/// - The caller must have validated that `RFIFOREST(fd) >= 8 + copy_len` before calling,
///   matching the C original's requirement.
pub unsafe fn clif_paperpopupwrite_save(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;

    if !session_exists(fd) {
        return 0;
    }

    // RFIFOB(fd, 5) — inventory slot index.
    let rdata = rfifop(fd, 0);
    if rdata.is_null() {
        return 0;
    }

    let slot = *rdata.add(5) as usize;
    // Clamp to valid inventory range (MAX_INVENTORY = 52; C had no bounds check).
    if slot >= MAX_INVENTORY {
        return 0;
    }

    // SWAP16(RFIFOW(fd, 6)) — big-endian u16 copy length.
    let copy_len = u16::from_be_bytes([*rdata.add(6), *rdata.add(7)]) as usize;
    // Clamp to note buffer size (300 bytes).
    let copy_len = copy_len.min(300);

    // Build the incoming note in a local zero-initialised buffer.
    let mut input = [0i8; 300];
    let src = rdata.add(8) as *const i8;
    // SAFETY: copy_len ≤ 300; src points into session rdata (valid for the packet duration).
    std::ptr::copy_nonoverlapping(src, input.as_mut_ptr(), copy_len);
    // Remaining bytes stay zero.

    let note = pe.read().player.inventory.inventory[slot].note;

    // Only update if the note actually changed.
    if note != input {
        std::ptr::copy_nonoverlapping(
            input.as_ptr(),
            pe.write().player.inventory.inventory[slot]
                .note
                .as_mut_ptr(),
            300,
        );
    }
    0
}

/// Reads a new profile picture and profile text from the incoming packet.
///
///
/// Packet layout (incoming):
/// ```text
/// [... header ...][pic_len_hi @ 5][pic_len_lo @ 6][...pic bytes (profilepic_size bytes)...]
///                  [txt_len @ 5+profilepic_size][...txt bytes (profile_size bytes)...]
/// ```
/// where `profilepic_size = raw_u16 + 2` (u16 wrapping) and `profile_size = raw_byte + 1` (u8 wrapping).
/// - Bytes 5–6: big-endian u16 raw image length; `profilepic_size = raw + 2` (u16 wrapping).
/// - Byte at `5 + profilepic_size`: raw text length byte; `profile_size = raw + 1` (u8 wrapping).
/// - Bytes `5 .. 5+profilepic_size`: picture data.
/// - Bytes `5+profilepic_size .. 5+profilepic_size+profile_size`: profile text.
///
/// # Safety
/// - The caller must have validated that `RFIFOREST(fd) >= 5 + profilepic_size + 1 + profile_size`
///   before calling, matching the C original's requirement.
pub unsafe fn clif_changeprofile(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;

    // Project convention: guard against closed sessions (C original omitted this).
    if !session_exists(fd) {
        return 0;
    }

    let rdata = rfifop(fd, 0);
    if rdata.is_null() {
        return 0;
    }

    // SWAP16(RFIFOW(fd, 5)) + 2 — matches C `unsigned short profilepic_size = ... + 2`.
    // wrapping_add preserves C's u16 wrap-on-overflow semantics (e.g. 65534 + 2 = 0).
    let profilepic_size: u16 = u16::from_be_bytes([*rdata.add(5), *rdata.add(6)]).wrapping_add(2);
    let profilepic_usize = profilepic_size as usize; // always ≤ 65535 — safe index

    // RFIFOB(fd, 5 + profilepic_size) + 1 — matches C `unsigned char profile_size = ... + 1`.
    // wrapping_add preserves C's u8 wrap-on-overflow semantics (255 + 1 = 0).
    let profile_size: u8 = (*rdata.add(5 + profilepic_usize)).wrapping_add(1);
    let profile_usize = profile_size as usize; // always ≤ 255 — safe index

    // Write sizes back to pe.
    {
        let mut w = pe.write();
        w.profilepic_size = profilepic_size;
        w.profile_size = profile_size;
    }

    // Copy picture data: RFIFOP(fd, 5), length = profilepic_size.
    std::ptr::copy_nonoverlapping(
        rdata.add(5) as *const i8,
        pe.write().profilepic_data.as_mut_ptr(),
        profilepic_usize,
    );

    // Copy profile text: RFIFOP(fd, 5 + profilepic_size), length = profile_size.
    std::ptr::copy_nonoverlapping(
        rdata.add(5 + profilepic_usize) as *const i8,
        pe.write().profile_data.as_mut_ptr(),
        profile_usize,
    );

    0
}

/// Validates that the byte immediately following the current packet is a valid
/// framing byte (`0xAA`). Sets eof=1 and returns 1 if the framing is wrong.
///
/// Not declared in any `.h` file.
///
/// Logic:
/// - If `RFIFOREST(fd) > len` and the byte at `fd[len]` is not `0xAA`, the
///   session has framing corruption → `session_set_eof(fd, 1)`, return 1.
/// - Otherwise return 0 (either there is no next byte yet, or it is valid).
///
/// # Safety
/// Pure fd-based — no struct dereferences.
pub unsafe fn check_packet_size(fd: SessionId, len: i32) -> i32 {
    if len < 0 {
        return 0;
    }
    let len_usize = len as usize;

    // RFIFOREST(fd) > (size_t)len
    if rfiforest(fd) as usize > len_usize {
        // RFIFOB(fd, len) — byte immediately after the current packet.
        let rdata = rfifop(fd, 0);
        if rdata.is_null() {
            return 0;
        }
        if *rdata.add(len_usize) != 0xAA {
            session_set_eof(fd, 1);
            return 1;
        }
    }
    0
}

// ─── Task 19: Mob broadcast ────────────────────────────────────────────────────

/// Broadcasts a mob's facing direction to all nearby players (area, excluding self).
///
/// Builds a 16-byte local buffer and sends it via `clif_send` with `AREA_WOS = 5`.
///
/// Packet layout (16 bytes):
/// ```text
/// [0xAA][0x00][0x07][0x11][0x03][mob_id (4 bytes BE)][side][0×6 zeros]
/// ```
/// This uses `WBUF*` macros — a pre-allocated stack buffer, not a session WFIFO.
/// The buffer is passed raw to `clif_send` which routes it to nearby sessions.
///
/// # Safety
/// `mob` must be a valid, non-null pointer to an initialised [`MobSpawnData`].
pub unsafe fn clif_sendmob_side(mob: *mut crate::game::mob::MobSpawnData) -> i32 {
    if mob.is_null() {
        return 0;
    }
    let mob = &*mob;

    // Build 16-byte stack buffer matching C's WBUFB/WBUFL writes.
    // Bytes 10–15 are zero-initialized (C left them uninitialized; zero is safe).
    let mut buf = [0u8; 16];
    buf[0] = 0xAA;
    buf[1] = 0x00;
    buf[2] = 0x07;
    buf[3] = 0x11;
    buf[4] = 0x03;
    // WBUFL(buf, 5) = SWAP32(mob->bl.id) — big-endian mob id.
    let id_be = mob.id.to_be_bytes();
    buf[5..9].copy_from_slice(&id_be);
    // WBUFB(buf, 9) = mob->side — cast from i32 to u8.
    buf[9] = mob.side as u8;

    // clif_send(buf, 16, BroadcastSrc { id: &mob->bl, m: AREA_WOS=5)
    super::clif_send(
        buf.as_ptr(),
        16,
        super::BroadcastSrc {
            id: mob.id,
            m: mob.m,
            x: mob.x,
            y: mob.y,
            bl_type: BL_MOB as u8,
        },
        5,
    )
}

// ─── clif_updatestate / broadcast_update_state ────────────────────────────────

use crate::game::block::AreaType;
use crate::game::block_grid;
use crate::game::pc::{
    EQ_ARMOR, EQ_BOOTS, EQ_COAT, EQ_CROWN, EQ_FACEACC, EQ_FACEACCTWO, EQ_HELM, EQ_MANTLE,
    EQ_NECKLACE, EQ_SHIELD, EQ_WEAP, FLAG_HELM, FLAG_NECKLACE, OPT_FLAG_STEALTH,
};

use crate::common::constants::entity::player::OPT_FLAG_GHOSTS;

// Direct Rust imports (with _us suffix aliases to avoid name conflicts)
use crate::database::item_db;
use crate::game::map_parse::groups::clif_isingroup as clif_isingroup_us;
use crate::game::map_parse::movement::clif_charspecific as clif_charspecific_us;
use crate::game::pc::pc_isequip as pc_isequip_us;

/// Write the state packet for `sd` (the player being viewed) into `src_sd`'s
/// (the viewer's) write buffer.
///
/// In C terms: `sd` = `va_arg(ap, USER*)` (the player whose state is sent),
/// `src_sd` = `(USER*)bl` (the viewer who receives the packet).
unsafe fn write_state_packet(sd: &PlayerEntity, src_sd: &PlayerEntity) {
    let sd_r_guard = sd.read();
    let src_r_guard = src_sd.read();
    let sd_r = &*sd_r_guard;
    let src_r = &*src_r_guard;
    // Bridge local for callees (pc_isequip, clif_isingroup) still expecting *mut.
    // Derived from read guard — safe because callees only read through this pointer.
    let sd_mut =
        sd_r as *const crate::game::pc::MapSessionData as *mut crate::game::pc::MapSessionData;

    // Guard: bail if broadcaster's session is gone (matches C clif_updatestate).
    if !session_exists(sd_r.fd) {
        return;
    }

    let src_fd = src_r.fd;

    wfifohead(src_fd, 512);
    let p = wfifop(src_fd, 0);
    if p.is_null() {
        return;
    }

    wb(p, 0, 0xAA);
    wb(p, 3, 0x1D);
    // WFIFOL(src_sd->fd, 5) = SWAP32(sd->bl.id)  — big-endian
    wl_be(p, 5, sd_r.id);

    if sd_r.player.combat.state == 4 {
        // Disguised state: compact packet with name only.
        wb(p, 9, 1);
        wb(p, 10, 15);
        wb(p, 11, sd_r.player.combat.state as u8);
        // WFIFOW(src_sd->fd, 12) = SWAP16(sd->disguise + 32768)
        ww_be(p, 12, sd_r.disguise.wrapping_add(32768));
        wb(p, 14, sd_r.disguise_color as u8);

        let name_bytes = sd_r.player.identity.name.as_bytes();
        let name_len = name_bytes.len();

        wb(p, 16, name_len as u8);
        // len += strlen(name) + 1
        let len = name_len + 1;
        let dst = wfifop(src_fd, 17);
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(name_bytes.as_ptr(), dst, name_len);
        }

        // WFIFOW(src_sd->fd, 1) = SWAP16(len + 13)
        ww_be(p, 1, (len + 13) as u16);
        wfifoset(src_fd, encrypt(src_fd) as usize);
    } else {
        // Normal / stealth / invisible states.

        // WFIFOW(src_sd->fd, 9) = SWAP16(sd->status.sex)
        ww_be(p, 9, sd_r.player.identity.sex as u16);

        // Invisibility/stealth state: show invisible state (5) to GMs and group members;
        // non-GMs see the raw state.
        let invis_cond = (sd_r.player.combat.state == 2 || (sd_r.optFlags & OPT_FLAG_STEALTH) != 0)
            && sd_r.id != src_r.id
            && (src_r.player.identity.gm_level != 0
                || clif_isingroup_us(src_sd, sd_mut) != 0
                || (sd_r.gfx.dye == src_r.gfx.dye && sd_r.gfx.dye != 0 && src_r.gfx.dye != 0));

        if invis_cond {
            wb(p, 11, 5);
        } else {
            wb(p, 11, sd_r.player.combat.state as u8);
        }

        // Stealth-without-state override: show as "invisible" state 2.
        // Note: clif_charlook_sub has || bl.id == src_sd.id; C original did not — that port may have an extra clause.
        if (sd_r.optFlags & OPT_FLAG_STEALTH) != 0
            && sd_r.player.combat.state == 0
            && src_r.player.identity.gm_level == 0
        {
            wb(p, 11, 2);
        }

        // Disguise id.
        if sd_r.player.combat.state == 3 {
            ww_be(p, 12, sd_r.disguise);
        } else {
            ww_be(p, 12, 0u16);
        }

        wb(p, 14, sd_r.speed as u8);
        wb(p, 15, 0);
        wb(p, 16, sd_r.player.appearance.face as u8);
        wb(p, 17, sd_r.player.appearance.hair as u8);
        wb(p, 18, sd_r.player.appearance.hair_color as u8);
        wb(p, 19, sd_r.player.appearance.face_color as u8);
        wb(p, 20, sd_r.player.appearance.skin_color as u8);

        // armor / coat  (offsets 21–23)
        if pc_isequip_us(sd_mut, EQ_ARMOR) == 0 {
            ww_be(p, 21, sd_r.player.identity.sex as u16);
        } else {
            if sd_r.player.inventory.equip[EQ_ARMOR as usize].custom_look != 0 {
                ww_be(
                    p,
                    21,
                    sd_r.player.inventory.equip[EQ_ARMOR as usize].custom_look as u16,
                );
            } else {
                ww_be(
                    p,
                    21,
                    item_db::search(pc_isequip_us(sd_mut, EQ_ARMOR) as u32).look as u16,
                );
            }
            if sd_r.player.appearance.armor_color > 0 {
                wb(p, 23, sd_r.player.appearance.armor_color as u8);
            } else if sd_r.player.inventory.equip[EQ_ARMOR as usize].custom_look != 0 {
                wb(
                    p,
                    23,
                    sd_r.player.inventory.equip[EQ_ARMOR as usize].custom_look_color as u8,
                );
            } else {
                wb(
                    p,
                    23,
                    item_db::search(pc_isequip_us(sd_mut, EQ_ARMOR) as u32).look_color as u8,
                );
            }
        }
        if pc_isequip_us(sd_mut, EQ_COAT) != 0 {
            ww_be(
                p,
                21,
                item_db::search(pc_isequip_us(sd_mut, EQ_COAT) as u32).look as u16,
            );
            if sd_r.player.appearance.armor_color > 0 {
                wb(p, 23, sd_r.player.appearance.armor_color as u8);
            } else {
                wb(
                    p,
                    23,
                    item_db::search(pc_isequip_us(sd_mut, EQ_COAT) as u32).look_color as u8,
                );
            }
        }

        // weapon  (offsets 24–26)
        if pc_isequip_us(sd_mut, EQ_WEAP) == 0 {
            ww_be(p, 24, 0xFFFF);
            wb(p, 26, 0x0);
        } else if sd_r.player.inventory.equip[EQ_WEAP as usize].custom_look != 0 {
            ww_be(
                p,
                24,
                sd_r.player.inventory.equip[EQ_WEAP as usize].custom_look as u16,
            );
            wb(
                p,
                26,
                sd_r.player.inventory.equip[EQ_WEAP as usize].custom_look_color as u8,
            );
        } else {
            ww_be(
                p,
                24,
                item_db::search(pc_isequip_us(sd_mut, EQ_WEAP) as u32).look as u16,
            );
            wb(
                p,
                26,
                item_db::search(pc_isequip_us(sd_mut, EQ_WEAP) as u32).look_color as u8,
            );
        }

        // shield  (offsets 27–29)
        if pc_isequip_us(sd_mut, EQ_SHIELD) == 0 {
            ww_be(p, 27, 0xFFFF);
            wb(p, 29, 0);
        } else if sd_r.player.inventory.equip[EQ_SHIELD as usize].custom_look != 0 {
            ww_be(
                p,
                27,
                sd_r.player.inventory.equip[EQ_SHIELD as usize].custom_look as u16,
            );
            wb(
                p,
                29,
                sd_r.player.inventory.equip[EQ_SHIELD as usize].custom_look_color as u8,
            );
        } else {
            ww_be(
                p,
                27,
                item_db::search(pc_isequip_us(sd_mut, EQ_SHIELD) as u32).look as u16,
            );
            wb(
                p,
                29,
                item_db::search(pc_isequip_us(sd_mut, EQ_SHIELD) as u32).look_color as u8,
            );
        }

        // helm  (offsets 30–32)
        if pc_isequip_us(sd_mut, EQ_HELM) == 0
            || (sd_r.player.appearance.setting_flags & FLAG_HELM) == 0
            || item_db::search(pc_isequip_us(sd_mut, EQ_HELM) as u32).look == -1
        {
            wb(p, 30, 0);
            ww_be(p, 31, 0xFFFF);
        } else {
            wb(p, 30, 1);
            if sd_r.player.inventory.equip[EQ_HELM as usize].custom_look != 0 {
                wb(
                    p,
                    31,
                    sd_r.player.inventory.equip[EQ_HELM as usize].custom_look as u8,
                );
                wb(
                    p,
                    32,
                    sd_r.player.inventory.equip[EQ_HELM as usize].custom_look_color as u8,
                );
            } else {
                wb(
                    p,
                    31,
                    item_db::search(pc_isequip_us(sd_mut, EQ_HELM) as u32).look as u8,
                );
                wb(
                    p,
                    32,
                    item_db::search(pc_isequip_us(sd_mut, EQ_HELM) as u32).look_color as u8,
                );
            }
        }

        // face acc  (offsets 33–35)
        if pc_isequip_us(sd_mut, EQ_FACEACC) == 0 {
            ww_be(p, 33, 0xFFFF);
            wb(p, 35, 0x0);
        } else {
            ww_be(
                p,
                33,
                item_db::search(pc_isequip_us(sd_mut, EQ_FACEACC) as u32).look as u16,
            );
            wb(
                p,
                35,
                item_db::search(pc_isequip_us(sd_mut, EQ_FACEACC) as u32).look_color as u8,
            );
        }

        // crown  (offsets 36–38; also writes byte 30)
        if pc_isequip_us(sd_mut, EQ_CROWN) == 0 {
            ww_be(p, 36, 0xFFFF);
            wb(p, 38, 0x0);
        } else {
            wb(p, 30, 0); // crown overrides helm flag at byte 30
            if sd_r.player.inventory.equip[EQ_CROWN as usize].custom_look != 0 {
                ww_be(
                    p,
                    36,
                    sd_r.player.inventory.equip[EQ_CROWN as usize].custom_look as u16,
                );
                wb(
                    p,
                    38,
                    sd_r.player.inventory.equip[EQ_CROWN as usize].custom_look_color as u8,
                );
            } else {
                ww_be(
                    p,
                    36,
                    item_db::search(pc_isequip_us(sd_mut, EQ_CROWN) as u32).look as u16,
                );
                wb(
                    p,
                    38,
                    item_db::search(pc_isequip_us(sd_mut, EQ_CROWN) as u32).look_color as u8,
                );
            }
        }

        // face acc two  (offsets 39–41)
        if pc_isequip_us(sd_mut, EQ_FACEACCTWO) == 0 {
            ww_be(p, 39, 0xFFFF);
            wb(p, 41, 0x0);
        } else {
            ww_be(
                p,
                39,
                item_db::search(pc_isequip_us(sd_mut, EQ_FACEACCTWO) as u32).look as u16,
            );
            wb(
                p,
                41,
                item_db::search(pc_isequip_us(sd_mut, EQ_FACEACCTWO) as u32).look_color as u8,
            );
        }

        // mantle  (offsets 42–44)
        if pc_isequip_us(sd_mut, EQ_MANTLE) == 0 {
            ww_be(p, 42, 0xFFFF);
            wb(p, 44, 0xFF);
        } else {
            ww_be(
                p,
                42,
                item_db::search(pc_isequip_us(sd_mut, EQ_MANTLE) as u32).look as u16,
            );
            wb(
                p,
                44,
                item_db::search(pc_isequip_us(sd_mut, EQ_MANTLE) as u32).look_color as u8,
            );
        }

        // necklace  (offsets 45–47)
        if pc_isequip_us(sd_mut, EQ_NECKLACE) == 0
            || (sd_r.player.appearance.setting_flags & FLAG_NECKLACE) == 0
            || item_db::search(pc_isequip_us(sd_mut, EQ_NECKLACE) as u32).look == -1
        {
            ww_be(p, 45, 0xFFFF);
            wb(p, 47, 0x0);
        } else {
            ww_be(
                p,
                45,
                item_db::search(pc_isequip_us(sd_mut, EQ_NECKLACE) as u32).look as u16,
            );
            wb(
                p,
                47,
                item_db::search(pc_isequip_us(sd_mut, EQ_NECKLACE) as u32).look_color as u8,
            );
        }

        // boots  (offsets 48–50)
        if pc_isequip_us(sd_mut, EQ_BOOTS) == 0 {
            ww_be(p, 48, sd_r.player.identity.sex as u16);
            wb(p, 50, 0x0);
        } else if sd_r.player.inventory.equip[EQ_BOOTS as usize].custom_look != 0 {
            ww_be(
                p,
                48,
                sd_r.player.inventory.equip[EQ_BOOTS as usize].custom_look as u16,
            );
            wb(
                p,
                50,
                sd_r.player.inventory.equip[EQ_BOOTS as usize].custom_look_color as u8,
            );
        } else {
            ww_be(
                p,
                48,
                item_db::search(pc_isequip_us(sd_mut, EQ_BOOTS) as u32).look as u16,
            );
            wb(
                p,
                50,
                item_db::search(pc_isequip_us(sd_mut, EQ_BOOTS) as u32).look_color as u8,
            );
        }

        // title/outline/color bytes 51–53
        wb(p, 51, 0);
        wb(p, 52, 128);
        wb(p, 53, 0);

        // Title color: hidden for invisible chars unless matching dye group.
        if sd_r.gfx.dye != 0
            && src_r.gfx.dye != 0
            && src_r.gfx.dye != sd_r.gfx.dye
            && sd_r.player.combat.state == 2
        {
            wb(p, 51, 0);
        } else if sd_r.gfx.dye != 0 {
            wb(p, 51, sd_r.gfx.title_color);
        } else {
            wb(p, 51, 0);
        }

        // Name field (offset 54 = length, 55+ = name bytes).
        let name_bytes = sd_r.player.identity.name.as_bytes();
        let name_len = name_bytes.len();

        // Clan and group color at byte 53.
        if src_r.player.social.clan == sd_r.player.social.clan
            && src_r.player.social.clan > 0
            && src_r.player.identity.id != sd_r.player.identity.id
        {
            wb(p, 53, 3);
        }
        if clif_isingroup_us(src_sd, sd_mut) != 0
            && sd_r.player.identity.id != src_r.player.identity.id
        {
            wb(p, 53, 2);
        }

        let len = if sd_r.player.combat.state != 5 && sd_r.player.combat.state != 2 {
            wb(p, 54, name_len as u8);
            let dst = wfifop(src_fd, 55);
            if !dst.is_null() {
                std::ptr::copy_nonoverlapping(name_bytes.as_ptr(), dst, name_len);
            }
            name_len
        } else {
            wb(p, 54, 0);
            0
        };

        // GM/clone gfx override: overwrite appearance fields.
        if (sd_r.player.identity.gm_level != 0 && sd_r.gfx.toggle != 0) || sd_r.clone != 0 {
            let gfx = &sd_r.gfx;
            wb(p, 16, gfx.face);
            wb(p, 17, gfx.hair);
            wb(p, 18, gfx.chair);
            wb(p, 19, gfx.cface);
            wb(p, 20, gfx.cskin);
            ww_be(p, 21, gfx.armor);
            if gfx.dye > 0 {
                wb(p, 23, gfx.dye);
            } else {
                wb(p, 23, gfx.carmor);
            }
            ww_be(p, 24, gfx.weapon);
            wb(p, 26, gfx.cweapon);
            ww_be(p, 27, gfx.shield);
            wb(p, 29, gfx.cshield);

            if gfx.helm < 255 {
                wb(p, 30, 1);
            } else if gfx.crown < 65535 {
                wb(p, 30, 0xFF);
            } else {
                wb(p, 30, 0);
            }

            wb(p, 31, gfx.helm as u8);
            wb(p, 32, gfx.chelm);
            ww_be(p, 33, gfx.face_acc);
            wb(p, 35, gfx.cface_acc);
            ww_be(p, 36, gfx.crown);
            wb(p, 38, gfx.ccrown);
            ww_be(p, 39, gfx.face_acc_t);
            wb(p, 41, gfx.cface_acc_t);
            ww_be(p, 42, gfx.mantle);
            wb(p, 44, gfx.cmantle);
            ww_be(p, 45, gfx.necklace);
            wb(p, 47, gfx.cnecklace);
            ww_be(p, 48, gfx.boots);
            wb(p, 50, gfx.cboots);

            // gfx name override.
            let gfx_name_ptr = gfx.name.as_ptr();
            let gfx_name_len = libc::strlen(gfx_name_ptr);
            let visible = sd_r.player.combat.state != 2 && sd_r.player.combat.state != 5;
            let gfx_name_empty = gfx_name_len == 0;
            let final_len = if visible && !gfx_name_empty {
                wb(p, 52, gfx_name_len as u8);
                let dst = wfifop(src_fd, 53);
                if !dst.is_null() {
                    std::ptr::copy_nonoverlapping(gfx_name_ptr as *const u8, dst, gfx_name_len);
                }
                gfx_name_len
            } else {
                wb(p, 52, 0);
                1
            };

            ww_be(p, 1, (final_len + 55 + 3) as u16);
            wfifoset(src_fd, encrypt(src_fd) as usize);
            // Fall through to ghost logic below.
        } else {
            ww_be(p, 1, (len + 55 + 3) as u16);
            wfifoset(src_fd, encrypt(src_fd) as usize);
        }
    }

    // Ghost logic — after the packet is sent, handle "show_ghosts" map setting.
    {
        let map_ptr = crate::database::map_db::raw_map_ptr();
        if !map_ptr.is_null() {
            let (sd_m, sd_state, sd_id) = {
                let r = sd.read();
                (r.m, r.player.combat.state, r.id)
            };
            let (src_id, src_fd, src_state, src_opt) = {
                let r = src_sd.read();
                (r.id, r.fd, r.player.combat.state, r.optFlags)
            };
            let slot = &*map_ptr.add(sd_m as usize);
            if slot.show_ghosts != 0 && sd_state == 1 && src_id != sd_id {
                if src_state != 1 && (src_opt & OPT_FLAG_GHOSTS) == 0 {
                    // Send a 9-byte "ghost" packet to src_sd.
                    // C re-used committed WFIFO without a new WFIFOHEAD; this explicit head is safer.
                    wfifohead(src_fd, 9);
                    let p2 = wfifop(src_fd, 0);
                    if !p2.is_null() {
                        wb(p2, 0, 0xAA);
                        wb(p2, 1, 0x00);
                        wb(p2, 2, 0x06);
                        wb(p2, 3, 0x0E);
                        wb(p2, 4, 0x03);
                        wl_be(p2, 5, sd_id);
                        wfifoset(src_fd, encrypt(src_fd) as usize);
                    }
                } else {
                    clif_charspecific_us(src_id as i32, sd_id as i32);
                }
            }
        }
    }
}

/// Broadcast `src`'s appearance state to all PCs in the area.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn broadcast_update_state(src: &PlayerEntity) {
    let (m, x, y) = {
        let r = src.read();
        (r.m as i32, r.x as i32, r.y as i32)
    };
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let slot = &*crate::database::map_db::raw_map_ptr().add(m as usize);
        let ids =
            block_grid::ids_in_area(grid, x, y, AreaType::Area, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                write_state_packet(src, &pc_arc);
            }
        }
    }
}

// ─── clif_clickonplayer ───────────────────────────────────────────────────────

use crate::common::player::legends::MAX_LEGENDS;
use crate::game::pc::{map_msg, FLAG_EXCHANGE, FLAG_GROUP};

// Direct Rust imports (with _cop suffix aliases to avoid name conflicts)
use crate::database::clan_db;
use crate::database::class_db::name as classdb_name_fn;
use crate::game::client::handlers::{clif_getName, clif_isregistered};

// map_id2sd_cop: typed lookup returning Arc<PlayerEntity> for use in clif_clickonplayer.
#[inline]
fn map_id2sd_cop(id: u32) -> Option<std::sync::Arc<PlayerEntity>> {
    crate::game::map_server::map_id2sd_pc(id)
}

/// Replace first occurrence of `orig` in `src` with `rep`. Returns a pointer into
/// the caller-provided `buf`.
///
/// # Safety
/// All pointer arguments must be valid, null-terminated C strings for the duration
/// of the call.  The returned pointer is valid as long as `buf` is alive.
unsafe fn replace_str_rust(
    src: *const i8,
    orig: &[u8],
    rep: *const i8,
    buf: &mut [u8; 4096],
) -> *const i8 {
    // Strip trailing NUL(s) from orig so orig_len is the actual string length.
    // Callers may pass NUL-terminated byte literals (e.g. b"$player\0"); strstr
    // needs the NUL excluded, and `tail` must point past the matched bytes only.
    let orig_bytes = match orig.iter().position(|&b| b == 0) {
        Some(n) => &orig[..n],
        None => orig,
    };
    // Fast path: if orig is not present, return src unchanged.
    let p = libc::strstr(src, orig_bytes.as_ptr() as *const i8);
    if p.is_null() {
        return src;
    }
    let prefix_len = (p as usize) - (src as usize);
    let rep_len = libc::strlen(rep);
    let tail = p.add(orig_bytes.len()); // points past the matched orig bytes
                                        // Copy prefix.
    std::ptr::copy_nonoverlapping(src as *const u8, buf.as_mut_ptr(), prefix_len.min(4095));
    // Append rep.
    let after_prefix = prefix_len.min(4095);
    let copy_rep = rep_len.min(4095 - after_prefix);
    std::ptr::copy_nonoverlapping(
        rep as *const u8,
        buf.as_mut_ptr().add(after_prefix),
        copy_rep,
    );
    // Append tail.
    let after_rep = after_prefix + copy_rep;
    let tail_len = libc::strlen(tail).min(4095 - after_rep);
    std::ptr::copy_nonoverlapping(tail as *const u8, buf.as_mut_ptr().add(after_rep), tail_len);
    buf[after_rep + tail_len] = 0;
    buf.as_ptr() as *const i8
}

/// Send the player inspection/profile packet to `sd` (viewer) for `bl` (target player).
///
///
/// `target_id` must be the entity ID of the player being clicked on.
/// [`MapSessionData`] (i.e. `bl_type == BL_PC`), retrievable via `map_id2sd`.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn clif_clickonplayer(pe: &PlayerEntity, target_id: u32) -> i32 {
    let tpe = match map_id2sd_cop(target_id) {
        Some(arc) => arc,
        None => return 0,
    };
    let tsd = &*tpe; // typed ref used for field accesses below

    let fd = pe.fd;

    if !session_exists(fd) {
        return 0;
    }

    // Reserve worst-case buffer: equip_status up to 255 bytes, profile data, etc.
    wfifohead(fd, 65535);
    let p = wfifop(fd, 0);
    if p.is_null() {
        return 0;
    }

    wb(p, 0, 0xAA);
    wb(p, 3, 0x34);

    // `len` tracks the number of dynamic bytes written, starting after the 5-byte base header.
    // All writes use absolute offset = len + 5  (for byte fields) or len + 6 (for data fields).
    // This matches the C pattern: WFIFOB(fd, len+5)=count, WFIFOP(fd, len+6)=data, len+=n+1.
    let mut len: usize = 0;

    // ── Title ─────────────────────────────────────────────────────────────────
    {
        let title = tsd.read().player.identity.title.clone();
        let title_bytes = title.as_bytes();
        let title_len = title_bytes.len();
        if title_len > 0 {
            wb(p, len + 5, title_len as u8);
            std::ptr::copy_nonoverlapping(title_bytes.as_ptr(), p.add(len + 6), title_len);
            len += title_len + 1;
        } else {
            wb(p, len + 5, 0);
            len += 1;
        }
    }

    // ── Clan name ─────────────────────────────────────────────────────────────
    {
        let t_clan = tsd.read().player.social.clan;
        if t_clan > 0 {
            let clan_name = clan_db::name(t_clan as i32);
            if !clan_name.is_null() {
                let clan_len = libc::strlen(clan_name);
                wb(p, len + 5, clan_len as u8);
                std::ptr::copy_nonoverlapping(clan_name as *const u8, p.add(len + 6), clan_len);
                len += clan_len + 1;
            } else {
                wb(p, len + 5, 0);
                len += 1;
            }
        } else {
            wb(p, len + 5, 0);
            len += 1;
        }
    }

    // ── Clan title ────────────────────────────────────────────────────────────
    {
        let ctitle = tsd.read().player.social.clan_title.clone();
        let ctitle_bytes = ctitle.as_bytes();
        let ctitle_len = ctitle_bytes.len();
        if ctitle_len > 0 {
            wb(p, len + 5, ctitle_len as u8);
            std::ptr::copy_nonoverlapping(ctitle_bytes.as_ptr(), p.add(len + 6), ctitle_len);
            len += ctitle_len + 1;
        } else {
            wb(p, len + 5, 0);
            len += 1;
        }
    }

    // ── Class name ────────────────────────────────────────────────────────────
    {
        let (t_class, t_mark) = {
            let r = tsd.read();
            (
                r.player.progression.class as i32,
                r.player.progression.mark as i32,
            )
        };
        let cn = classdb_name_fn(t_class, t_mark);
        let cn_bytes = cn.as_bytes();
        if !cn_bytes.is_empty() {
            wb(p, len + 5, cn_bytes.len() as u8);
            std::ptr::copy_nonoverlapping(cn_bytes.as_ptr(), p.add(len + 6), cn_bytes.len());
            len += cn_bytes.len() + 1;
        } else {
            wb(p, len + 5, 0);
            len += 1;
        }
    }

    // ── Player name ───────────────────────────────────────────────────────────
    {
        let t_name = tsd.read().player.identity.name.clone();
        let name_bytes = t_name.as_bytes();
        let name_len = name_bytes.len();
        wb(p, len + 5, name_len as u8);
        std::ptr::copy_nonoverlapping(name_bytes.as_ptr(), p.add(len + 6), name_len);
        len += name_len; // C: len += strlen(name)  (no +1 here, intentional)
    }

    // ── Fixed appearance fields (offsets relative to len+6 after name) ────────
    {
        let r = tsd.read();
        // WFIFOW(fd, len+6) = SWAP16(sex)
        ww_be(p, len + 6, r.player.identity.sex as u16);
        // WFIFOB(fd, len+8) = state
        wb(p, len + 8, r.player.combat.state as u8);
        // WFIFOW(fd, len+9) = SWAP16(0)  — default (overridden below for disguise states)
        ww_be(p, len + 9, 0);
        // WFIFOB(fd, len+11) = speed
        wb(p, len + 11, r.speed as u8);

        if r.player.combat.state == 3 {
            ww_be(p, len + 9, r.disguise);
        } else if r.player.combat.state == 4 {
            ww_be(p, len + 9, r.disguise.wrapping_add(32768));
            wb(p, len + 11, r.disguise_color as u8);
        }

        wb(p, len + 12, 0);
        wb(p, len + 13, r.player.appearance.face as u8);
        wb(p, len + 14, r.player.appearance.hair as u8);
        wb(p, len + 15, r.player.appearance.hair_color as u8);
        wb(p, len + 16, r.player.appearance.face_color as u8);
        wb(p, len + 17, r.player.appearance.skin_color as u8);
    }

    len += 14; // advances past the 14-byte fixed block (bytes 6..17 = 12 bytes + 2 for sw)

    // ── Armor / coat slot (look + color) ──────────────────────────────────────
    // Writes at len+4 (ww) and len+6 (wb), then len += 3.
    // pc_isequip_us still takes *mut MapSessionData — derived from read guard (read-only).
    let tsd_ptr = &*tsd.read() as *const crate::game::pc::MapSessionData
        as *mut crate::game::pc::MapSessionData;
    if pc_isequip_us(tsd_ptr, EQ_ARMOR) == 0 {
        ww_be(p, len + 4, tsd.read().player.identity.sex as u16);
    } else {
        if tsd.read().player.inventory.equip[EQ_ARMOR as usize].custom_look != 0 {
            ww_be(
                p,
                len + 4,
                tsd.read().player.inventory.equip[EQ_ARMOR as usize].custom_look as u16,
            );
        } else {
            ww_be(
                p,
                len + 4,
                item_db::search(pc_isequip_us(tsd_ptr, EQ_ARMOR) as u32).look as u16,
            );
        }
        if tsd.read().player.appearance.armor_color > 0 {
            wb(p, len + 6, tsd.read().player.appearance.armor_color as u8);
        } else if tsd.read().player.inventory.equip[EQ_ARMOR as usize].custom_look != 0 {
            wb(
                p,
                len + 6,
                tsd.read().player.inventory.equip[EQ_ARMOR as usize].custom_look_color as u8,
            );
        } else {
            wb(
                p,
                len + 6,
                item_db::search(pc_isequip_us(tsd_ptr, EQ_ARMOR) as u32).look_color as u8,
            );
        }
    }
    // EQ_COAT overrides armor look if equipped.
    if pc_isequip_us(tsd_ptr, EQ_COAT) != 0 {
        ww_be(
            p,
            len + 4,
            item_db::search(pc_isequip_us(tsd_ptr, EQ_COAT) as u32).look as u16,
        );
        if tsd.read().player.appearance.armor_color > 0 {
            wb(p, len + 6, tsd.read().player.appearance.armor_color as u8);
        } else {
            wb(
                p,
                len + 6,
                item_db::search(pc_isequip_us(tsd_ptr, EQ_COAT) as u32).look_color as u8,
            );
        }
    }
    len += 3;

    // ── Weapon slot ───────────────────────────────────────────────────────────
    if pc_isequip_us(tsd_ptr, EQ_WEAP) == 0 {
        ww_be(p, len + 4, 0xFFFF);
        wb(p, len + 6, 0);
    } else if tsd.read().player.inventory.equip[EQ_WEAP as usize].custom_look != 0 {
        ww_be(
            p,
            len + 4,
            tsd.read().player.inventory.equip[EQ_WEAP as usize].custom_look as u16,
        );
        wb(
            p,
            len + 6,
            tsd.read().player.inventory.equip[EQ_WEAP as usize].custom_look_color as u8,
        );
    } else {
        ww_be(
            p,
            len + 4,
            item_db::search(pc_isequip_us(tsd_ptr, EQ_WEAP) as u32).look as u16,
        );
        wb(
            p,
            len + 6,
            item_db::search(pc_isequip_us(tsd_ptr, EQ_WEAP) as u32).look_color as u8,
        );
    }
    len += 3;

    // ── Shield slot ───────────────────────────────────────────────────────────
    if pc_isequip_us(tsd_ptr, EQ_SHIELD) == 0 {
        ww_be(p, len + 4, 0xFFFF);
        wb(p, len + 6, 0);
    } else if tsd.read().player.inventory.equip[EQ_SHIELD as usize].custom_look != 0 {
        ww_be(
            p,
            len + 4,
            tsd.read().player.inventory.equip[EQ_SHIELD as usize].custom_look as u16,
        );
        wb(
            p,
            len + 6,
            tsd.read().player.inventory.equip[EQ_SHIELD as usize].custom_look_color as u8,
        );
    } else {
        ww_be(
            p,
            len + 4,
            item_db::search(pc_isequip_us(tsd_ptr, EQ_SHIELD) as u32).look as u16,
        );
        wb(
            p,
            len + 6,
            item_db::search(pc_isequip_us(tsd_ptr, EQ_SHIELD) as u32).look_color as u8,
        );
    }
    len += 3;

    // ── Helm slot ─────────────────────────────────────────────────────────────
    if pc_isequip_us(tsd_ptr, EQ_HELM) == 0
        || (tsd.read().player.appearance.setting_flags & FLAG_HELM) == 0
        || item_db::search(pc_isequip_us(tsd_ptr, EQ_HELM) as u32).look == -1
    {
        wb(p, len + 4, 0);
        ww_be(p, len + 5, 0xFFFF);
    } else {
        wb(p, len + 4, 1);
        if tsd.read().player.inventory.equip[EQ_HELM as usize].custom_look != 0 {
            // C writes customLook as byte (WFIFOB) and customLookColor as byte — helm uses bytes not words.
            wb(
                p,
                len + 5,
                tsd.read().player.inventory.equip[EQ_HELM as usize].custom_look as u8,
            );
            wb(
                p,
                len + 6,
                tsd.read().player.inventory.equip[EQ_HELM as usize].custom_look_color as u8,
            );
        } else {
            wb(
                p,
                len + 5,
                item_db::search(pc_isequip_us(tsd_ptr, EQ_HELM) as u32).look as u8,
            );
            wb(
                p,
                len + 6,
                item_db::search(pc_isequip_us(tsd_ptr, EQ_HELM) as u32).look_color as u8,
            );
        }
    }
    len += 3;

    // ── Face acc slot ─────────────────────────────────────────────────────────
    if pc_isequip_us(tsd_ptr, EQ_FACEACC) == 0 {
        ww_be(p, len + 4, 0xFFFF);
        wb(p, len + 6, 0);
    } else {
        ww_be(
            p,
            len + 4,
            item_db::search(pc_isequip_us(tsd_ptr, EQ_FACEACC) as u32).look as u16,
        );
        wb(
            p,
            len + 6,
            item_db::search(pc_isequip_us(tsd_ptr, EQ_FACEACC) as u32).look_color as u8,
        );
    }
    len += 3;

    // ── Crown slot ────────────────────────────────────────────────────────────
    if pc_isequip_us(tsd_ptr, EQ_CROWN) == 0 {
        ww_be(p, len + 4, 0xFFFF);
        wb(p, len + 6, 0);
    } else {
        wb(p, len, 0); // C: WFIFOB(fd, len) = 0 (extra byte written before the crown data)
        if tsd.read().player.inventory.equip[EQ_CROWN as usize].custom_look != 0 {
            ww_be(
                p,
                len + 4,
                tsd.read().player.inventory.equip[EQ_CROWN as usize].custom_look as u16,
            );
            wb(
                p,
                len + 6,
                tsd.read().player.inventory.equip[EQ_CROWN as usize].custom_look_color as u8,
            );
        } else {
            ww_be(
                p,
                len + 4,
                item_db::search(pc_isequip_us(tsd_ptr, EQ_CROWN) as u32).look as u16,
            );
            wb(
                p,
                len + 6,
                item_db::search(pc_isequip_us(tsd_ptr, EQ_CROWN) as u32).look_color as u8,
            );
        }
    }
    len += 3;

    // ── Face acc two slot ─────────────────────────────────────────────────────
    if pc_isequip_us(tsd_ptr, EQ_FACEACCTWO) == 0 {
        ww_be(p, len + 4, 0xFFFF);
        wb(p, len + 6, 0);
    } else {
        ww_be(
            p,
            len + 4,
            item_db::search(pc_isequip_us(tsd_ptr, EQ_FACEACCTWO) as u32).look as u16,
        );
        wb(
            p,
            len + 6,
            item_db::search(pc_isequip_us(tsd_ptr, EQ_FACEACCTWO) as u32).look_color as u8,
        );
    }
    len += 3;

    // ── Mantle slot ───────────────────────────────────────────────────────────
    if pc_isequip_us(tsd_ptr, EQ_MANTLE) == 0 {
        ww_be(p, len + 4, 0xFFFF);
        wb(p, len + 6, 0xFF);
    } else {
        ww_be(
            p,
            len + 4,
            item_db::search(pc_isequip_us(tsd_ptr, EQ_MANTLE) as u32).look as u16,
        );
        wb(
            p,
            len + 6,
            item_db::search(pc_isequip_us(tsd_ptr, EQ_MANTLE) as u32).look_color as u8,
        );
    }
    len += 3;

    // ── Necklace slot ─────────────────────────────────────────────────────────
    if pc_isequip_us(tsd_ptr, EQ_NECKLACE) == 0
        || (tsd.read().player.appearance.setting_flags & FLAG_NECKLACE) == 0
        || item_db::search(pc_isequip_us(tsd_ptr, EQ_NECKLACE) as u32).look == -1
    {
        ww_be(p, len + 4, 0xFFFF);
        wb(p, len + 6, 0);
    } else {
        ww_be(
            p,
            len + 4,
            item_db::search(pc_isequip_us(tsd_ptr, EQ_NECKLACE) as u32).look as u16,
        );
        wb(
            p,
            len + 6,
            item_db::search(pc_isequip_us(tsd_ptr, EQ_NECKLACE) as u32).look_color as u8,
        );
    }
    len += 3;

    // ── Boots slot ────────────────────────────────────────────────────────────
    if pc_isequip_us(tsd_ptr, EQ_BOOTS) == 0 {
        ww_be(p, len + 4, tsd.read().player.identity.sex as u16);
        wb(p, len + 6, 0);
    } else if tsd.read().player.inventory.equip[EQ_BOOTS as usize].custom_look != 0 {
        ww_be(
            p,
            len + 4,
            tsd.read().player.inventory.equip[EQ_BOOTS as usize].custom_look as u16,
        );
        wb(
            p,
            len + 6,
            tsd.read().player.inventory.equip[EQ_BOOTS as usize].custom_look_color as u8,
        );
    } else {
        ww_be(
            p,
            len + 4,
            item_db::search(pc_isequip_us(tsd_ptr, EQ_BOOTS) as u32).look as u16,
        );
        wb(
            p,
            len + 6,
            item_db::search(pc_isequip_us(tsd_ptr, EQ_BOOTS) as u32).look_color as u8,
        );
    }
    len += 3;

    // ── Equip slots 0..14: icon, real_name, db_name, dura ────────────────────
    // Also builds the equip_status summary string for slot entries that have items.
    let mut equip_status = [0u8; 65536];
    let mut equip_status_len: usize = 0;

    for x in 0..14usize {
        let eq_id = tsd.read().player.inventory.equip[x].id;
        if eq_id > 0 {
            let (eq_custom_icon, eq_custom_icon_color, _eq_custom_look, eq_real_name, eq_dura) = {
                let r = tsd.read();
                let eq = &r.player.inventory.equip[x];
                (
                    eq.custom_icon,
                    eq.custom_icon_color,
                    eq.custom_look,
                    eq.real_name,
                    eq.dura,
                )
            };
            let eq_db = item_db::search(eq_id);

            // Icon
            let icon_w: u16 = if eq_custom_icon != 0 {
                (eq_custom_icon as u16).wrapping_add(49152)
            } else {
                eq_db.icon as u16
            };
            ww_be(p, len + 6, icon_w);
            let icon_color: u8 = if eq_custom_icon != 0 {
                eq_custom_icon_color as u8
            } else {
                eq_db.icon_color
            };
            wb(p, len + 8, icon_color);
            len += 3;

            // Real name (or DB name if no real name).
            let name_ptr: *const u8 = if eq_real_name[0] != 0 {
                eq_real_name.as_ptr() as *const u8
            } else {
                eq_db.name.as_ptr() as *const u8
            };
            let name_len = libc::strlen(name_ptr as *const i8);
            wb(p, len + 6, name_len as u8);
            std::ptr::copy_nonoverlapping(name_ptr, p.add(len + 7), name_len);
            len += name_len + 1;

            // DB name (always from itemdb).
            let dbname_ptr: *const u8 = eq_db.name.as_ptr() as *const u8;
            let dbname_len = libc::strlen(dbname_ptr as *const i8);
            wb(p, len + 6, dbname_len as u8);
            std::ptr::copy_nonoverlapping(dbname_ptr, p.add(len + 7), dbname_len);
            len += dbname_len + 1;

            // Dura (u32 big-endian).
            wl_be(p, len + 6, eq_dura as u32);
            len += 5;

            // Build equip_status summary string for weapon/armor item types (3..=16).
            let item_type = eq_db.typ as i32;
            if (3..=16).contains(&item_type) {
                let nameof: *const i8 = if eq_real_name[0] != 0 {
                    eq_real_name.as_ptr()
                } else {
                    eq_db.name.as_ptr()
                };
                let msgnum = clif_mapmsgnum(tsd, x as i32);
                if msgnum >= 0 && !nameof.is_null() {
                    let mut buff = [0i8; 256];
                    libc::snprintf(
                        buff.as_mut_ptr(),
                        buff.len(),
                        map_msg()[msgnum as usize].message.as_ptr(),
                        nameof,
                    );
                    let buff_len = libc::strlen(buff.as_ptr());
                    let remaining = equip_status.len().saturating_sub(equip_status_len + 2);
                    let copy_len = buff_len.min(remaining);
                    std::ptr::copy_nonoverlapping(
                        buff.as_ptr() as *const u8,
                        equip_status.as_mut_ptr().add(equip_status_len),
                        copy_len,
                    );
                    equip_status_len += copy_len;
                    // Append "\x0A" separator.
                    if equip_status_len < equip_status.len() - 1 {
                        equip_status[equip_status_len] = 0x0A;
                        equip_status_len += 1;
                    }
                }
            }
        } else {
            // Empty slot.
            ww_be(p, len + 6, 0);
            wb(p, len + 8, 0);
            wb(p, len + 9, 0);
            wb(p, len + 10, 0);
            wl_be(p, len + 11, 0);
            len += 10;
        }
    }

    // ── Equip status summary string ───────────────────────────────────────────
    if equip_status_len == 0 {
        let no_items = b"No items equipped.\0";
        equip_status_len = no_items.len() - 1;
        equip_status[..equip_status_len].copy_from_slice(&no_items[..equip_status_len]);
    }
    let equip_len = equip_status_len.min(255);
    equip_status[equip_len] = 0; // NUL-terminate at cap
    wb(p, len + 6, equip_len as u8);
    std::ptr::copy_nonoverlapping(equip_status.as_ptr(), p.add(len + 7), equip_len);
    len += equip_len + 1;

    // ── Target player ID ──────────────────────────────────────────────────────
    wl_be(p, len + 6, target_id);
    len += 4;

    // ── Group / exchange / gender flags ───────────────────────────────────────
    let (t_setting_flags, t_sex) = {
        let r = tsd.read();
        (r.player.appearance.setting_flags, r.player.identity.sex)
    };
    wb(
        p,
        len + 6,
        if (t_setting_flags & FLAG_GROUP) != 0 {
            1
        } else {
            0
        },
    );
    wb(
        p,
        len + 7,
        if (t_setting_flags & FLAG_EXCHANGE) != 0 {
            1
        } else {
            0
        },
    );
    wb(p, len + 8, (2u8).wrapping_sub(t_sex as u8));
    len += 3;

    ww_be(p, len + 6, 0);
    len += 2;

    // ── Profile picture and profile data ──────────────────────────────────────
    let (ppic_size, prof_size) = {
        let r = tsd.read();
        (r.profilepic_size as usize, r.profile_size as usize)
    };
    {
        let r = tsd.read();
        std::ptr::copy_nonoverlapping(
            r.profilepic_data.as_ptr() as *const u8,
            p.add(len + 6),
            ppic_size,
        );
    }
    len += ppic_size;

    {
        let r = tsd.read();
        std::ptr::copy_nonoverlapping(
            r.profile_data.as_ptr() as *const u8,
            p.add(len + 6),
            prof_size,
        );
    }
    len += prof_size;

    // ── Legends ───────────────────────────────────────────────────────────────
    let mut legend_count: u16 = 0;
    {
        let r = tsd.read();
        for x in 0..MAX_LEGENDS {
            let lg = &r.player.legends.legends[x];
            if lg.text[0] != 0 && lg.name[0] != 0 {
                legend_count += 1;
            }
        }
    }
    ww_be(p, len + 6, legend_count);
    len += 2;

    for x in 0..MAX_LEGENDS {
        let (lg_text, lg_name, lg_icon, lg_color, lg_tchaid) = {
            let r = tsd.read();
            let lg = &r.player.legends.legends[x];
            (lg.text, lg.name, lg.icon, lg.color, lg.tchaid)
        };
        if lg_text[0] == 0 || lg_name[0] == 0 {
            continue;
        }
        wb(p, len + 6, lg_icon as u8);
        wb(p, len + 7, lg_color as u8);

        if lg_tchaid > 0 {
            let char_name = clif_getName(lg_tchaid);
            let text_ptr = lg_text.as_ptr();
            let mut repl_buf = [0u8; 4096];
            let bff = replace_str_rust(
                text_ptr,
                b"$player\0",
                char_name as *const i8,
                &mut repl_buf,
            );
            let bff_len = if bff.is_null() { 0 } else { libc::strlen(bff) };
            wb(p, len + 8, bff_len as u8);
            if !bff.is_null() && bff_len > 0 {
                std::ptr::copy_nonoverlapping(bff as *const u8, p.add(len + 9), bff_len);
            }
            len += bff_len + 3;
        } else {
            let text_len = libc::strlen(lg_text.as_ptr());
            wb(p, len + 8, text_len as u8);
            std::ptr::copy_nonoverlapping(lg_text.as_ptr() as *const u8, p.add(len + 9), text_len);
            len += text_len + 3;
        }
    }

    // ── Gender byte + registered flag ─────────────────────────────────────────
    let tsd_sex = tsd.read().player.identity.sex;
    let tsd_id = tsd.read().player.identity.id;
    wb(p, len + 6, (3u8).wrapping_sub(tsd_sex as u8));
    wb(
        p,
        len + 7,
        if clif_isregistered(tsd_id).await > 0 {
            1
        } else {
            0
        },
    );
    len += 5;

    // ── Packet size field ─────────────────────────────────────────────────────
    ww_be(p, 1, (len + 3) as u16);
    wfifoset(fd, encrypt(fd) as usize);

    // ── Lua onClick hook ──────────────────────────────────────────────────────
    {
        dispatch("onClick", None, &[pe.id, tsd.id]);
    }

    0
}

// ─── Board list packet ────────────────────────────────────────────────────────

/// Sends the filtered board list to a player.
///
///
/// Iterates all 256 sort-order slots and, for each slot, finds the first board
/// (id 0..256) whose `sort`, `level`, `gmlevel`, `path`, and `clan` match the
/// player's character status.  The matching board's id and name are appended to
/// the packet in big-endian order, mirroring the original `SWAP16` / `strcpy`
/// sequence.
///
/// ## Packet layout (relative offsets)
/// ```text
/// [0]    0xAA  opcode
/// [1..2] len+3 big-endian (written last)
/// [3]    0x31
/// [4]    3
/// [5]    1
/// [6]    13
/// [7..16] "YuriBoards\0"   (10 chars + null)
/// [20]   b_count           (written after loop)
/// for each matched board (len starts at 15):
///   [len+6..len+7]  board_id big-endian (SWAP16)
///   [len+8]         name byte length
///   [len+9..]       name bytes (no null terminator written)
///   len += strlen(name) + 3
/// ```
///
/// # Safety
/// Calls unsafe packet I/O functions (`wfifohead`, `wfifop`, `wfifoset`, `encrypt`).
pub unsafe fn clif_showboards(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;

    if !session_exists(fd) {
        return 0;
    }

    wfifohead(fd, 65535);
    let p = wfifop(fd, 0);
    if p.is_null() {
        return 0;
    }

    // Fixed header.
    wb(p, 0, 0xAA);
    wb(p, 3, 0x31);
    wb(p, 4, 3);
    wb(p, 5, 1);
    wb(p, 6, 13);
    // "YuriBoards" + null at pos 7.
    let label = b"YuriBoards\0";
    std::ptr::copy_nonoverlapping(label.as_ptr(), p.add(7), label.len());

    let mut len: usize = 15;
    let mut b_count: u8 = 0;

    let (player_level, player_gmlevel, player_path, player_clan) = {
        let r = pe.read();
        (
            r.player.progression.level as i32,
            r.player.identity.gm_level as i32,
            r.player.progression.class as i32,
            r.player.social.clan as i32,
        )
    };

    // Double-loop: outer = sort order 0..256, inner = board id 0..256.
    // Uses `searchexist` (returns null for missing ids) so no new API is needed.
    for sort_order in 0..256_i32 {
        for x in 0..256_i32 {
            let b = match board_db::searchexist(x) {
                Some(b) => b,
                None => continue,
            };
            if b.sort == sort_order
                && b.level <= player_level
                && b.gmlevel <= player_gmlevel
                && (b.path == player_path || b.path == 0)
                && (b.clan == player_clan || b.clan == 0)
            {
                let name_len = libc::strlen(b.name.as_ptr());
                // board id (big-endian u16).
                ww_be(p, len + 6, x as u16);
                // name byte-length at len+8; name bytes starting at len+9.
                wb(p, len + 8, name_len as u8);
                std::ptr::copy_nonoverlapping(
                    b.name.as_ptr() as *const u8,
                    p.add(len + 9),
                    name_len,
                );
                len += name_len + 3;
                b_count += 1;
                break; // only first matching board per sort-order slot
            }
        }
    }

    // Board count at fixed offset 20; packet length at bytes 1-2.
    wb(p, 20, b_count);
    ww_be(p, 1, (len + 3) as u16);

    wfifoset(fd, encrypt(fd) as usize);
    0
}
