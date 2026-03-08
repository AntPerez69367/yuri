//! Visual/status packet builders — Rust port of the `clif_sendupdatestatus` group.
//!
//! Ported from `c_src/sl_compat.c` lines 3264–3512.
//! All functions build and send binary status packets to a player's client.
//!
//! ## Byte-order convention
//! - `WFIFOL(fd, pos) = SWAP32(val)` → big-endian → `val.to_be_bytes()`
//! - `WFIFOL(fd, pos) = val` (no SWAP32) → little-endian → `val.to_le_bytes()`

#![allow(non_snake_case)]

use std::os::raw::{c_char, c_float, c_int, c_uint};

use crate::database::class_db;
use crate::ffi::session::{
    rust_session_available, rust_session_commit, rust_session_exists, rust_session_get_client_ip,
    rust_session_get_data, rust_session_get_eof, rust_session_increment, rust_session_rdata_ptr,
    rust_session_set_eof, rust_session_wdata_ptr, rust_session_wfifohead,
};
use crate::game::pc::MapSessionData;
use crate::servers::char::charstatus::MAX_INVENTORY;

// Re-declared per-module (Rust requires per-module extern declarations; this is not a duplicate of mod.rs).
extern "C" {
    fn encrypt(fd: c_int) -> c_int;
}

// ─── Buffer write helpers ─────────────────────────────────────────────────────

/// Write a single byte into the write buffer at `pos`.
///
/// # Safety
/// `p` must be a valid non-null pointer from `rust_session_wdata_ptr`, and `pos`
/// must lie within the allocated buffer region.
#[inline]
unsafe fn wb(p: *mut u8, pos: usize, val: u8) {
    *p.add(pos) = val;
}

/// Write a 4-byte big-endian integer at `pos`.
///
/// Mirrors `WFIFOL(fd, pos) = SWAP32(val)`.
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
/// Mirrors `WFIFOL(fd, pos) = val` (no SWAP32 in original C).
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
/// Mirrors `WFIFOW(fd, pos) = SWAP16(val)`.
///
/// # Safety
/// `p` must be a valid write buffer pointer with at least `pos + 2` writable bytes.
#[inline]
unsafe fn ww_be(p: *mut u8, pos: usize, val: u16) {
    std::ptr::copy_nonoverlapping(val.to_be_bytes().as_ptr(), p.add(pos), 2);
}

// ─── Exported helpers (declared in c_src/map_parse.h; called from C) ─────────

/// Returns experience needed to reach the next level (TNL = To Next Level).
///
/// Mirrors `clif_getLevelTNL` in `c_src/sl_compat.c` (line 3319).
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
#[no_mangle]
pub unsafe extern "C" fn clif_getLevelTNL(sd: *mut MapSessionData) -> c_int {
    let sd = &*sd;
    let mut path = sd.status.class as c_int;
    let level = sd.status.level as c_int;

    if path > 5 {
        path = class_db::path(path);
    }

    if level < 99 {
        class_db::level(path, level) as c_int - sd.status.exp as c_int
    } else {
        0
    }
}

/// Returns the current XP bar fill percentage (0.0–100.0).
///
/// Mirrors `clif_getXPBarPercent` in `c_src/sl_compat.c` (line 3331).
///
/// Mutates `sd->underLevelFlag` as a side effect (faithfully reproduced from C).
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
#[no_mangle]
pub unsafe extern "C" fn clif_getXPBarPercent(sd: *mut MapSessionData) -> c_float {
    let sd = &mut *sd;

    // C normalises path twice — the first assignment is overwritten immediately;
    // reproduced faithfully. The `let _ = path` silences the dead-assignment lint.
    let mut path = sd.status.class as c_int;
    if path > 5 {
        path = class_db::path(path);
    }
    let _ = path; // dead assignment; C re-reads sd->status.class next

    path = sd.status.class as c_int;
    let level = sd.status.level as c_int;
    if path > 5 {
        path = class_db::path(path);
    }

    if level < 99 {
        let exp_in_level = class_db::level(path, level) as c_int
            - class_db::level(path, level - 1) as c_int;
        let tnl = class_db::level(path, level) as c_int - sd.status.exp as c_int;
        let percentage = (exp_in_level - tnl) as c_float / exp_in_level as c_float * 100.0;

        if sd.underLevelFlag == 0
            && sd.status.exp < class_db::level(path, level - 1)
        {
            sd.underLevelFlag = sd.status.level as i8;
        }

        if sd.underLevelFlag as u8 != sd.status.level {
            sd.underLevelFlag = 0;
        }

        if sd.underLevelFlag != 0 {
            return sd.status.exp as c_float / class_db::level(path, level) as c_float * 100.0;
        }

        percentage
    } else {
        sd.status.exp as c_float / 4_294_967_295_f32 * 100.0
    }
}

// ─── Status packet senders ────────────────────────────────────────────────────

/// Sends a full HP/MP/EXP/money status update packet.
///
/// Mirrors `clif_sendupdatestatus` in `c_src/sl_compat.c` (line 3264).
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
#[no_mangle]
pub unsafe extern "C" fn clif_sendupdatestatus(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() {
        return 0;
    }
    let sd = &*sd;
    let fd = sd.fd;

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    rust_session_wfifohead(fd, 33);
    let p = rust_session_wdata_ptr(fd, 0);
    if p.is_null() {
        return 0;
    }

    wb(p, 0, 0xAA);
    wb(p, 1, 0x00);
    wb(p, 2, 0x1C);
    wb(p, 3, 0x08);
    // byte 4: not written in C (left as-is after WFIFOHEAD zeroing)
    wb(p, 5, 0x38);
    wl_be(p, 6, sd.status.hp);
    wl_be(p, 10, sd.status.mp);
    wl_be(p, 14, sd.status.exp);
    wl_be(p, 18, sd.status.money);
    wl_be(p, 22, 0x00);
    wb(p, 26, 0x00);
    wb(p, 27, 0x00);
    wb(p, 28, sd.blind as u8);
    wb(p, 29, sd.drunk as u8);
    wb(p, 30, 0x00);
    wb(p, 31, 0x73);
    wb(p, 32, 0x35);

    // encrypt() returns 1 on error or pkt_len (≥ 3) on success; never negative.
    rust_session_commit(fd, encrypt(fd) as usize);
    0
}

