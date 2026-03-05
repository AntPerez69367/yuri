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
//! `extern "C"` stubs below. As each handler is ported to Rust, remove its stub
//! and add a Rust implementation in its place.

use std::os::raw::{c_char, c_int, c_void};
use crate::ffi::session::{
    rust_session_exists, rust_session_get_data, rust_session_get_eof,
    rust_session_rdata_ptr, rust_session_set_eof, rust_session_skip,
    rust_session_available,
};
use crate::session::get_session_manager;

// ─── Session buffer helpers ───────────────────────────────────────────────────

/// Read one byte from session recv buffer at `pos`.
/// Mirrors `RFIFOB(fd, pos)`.
#[inline]
unsafe fn rbyte(fd: c_int, pos: usize) -> u8 {
    let p = rust_session_rdata_ptr(fd, pos);
    if p.is_null() { 0 } else { *p }
}

/// Read two bytes at `pos` as big-endian u16.
/// Mirrors `SWAP16(RFIFOW(fd, pos))`.
#[inline]
unsafe fn rword_be(fd: c_int, pos: usize) -> u16 {
    let p = rust_session_rdata_ptr(fd, pos);
    if p.is_null() { return 0; }
    u16::from_be_bytes([*p, *p.add(1)])
}

/// Read four bytes at `pos` as big-endian u32.
/// Mirrors `SWAP32(RFIFOL(fd, pos))`.
#[inline]
unsafe fn rlong_be(fd: c_int, pos: usize) -> u32 {
    let p = rust_session_rdata_ptr(fd, pos);
    if p.is_null() { return 0; }
    u32::from_be_bytes([*p, *p.add(1), *p.add(2), *p.add(3)])
}

/// Raw pointer into recv buffer at `pos`.
/// Mirrors `RFIFOP(fd, pos)`.
#[inline]
unsafe fn rptr(fd: c_int, pos: usize) -> *const c_char {
    rust_session_rdata_ptr(fd, pos) as *const c_char
}

// ─── C extern declarations ────────────────────────────────────────────────────

