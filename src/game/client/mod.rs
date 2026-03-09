//! Client packet dispatcher — Rust replacement for `clif_parse` in `map_parse.c`.
//!
//! Packet framing (custom Nexon/TK protocol):
//!   [0]     = 0xAA magic
//!   [1..2]  = payload length (u16 big-endian)
//!   [3]     = opcode
//!   [4]     = encryption seed
//!   [5..]   = payload
//!
//! Total packet size = length_field + 3.
//!
//! All handler functions remain in C (`map_parse.c`) for now. They are called via
//! Client packet send functions. Remove stubs from this module
//! and add a Rust implementation in its place.

pub mod handlers;
pub mod visual;

// Shims for visual.rs functions that take *mut MapSessionData but are called from
// the dispatcher with *mut std::ffi::c_void. These thin wrappers perform the pointer cast.
#[inline]
unsafe fn clif_cancelafk(sd: *mut std::ffi::c_void) {
    visual::clif_cancelafk(sd as *mut crate::game::pc::MapSessionData);
}
#[inline]
unsafe fn clif_sendprofile(sd: *mut std::ffi::c_void) {
    visual::clif_sendprofile(sd as *mut crate::game::pc::MapSessionData);
}
#[inline]
unsafe fn clif_sendboard(sd: *mut std::ffi::c_void) {
    visual::clif_sendboard(sd as *mut crate::game::pc::MapSessionData);
}

use crate::session::{
    rust_session_exists, rust_session_get_data, rust_session_get_eof,
    rust_session_rdata_ptr, rust_session_set_eof, rust_session_skip,
    rust_session_available, rust_session_wfifohead, rust_session_wdata_ptr,
    rust_session_commit,
};
use crate::session::get_session_manager;
use crate::database::map_db::BlockList;
use crate::game::pc::MapSessionData;

// ─── Session buffer helpers ───────────────────────────────────────────────────

/// Read one byte from session recv buffer at `pos`.
/// Read one byte from the receive FIFO at offset `pos`.
#[inline]
unsafe fn rbyte(fd: i32, pos: usize) -> u8 {
    let p = rust_session_rdata_ptr(fd, pos);
    if p.is_null() { 0 } else { *p }
}

/// Read two bytes at `pos` as big-endian u16.
/// Read a big-endian u16 from the receive FIFO at offset `pos`.
#[inline]
unsafe fn rword_be(fd: i32, pos: usize) -> u16 {
    let p = rust_session_rdata_ptr(fd, pos);
    if p.is_null() { return 0; }
    u16::from_be_bytes([*p, *p.add(1)])
}

/// Read four bytes at `pos` as big-endian u32.
/// Read a big-endian u32 from the receive FIFO at offset `pos`.
#[inline]
unsafe fn rlong_be(fd: i32, pos: usize) -> u32 {
    let p = rust_session_rdata_ptr(fd, pos);
    if p.is_null() { return 0; }
    u32::from_be_bytes([*p, *p.add(1), *p.add(2), *p.add(3)])
}

/// Raw pointer into recv buffer at `pos`.
/// Return a raw pointer into the receive FIFO at offset `pos`.
#[inline]
unsafe fn rptr(fd: i32, pos: usize) -> *const i8 {
    rust_session_rdata_ptr(fd, pos) as *const i8
}


// Dispatcher calls functions with sd: *mut std::ffi::c_void. Thin wrappers do the cast.
// Full-path references in wrapper bodies avoid name conflicts.

use crate::network::crypt::{decrypt, encrypt};
use crate::game::scripting::pc_accessors::{
    sl_pc_time, sl_pc_set_time, sl_pc_chat_timer, sl_pc_set_chat_timer,
    sl_pc_attacked, sl_pc_set_attacked, sl_pc_attack_speed, sl_pc_loaded,
    sl_pc_paralyzed, sl_pc_sleep, sl_pc_status_id, sl_pc_status_gm_level,
    sl_pc_status_mute, sl_pc_inventory_id, sl_pc_bl_m, sl_map_spell,
};
use crate::database::item_db::rust_itemdb_thrownconfirm;
use crate::game::pc::rust_pc_atkspeed;
use crate::game::time_util::timer_insert;