/// Sends a compact status update (EXP, money, XP%, blind/drunk, flags, settingFlags).
///
/// Mirrors `clif_sendupdatestatus2` in `c_src/sl_compat.c` (line 3292).
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
#[no_mangle]
pub unsafe extern "C" fn clif_sendupdatestatus2(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() {
        return 0;
    }

    // Compute percentage before taking the shared ref (clif_getXPBarPercent mutates sd).
    let percentage = clif_getXPBarPercent(sd);

    let sd = &*sd;
    let fd = sd.fd;

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    rust_session_wfifohead(fd, 25);
    let p = rust_session_wdata_ptr(fd, 0);
    if p.is_null() {
        return 0;
    }

    wb(p, 0, 0xAA);
    // bytes 1–2: not written in C
    wb(p, 3, 0x08);
    // byte 4: not written in C
    wb(p, 5, 0x18);
    wl_be(p, 6, sd.status.exp);
    wl_be(p, 10, sd.status.money);
    wb(p, 14, percentage as u8); // saturates to 0/255 for NaN or out-of-range (Rust 1.45+)
    wb(p, 15, sd.drunk as u8);
    wb(p, 16, sd.blind as u8);
    wb(p, 17, 0x00);
    wb(p, 18, 0x00);
    wb(p, 19, 0x00);
    // sd->flags is c_ulong (64-bit on Linux x86-64); C WFIFOB truncates to low byte.
    wb(p, 20, sd.flags as u8);
    wb(p, 21, 0x01);
    wl_be(p, 22, sd.status.setting_flags as u32);

    // encrypt() returns 1 on error or pkt_len (≥ 3) on success; never negative.
    rust_session_commit(fd, encrypt(fd) as usize);
    0
}

/// Sends a status update after a kill (EXP, money, XP%, settingFlags, TNL, armor/dam/hit).
///
/// Mirrors `clif_sendupdatestatus_onkill` in `c_src/sl_compat.c` (line 3365).
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
#[no_mangle]
pub unsafe extern "C" fn clif_sendupdatestatus_onkill(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() {
        return 0;
    }

    // Compute before taking the shared ref — mirrors C's ordering (nullpo_ret comes after).
    let tnl = clif_getLevelTNL(sd);
    let percentage = clif_getXPBarPercent(sd);

    let sdr = &*sd;
    let fd = sdr.fd;

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    rust_session_wfifohead(fd, 33);
    let p = rust_session_wdata_ptr(fd, 0);
    if p.is_null() {
        return 0;
    }

    wb(p, 0, 0xAA);
    wb(p, 1, 0x00);
    wb(p, 2, 0x1C);
    wb(p, 3, 0x08);
    // byte 4: not written in C
    wb(p, 5, 0x19);
    wl_be(p, 6, sdr.status.exp);
    wl_be(p, 10, sdr.status.money);
    wb(p, 14, percentage as u8); // saturates to 0/255 for NaN or out-of-range (Rust 1.45+)
    wb(p, 15, sdr.drunk as u8);
    wb(p, 16, sdr.blind as u8);
    wb(p, 17, 0);
    wb(p, 18, 0);
    wb(p, 19, 0);
    // sd->flags is c_ulong (64-bit on Linux x86-64); C WFIFOB truncates to low byte.
    wb(p, 20, sdr.flags as u8);
    wb(p, 21, 0);
    wl_be(p, 22, sdr.status.setting_flags as u32);
    wl_be(p, 26, tnl as u32);
    wb(p, 30, sdr.armor as u8);
    wb(p, 31, sdr.dam as u8);
    wb(p, 32, sdr.hit as u8);

    // encrypt() returns 1 on error or pkt_len (≥ 3) on success; never negative.
    rust_session_commit(fd, encrypt(fd) as usize);
    0
}

/// Sends a full status update after equipping an item (all stats, XP, TNL, combat stats).
///
/// Mirrors `clif_sendupdatestatus_onequip` in `c_src/sl_compat.c` (line 3401).
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
#[no_mangle]
pub unsafe extern "C" fn clif_sendupdatestatus_onequip(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() {
        return 0;
    }

    let tnl = clif_getLevelTNL(sd);
    let percentage = clif_getXPBarPercent(sd);

    let sdr = &*sd;
    let fd = sdr.fd;

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    rust_session_wfifohead(fd, 62);
    let p = rust_session_wdata_ptr(fd, 0);
    if p.is_null() {
        return 0;
    }

    wb(p, 0, 0xAA);
    wb(p, 1, 0x00);
    wb(p, 2, 65);
    wb(p, 3, 0x08);
    // byte 4: not written in C
    wb(p, 5, 89);
    wb(p, 6, 0x00);
    wb(p, 7, sdr.status.country as u8);
    wb(p, 8, sdr.status.totem);
    wb(p, 9, 0x00);
    wb(p, 10, sdr.status.level);
    wl_be(p, 11, sdr.max_hp);
    wl_be(p, 15, sdr.max_mp);
    wb(p, 19, sdr.might as u8);
    wb(p, 20, sdr.will as u8);
    wb(p, 21, 0x03);
    wb(p, 22, 0x03);
    wb(p, 23, sdr.grace as u8);
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
    wb(p, 34, sdr.status.maxinv);
    wl_be(p, 35, sdr.status.exp);
    wl_be(p, 39, sdr.status.money);
    wb(p, 43, percentage as u8); // saturates to 0/255 for NaN or out-of-range (Rust 1.45+)
    wb(p, 44, sdr.drunk as u8);
    wb(p, 45, sdr.blind as u8);
    wb(p, 46, 0x00);
    wb(p, 47, 0x00);
    wb(p, 48, 0x00);
    // sd->flags is c_ulong (64-bit on Linux x86-64); C WFIFOB truncates to low byte.
    wb(p, 49, sdr.flags as u8);
    wb(p, 50, 0x00);
    wl_be(p, 51, sdr.status.setting_flags as u32);
    wl_be(p, 55, tnl as u32);
    wb(p, 59, sdr.armor as u8);
    wb(p, 60, sdr.dam as u8);
    wb(p, 61, sdr.hit as u8);

    // encrypt() returns 1 on error or pkt_len (≥ 3) on success; never negative.
    rust_session_commit(fd, encrypt(fd) as usize);
    0
}

