#![allow(non_snake_case, dead_code, unused_variables)]
//! Port of `c_src/map_parse.c` — client packet handlers and send helpers.
//!
//! Submodule layout:
//!   packet       — FIFO helpers + clif_send routing layer
//!   player_state — sendstatus, sendxy, sendid, sendmapinfo (login sequence)
//!   visual       — object look/spawn system (clif_*look*, clif_spawn)
//!   movement     — parsewalk, chararea, sendmapdata
//!   combat       — parseattack, magic, dura system
//!   chat         — parsesay, parsewisp, broadcast
//!   dialogs      — scriptmes, scriptmenu, buydialog, selldialog, input
//!   items        — parseuseitem, parseunequip, parsewield, throwitem
//!   trading      — clif_exchange_* family
//!   groups       — party/group status, add, update, leave
//!   events       — rankings, reward parcels

pub mod packet;
pub mod player_state;
pub mod visual;
pub mod movement;
pub mod combat;
pub mod chat;
pub mod dialogs;
pub mod items;
pub mod trading;
pub mod groups;
pub mod events;

// ─── clif_parse — main packet dispatcher ─────────────────────────────────────

use std::ffi::{c_char, c_int, c_uint};

use crate::database::map_db::map;
use crate::session::{rust_session_exists, rust_session_get_data, rust_session_get_eof, rust_session_set_eof};
use crate::timer::timer_insert;
use crate::game::pc::MapSessionData;

use crate::game::map_parse::packet::{
    rfifob, rfifow, rfifol, rfifop, rfiforest, rfifoskip, swap16, swap32, decrypt,
};

// Rust-native functions (all pub unsafe extern "C")
use crate::game::map_parse::movement::{clif_parsewalk, clif_parsemap};
use crate::game::map_parse::combat::{clif_parseattack, clif_parsemagic};
use crate::game::map_parse::chat::{clif_parsesay, clif_parsewisp, clif_parseignore, clif_sendminitext};
use crate::game::map_parse::items::{
    clif_parsegetitem, clif_parsewield, clif_parseunequip, clif_parseuseitem,
    clif_parseeatitem, clif_parsethrow, clif_throwconfirm, clif_dropgold, clif_open_sub,
    clif_parsechangespell,
};
use crate::game::map_parse::trading::{clif_handitem, clif_handgold, clif_parse_exchange};
use crate::game::map_parse::groups::{clif_groupstatus, clif_addgroup, clif_parseparcel, clif_huntertoggle, clif_sendhunternote};
use crate::game::map_parse::events::{clif_sendRewardInfo, clif_getReward, clif_parseranking as rust_clif_parseranking};
use crate::game::map_parse::player_state::{clif_mystaytus, clif_refresh};
use crate::game::map_parse::dialogs::{clif_parsenpcdialog, clif_handle_clickgetinfo, clif_closeit};

// crate::session::get_fd_max() is defined in the binary — cannot import from library, must keep extern "C".
// ─── Direct Rust imports (replacing extern "C" declarations) ─────────────────

use crate::game::client::visual::{
    clif_cancelafk, clif_print_disconnect, clif_user_list, clif_debug,
    clif_paperpopupwrite_save, clif_changeprofile, clif_sendboard,
};
use crate::game::client::handlers::{
    clif_handle_disconnect, clif_handle_missingobject, clif_handle_powerboards,
    clif_parsedropitem, clif_postitem, clif_handle_boards, clif_handle_menuinput,
    clif_parsechangepos, clif_parsefriends, clif_changestatus, clif_accept2,
};
use crate::game::map_parse::movement::{
    clif_parseside, clif_parselookat, clif_parselookat_2, clif_parseviewchange,
};
use crate::game::map_parse::chat::clif_parseemotion;
use crate::game::map_parse::dialogs::clif_sendtowns;
use crate::game::client::visual::clif_sendprofile;
use crate::game::map_parse::player_state::clif_sendminimap;
use crate::network::crypt::{send_meta, send_metalist};
use crate::database::item_db::rust_itemdb_thrownconfirm as itemdb_thrownconfirm;
use crate::game::pc::rust_pc_atkspeed;

// pc_warp: actual fn takes c_int, old extern had u16 — wrap with cast.
#[inline]
unsafe fn pc_warp(sd: *mut MapSessionData, map_id: u16, x: u16, y: u16) {
    crate::game::pc::rust_pc_warp(sd, map_id as c_int, x as c_int, y as c_int);
}
// clif_debug: actual fn takes c_int for len, old extern had u16 — wrap with cast.
#[inline]
unsafe fn clif_debug_u16(buf: *const u8, len: u16) {
    clif_debug(buf, len as c_int);
}
// createdb_start takes *mut c_void in handlers.rs.
#[inline]
unsafe fn createdb_start(sd: *mut MapSessionData) {
    crate::game::client::handlers::createdb_start(sd as *mut std::ffi::c_void);
}