// Dispatcher wrappers — match dispatcher's *mut std::ffi::c_void calling convention.
type SD = *mut crate::game::pc::MapSessionData;
#[inline] unsafe fn clif_isignore(src: *mut std::ffi::c_void, dst: *mut std::ffi::c_void) -> i32 {
    crate::game::map_parse::chat::clif_isignore(src as SD, dst as SD)
}
#[inline] unsafe fn decrypt_fd(fd: i32) { decrypt(fd); }
#[inline] unsafe fn clif_handle_disconnect(sd: *mut std::ffi::c_void) { crate::game::client::handlers::clif_handle_disconnect(sd as SD); }
#[inline] unsafe fn clif_closeit(sd: *mut std::ffi::c_void) { crate::game::map_parse::dialogs::clif_closeit(sd as SD); }
#[inline] unsafe fn clif_mystaytus(sd: *mut std::ffi::c_void) { crate::game::map_parse::player_state::clif_mystaytus(sd as SD); }
#[inline] unsafe fn clif_groupstatus(sd: *mut std::ffi::c_void) { crate::game::map_parse::groups::clif_groupstatus(sd as SD); }
#[inline] unsafe fn clif_refresh(sd: *mut std::ffi::c_void) { crate::game::map_parse::player_state::clif_refresh(sd as SD); }
#[inline] unsafe fn clif_changestatus(sd: *mut std::ffi::c_void, status: u8) { crate::game::client::handlers::clif_changestatus(sd as SD, status as i32); }
#[inline] unsafe fn clif_accept2(fd: i32, name: *const i8, val: u8) { crate::game::client::handlers::clif_accept2(fd, name as *mut i8, val as i32); }
#[inline] unsafe fn clif_parsemap(sd: *mut std::ffi::c_void) { crate::game::map_parse::movement::clif_parsemap(sd as SD); }
#[inline] unsafe fn clif_parsewalk(sd: *mut std::ffi::c_void) { crate::game::map_parse::movement::clif_parsewalk(sd as SD); }
#[inline] unsafe fn clif_parsewalkpong(sd: *mut std::ffi::c_void) { crate::game::map_parse::movement::clif_parsewalkpong(sd as SD); }
#[inline] unsafe fn clif_handle_missingobject(sd: *mut std::ffi::c_void) { crate::game::client::handlers::clif_handle_missingobject(sd as SD); }
#[inline] unsafe fn clif_parselookat(sd: *mut std::ffi::c_void) { crate::game::map_parse::movement::clif_parselookat(sd as SD); }
#[inline] unsafe fn clif_parselookat_2(sd: *mut std::ffi::c_void) { crate::game::map_parse::movement::clif_parselookat_2(sd as SD); }
#[inline] unsafe fn clif_open_sub(sd: *mut std::ffi::c_void) { crate::game::map_parse::items::clif_open_sub(sd as SD); }
#[inline] unsafe fn clif_handle_clickgetinfo(sd: *mut std::ffi::c_void) { crate::game::map_parse::dialogs::clif_handle_clickgetinfo(sd as SD); }
#[inline] unsafe fn clif_parseviewchange(sd: *mut std::ffi::c_void) { crate::game::map_parse::movement::clif_parseviewchange(sd as SD); }
#[inline] unsafe fn clif_parseside(sd: *mut std::ffi::c_void) { crate::game::map_parse::movement::clif_parseside(sd as SD); }
#[inline] unsafe fn clif_parseemotion(sd: *mut std::ffi::c_void) { crate::game::map_parse::chat::clif_parseemotion(sd as SD); }
#[inline] unsafe fn clif_parsesay(sd: *mut std::ffi::c_void) { crate::game::map_parse::chat::clif_parsesay(sd as SD); }
#[inline] unsafe fn clif_parsewisp(sd: *mut std::ffi::c_void) { crate::game::map_parse::chat::clif_parsewisp(sd as SD); }
#[inline] unsafe fn clif_parseignore(sd: *mut std::ffi::c_void) { crate::game::map_parse::chat::clif_parseignore(sd as SD); }
#[inline] unsafe fn clif_parsefriends(sd: *mut std::ffi::c_void, list: *const i8, len: i32) { crate::game::client::handlers::clif_parsefriends(sd as SD, list, len); }
#[inline] unsafe fn clif_user_list(sd: *mut std::ffi::c_void) { crate::game::client::visual::clif_user_list(sd as SD); }
#[inline] unsafe fn clif_addgroup(sd: *mut std::ffi::c_void) { crate::game::map_parse::groups::clif_addgroup(sd as SD); }
#[inline] unsafe fn clif_parsegetitem(sd: *mut std::ffi::c_void) { crate::game::map_parse::items::clif_parsegetitem(sd as SD); }
#[inline] unsafe fn clif_parsedropitem(sd: *mut std::ffi::c_void) { crate::game::client::handlers::clif_parsedropitem(sd as SD); }
#[inline] unsafe fn clif_parseeatitem(sd: *mut std::ffi::c_void) { crate::game::map_parse::items::clif_parseeatitem(sd as SD); }
#[inline] unsafe fn clif_parseuseitem(sd: *mut std::ffi::c_void) { crate::game::map_parse::items::clif_parseuseitem(sd as SD); }
#[inline] unsafe fn clif_parseunequip(sd: *mut std::ffi::c_void) { crate::game::map_parse::items::clif_parseunequip(sd as SD); }
#[inline] unsafe fn clif_parsewield(sd: *mut std::ffi::c_void) { crate::game::map_parse::items::clif_parsewield(sd as SD); }
#[inline] unsafe fn clif_parsethrow(sd: *mut std::ffi::c_void) { crate::game::map_parse::items::clif_parsethrow(sd as SD); }
#[inline] unsafe fn clif_throwconfirm(sd: *mut std::ffi::c_void) { crate::game::map_parse::items::clif_throwconfirm(sd as SD); }
#[inline] unsafe fn clif_dropgold(sd: *mut std::ffi::c_void, amount: u32) { crate::game::map_parse::items::clif_dropgold(sd as SD, amount); }
#[inline] unsafe fn clif_postitem(sd: *mut std::ffi::c_void) { crate::game::client::handlers::clif_postitem(sd as SD); }
#[inline] unsafe fn clif_handitem(sd: *mut std::ffi::c_void) { crate::game::map_parse::trading::clif_handitem(sd as SD); }
#[inline] unsafe fn clif_handgold(sd: *mut std::ffi::c_void) { crate::game::map_parse::trading::clif_handgold(sd as SD); }
#[inline] unsafe fn clif_parsemagic(sd: *mut std::ffi::c_void) { crate::game::map_parse::combat::clif_parsemagic(sd as SD); }
#[inline] unsafe fn clif_parseattack(sd: *mut std::ffi::c_void) { crate::game::map_parse::combat::clif_parseattack(sd as SD); }
#[inline] unsafe fn clif_sendminitext(sd: *mut std::ffi::c_void, msg: *const i8) { crate::game::map_parse::chat::clif_sendminitext(sd as SD, msg); }
#[inline] unsafe fn clif_parsenpcdialog(sd: *mut std::ffi::c_void) { crate::game::map_parse::dialogs::clif_parsenpcdialog(sd as SD); }
#[inline] unsafe fn clif_handle_menuinput(sd: *mut std::ffi::c_void) { crate::game::client::handlers::clif_handle_menuinput(sd as SD); }
#[inline] unsafe fn clif_paperpopupwrite_save(sd: *mut std::ffi::c_void) { crate::game::client::visual::clif_paperpopupwrite_save(sd as SD); }
#[inline] unsafe fn clif_parsechangespell(sd: *mut std::ffi::c_void) { crate::game::map_parse::items::clif_parsechangespell(sd as SD); }
#[inline] unsafe fn clif_parsechangepos(sd: *mut std::ffi::c_void) { crate::game::client::handlers::clif_parsechangepos(sd as SD); }
#[inline] unsafe fn rust_pc_warp(sd: *mut std::ffi::c_void, m: i32, x: i32, y: i32) -> i32 { crate::game::pc::rust_pc_warp(sd as SD, m, x, y) }
#[inline] unsafe fn clif_changeprofile(sd: *mut std::ffi::c_void) { crate::game::client::visual::clif_changeprofile(sd as SD); }
#[inline] unsafe fn clif_handle_boards(sd: *mut std::ffi::c_void) { crate::game::client::handlers::clif_handle_boards(sd as SD); }
#[inline] unsafe fn clif_handle_powerboards(sd: *mut std::ffi::c_void) { crate::game::client::handlers::clif_handle_powerboards(sd as SD); }
#[inline] unsafe fn clif_parseparcel(sd: *mut std::ffi::c_void) { crate::game::map_parse::groups::clif_parseparcel(sd as SD); }
#[inline] unsafe fn clif_parseranking(sd: *mut std::ffi::c_void, fd: i32) { crate::game::map_parse::events::clif_parseranking(sd as SD, fd); }
#[inline] unsafe fn clif_sendRewardInfo(sd: *mut std::ffi::c_void, fd: i32) { crate::game::map_parse::events::clif_sendRewardInfo(sd as SD, fd); }
#[inline] unsafe fn clif_getReward(sd: *mut std::ffi::c_void, fd: i32) { crate::game::map_parse::events::clif_getReward(sd as SD, fd); }
#[inline] unsafe fn clif_sendtowns(sd: *mut std::ffi::c_void) { crate::game::map_parse::dialogs::clif_sendtowns(sd as SD); }
#[inline] unsafe fn clif_huntertoggle(sd: *mut std::ffi::c_void) { crate::game::map_parse::groups::clif_huntertoggle(sd as SD); }
#[inline] unsafe fn clif_sendhunternote(sd: *mut std::ffi::c_void) { crate::game::map_parse::groups::clif_sendhunternote(sd as SD); }
#[inline] unsafe fn clif_sendminimap(sd: *mut std::ffi::c_void) { crate::game::map_parse::player_state::clif_sendminimap(sd as SD); }
#[inline] unsafe fn clif_parse_exchange(sd: *mut std::ffi::c_void) { crate::game::map_parse::trading::clif_parse_exchange(sd as SD); }
#[inline] unsafe fn send_meta(sd: *mut std::ffi::c_void) { crate::network::crypt::send_meta(sd as SD); }
#[inline] unsafe fn send_metalist(sd: *mut std::ffi::c_void) { crate::network::crypt::send_metalist(sd as SD); }
#[inline] unsafe fn createdb_start(sd: *mut std::ffi::c_void) { crate::game::client::handlers::createdb_start(sd); }