/// Sends a status update after unequipping an item (HP/MP + armor + XP + TNL).
///
/// Mirrors `clif_sendupdatestatus_onunequip` in `c_src/sl_compat.c` (line 3460).
///
/// HP (offset 11) and MP (offset 15) use **little-endian** byte order — the C code
/// writes them without SWAP32. TNL at offset 50 is also little-endian.
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
#[no_mangle]
pub unsafe extern "C" fn clif_sendupdatestatus_onunequip(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() {
        return 0;
    }

    let tnl = clif_getLevelTNL(sd);
    let percentage = clif_getXPBarPercent(sd);

    let sdr = &*sd;
    let fd = sdr.fd;

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    rust_session_wfifohead(fd, 52);
    let p = rust_session_wdata_ptr(fd, 0);
    if p.is_null() {
        return 0;
    }

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
    wl_le(p, 11, sdr.status.hp);
    wl_le(p, 15, sdr.status.mp);
    wb(p, 19, 0);
    wb(p, 20, 0);
    wb(p, 21, 0);
    wb(p, 22, 0);
    wb(p, 23, 0);
    wb(p, 24, 0);
    wb(p, 25, 0);
    wb(p, 26, sdr.armor as u8);
    wb(p, 27, 0);
    wb(p, 28, 0);
    wb(p, 29, 0);
    wb(p, 30, 0);
    wb(p, 31, 0);
    wb(p, 32, 0);
    wb(p, 33, 0);
    wb(p, 34, 0);
    wl_be(p, 35, sdr.status.exp);
    wl_be(p, 39, sdr.status.money);
    wb(p, 43, percentage as u8); // saturates to 0/255 for NaN or out-of-range (Rust 1.45+)
    wb(p, 44, sdr.drunk as u8);
    wb(p, 45, sdr.blind as u8);
    wb(p, 46, 0x00);
    wb(p, 47, 0x00);
    wb(p, 48, 0x00);
    // sd->flags is c_ulong (64-bit on Linux x86-64); C WFIFOB truncates to low byte.
    wb(p, 49, sdr.flags as u8);
    // No SWAP32 in C → little-endian store.
    wl_le(p, 50, tnl as u32);

    // encrypt() returns 1 on error or pkt_len (≥ 3) on success; never negative.
    rust_session_commit(fd, encrypt(fd) as usize);
    0
}

// ─── Utility / packet-builder functions ──────────────────────────────────────

/// Clears AFK state on the player session.
///
/// Mirrors `clif_cancelafk` in `c_src/sl_compat.c` (line 4752).
/// Sets `sd->afktime = 0` and `sd->afk = 0`. No packet is sent.
///
/// Declared in `c_src/map_parse.h` line 217.
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
#[no_mangle]
pub unsafe extern "C" fn clif_cancelafk(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() {
        return 0;
    }
    let sd = &mut *sd;
    sd.afktime = 0;
    sd.afk = 0;
    0
}

/// Sends a "destroy old objects" packet (opcode 0x58) to the client.
///
/// Mirrors `clif_destroyold` in `c_src/sl_compat.c` (line 3206).
/// Fixed 6-byte packet (3-byte header + 3-byte payload).
///
/// Declared in `c_src/map_parse.h` line 159.
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
#[no_mangle]
pub unsafe extern "C" fn clif_destroyold(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() {
        return 0;
    }
    let sd = &*sd;
    let fd = sd.fd;

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    // Packet: [0xAA][len_hi][len_lo][0x58][0x03][0x00]
    // Length field = 3 (payload bytes after the 3-byte header).
    rust_session_wfifohead(fd, 6);
    let p = rust_session_wdata_ptr(fd, 0);
    if p.is_null() {
        return 0;
    }

    wb(p, 0, 0xAA);
    ww_be(p, 1, 3); // WFIFOW(fd,1) = SWAP16(3)
    wb(p, 3, 0x58);
    wb(p, 4, 0x03);
    wb(p, 5, 0x00);

    rust_session_commit(fd, encrypt(fd) as usize);
    0
}

/// Maps an equipment slot ID to a map-message number.
///
/// Mirrors `clif_mapmsgnum` in `c_src/sl_compat.c` (line 3184).
/// Pure switch/case — no packet is sent. Returns `-1` for unknown slot IDs.
///
/// EQ_ slot enum values (from `c_src/item_db.h`, 0-based):
/// EQ_WEAP=0, EQ_ARMOR=1, EQ_SHIELD=2, EQ_HELM=3, EQ_LEFT=4, EQ_RIGHT=5,
/// EQ_SUBLEFT=6, EQ_SUBRIGHT=7, EQ_FACEACC=8, EQ_CROWN=9,
/// EQ_MANTLE=10, EQ_NECKLACE=11, EQ_BOOTS=12, EQ_COAT=13.
/// EQ_FACEACCTWO=14 exists in `item_db.h` but is not handled by this function
/// (falls through to return -1), consistent with the C original.
///
/// MAP_EQ* enum values (from `c_src/map_server.h`, starts at MAP_EQHELM=13):
/// MAP_EQHELM=13, MAP_EQWEAP=14, MAP_EQARMOR=15, MAP_EQSHIELD=16,
/// MAP_EQLEFT=17, MAP_EQRIGHT=18, MAP_EQSUBLEFT=19, MAP_EQSUBRIGHT=20,
/// MAP_EQFACEACC=21, MAP_EQCROWN=22, MAP_EQMANTLE=23, MAP_EQNECKLACE=24,
/// MAP_EQBOOTS=25, MAP_EQCOAT=26.
///
/// # Safety
/// `_sd` is unused but present for ABI compatibility with C callers.
#[no_mangle]
pub unsafe extern "C" fn clif_mapmsgnum(_sd: *mut MapSessionData, id: c_int) -> c_int {
    match id {
        3  => 13, // EQ_HELM=3     → MAP_EQHELM=13
        0  => 14, // EQ_WEAP=0     → MAP_EQWEAP=14
        1  => 15, // EQ_ARMOR=1    → MAP_EQARMOR=15
        2  => 16, // EQ_SHIELD=2   → MAP_EQSHIELD=16
        4  => 17, // EQ_LEFT=4     → MAP_EQLEFT=17
        5  => 18, // EQ_RIGHT=5    → MAP_EQRIGHT=18
        6  => 19, // EQ_SUBLEFT=6  → MAP_EQSUBLEFT=19
        7  => 20, // EQ_SUBRIGHT=7 → MAP_EQSUBRIGHT=20
        8  => 21, // EQ_FACEACC=8  → MAP_EQFACEACC=21
        9  => 22, // EQ_CROWN=9    → MAP_EQCROWN=22
        10 => 23, // EQ_MANTLE=10  → MAP_EQMANTLE=23
        11 => 24, // EQ_NECKLACE=11 → MAP_EQNECKLACE=24
        12 => 25, // EQ_BOOTS=12   → MAP_EQBOOTS=25
        13 => 26, // EQ_COAT=13    → MAP_EQCOAT=26
        _  => -1,
    }
}