/// Main packet dispatcher. Mirrors `int clif_parse(int fd)` from `c_src/map_parse.c`.
// clif_parse — ported to rust (src/game/map_parse/mod.rs)
pub unsafe fn clif_parse(fd: c_int) -> c_int {
    if fd < 0 { return 0; }
    if rust_session_exists(fd) == 0 { return 0; }

    let sd = rust_session_get_data(fd) as *mut MapSessionData;

    if rust_session_get_eof(fd) != 0 {
        if !sd.is_null() {
            libc::printf(b"[map] [session_eof] name=%s\n\0".as_ptr() as *const c_char,
                (*sd).status.name.as_ptr());
            clif_handle_disconnect(sd);
            clif_closeit(sd);
        }
        clif_print_disconnect(fd);
        // session_eof(fd) is a C inline that calls rust_session_set_eof(fd, 1)
        rust_session_set_eof(fd, 1);
        return 0;
    }

    if rfiforest(fd) > 0 && rfifob(fd, 0) != 0xAA {
        rust_session_set_eof(fd, 13);
        return 0;
    }

    if rfiforest(fd) < 3 { return 0; }

    let len = swap16(rfifow(fd, 1)) as usize + 3;

    if rfiforest(fd) < len as c_int { return 0; }

    if sd.is_null() {
        match rfifob(fd, 3) {
            0x10 => {
                clif_accept2(fd, rfifop(fd, 16) as *mut c_char, rfifob(fd, 15) as c_int);
            }
            _ => {}
        }
        rfifoskip(fd, len);
        return 0;
    }

    // sd is non-null past here
    let current_seed = rfifob(fd, 4);
    (*sd).PrevSeed = current_seed;
    (*sd).NextSeed = current_seed.wrapping_add(1);

    // Dual login check
    let mut logincount = 0i32;
    for i in 0..crate::session::get_fd_max() {
        if rust_session_exists(i) != 0 {
            let tsd = rust_session_get_data(i) as *mut MapSessionData;
            if !tsd.is_null() {
                if (*sd).status.id == (*tsd).status.id {
                    logincount += 1;
                }
                if logincount >= 2 {
                    libc::printf(
                        b"%s attempted dual login on IP:%s\n\0".as_ptr() as *const c_char,
                        (*sd).status.name.as_ptr(),
                        (*sd).status.ipaddress.as_ptr(),
                    );
                    rust_session_set_eof((*sd).fd, 1);
                    rust_session_set_eof((*tsd).fd, 1);
                    break;
                }
            }
        }
    }

    // Incoming packet decryption
    decrypt(fd);

    match rfifob(fd, 3) {
        0x05 => {
            clif_parsemap(sd);
        }
        0x06 => {
            clif_cancelafk(sd);
            clif_parsewalk(sd);
        }
        0x07 => {
            clif_cancelafk(sd);
            (*sd).time += 1;
            if (*sd).time < 4 {
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
            if (*sd).status.gm_level != 0 {
                clif_parsesay(sd);
            } else {
                (*sd).chat_timer += 1;
                if (*sd).chat_timer < 2 && (*sd).status.mute == 0 {
                    clif_parsesay(sd);
                }
            }
        }
        0x0F => {
            clif_cancelafk(sd);
            (*sd).time += 1;
            if (*sd).paralyzed == 0 && (*sd).sleep == 1.0f32 {
                if (*sd).time < 4 {
                    if (*map.add((*sd).bl.m as usize)).spell != 0 || (*sd).status.gm_level != 0 {
                        clif_parsemagic(sd);
                    } else {
                        clif_sendminitext(sd, b"That doesn't work here.\0".as_ptr() as *const c_char);
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
            (*sd).time += 1;
            if (*sd).attacked != 1 && (*sd).attack_speed > 0 {
                (*sd).attacked = 1;
                let delay = (((*sd).attack_speed as c_uint) * 1000) / 60;
                timer_insert(
                    delay,
                    delay,
                    Some(rust_pc_atkspeed as unsafe fn(i32, i32) -> i32),
                    (*sd).status.id as c_int,
                    0,
                );
                clif_parseattack(sd);
            }
        }
        0x17 => {
            clif_cancelafk(sd);
            let pos = rfifob((*sd).fd, 6) as usize;
            let confirm = rfifob((*sd).fd, 5);
            // pos is 1-based; inventory is 0-based. Guard against underflow and OOB.
            if pos == 0 || pos - 1 >= (*sd).status.inventory.len() {
                rfifoskip(fd, len);
                return 0;
            }
            if itemdb_thrownconfirm((*sd).status.inventory[pos - 1].id) == 1 {
                if confirm == 1 {
                    clif_parsethrow(sd);
                } else {
                    clif_throwconfirm(sd);
                }
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
            if (*sd).loaded != 0 {
                clif_changestatus(sd, rfifob((*sd).fd, 6) as c_int);
            }
        }
        0x1C => {
            clif_cancelafk(sd);
            clif_parseuseitem(sd);
        }
        0x1D => {
            clif_cancelafk(sd);
            (*sd).time += 1;
            if (*sd).time < 4 {
                clif_parseemotion(sd);
            }
        }
        0x1E => {
            clif_cancelafk(sd);
            (*sd).time += 1;
            if (*sd).time < 4 {
                clif_parsewield(sd);
            }
        }
        0x1F => {
            clif_cancelafk(sd);
            if (*sd).time < 4 {
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
            clif_dropgold(sd, swap32(rfifol((*sd).fd, 5)));
        }
        0x27 => {
            clif_cancelafk(sd);
            // reserved for quest tab — no-op
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
            if rfifob((*sd).fd, 5) == 0 {
                clif_mystaytus(sd);
            } else {
                clif_groupstatus(sd);
            }
        }
        0x2E => {
            clif_cancelafk(sd);
            clif_addgroup(sd);
        }
        0x30 => {
            clif_cancelafk(sd);
            if rfifob((*sd).fd, 5) == 1 {
                clif_parsechangespell(sd);
            } else {
                clif_parsechangepos(sd);
            }
        }
        0x32 => {
            clif_cancelafk(sd);
            clif_parsewalk(sd);
        }
        // NOTE: 0x34 falls through to 0x38 in the original C (missing break).
        // Preserved here: call both clif_postitem AND clif_refresh for 0x34.
        0x34 => {
            clif_cancelafk(sd);
            clif_postitem(sd);
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
            pc_warp(sd,
                swap16(rfifow((*sd).fd, 5)),
                swap16(rfifow((*sd).fd, 7)),
                swap16(rfifow((*sd).fd, 9)),
            );
        }
        0x41 => {
            clif_cancelafk(sd);
            clif_parseparcel(sd);
        }
        0x42 => {
            // client crash debug — no-op
        }
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
        0x60 => {
            // PING — no-op
        }
        0x66 => {
            clif_cancelafk(sd);
            clif_sendtowns(sd);
        }
        0x69 => {
            // obstruction — disabled
        }
        0x6B => {
            clif_cancelafk(sd);
            createdb_start(sd);
        }
        0x73 => {
            clif_cancelafk(sd);
            if rfifob((*sd).fd, 5) == 0x04 {
                clif_sendprofile(sd);
            }
            if rfifob((*sd).fd, 5) == 0x00 {
                clif_sendboard(sd);
            }
        }
        0x75 => {
            // clif_parsewalkpong is in movement.rs but called via C ABI
            crate::game::map_parse::movement::clif_parsewalkpong(sd);
        }
        0x77 => {
            clif_cancelafk(sd);
            let name_ptr = rfifop((*sd).fd, 5) as *const c_char;
            let name_len = swap16(rfifow((*sd).fd, 1)) as c_int - 5;
            clif_parsefriends(sd, name_ptr, name_len);
        }
        0x7B => {
            libc::printf(b"request: %u\n\0".as_ptr() as *const c_char,
                rfifob((*sd).fd, 5) as c_uint);
            match rfifob((*sd).fd, 5) {
                0 => { send_meta(sd); }
                1 => { send_metalist(sd); }
                _ => {}
            }
        }
        0x7C => {
            clif_cancelafk(sd);
            clif_sendminimap(sd);
        }
        0x7D => {
            clif_cancelafk(sd);
            match rfifob(fd, 5) {
                5 => { clif_sendRewardInfo(sd, fd); }
                6 => { clif_getReward(sd, fd); }
                _ => { rust_clif_parseranking(sd, fd); }
            }
        }
        0x82 => {
            clif_cancelafk(sd);
            clif_parseviewchange(sd);
        }
        0x83 => {
            // screenshots — no-op
        }
        0x84 => {
            clif_cancelafk(sd);
            clif_huntertoggle(sd);
        }
        0x85 => {
            clif_sendhunternote(sd);
            clif_cancelafk(sd);
        }
        _ => {
            libc::printf(
                b"[Map] Unknown Packet ID: %02X\nPacket content:\n\0".as_ptr() as *const c_char,
                rfifob((*sd).fd, 3) as c_uint,
            );
            clif_debug_u16(rfifop((*sd).fd, 0), swap16(rfifow((*sd).fd, 1)));
        }
    }

    rfifoskip(fd, len);
    0
}