// ─── Send-type constants (from map_parse.h) ───────────────────────────────────

const ALL_CLIENT:  i32 = 0;
const SAMESRV:     i32 = 1;
const SAMEMAP:     i32 = 2;
const SAMEMAP_WOS: i32 = 3;
const AREA:        i32 = 4;
const AREA_WOS:    i32 = 5;
const SAMEAREA:    i32 = 6;
const SAMEAREA_WOS: i32 = 7;
const CORNER:      i32 = 8;
const SELF:        i32 = 9;

/// BL_PC type constant (from map_server.h).
const BL_PC: u8 = 0x01;

// ─── clif_send / clif_sendtogm ────────────────────────────────────────────────

/// Send `buf[0..len]` to clients matching `type`, applying ignore-list filtering.
///
/// Send a packet to a specific client fd.
///
/// # Safety
///
/// - `buf` must point to at least `len` readable bytes.
/// - `bl` must be a valid pointer to an initialized `BlockList`. When
///   `bl.bl_type == BL_PC`, it may be cast to `*mut MapSessionData`. When
///   `send_type == SELF`, `bl` must point to a `MapSessionData` (`bl_type == BL_PC`).
pub unsafe fn clif_send(
    buf: *const u8,
    len: i32,
    bl: *mut BlockList,
    send_type: i32,
) -> i32 {
    // Compute once: non-null only when `bl` is a player (BL_PC), used for ignore-list checks.
    let tsd: *mut MapSessionData = if (*bl).bl_type == BL_PC {
        bl as *mut MapSessionData
    } else {
        std::ptr::null_mut()
    };

    match send_type {
        ALL_CLIENT | SAMESRV => {
            for i_fd in get_session_manager().get_all_fds() {
                let sd = rust_session_get_data(i_fd) as *mut MapSessionData;
                if sd.is_null() {
                    continue;
                }
                // Ignore-list: skip if this is a whisper (opcode 0x0D) that src ignores.
                if !tsd.is_null()
                    && !buf.is_null()
                    && *buf.add(3) == 0x0D
                    && clif_isignore(tsd as *mut std::ffi::c_void, sd as *mut std::ffi::c_void) == 0
                {
                    continue;
                }
                send_to_fd(i_fd, buf, len);
            }
        }
        SAMEMAP => {
            for i_fd in get_session_manager().get_all_fds() {
                let sd = rust_session_get_data(i_fd) as *mut MapSessionData;
                if sd.is_null() {
                    continue;
                }
                if (*sd).bl.m != (*bl).m {
                    continue;
                }
                if !tsd.is_null()
                    && !buf.is_null()
                    && *buf.add(3) == 0x0D
                    && clif_isignore(tsd as *mut std::ffi::c_void, sd as *mut std::ffi::c_void) == 0
                {
                    continue;
                }
                send_to_fd(i_fd, buf, len);
            }
        }
        SAMEMAP_WOS => {
            for i_fd in get_session_manager().get_all_fds() {
                let sd = rust_session_get_data(i_fd) as *mut MapSessionData;
                if sd.is_null() {
                    continue;
                }
                if (*sd).bl.m != (*bl).m {
                    continue;
                }
                // Skip sending to `bl` itself when it is a player.
                if !tsd.is_null() && sd == tsd {
                    continue;
                }
                if !tsd.is_null()
                    && !buf.is_null()
                    && *buf.add(3) == 0x0D
                    && clif_isignore(tsd as *mut std::ffi::c_void, sd as *mut std::ffi::c_void) == 0
                {
                    continue;
                }
                send_to_fd(i_fd, buf, len);
            }
        }
        AREA | AREA_WOS => {
            clif_send_area(
                (*bl).m as i32,
                (*bl).x as i32,
                (*bl).y as i32,
                AREA,
                send_type,
                buf,
                len,
                bl,
            );
        }
        SAMEAREA | SAMEAREA_WOS => {
            clif_send_area(
                (*bl).m as i32,
                (*bl).x as i32,
                (*bl).y as i32,
                SAMEAREA,
                send_type,
                buf,
                len,
                bl,
            );
        }
        CORNER => {
            clif_send_area(
                (*bl).m as i32,
                (*bl).x as i32,
                (*bl).y as i32,
                CORNER,
                send_type,
                buf,
                len,
                bl,
            );
        }
        SELF => {
            let sd = bl as *mut MapSessionData;
            send_to_fd((*sd).fd, buf, len);
        }
        _ => {}
    }
    0
}