/// Sends a popup message packet (opcode 0x0A) to the client.
///
/// Mirrors `clif_popup` in `c_src/sl_compat.c` (line 2497).
///
/// Packet layout (total = `str_len + 8`):
/// ```text
/// [0xAA][len_hi][len_lo][0x0A][0x03][0x08][str_hi][str_lo][...string bytes...]
/// ```
/// where the length field = `str_len + 5`.
///
/// Declared in `c_src/map_parse.h` line 75.
///
/// # Safety
/// - `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
/// - `buf` must be a valid, null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn clif_popup(sd: *mut MapSessionData, buf: *const c_char) -> c_int {
    if sd.is_null() {
        return 0;
    }
    let sd = &*sd;
    let fd = sd.fd;

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    // Measure string length without the null terminator.
    let str_bytes = std::ffi::CStr::from_ptr(buf).to_bytes();
    let str_len = str_bytes.len();

    // C: WFIFOHEAD(sd->fd, strlen(buf) + 5 + 3) — total = str_len + 8.
    rust_session_wfifohead(fd, str_len + 8);
    let p = rust_session_wdata_ptr(fd, 0);
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

    rust_session_commit(fd, encrypt(fd) as usize);
    0
}

/// Sends the profile page URL to the client (opcode 0x62, subtype 0x04).
///
/// Mirrors `clif_sendprofile` in `c_src/sl_compat.c` (line 2378).
/// Hardcoded URL: `"https://www.website.com/users"` (29 bytes).
///
/// Packet layout:
/// ```text
/// [0xAA][len_hi][len_lo][0x62][??][0x04][url_len_byte][...url bytes...]
/// ```
/// where `len = url_len + 7`.
///
/// Previously declared as `extern "C"` in `src/game/client/mod.rs`; now a Rust impl.
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
#[no_mangle]
pub unsafe extern "C" fn clif_sendprofile(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() {
        return 0;
    }
    let sd = &*sd;
    let fd = sd.fd;

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    let url: &[u8] = b"https://www.website.com/users";
    let url_len = url.len(); // 29 bytes

    // C has no WFIFOHEAD; add for safety.
    // Total packet = url_len + 7 (payload) + 3 (header overhead) = url_len + 10.
    rust_session_wfifohead(fd, url_len + 10);
    let p = rust_session_wdata_ptr(fd, 0);
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

    rust_session_commit(fd, encrypt(fd) as usize);
    0
}

/// Sends the board page URLs to the client (opcode 0x62, subtype 0x00).
///
/// Mirrors `clif_sendboard` in `c_src/sl_compat.c` (line 2397).
/// Three hardcoded URLs packed sequentially, each preceded by a length byte:
/// - url1: `"https://www.website.com/boards"` (30 bytes)
/// - url2: `"https://www.website.com/boards"` (30 bytes)
/// - url3: `"?abc123"` (7 bytes)
///
/// Previously declared as `extern "C"` in `src/game/client/mod.rs`; now a Rust impl.
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
#[no_mangle]
pub unsafe extern "C" fn clif_sendboard(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() {
        return 0;
    }
    let sd = &*sd;
    let fd = sd.fd;

    let url1: &[u8] = b"https://www.website.com/boards";
    let url2: &[u8] = b"https://www.website.com/boards";
    let url3: &[u8] = b"?abc123";

    // C len accumulates: starts at 6, then += strlen(urlN) + 1 for each url.
    // Total payload = 6 + (url1_len+1) + (url2_len+1) + (url3_len+1).
    let total_payload = 6 + (url1.len() + 1) + (url2.len() + 1) + (url3.len() + 1);
    // C has no WFIFOHEAD; add for safety. Reserve total_payload + 3 bytes.
    rust_session_wfifohead(fd, total_payload + 3);
    let p = rust_session_wdata_ptr(fd, 0);
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

    rust_session_commit(fd, encrypt(fd) as usize);
    0
}

// ─── Task 16: utility functions ───────────────────────────────────────────────

/// Maps an equipment slot ID to its item-type integer.
///
/// Mirrors `clif_getequiptype` in `c_src/sl_compat.c` (line 2697).
/// Pure switch/case — no packet is sent. Returns `0` for unknown slot IDs.
///
/// EQ_ slot enum values (0-based, from `c_src/item_db.h`):
/// EQ_WEAP=0, EQ_ARMOR=1, EQ_SHIELD=2, EQ_HELM=3, EQ_LEFT=4, EQ_RIGHT=5,
/// EQ_SUBLEFT=6, EQ_SUBRIGHT=7, EQ_FACEACC=8, EQ_CROWN=9,
/// EQ_MANTLE=10, EQ_NECKLACE=11, EQ_BOOTS=12, EQ_COAT=13.
/// EQ_FACEACCTWO=14 is not handled (falls through to return 0), consistent with C.
///
/// # Safety
/// No pointer dereferences — this function is pure.
#[no_mangle]
pub unsafe extern "C" fn clif_getequiptype(val: c_int) -> c_int {
    match val {
        0  => 1,  // EQ_WEAP=0      → type 1
        1  => 2,  // EQ_ARMOR=1     → type 2
        2  => 3,  // EQ_SHIELD=2    → type 3
        3  => 4,  // EQ_HELM=3      → type 4
        11 => 6,  // EQ_NECKLACE=11 → type 6
        4  => 7,  // EQ_LEFT=4      → type 7
        5  => 8,  // EQ_RIGHT=5     → type 8
        12 => 13, // EQ_BOOTS=12    → type 13
        10 => 14, // EQ_MANTLE=10   → type 14
        13 => 16, // EQ_COAT=13     → type 16
        6  => 20, // EQ_SUBLEFT=6   → type 20
        7  => 21, // EQ_SUBRIGHT=7  → type 21
        8  => 22, // EQ_FACEACC=8   → type 22
        9  => 23, // EQ_CROWN=9     → type 23
        _  => 0,
    }
}

