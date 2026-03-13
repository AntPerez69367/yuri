#![allow(non_snake_case, dead_code, unused_variables)]
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


use crate::database::map_db::raw_map_ptr;
use crate::session::{SessionId, session_exists, session_get_data, session_get_eof, session_set_eof};
use crate::game::time_util::timer_insert;
use crate::game::pc::MapSessionData;

use crate::game::map_parse::packet::{
    rfifob, rfifow, rfifol, rfifop, rfiforest, rfifoskip, swap16, swap32, decrypt,
};

// Rust-native functions
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

// get_fd_max is defined in the binary; import via the session module.

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
use crate::database::item_db;
use crate::game::pc::rust_pc_atkspeed;

// pc_warp: actual fn takes i32, old extern had u16 — wrap with cast.
#[inline]
unsafe fn pc_warp(sd: *mut MapSessionData, map_id: u16, x: u16, y: u16) {
    let _ = crate::game::pc::rust_pc_warp(sd, map_id as i32, x as i32, y as i32);
}
// clif_debug: actual fn takes i32 for len, old extern had u16 — wrap with cast.
#[inline]
unsafe fn clif_debug_u16(buf: *const u8, len: u16) {
    clif_debug(buf, len as i32);
}
// createdb_start wrapper.
#[inline]
unsafe fn createdb_start(sd: *mut MapSessionData) {
    crate::game::client::handlers::createdb_start(sd);
}

/// Main packet dispatcher.
// clif_parse — ported to rust (src/game/map_parse/mod.rs)
pub async unsafe fn clif_parse(fd: SessionId) -> i32 {
    if fd.raw() < 0 { return 0; }
    if !session_exists(fd) { return 0; }

    let sd = session_get_data(fd);

    if session_get_eof(fd) != 0 {
        if !sd.is_null() {
            libc::printf(b"[map] [session_eof] name=%s\n\0".as_ptr() as *const i8,
                (*sd).status.name.as_ptr());
            clif_handle_disconnect(sd).await;
            clif_closeit(sd);
        }
        clif_print_disconnect(fd);
        // session_eof(fd) is a C inline that calls session_set_eof(fd, 1)
        session_set_eof(fd, 1);
        return 0;
    }

    if rfiforest(fd) > 0 && rfifob(fd, 0) != 0xAA {
        session_set_eof(fd, 13);
        return 0;
    }

    if rfiforest(fd) < 3 { return 0; }

    let len = swap16(rfifow(fd, 1)) as usize + 3;

    if rfiforest(fd) < len as i32 { return 0; }

    if sd.is_null() {
        match rfifob(fd, 3) {
            0x10 => {
                clif_accept2(fd, rfifop(fd, 16) as *mut i8, rfifob(fd, 15) as i32).await;
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
        if session_exists(SessionId::from_raw(i)) {
            let tsd = session_get_data(SessionId::from_raw(i));
            if !tsd.is_null() {
                if (*sd).status.id == (*tsd).status.id {
                    logincount += 1;
                }
                if logincount >= 2 {
                    libc::printf(
                        b"%s attempted dual login on IP:%s\n\0".as_ptr() as *const i8,
                        (*sd).status.name.as_ptr(),
                        (*sd).status.ipaddress.as_ptr(),
                    );
                    session_set_eof((*sd).fd, 1);
                    session_set_eof((*tsd).fd, 1);
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
                    if (*raw_map_ptr().add((*sd).bl.m as usize)).spell != 0 || (*sd).status.gm_level != 0 {
                        clif_parsemagic(&mut *sd);
                    } else {
                        clif_sendminitext(sd, b"That doesn't work here.\0".as_ptr() as *const i8);
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
                let delay = (((*sd).attack_speed as u32) * 1000) / 60;
                timer_insert(
                    delay,
                    delay,
                    Some(rust_pc_atkspeed as unsafe fn(i32, i32) -> i32),
                    (*sd).bl.id as i32,
                    0,
                );
                clif_parseattack(&mut *sd);
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
            if item_db::search((*sd).status.inventory[pos - 1].id).thrownconfirm == 1 {
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
                clif_changestatus(sd, rfifob((*sd).fd, 6) as i32);
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
                clif_mystaytus(sd).await;
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
            clif_handle_boards(sd).await;
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
            clif_handle_clickgetinfo(sd).await;
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
            // clif_parsewalkpong is in movement.rs
            crate::game::map_parse::movement::clif_parsewalkpong(sd);
        }
        0x77 => {
            clif_cancelafk(sd);
            let name_ptr = rfifop((*sd).fd, 5) as *const i8;
            let name_len = swap16(rfifow((*sd).fd, 1)) as i32 - 5;
            clif_parsefriends(sd, name_ptr, name_len).await;
        }
        0x7B => {
            libc::printf(b"request: %u\n\0".as_ptr() as *const i8,
                rfifob((*sd).fd, 5) as u32);
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
                5 => { clif_sendRewardInfo(sd, fd).await; }
                6 => { clif_getReward(sd, fd).await; }
                _ => { rust_clif_parseranking(sd, fd).await; }
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
            clif_huntertoggle(sd).await;
        }
        0x85 => {
            clif_sendhunternote(sd).await;
            clif_cancelafk(sd);
        }
        _ => {
            libc::printf(
                b"[Map] Unknown Packet ID: %02X\nPacket content:\n\0".as_ptr() as *const i8,
                rfifob((*sd).fd, 3) as u32,
            );
            clif_debug_u16(rfifop((*sd).fd, 0), swap16(rfifow((*sd).fd, 1)));
        }
    }

    rfifoskip(fd, len);
    0
}