/// Send `buf[0..len]` to clients matching `type`, without ignore-list filtering.
///
/// Send a packet to all GM players on the map.
///
/// # Safety
///
/// - `buf` must point to at least `len` readable bytes.
/// - `bl` must be a valid pointer to an initialized `BlockList`. When
///   `bl.bl_type == BL_PC`, it may be cast to `*mut MapSessionData`. When
///   `send_type == SELF`, `bl` must point to a `MapSessionData` (`bl_type == BL_PC`).
pub unsafe fn clif_sendtogm(
    buf: *const u8,
    len: i32,
    bl: *mut BlockList,
    send_type: i32,
) -> i32 {
    match send_type {
        ALL_CLIENT | SAMESRV => {
            for i_fd in get_session_manager().get_all_fds() {
                let sd = rust_session_get_data(i_fd) as *mut MapSessionData;
                if sd.is_null() {
                    continue;
                }
                send_to_fd(i_fd, buf, len);
            }
        }
        SAMEMAP => {
            for i_fd in get_session_manager().get_all_fds() {
                let sd = rust_session_get_data(i_fd) as *mut MapSessionData;
                if sd.is_null() {
                    continue;
                }
                if (*sd).bl.m != (*bl).m {
                    continue;
                }
                send_to_fd(i_fd, buf, len);
            }
        }
        SAMEMAP_WOS => {
            let src_sd = if (*bl).bl_type == BL_PC {
                bl as *mut MapSessionData
            } else {
                std::ptr::null_mut()
            };
            for i_fd in get_session_manager().get_all_fds() {
                let sd = rust_session_get_data(i_fd) as *mut MapSessionData;
                if sd.is_null() {
                    continue;
                }
                if (*sd).bl.m != (*bl).m {
                    continue;
                }
                if !src_sd.is_null() && sd == src_sd {
                    continue;
                }
                send_to_fd(i_fd, buf, len);
            }
        }
        AREA | AREA_WOS => {
            clif_send_area(
                (*bl).m as i32,
                (*bl).x as i32,
                (*bl).y as i32,
                AREA,
                send_type,
                buf,
                len,
                bl,
            );
        }
        SAMEAREA | SAMEAREA_WOS => {
            clif_send_area(
                (*bl).m as i32,
                (*bl).x as i32,
                (*bl).y as i32,
                SAMEAREA,
                send_type,
                buf,
                len,
                bl,
            );
        }
        CORNER => {
            clif_send_area(
                (*bl).m as i32,
                (*bl).x as i32,
                (*bl).y as i32,
                CORNER,
                send_type,
                buf,
                len,
                bl,
            );
        }
        SELF => {
            let sd = bl as *mut MapSessionData;
            send_to_fd((*sd).fd, buf, len);
        }
        _ => {}
    }
    0
}