/// Returns the item area for a player session (stub — always returns 0).
///
/// Mirrors `clif_getitemarea` in `c_src/sl_compat.c` (line 2827).
/// Declared in `c_src/map_parse.h` line 111.
///
/// # Safety
/// `_sd` is unused; safe to call with any pointer (including null).
#[no_mangle]
pub unsafe extern "C" fn clif_getitemarea(
    _sd: *mut crate::game::pc::MapSessionData,
) -> c_int {
    0
}

/// Returns the XP required to reach the given level.
///
/// Mirrors `clif_getlvlxp` in `c_src/sl_compat.c` (line 2807).
/// Formula: `(level / 0.2)^2` rounded to nearest integer.
///
/// C original: `pow((level / 0.2), 2)` cast from `float + 0.5` to `unsigned int`.
///
/// # Safety
/// Pure math function — no pointer dereferences.
#[no_mangle]
pub unsafe extern "C" fn clif_getlvlxp(level: c_int) -> c_uint {
    let xp = (level as f64 / 0.2_f64).powi(2);
    (xp + 0.5) as c_uint
}

/// Sends the current map weather to the client (opcode 0x1F).
///
/// Mirrors `clif_sendweather` in `c_src/sl_compat.c` (line 2831).
/// Declared in `c_src/map_parse.h` line 76.
///
/// Packet layout (6 bytes total):
/// ```text
/// [0xAA][len_hi][len_lo][0x1F][seq][weather_byte]
/// ```
/// `len = SWAP16(3)` (big-endian 3) — 3 payload bytes after the 3-byte header.
/// `seq` is the per-session sequence counter from `rust_session_increment`.
///
/// The weather byte is taken from `map[sd->bl.m].weather` only when
/// `sd->status.setting_flags & FLAG_WEATHER` is set; otherwise 0.
/// `FLAG_WEATHER = 32` (bit 5) from `c_src/mmo.h` line 45.
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
/// `crate::ffi::map_db::map` must be initialised before calling this function.
#[no_mangle]
pub unsafe extern "C" fn clif_sendweather(
    sd: *mut crate::game::pc::MapSessionData,
) -> c_int {
    if sd.is_null() {
        return 0;
    }
    let sdr = &*sd;
    let fd = sdr.fd;

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    // FLAG_WEATHER = 32 (mmo.h line 45). setting_flags is u16.
    const FLAG_WEATHER: u16 = 32;
    let weather_byte: u8 = if sdr.status.setting_flags & FLAG_WEATHER != 0 {
        let map_ptr = crate::ffi::map_db::map;
        if map_ptr.is_null() {
            0
        } else {
            (*map_ptr.add(sdr.bl.m as usize)).weather
        }
    } else {
        0
    };

    // WFIFOHEADER(fd, 0x1F, 3) expands to (session.h line 97):
    //   WFIFOB(fd, 0) = 0xAA
    //   WFIFOW(fd, 1) = SWAP16(3)              → big-endian 3
    //   WFIFOB(fd, 3) = 0x1F                   (opcode)
    //   WFIFOB(fd, 4) = rust_session_increment(fd)
    // Then: WFIFOB(fd, 5) = weather_byte
    // Total packet = 6 bytes (header 3 + payload 3).
    rust_session_wfifohead(fd, 6);
    let p = rust_session_wdata_ptr(fd, 0);
    if p.is_null() {
        return 0;
    }

    wb(p, 0, 0xAA);
    ww_be(p, 1, 3); // SWAP16(3) — payload length
    wb(p, 3, 0x1F); // opcode
    wb(p, 4, rust_session_increment(fd));
    wb(p, 5, weather_byte);

    rust_session_commit(fd, encrypt(fd) as usize);
    0
}