extern "C" {
    // Crypto
    fn decrypt(fd: c_int);
    // Disconnect
    fn clif_handle_disconnect(sd: *mut c_void);
    fn clif_closeit(sd: *mut c_void);
    fn clif_print_disconnect(fd: c_int);
    // AFK / state
    fn clif_cancelafk(sd: *mut c_void);
    fn clif_mystaytus(sd: *mut c_void);
    fn clif_groupstatus(sd: *mut c_void);
    fn clif_refresh(sd: *mut c_void);
    fn clif_changestatus(sd: *mut c_void, status: u8);
    // Pre-login accept
    fn clif_accept2(fd: c_int, name: *const c_char, val: u8);
    // Movement
    fn clif_parsemap(sd: *mut c_void);
    fn clif_parsewalk(sd: *mut c_void);
    fn clif_parsewalkpong(sd: *mut c_void);
    fn clif_handle_missingobject(sd: *mut c_void);
    // Look / interact
    fn clif_parselookat(sd: *mut c_void);
    fn clif_parselookat_2(sd: *mut c_void);
    fn clif_open_sub(sd: *mut c_void);
    fn clif_handle_clickgetinfo(sd: *mut c_void);
    fn clif_parseviewchange(sd: *mut c_void);
    fn clif_parseside(sd: *mut c_void);
    fn clif_parseemotion(sd: *mut c_void);
    // Chat / social
    fn clif_parsesay(sd: *mut c_void);
    fn clif_parsewisp(sd: *mut c_void);
    fn clif_parseignore(sd: *mut c_void);
    fn clif_parsefriends(sd: *mut c_void, list: *const c_char, len: c_int);
    fn clif_user_list(sd: *mut c_void);
    fn clif_addgroup(sd: *mut c_void);
    // Items
    fn clif_parsegetitem(sd: *mut c_void);
    fn clif_parsedropitem(sd: *mut c_void);
    fn clif_parseeatitem(sd: *mut c_void);
    fn clif_parseuseitem(sd: *mut c_void);
    fn clif_parseunequip(sd: *mut c_void);
    fn clif_parsewield(sd: *mut c_void);
    fn clif_parsethrow(sd: *mut c_void);
    fn clif_throwconfirm(sd: *mut c_void);
    fn clif_dropgold(sd: *mut c_void, amount: u32);
    fn clif_postitem(sd: *mut c_void);
    fn clif_handitem(sd: *mut c_void);
    fn clif_handgold(sd: *mut c_void);
    // itemdb_thrownconfirm is a static inline → use the Rust backing fn
    fn rust_itemdb_thrownconfirm(id: u32) -> c_int;
    // Combat / magic
    fn clif_parsemagic(sd: *mut c_void);
    fn clif_parseattack(sd: *mut c_void);
    fn clif_sendminitext(sd: *mut c_void, msg: *const c_char);
    // NPC / menus
    fn clif_parsenpcdialog(sd: *mut c_void);
    fn clif_handle_menuinput(sd: *mut c_void);
    fn clif_paperpopupwrite_save(sd: *mut c_void);
    // Spells / position
    fn clif_parsechangespell(sd: *mut c_void);
    fn clif_parsechangepos(sd: *mut c_void);
    // Warp (rust_pc_warp — pc_warp is a static inline in pc.h)
    fn rust_pc_warp(sd: *mut c_void, m: c_int, x: c_int, y: c_int) -> c_int;
    // Profile
    fn clif_changeprofile(sd: *mut c_void);
    fn clif_sendprofile(sd: *mut c_void);
    // Boards / mail
    fn clif_handle_boards(sd: *mut c_void);
    fn clif_handle_powerboards(sd: *mut c_void);
    fn clif_parseparcel(sd: *mut c_void);
    fn clif_sendboard(sd: *mut c_void);
    // Ranking / towns
    fn clif_parseranking(sd: *mut c_void, fd: c_int);
    fn clif_sendRewardInfo(sd: *mut c_void, fd: c_int);
    fn clif_getReward(sd: *mut c_void, fd: c_int);
    fn clif_sendtowns(sd: *mut c_void);
    // Hunter / minimap
    fn clif_huntertoggle(sd: *mut c_void);
    fn clif_sendhunternote(sd: *mut c_void);
    fn clif_sendminimap(sd: *mut c_void);
    // Trade / meta / creation
    fn clif_parse_exchange(sd: *mut c_void);
    fn send_meta(sd: *mut c_void);
    fn send_metalist(sd: *mut c_void);
    fn createdb_start(sd: *mut c_void);
    // pc_atkspeed is a static inline → use the Rust backing fn
    fn rust_pc_atkspeed(id: c_int, v: c_int) -> c_int;
    // Timer insertion (from c_deps/timer.c via ffi::timer)
    fn timer_insert(
        tick: u32, interval: u32,
        func: Option<unsafe extern "C" fn(c_int, c_int) -> c_int>,
        id: c_int, data: c_int,
    ) -> c_int;

    // USER struct accessors (sl_compat.c)
    fn sl_pc_time(sd: *mut c_void) -> c_int;
    fn sl_pc_set_time(sd: *mut c_void, v: c_int);
    fn sl_pc_chat_timer(sd: *mut c_void) -> c_int;
    fn sl_pc_set_chat_timer(sd: *mut c_void, v: c_int);
    fn sl_pc_attacked(sd: *mut c_void) -> c_int;
    fn sl_pc_set_attacked(sd: *mut c_void, v: c_int);
    fn sl_pc_attack_speed(sd: *mut c_void) -> c_int;
    fn sl_pc_loaded(sd: *mut c_void) -> c_int;
    fn sl_pc_paralyzed(sd: *mut c_void) -> c_int;
    fn sl_pc_sleep(sd: *mut c_void) -> c_int;
    fn sl_pc_status_id(sd: *mut c_void) -> c_int;
    fn sl_pc_status_gm_level(sd: *mut c_void) -> c_int;
    fn sl_pc_status_mute(sd: *mut c_void) -> c_int;
    fn sl_pc_inventory_id(sd: *mut c_void, pos: c_int) -> u32;
    fn sl_map_spell(m: c_int) -> c_int;
    fn sl_pc_bl_m(sd: *mut c_void) -> c_int;
}

// ─── Dual-login check ─────────────────────────────────────────────────────────

/// Returns `true` if a duplicate session was detected (both connections closed).
///
/// Uses the session manager's fd map directly — no fixed-size buffer needed.
unsafe fn check_dual_login(fd: c_int, sd: *mut c_void) -> bool {
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
#[no_mangle]
pub unsafe extern "C" fn rust_clif_parse(fd: c_int) -> c_int {
    if rust_session_exists(fd) == 0 {
        return 0;
    }

    let sd = rust_session_get_data(fd);

    // EOF → disconnect and clean up
    if rust_session_get_eof(fd) != 0 {
        if !sd.is_null() {
            clif_handle_disconnect(sd);
            clif_closeit(sd);
        }
        clif_print_disconnect(fd);
        rust_session_set_eof(fd, 1);
        return 0;
    }

    // Validate packet header: must start with 0xAA
    let avail = rust_session_available(fd);
    if avail > 0 && rbyte(fd, 0) != 0xAA {
        rust_session_set_eof(fd, 13);
        return 0;
    }
    if avail < 3 { return 0; }

    let pkt_len = rword_be(fd, 1) as usize + 3;
    if avail < pkt_len { return 0; }

    // Pre-login: only opcode 0x10 (character accept) is allowed
    if sd.is_null() {
        if rbyte(fd, 3) == 0x10 {
            clif_accept2(fd, rptr(fd, 16), rbyte(fd, 15));
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
                            b"That doesn't work here.\0".as_ptr() as *const c_char,
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
            let pos = rbyte(fd, 6) as c_int;
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
            rust_pc_warp(sd, rword_be(fd, 5) as c_int, rword_be(fd, 7) as c_int, rword_be(fd, 9) as c_int);
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
            let friends_len = rword_be(fd, 1) as c_int - 5;
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