/// Write `buf[0..len]` into fd's send buffer and commit.
///
/// Encrypt and commit `buf` to the send FIFO.
///
/// # Safety
///
/// - `fd` must be a live session registered with the session manager.
/// - `buf` must point to at least `len` readable bytes.
#[inline]
unsafe fn send_to_fd(fd: i32, buf: *const u8, len: i32) {
    rust_session_wfifohead(fd, (len as usize) + 3);
    let wptr = rust_session_wdata_ptr(fd, 0);
    if !wptr.is_null() {
        std::ptr::copy_nonoverlapping(buf, wptr, len as usize);
    }
    rust_session_commit(fd, encrypt(fd) as usize);
}

// ─── clif_send_area / clif_send_sub ─────────────────────────────────────────

/// Channel registry: (global_reg name, channel byte value).
///
/// When a 0x0D packet carries one of these channel byte values, the player must
/// have the corresponding global_reg set ≥ 1 to receive it.  The channel byte is
/// temporarily zeroed out while writing to the session buffer, then restored.
const CHANNEL_REGS: &[(&str, u8)] = &[
    ("chann_en", 10),
    ("chann_es", 11),
    ("chann_fr", 12),
    ("chann_cn", 13),
    ("chann_pt", 14),
    ("chann_id", 15),
];

/// Decide whether the packet in `buf` should be sent to the target session `sd`,
/// given that the source is `src_bl` and the send type is `send_type`.
///
/// Returns `true` if the packet should be delivered.
///
/// Filtering logic for area sends, excluding the
/// channel-write and WFIFO operations (those live in `send_to_area`).
///
/// # Safety
/// - `sd` must be a valid, initialized `MapSessionData`.
/// - `src_bl` must be a valid `BlockList` pointer. It may be a `MapSessionData`
///   when `bl_type == BL_PC`.
/// - `buf` must point to at least `len` readable bytes.
#[inline]
unsafe fn should_send_to(
    sd: *mut MapSessionData,
    src_bl: *mut BlockList,
    send_type: i32,
    buf: *const u8,
    len: i32,
) -> bool {
    use crate::game::pc::{OPT_FLAG_STEALTH, OPT_FLAG_GHOSTS};
    use crate::database::map_db::map;
    use crate::database::map_db::MAP_SLOTS;

    if sd.is_null() || src_bl.is_null() {
        return false;
    }

    // Derive tsd: source player (non-null only when src_bl is BL_PC).
    let tsd: *mut MapSessionData = if (*src_bl).bl_type == BL_PC {
        src_bl as *mut MapSessionData
    } else {
        std::ptr::null_mut()
    };

    // ── Stealth filter ────────────────────────────────────────────────────────
    // If source is stealthed, only send to GMs or to the source themselves.
    if !tsd.is_null() {
        if ((*tsd).optFlags & OPT_FLAG_STEALTH) != 0
            && (*sd).status.gm_level == 0
            && (*sd).status.id != (*tsd).status.id
        {
            return false;
        }

        // ── Ghost filter ──────────────────────────────────────────────────────
        // If the map shows ghosts and the source is a ghost (state==1), only
        // send to other ghosts or to players that opted into ghost visibility.
        let m_idx = (*tsd).bl.m as usize;
        if !map.is_null() && m_idx < MAP_SLOTS {
            let map_slot = &*map.add(m_idx);
            if map_slot.show_ghosts != 0
                && (*tsd).status.state == 1
                && (*tsd).bl.id != (*sd).bl.id
                && (*sd).status.state != 1
                && ((*sd).optFlags & OPT_FLAG_GHOSTS) == 0
            {
                return false;
            }
        }
    }

    // ── Ignore-list filter (whisper-like packets, opcode 0x0D) ───────────────
    if !tsd.is_null() && len >= 4 && !buf.is_null() && *buf.add(3) == 0x0D {
        if clif_isignore(tsd as *mut std::ffi::c_void, sd as *mut std::ffi::c_void) == 0 {
            return false;
        }
    }

    // ── WOS (without self) filter ─────────────────────────────────────────────
    match send_type {
        AREA_WOS | SAMEAREA_WOS => {
            if src_bl == &mut (*sd).bl as *mut BlockList {
                return false;
            }
        }
        _ => {}
    }

    // ── Session liveness ──────────────────────────────────────────────────────
    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return false;
    }

    true
}