/// Returns whether a ghost player (`tsd`) should be visible to `sd`.
///
/// Mirrors `clif_show_ghost` in `c_src/sl_compat.c` (line 2813).
/// Declared in `c_src/map_parse.h` line 59.
///
/// Logic:
/// - GMs (`sd->status.gm_level != 0`) always see ghosts → 1.
/// - If `map[sd->bl.m].show_ghosts != 0` → 1 (map forces ghost visibility).
/// - If `tsd->status.state != 1` (tsd is not a ghost) → 1.
/// - If `sd->bl.id == tsd->bl.id` (same entity) → 1.
/// - On PvP maps: 1 only if `sd->status.state == 1` AND `sd->optFlags & 256`
///   (`optFlag_ghosts = 256`, from `c_src/map_server.h` line 34).
/// - On non-PvP maps: always 1.
///
/// # Safety
/// Both `sd` and `tsd` must be valid, non-null pointers to initialised [`MapSessionData`].
/// `crate::ffi::map_db::map` must be initialised before calling this function.
#[no_mangle]
pub unsafe extern "C" fn clif_show_ghost(
    sd: *mut crate::game::pc::MapSessionData,
    tsd: *mut crate::game::pc::MapSessionData,
) -> c_int {
    if sd.is_null() || tsd.is_null() {
        return 1;
    }
    let sdr = &*sd;
    let tsdr = &*tsd;

    if sdr.status.gm_level != 0 {
        return 1;
    }

    let map_ptr = crate::ffi::map_db::map;
    if map_ptr.is_null() {
        return 1;
    }
    let map_slot = &*map_ptr.add(sdr.bl.m as usize);

    // If map shows ghosts, tsd is not a ghost (state != 1), or same entity → visible.
    if map_slot.show_ghosts != 0 || tsdr.status.state != 1 || sdr.bl.id == tsdr.bl.id {
        return 1;
    }

    if map_slot.pvp != 0 {
        // optFlag_ghosts = 256 (map_server.h line 34). optFlags is c_ulong (64-bit on Linux).
        const OPT_FLAG_GHOSTS: u64 = 256;
        if sdr.status.state == 1 && (sdr.optFlags as u64 & OPT_FLAG_GHOSTS) != 0 {
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
/// Mirrors `clif_user_list` in `c_src/sl_compat.c` (line 2781).
///
/// Sends a 4-byte little-endian packet to `char_fd`:
/// ```text
/// [0x0B][0x30][fd_lo][fd_hi]
/// ```
/// Opcode 0x300B, then `sd->fd` as LE u16. Internal server-to-server packets
/// use no SWAP16, so fields are little-endian.
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
#[no_mangle]
pub unsafe extern "C" fn clif_user_list(
    sd: *mut crate::game::pc::MapSessionData,
) -> c_int {
    if sd.is_null() {
        return 0;
    }
    let sdr = &*sd;

    if rust_session_exists(sdr.fd) == 0 {
        rust_session_set_eof(sdr.fd, 8);
        return 0;
    }

    let cfd = crate::game::map_server::char_fd;
    if cfd == 0 {
        return 0;
    }

    rust_session_wfifohead(cfd, 4);
    let p = rust_session_wdata_ptr(cfd, 0);
    if p.is_null() {
        return 0;
    }

    // WFIFOW(char_fd, 0) = 0x300B — no SWAP16 → little-endian
    std::ptr::copy_nonoverlapping(0x300Bu16.to_le_bytes().as_ptr(), p, 2);
    // WFIFOW(char_fd, 2) = sd->fd — no SWAP16 → little-endian; int truncated to u16
    std::ptr::copy_nonoverlapping((sdr.fd as u16).to_le_bytes().as_ptr(), p.add(2), 2);

    rust_session_commit(cfd, 4);
    0
}

/// Logs a hex dump of a byte buffer for debugging.
///
/// Mirrors `clif_debug` in `c_src/sl_compat.c` (line 2766).
/// The C original uses `printf` to print two lines: hex bytes and printable chars.
/// This port emits a single `tracing::debug!` line with all hex-formatted bytes.
///
/// # Safety
/// `data` must be a valid pointer to at least `len` readable bytes, or null.
/// If `data` is null or `len <= 0`, this function is a no-op.
#[no_mangle]
pub unsafe extern "C" fn clif_debug(data: *const u8, len: c_int) -> c_int {
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
/// Mirrors `clif_sendurl` in `c_src/sl_compat.c` (line 2363).
/// Declared in `c_src/map_parse.h` line 30.
///
/// Packet layout (total = `url_len + 11`):
/// ```text
/// [0xAA][len_hi][len_lo][0x66][0x03][type][url_len_hi][url_len_lo][...url bytes...]
/// ```
/// where the length field = `url_len + 8` (bytes after the 3-byte header).
/// The length field is written last, matching the C source ordering.
///
/// # Safety
/// - `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
/// - `url` must be a valid, null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn clif_sendurl(
    sd: *mut MapSessionData,
    ty: c_int,
    url: *const c_char,
) -> c_int {
    if sd.is_null() {
        return 0;
    }
    let sd = &*sd;
    let fd = sd.fd;

    // C original had no session guard, but project convention adds one defensively.
    // No set_eof here — we just skip the send if the session is already gone.
    if rust_session_exists(fd) == 0 {
        return 0;
    }

    let url_bytes = std::ffi::CStr::from_ptr(url).to_bytes();
    let url_len = url_bytes.len();

    // C had no WFIFOHEAD — add one for safety.
    // Total packet = 3 (framing header) + url_len + 8 (payload before url) = url_len + 11.
    rust_session_wfifohead(fd, url_len + 11);
    let p = rust_session_wdata_ptr(fd, 0);
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

    rust_session_commit(fd, encrypt(fd) as usize);
    0
}

/// Logs a connection timeout and marks the session for closure.
///
/// Mirrors `clif_timeout` in `c_src/sl_compat.c` (line 2431).
/// Declared in `c_src/map_parse.h` line 121.
///
/// Guard checks (matching C source):
/// - If `fd == char_fd` → return 0 (never time out the char-server link).
/// - If `fd <= 1` → return 0 (reserved / stdin / stdout fds).
/// - If session does not exist → return 0.
/// - If `rust_session_get_data(fd)` is null → set eof=12 then return 0.
///
/// On a valid player session, logs `"<name> (IP: a.b.c.d) timed out!"` via
/// `tracing::info!` and sets eof=1.
///
/// # Safety
/// Safe to call with any fd value. No struct dereferences occur before `sd_ptr` is
/// verified non-null. All pointer dereferences follow their respective null checks.
#[no_mangle]
pub unsafe extern "C" fn clif_timeout(fd: c_int) -> c_int {
    if fd == crate::game::map_server::char_fd {
        return 0;
    }
    if fd <= 1 {
        return 0;
    }
    if rust_session_exists(fd) == 0 {
        return 0;
    }

    // Mirrors `if (!rust_session_get_data(fd)) rust_session_set_eof(fd, 12);`
    // in C — set eof then fall through to the nullpo_ret which returns 0.
    let sd_ptr = rust_session_get_data(fd) as *mut MapSessionData;
    if sd_ptr.is_null() {
        rust_session_set_eof(fd, 12);
        return 0;
    }

    let sdr = &*sd_ptr;

    // Decompose IP into four octets (little-endian byte order in sin_addr).
    let raw_ip = rust_session_get_client_ip(fd);
    let a = raw_ip & 0xff;
    let b = (raw_ip >> 8) & 0xff;
    let c = (raw_ip >> 16) & 0xff;
    let d = (raw_ip >> 24) & 0xff;

    // sd->status.name is [i8; 16] — interior pointer from a fixed array, never null.
    let name_display = std::ffi::CStr::from_ptr(sdr.status.name.as_ptr() as *const c_char)
        .to_string_lossy()
        .into_owned();

    tracing::info!(
        "{} (IP: {}.{}.{}.{}) timed out!",
        name_display, a, b, c, d
    );
    rust_session_set_eof(fd, 1);
    0
}

/// Sends a paper-popup display packet (opcode 0x35) to the client.
///
/// Mirrors `clif_paperpopup` in `c_src/sl_compat.c` (line 2456).
/// Declared in `c_src/map_parse.h` line 263.
///
/// Packet layout (total = `str_len + 14`):
/// ```text
/// [0xAA][len_hi][len_lo][0x35][0x00][0x00][width][height][0x00][str_hi][str_lo][...string bytes...]
/// ```
/// where the length field = `str_len + 11` (bytes after the 3-byte framing header).
/// Byte 4 is not written in C (WFIFOHEAD leaves it as zero); written explicitly here for safety.
///
/// # Safety
/// - `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
/// - `buf` must be a valid, null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn clif_paperpopup(
    sd: *mut MapSessionData,
    buf: *const c_char,
    width: c_int,
    height: c_int,
) -> c_int {
    if sd.is_null() {
        return 0;
    }
    let sdr = &*sd;
    let fd = sdr.fd;

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    let str_bytes = std::ffi::CStr::from_ptr(buf).to_bytes();
    let str_len = str_bytes.len();

    // C: WFIFOHEAD(fd, strlen(buf) + 11 + 3) — total = str_len + 14.
    rust_session_wfifohead(fd, str_len + 14);
    let p = rust_session_wdata_ptr(fd, 0);
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

    rust_session_commit(fd, encrypt(fd) as usize);
    0
}

/// Sends a paper-popup write packet (opcode 0x1B) to the client.
///
/// Mirrors `clif_paperpopupwrite` in `c_src/sl_compat.c` (line 2476).
/// Declared in `c_src/map_parse.h` line 261.
///
/// Identical layout to [`clif_paperpopup`] except:
/// - opcode at byte 3 is `0x1B` instead of `0x35`.
/// - byte 5 carries `invslot` instead of `0x00`.
/// - byte 6 is `0x00`, byte 7 is `width`, byte 8 is `height`.
///
/// # Safety
/// - `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
/// - `buf` must be a valid, null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn clif_paperpopupwrite(
    sd: *mut MapSessionData,
    buf: *const c_char,
    width: c_int,
    height: c_int,
    invslot: c_int,
) -> c_int {
    if sd.is_null() {
        return 0;
    }
    let sdr = &*sd;
    let fd = sdr.fd;

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    let str_bytes = std::ffi::CStr::from_ptr(buf).to_bytes();
    let str_len = str_bytes.len();

    // C: WFIFOHEAD(fd, strlen(buf) + 11 + 3) — total = str_len + 14.
    rust_session_wfifohead(fd, str_len + 14);
    let p = rust_session_wdata_ptr(fd, 0);
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

    rust_session_commit(fd, encrypt(fd) as usize);
    0
}

/// Sends a fixed 7-byte test packet (opcode 0x63) with a per-call incrementing counter.
///
/// Mirrors `clif_sendtest` in `c_src/sl_compat.c` (line 3575).
/// Declared in `c_src/map_parse.h` line 70.
///
/// The C original uses a `static int number` that increments after each send.
/// This port uses `static mut SENDTEST_NUMBER: u8` with wrapping arithmetic.
///
/// Packet layout (7 bytes fixed):
/// ```text
/// [0xAA][0x00][0x04][0x63][0x03][number][0x00]
/// ```
/// Bytes 1–2 are the big-endian length field (= 4, written literally as 0x00, 0x04).
/// The packet is committed via `encrypt(fd)` matching the C `WFIFOSET(fd, encrypt(...))`.
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
#[no_mangle]
pub unsafe extern "C" fn clif_sendtest(sd: *mut MapSessionData) -> c_int {
    static mut SENDTEST_NUMBER: u8 = 0;

    if sd.is_null() {
        return 0;
    }
    let sdr = &*sd;
    let fd = sdr.fd;

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    rust_session_wfifohead(fd, 7);
    let p = rust_session_wdata_ptr(fd, 0);
    if p.is_null() {
        return 0;
    }

    wb(p, 0, 0xAA);
    wb(p, 1, 0x00);
    wb(p, 2, 0x04);
    wb(p, 3, 0x63);
    wb(p, 4, 0x03);
    // SAFETY: single-threaded game loop; no concurrent access to SENDTEST_NUMBER.
    wb(p, 5, unsafe { SENDTEST_NUMBER });
    wb(p, 6, 0x00);

    rust_session_commit(fd, encrypt(fd) as usize);
    // Increment after send, matching C post-increment.
    unsafe { SENDTEST_NUMBER = SENDTEST_NUMBER.wrapping_add(1) };
    0
}

/// Logs the disconnect reason for a session and returns.
///
/// Mirrors `clif_print_disconnect` in `c_src/sl_compat.c` (line 3418).
/// Declared in `c_src/map_parse.h` (searched but not found there; declared as
/// `extern "C"` in `src/game/client/mod.rs` prior to this port).
///
/// Returns early (0) when eof == 4 (`ZERO_RECV_ERROR/NORMAL`) — the C source
/// also returns early in that case without printing.
///
/// # Safety
/// No pointer dereferences — reads only the eof flag via `rust_session_get_eof`.
#[no_mangle]
pub unsafe extern "C" fn clif_print_disconnect(fd: c_int) -> c_int {
    let eof = rust_session_get_eof(fd);
    if eof == 4 {
        return 0;
    }

    let reason = match eof {
        0x00 | 0x01 => "NORMAL_EOF",
        0x02        => "SOCKET_SEND_ERROR",
        0x03        => "SOCKET_RECV_ERROR",
        0x04        => "ZERO_RECV_ERROR(NORMAL)",
        0x05        => "MISSING_WDATA",
        0x06        => "WDATA_REALLOC",
        0x07        => "NO_MMO_DATA",
        0x08        => "SESSIONDATA_EXISTS",
        0x09        => "PLAYER_CONNECTING",
        0x0A        => "INVALID_EXCHANGE",
        0x0B        => "ACCEPT_NAMELEN_ERROR",
        0x0C        => "PLAYER_TIMEOUT",
        0x0D        => "INVALID_PACKET_HEADER",
        0x0E        => "WPE_HACK",
        _           => "UNKNOWN",
    };

    tracing::info!("[map] disconnect fd={} reason={}", fd, reason);
    0
}

// ─── Task 18: RFIFO-reader functions ──────────────────────────────────────────

/// Saves the player's paper-popup note text for the given inventory slot.
///
/// Mirrors `clif_paperpopupwrite_save` in `c_src/sl_compat.c` (line 2431).
/// Not declared in `map_parse.h`; declared in `c_src/sl_compat.c` only.
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
/// - `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
/// - The caller must have validated that `RFIFOREST(fd) >= 8 + copy_len` before calling,
///   matching the C original's requirement.
#[no_mangle]
pub unsafe extern "C" fn clif_paperpopupwrite_save(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() {
        return 0;
    }
    let fd = (*sd).fd;

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    // RFIFOB(fd, 5) — inventory slot index.
    let rdata = rust_session_rdata_ptr(fd, 0);
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
    // Remaining bytes stay zero — mirrors C's memset(input, 0, 300) + memcpy.

    let note = &(*sd).status.inventory[slot].note;

    // Only update if the note actually changed (mirrors C's strcmp guard).
    if *note != input {
        std::ptr::copy_nonoverlapping(input.as_ptr(), (*sd).status.inventory[slot].note.as_mut_ptr(), 300);
    }
    0
}

/// Reads a new profile picture and profile text from the incoming packet.
///
/// Mirrors `clif_changeprofile` in `c_src/sl_compat.c` (line 3189).
/// Not declared in `map_parse.h`; declared in `c_src/sl_compat.c` only.
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
/// - `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
/// - The caller must have validated that `RFIFOREST(fd) >= 5 + profilepic_size + 1 + profile_size`
///   before calling, matching the C original's requirement.
#[no_mangle]
pub unsafe extern "C" fn clif_changeprofile(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() {
        return 0;
    }
    let fd = (*sd).fd;

    // Project convention: guard against closed sessions (C original omitted this).
    if rust_session_exists(fd) == 0 {
        return 0;
    }

    let rdata = rust_session_rdata_ptr(fd, 0);
    if rdata.is_null() {
        return 0;
    }

    // SWAP16(RFIFOW(fd, 5)) + 2 — matches C `unsigned short profilepic_size = ... + 2`.
    // wrapping_add preserves C's u16 wrap-on-overflow semantics (e.g. 65534 + 2 = 0).
    let profilepic_size: u16 =
        u16::from_be_bytes([*rdata.add(5), *rdata.add(6)]).wrapping_add(2);
    let profilepic_usize = profilepic_size as usize; // always ≤ 65535 — safe index

    // RFIFOB(fd, 5 + profilepic_size) + 1 — matches C `unsigned char profile_size = ... + 1`.
    // wrapping_add preserves C's u8 wrap-on-overflow semantics (255 + 1 = 0).
    let profile_size: u8 = (*rdata.add(5 + profilepic_usize)).wrapping_add(1);
    let profile_usize = profile_size as usize; // always ≤ 255 — safe index

    // Write sizes back to sd.
    (*sd).profilepic_size = profilepic_size;
    (*sd).profile_size    = profile_size;

    // Copy picture data: RFIFOP(fd, 5), length = profilepic_size.
    std::ptr::copy_nonoverlapping(
        rdata.add(5) as *const i8,
        (*sd).profilepic_data.as_mut_ptr(),
        profilepic_usize,
    );

    // Copy profile text: RFIFOP(fd, 5 + profilepic_size), length = profile_size.
    std::ptr::copy_nonoverlapping(
        rdata.add(5 + profilepic_usize) as *const i8,
        (*sd).profile_data.as_mut_ptr(),
        profile_usize,
    );

    0
}

/// Validates that the byte immediately following the current packet is a valid
/// framing byte (`0xAA`). Sets eof=1 and returns 1 if the framing is wrong.
///
/// Mirrors `check_packet_size` in `c_src/sl_compat.c` (line 3197).
/// Not declared in any `.h` file.
///
/// Logic:
/// - If `RFIFOREST(fd) > len` and the byte at `fd[len]` is not `0xAA`, the
///   session has framing corruption → `rust_session_set_eof(fd, 1)`, return 1.
/// - Otherwise return 0 (either there is no next byte yet, or it is valid).
///
/// # Safety
/// Pure fd-based — no struct dereferences.
#[no_mangle]
pub unsafe extern "C" fn check_packet_size(fd: c_int, len: c_int) -> c_int {
    if len < 0 {
        return 0;
    }
    let len_usize = len as usize;

    // RFIFOREST(fd) > (size_t)len
    if rust_session_available(fd) > len_usize {
        // RFIFOB(fd, len) — byte immediately after the current packet.
        let rdata = rust_session_rdata_ptr(fd, 0);
        if rdata.is_null() {
            return 0;
        }
        if *rdata.add(len_usize) != 0xAA {
            rust_session_set_eof(fd, 1);
            return 1;
        }
    }
    0
}

// ─── Task 19: Mob broadcast ────────────────────────────────────────────────────

/// Broadcasts a mob's facing direction to all nearby players (area, excluding self).
///
/// Mirrors `clif_sendmob_side` in `c_src/sl_compat.c` (line 2887).
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
#[no_mangle]
pub unsafe extern "C" fn clif_sendmob_side(mob: *mut crate::game::mob::MobSpawnData) -> c_int {
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
    let id_be = (mob.bl.id as u32).to_be_bytes();
    buf[5..9].copy_from_slice(&id_be);
    // WBUFB(buf, 9) = mob->side — cast from c_int to u8.
    buf[9] = mob.side as u8;

    // clif_send(buf, 16, &mob->bl, AREA_WOS=5)
    super::clif_send(buf.as_ptr(), 16, &mob.bl as *const _ as *mut _, 5)
}