/// Send `buf[0..len]` to all players in the spatial area defined by `area` around
/// `(x, y)` on map `m`, applying the filtering logic from `clif_send_sub`.
///
/// For channel packets (opcode 0x0D with byte 5 >= 10), the channel byte is
/// temporarily set to 0 before writing to each player's session buffer and
/// restored afterwards (mirrors the C behaviour).
///
/// # Safety
/// - `buf` must be a valid, writable pointer to at least `len` bytes.
///   The function temporarily mutates `buf[5]` for channel packets and restores it.
/// - `src_bl` must be a valid `BlockList` pointer.
/// - The `map` global and block grid must be initialized.
unsafe fn send_to_area(
    m: i32,
    x: i32,
    y: i32,
    area: crate::game::block::AreaType,
    buf: *mut u8,
    len: i32,
    src_bl: *mut BlockList,
    send_type: i32,
) {
    use crate::game::block::foreach_in_area;
    use crate::game::mob::BL_PC as BL_PC_I32;

    if buf.is_null() || src_bl.is_null() || len <= 0 {
        return;
    }

    // Determine if this is a channel packet: opcode 0x0D (byte 3) and channel byte (byte 5) >= 10.
    let is_channel_pkt = len >= 6 && *buf.add(3) == 0x0D && *buf.add(5) >= 10;

    foreach_in_area(m, x, y, area, BL_PC_I32, |bl| {
        let sd = bl as *mut MapSessionData;
        if !should_send_to(sd, src_bl, send_type, buf as *const u8, len) {
            return 0;
        }

        let fd = (*sd).fd;

        if is_channel_pkt {
            // Channel packet: check if the player has the matching channel reg.
            let ch_byte = *buf.add(5);
            let mut matched = false;
            for &(reg_name, ch_val) in CHANNEL_REGS {
                if ch_byte == ch_val {
                    // Check if player has this channel enabled (global_reg >= 1).
                    let reg_cstr = std::ffi::CString::new(reg_name).unwrap_or_default();
                    let v = crate::game::pc::rust_pc_readglobalreg(
                        sd,
                        reg_cstr.as_ptr() as *const i8,
                    );
                    if v >= 1 {
                        // Temporarily zero out channel byte, write, restore.
                        *buf.add(5) = 0;
                        rust_session_wfifohead(fd, (len as usize) + 3);
                        let wptr = rust_session_wdata_ptr(fd, 0);
                        if !wptr.is_null() {
                            std::ptr::copy_nonoverlapping(buf, wptr, len as usize);
                        }
                        rust_session_commit(fd, encrypt(fd) as usize);
                        *buf.add(5) = ch_byte;
                        matched = true;
                    }
                    break;
                }
            }
            // If channel byte doesn't match any known channel, send normally.
            if !matched && !CHANNEL_REGS.iter().any(|&(_, v)| v == ch_byte) {
                rust_session_wfifohead(fd, (len as usize) + 3);
                let wptr = rust_session_wdata_ptr(fd, 0);
                if !wptr.is_null() {
                    std::ptr::copy_nonoverlapping(buf, wptr, len as usize);
                }
                rust_session_commit(fd, encrypt(fd) as usize);
            }
        } else {
            // Normal packet: write directly.
            rust_session_wfifohead(fd, (len as usize) + 3);
            let wptr = rust_session_wdata_ptr(fd, 0);
            if !wptr.is_null() {
                std::ptr::copy_nonoverlapping(buf, wptr, len as usize);
            }
            rust_session_commit(fd, encrypt(fd) as usize);
        }

        0
    });
}

////
/// Wrapper around `send_to_area`
/// same signature as the old C function. `area_type` selects the spatial search
/// shape (AREA=4, SAMEAREA=6, CORNER=8); `send_type` is the send-type constant
/// (AREA_WOS=5, SAMEAREA_WOS=7, etc.) passed to the per-player filter.
///
/// # Safety
/// - `buf` must be a valid, writable pointer to at least `len` bytes.
/// - `src_bl` must be a valid `BlockList` pointer.
pub unsafe fn clif_send_area(
    m: i32,
    x: i32,
    y: i32,
    area_type: i32,
    send_type: i32,
    buf: *const u8,
    len: i32,
    src_bl: *mut BlockList,
) {
    use crate::game::block::AreaType;

    let area = match area_type {
        AREA      => AreaType::Area,
        SAMEAREA  => AreaType::SameArea,
        CORNER    => AreaType::Corner,
        _         => AreaType::Area,
    };

    // Cast away const: send_to_area temporarily mutates buf[5] for channel packets
    // and restores it before returning. The caller's buffer is logically unchanged.
    send_to_area(m, x, y, area, buf as *mut u8, len, src_bl, send_type);
}

// ─── Dual-login check ─────────────────────────────────────────────────────────

/// Returns `true` if a duplicate session was detected (both connections closed).
///
/// Uses the session manager's fd map directly — no fixed-size buffer needed.
unsafe fn check_dual_login(fd: i32, sd: *mut std::ffi::c_void) -> bool {
    let my_id = sl_pc_status_id(sd);
    let mut login_count = 0i32;
    for i_fd in get_session_manager().get_all_fds() {
        let tsd = rust_session_get_data(i_fd);
        if tsd.is_null() { continue; }
        if sl_pc_status_id(tsd) == my_id {
            login_count += 1;
        }
        if login_count >= 2 {
            tracing::warn!("[map] dual login char_id={} fd={} dup_fd={}", my_id, fd, i_fd);
            rust_session_set_eof(fd, 1);
            rust_session_set_eof(i_fd, 1);
            return true;
        }
    }
    false
}

// ─── Main dispatcher ──────────────────────────────────────────────────────────

/// Rust replacement for C `clif_parse(int fd)`.
/// Registered via `rust_session_set_default_parse` at map_server startup.
pub unsafe fn rust_clif_parse(fd: i32) -> i32 {
    if rust_session_exists(fd) == 0 {
        return 0;
    }

    let sd = rust_session_get_data(fd);

    // EOF → disconnect and clean up
    if rust_session_get_eof(fd) != 0 {
        tracing::info!("[map] [parse] fd={} eof reason={} sd_null={}", fd, rust_session_get_eof(fd), sd.is_null());
        if !sd.is_null() {
            clif_handle_disconnect(sd);
            clif_closeit(sd);
        }
        visual::clif_print_disconnect(fd);
        rust_session_set_eof(fd, 1);
        return 0;
    }

    // Validate packet header: must start with 0xAA
    let avail = rust_session_available(fd);
    if avail > 0 && rbyte(fd, 0) != 0xAA {
        tracing::warn!("[map] [parse] fd={} bad header byte0={:#04X} avail={}", fd, rbyte(fd, 0), avail);
        rust_session_set_eof(fd, 13);
        return 0;
    }
    if avail < 3 { return 0; }

    let pkt_len = rword_be(fd, 1) as usize + 3;
    if avail < pkt_len { return 0; }

    // Pre-login: only opcode 0x10 (character accept) is allowed
    if sd.is_null() {
        let op = rbyte(fd, 3);
        if op == 0x10 {
            tracing::debug!("[map] [parse] fd={} pre-login accept op=0x10", fd);
            clif_accept2(fd, rptr(fd, 16), rbyte(fd, 15));
        } else {
            tracing::debug!("[map] [parse] fd={} pre-login op={:#04X} dropped (sd not set)", fd, op);
        }
        rust_session_skip(fd, pkt_len);
        return 0;
    }

    // Dual-login check
    if check_dual_login(fd, sd) {
        rust_session_skip(fd, pkt_len);
        return 0;
    }

    decrypt(fd);

    tracing::debug!("[map] [parse] fd={} op={:#04X} pkt_len={}", fd, rbyte(fd, 3), pkt_len);
    match rbyte(fd, 3) {
        0x05 => {
            clif_parsemap(sd);
        }
        0x06 => {
            clif_cancelafk(sd);
            clif_parsewalk(sd);
        }
        0x07 => {
            clif_cancelafk(sd);
            sl_pc_set_time(sd, sl_pc_time(sd) + 1);
            if sl_pc_time(sd) < 4 {
                clif_parsegetitem(sd);
            }
        }
        0x08 => {
            clif_cancelafk(sd);
            clif_parsedropitem(sd);
        }
        0x09 => {
            clif_cancelafk(sd);
            clif_parselookat_2(sd);
        }
        0x0A => {
            clif_cancelafk(sd);
            clif_parselookat(sd);
        }
        0x0B => {
            clif_cancelafk(sd);
            clif_closeit(sd);
        }
        0x0C => {
            clif_handle_missingobject(sd);
        }
        0x0D => {
            clif_parseignore(sd);
        }
        0x0E => {
            clif_cancelafk(sd);
            if sl_pc_status_gm_level(sd) != 0 {
                clif_parsesay(sd);
            } else {
                sl_pc_set_chat_timer(sd, sl_pc_chat_timer(sd) + 1);
                if sl_pc_chat_timer(sd) < 2 && sl_pc_status_mute(sd) == 0 {
                    clif_parsesay(sd);
                }
            }
        }
        0x0F => {
            clif_cancelafk(sd);
            sl_pc_set_time(sd, sl_pc_time(sd) + 1);
            if sl_pc_paralyzed(sd) == 0 && sl_pc_sleep(sd) == 1 {
                if sl_pc_time(sd) < 4 {
                    if sl_map_spell(sl_pc_bl_m(sd)) != 0 || sl_pc_status_gm_level(sd) != 0 {
                        clif_parsemagic(sd);
                    } else {
                        clif_sendminitext(
                            sd,
                            b"That doesn't work here.\0".as_ptr() as *const i8,
                        );
                    }
                }
            }
        }
        0x11 => {
            clif_cancelafk(sd);
            clif_parseside(sd);
        }
        0x12 => {
            clif_cancelafk(sd);
            clif_parsewield(sd);
        }
        0x13 => {
            clif_cancelafk(sd);
            sl_pc_set_time(sd, sl_pc_time(sd) + 1);
            if sl_pc_attacked(sd) != 1 && sl_pc_attack_speed(sd) > 0 {
                sl_pc_set_attacked(sd, 1);
                let spd = sl_pc_attack_speed(sd);
                let delay = ((spd * 1000) / 60) as u32;
                timer_insert(
                    delay, delay, Some(rust_pc_atkspeed), sl_pc_status_id(sd), 0,
                );
                clif_parseattack(sd);
            }
        }
        0x17 => {
            clif_cancelafk(sd);
            let pos = rbyte(fd, 6) as i32;
            let confirm = rbyte(fd, 5);
            if rust_itemdb_thrownconfirm(sl_pc_inventory_id(sd, pos - 1)) == 1 {
                if confirm == 1 { clif_parsethrow(sd); } else { clif_throwconfirm(sd); }
            } else {
                clif_parsethrow(sd);
            }
        }
        0x18 => {
            clif_cancelafk(sd);
            clif_user_list(sd);
        }
        0x19 => {
            clif_cancelafk(sd);
            clif_parsewisp(sd);
        }
        0x1A => {
            clif_cancelafk(sd);
            clif_parseeatitem(sd);
        }
        0x1B => {
            if sl_pc_loaded(sd) != 0 {
                clif_changestatus(sd, rbyte(fd, 6));
            }
        }
        0x1C => {
            clif_cancelafk(sd);
            clif_parseuseitem(sd);
        }
        0x1D => {
            clif_cancelafk(sd);
            sl_pc_set_time(sd, sl_pc_time(sd) + 1);
            if sl_pc_time(sd) < 4 {
                clif_parseemotion(sd);
            }
        }
        0x1E => {
            clif_cancelafk(sd);
            sl_pc_set_time(sd, sl_pc_time(sd) + 1);
            if sl_pc_time(sd) < 4 {
                clif_parsewield(sd);
            }
        }
        0x1F => {
            clif_cancelafk(sd);
            if sl_pc_time(sd) < 4 {
                clif_parseunequip(sd);
            }
        }
        0x20 => {
            clif_cancelafk(sd);
            clif_open_sub(sd);
        }
        0x23 => {
            clif_paperpopupwrite_save(sd);
        }
        0x24 => {
            clif_cancelafk(sd);
            clif_dropgold(sd, rlong_be(fd, 5));
        }
        0x27 => {
            clif_cancelafk(sd);
            // Quest tab — no-op
        }
        0x29 => {
            clif_cancelafk(sd);
            clif_handitem(sd);
        }
        0x2A => {
            clif_cancelafk(sd);
            clif_handgold(sd);
        }
        0x2D => {
            clif_cancelafk(sd);
            if rbyte(fd, 5) == 0 { clif_mystaytus(sd); } else { clif_groupstatus(sd); }
        }
        0x2E => {
            clif_cancelafk(sd);
            clif_addgroup(sd);
        }
        0x30 => {
            clif_cancelafk(sd);
            if rbyte(fd, 5) == 1 { clif_parsechangespell(sd); } else { clif_parsechangepos(sd); }
        }
        0x32 => {
            clif_cancelafk(sd);
            clif_parsewalk(sd);
        }
        // 0x34 falls through to 0x38 in C — both fire
        0x34 => {
            clif_cancelafk(sd);
            clif_postitem(sd);
            clif_cancelafk(sd);
            clif_refresh(sd);
        }
        0x38 => {
            clif_cancelafk(sd);
            clif_refresh(sd);
        }
        0x39 => {
            clif_cancelafk(sd);
            clif_handle_menuinput(sd);
        }
        0x3A => {
            clif_cancelafk(sd);
            clif_parsenpcdialog(sd);
        }
        0x3B => {
            clif_cancelafk(sd);
            clif_handle_boards(sd);
        }
        0x3F => {
            rust_pc_warp(sd, rword_be(fd, 5) as i32, rword_be(fd, 7) as i32, rword_be(fd, 9) as i32);
        }
        0x41 => {
            clif_cancelafk(sd);
            clif_parseparcel(sd);
        }
        0x42 => { /* Client crash debug — no-op */ }
        0x43 => {
            clif_cancelafk(sd);
            clif_handle_clickgetinfo(sd);
        }
        0x4A => {
            clif_cancelafk(sd);
            clif_parse_exchange(sd);
        }
        0x4C => {
            clif_cancelafk(sd);
            clif_handle_powerboards(sd);
        }
        0x4F => {
            clif_cancelafk(sd);
            clif_changeprofile(sd);
        }
        0x60 => { /* PING — no-op */ }
        0x66 => {
            clif_cancelafk(sd);
            clif_sendtowns(sd);
        }
        0x69 => { /* Obstruction — no-op */ }
        0x6B => {
            clif_cancelafk(sd);
            createdb_start(sd);
        }
        0x73 => {
            if rbyte(fd, 5) == 0x04 {
                clif_sendprofile(sd);
            } else if rbyte(fd, 5) == 0x00 {
                clif_sendboard(sd);
            }
        }
        0x75 => {
            clif_parsewalkpong(sd);
        }
        0x77 => {
            clif_cancelafk(sd);
            let friends_len = rword_be(fd, 1) as i32 - 5;
            clif_parsefriends(sd, rptr(fd, 5), friends_len);
        }
        0x7B => match rbyte(fd, 5) {
            0 => send_meta(sd),
            1 => send_metalist(sd),
            _ => {}
        },
        0x7C => {
            clif_cancelafk(sd);
            clif_sendminimap(sd);
        }
        0x7D => {
            clif_cancelafk(sd);
            match rbyte(fd, 5) {
                5 => clif_sendRewardInfo(sd, fd),
                6 => clif_getReward(sd, fd),
                _ => clif_parseranking(sd, fd),
            }
        }
        0x82 => {
            clif_cancelafk(sd);
            clif_parseviewchange(sd);
        }
        0x83 => { /* Screenshots — no-op */ }
        0x84 => {
            clif_cancelafk(sd);
            clif_huntertoggle(sd);
        }
        0x85 => {
            clif_sendhunternote(sd);
            clif_cancelafk(sd);
        }
        op => {
            tracing::warn!("[map] [client] unknown packet op={:#04X}", op);
        }
    }

    rust_session_skip(fd, pkt_len);
    0
}
