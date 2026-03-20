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

pub mod chat;
pub mod combat;
pub mod dialogs;
pub mod events;
pub mod groups;
pub mod items;
pub mod movement;
pub mod packet;
pub mod player_state;
pub mod trading;
pub mod visual;

// ─── clif_parse — main packet dispatcher ─────────────────────────────────────

use crate::common::traits::LegacyEntity;
use crate::database::map_db::raw_map_ptr;
use crate::game::pc::MapSessionData;
use crate::game::player::entity::PlayerEntity;
use crate::game::time_util::timer_insert;
use crate::session::{
    session_exists, session_get_data, session_get_eof, session_set_eof, SessionId,
};

use crate::game::map_parse::packet::{
    decrypt, rfifob, rfifol, rfifop, rfiforest, rfifoskip, rfifow, swap16, swap32,
};

// Rust-native functions
use crate::game::map_parse::chat::{
    clif_parseignore, clif_parsesay, clif_parsewisp, clif_sendminitext,
};
use crate::game::map_parse::combat::{clif_parseattack, clif_parsemagic};
use crate::game::map_parse::dialogs::{
    clif_closeit, clif_handle_clickgetinfo, clif_parsenpcdialog,
};
use crate::game::map_parse::events::{clif_getReward, clif_parseranking, clif_sendRewardInfo};
use crate::game::map_parse::groups::{
    clif_addgroup, clif_groupstatus, clif_huntertoggle, clif_parseparcel, clif_sendhunternote,
};
use crate::game::map_parse::items::{
    clif_dropgold, clif_open_sub, clif_parsechangespell, clif_parseeatitem, clif_parsegetitem,
    clif_parsethrow, clif_parseunequip, clif_parseuseitem, clif_parsewield, clif_throwconfirm,
};
use crate::game::map_parse::movement::{clif_parsemap, clif_parsewalk};
use crate::game::map_parse::player_state::{clif_mystatus, clif_refresh};
use crate::game::map_parse::trading::{clif_handgold, clif_handitem, clif_parse_exchange};

// get_fd_max is defined in the binary; import via the session module.

use crate::database::item_db;
use crate::game::client::handlers::{
    clif_accept2, clif_changestatus, clif_handle_boards, clif_handle_disconnect,
    clif_handle_menuinput, clif_handle_missingobject, clif_handle_powerboards, clif_parsechangepos,
    clif_parsedropitem, clif_parsefriends, clif_postitem,
};
use crate::game::client::visual::clif_sendprofile;
use crate::game::client::visual::{
    clif_cancelafk, clif_changeprofile, clif_debug, clif_paperpopupwrite_save,
    clif_print_disconnect, clif_sendboard, clif_user_list,
};
use crate::game::map_parse::chat::clif_parseemotion;
use crate::game::map_parse::dialogs::clif_sendtowns;
use crate::game::map_parse::movement::{
    clif_parselookat, clif_parselookat_2, clif_parseside, clif_parseviewchange,
};
use crate::game::map_parse::player_state::clif_sendminimap;
use crate::game::pc::pc_atkspeed;
use crate::network::crypt::{send_meta, send_metalist};

// pc_warp: actual fn takes i32, old extern had u16 — wrap with cast.
// pc_warp in spatial.rs still takes *mut MapSessionData (not yet migrated).
#[inline]
unsafe fn pc_warp(pe: &PlayerEntity, map_id: u16, x: u16, y: u16) {
    let sd_ptr = &mut *pe.write() as *mut MapSessionData;
    let _ = crate::game::pc::pc_warp(sd_ptr, map_id as i32, x as i32, y as i32);
}
// clif_debug: actual fn takes i32 for len, old extern had u16 — wrap with cast.
#[inline]
unsafe fn clif_debug_u16(buf: *const u8, len: u16) {
    clif_debug(buf, len as i32);
}
// createdb_start wrapper.
#[inline]
unsafe fn createdb_start(pe: &PlayerEntity) {
    crate::game::client::handlers::createdb_start(pe);
}

/// Main packet dispatcher.
// clif_parse — ported to rust (src/game/map_parse/mod.rs)
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn clif_parse(fd: SessionId) -> i32 {
    if fd.raw() < 0 {
        return 0;
    }
    if !session_exists(fd) {
        return 0;
    }

    let sd = session_get_data(fd);

    if session_get_eof(fd) != 0 {
        if let Some(pe) = sd.as_deref() {
            libc::printf(c"[map] [session_eof] name=%s\n".as_ptr(), pe.name.as_ptr());
            clif_handle_disconnect(pe).await;
            clif_closeit(pe);
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

    if rfiforest(fd) < 3 {
        return 0;
    }

    let len = swap16(rfifow(fd, 1)) as usize + 3;

    if rfiforest(fd) < len as i32 {
        return 0;
    }

    if sd.is_none() {
        if rfifob(fd, 3) == 0x10 {
            clif_accept2(fd, rfifop(fd, 16) as *mut i8, rfifob(fd, 15) as i32).await;
        }
        rfifoskip(fd, len);
        return 0;
    }

    // sd is Some past here
    let pe = sd.as_deref().unwrap();
    {
        let mut legacy = pe.write();
        legacy.PrevSeed = rfifob(fd, 4);
        legacy.NextSeed = rfifob(fd, 4).wrapping_add(1);
    }

    // Dual login check
    let mut logincount = 0i32;
    for i in 0..crate::session::get_fd_max() {
        if session_exists(SessionId::from_raw(i)) {
            let tsd = session_get_data(SessionId::from_raw(i));
            if let Some(tpe) = tsd.as_deref() {
                if pe.read().player.identity.id == tpe.read().player.identity.id {
                    logincount += 1;
                }
                if logincount >= 2 {
                    libc::printf(
                        c"%s attempted dual login on IP:%s\n".as_ptr(),
                        pe.read().player.identity.name.as_ptr(),
                        pe.read().player.identity.ipaddress.as_ptr(),
                    );
                    session_set_eof(pe.fd, 1);
                    session_set_eof(tpe.fd, 1);
                    break;
                }
            }
        }
    }

    // Incoming packet decryption
    decrypt(fd);

    match rfifob(fd, 3) {
        0x05 => {
            clif_parsemap(pe);
        }
        0x06 => {
            clif_cancelafk(pe);
            clif_parsewalk(pe);
        }
        0x07 => {
            clif_cancelafk(pe);
            pe.write().time += 1;
            if pe.read().time < 4 {
                clif_parsegetitem(pe);
            }
        }
        0x08 => {
            clif_cancelafk(pe);
            clif_parsedropitem(pe);
        }
        0x09 => {
            clif_cancelafk(pe);
            clif_parselookat_2(pe);
        }
        0x0A => {
            clif_cancelafk(pe);
            clif_parselookat(pe);
        }
        0x0B => {
            clif_cancelafk(pe);
            clif_closeit(pe);
        }
        0x0C => {
            clif_handle_missingobject(pe);
        }
        0x0D => {
            clif_parseignore(pe);
        }
        0x0E => {
            clif_cancelafk(pe);
            if pe.read().player.identity.gm_level != 0 {
                clif_parsesay(pe);
            } else {
                pe.write().chat_timer += 1;
                if pe.read().chat_timer < 2 && pe.read().player.social.mute == 0 {
                    clif_parsesay(pe);
                }
            }
        }
        0x0F => {
            clif_cancelafk(pe);
            pe.write().time += 1;
            if pe.read().paralyzed == 0 && pe.read().sleep == 1.0f32 && pe.read().time < 4 {
                if (*raw_map_ptr().add(pe.read().m as usize)).spell != 0
                    || pe.read().player.identity.gm_level != 0
                {
                    clif_parsemagic(&mut pe.write());
                } else {
                    clif_sendminitext(pe, c"That doesn't work here.".as_ptr());
                }
            }
        }
        0x11 => {
            clif_cancelafk(pe);
            clif_parseside(pe);
        }
        0x12 => {
            clif_cancelafk(pe);
            clif_parsewield(pe);
        }
        0x13 => {
            clif_cancelafk(pe);
            pe.write().time += 1;
            if pe.read().attacked != 1 && pe.read().attack_speed > 0 {
                let attack_speed = pe.read().attack_speed;
                let id = pe.id;
                pe.write().attacked = 1;
                let delay = ((attack_speed as u32) * 1000) / 60;
                timer_insert(
                    delay,
                    delay,
                    Some(pc_atkspeed as unsafe fn(i32, i32) -> i32),
                    id as i32,
                    0,
                );
                clif_parseattack(&mut pe.write());
            }
        }
        0x17 => {
            clif_cancelafk(pe);
            let pe_fd = pe.fd;
            let pos = rfifob(pe_fd, 6) as usize;
            let confirm = rfifob(pe_fd, 5);
            // pos is 1-based; inventory is 0-based. Guard against underflow and OOB.
            if pos == 0 || pos > pe.read().player.inventory.inventory.len() {
                rfifoskip(fd, len);
                return 0;
            }
            let item_id = pe.read().player.inventory.inventory[pos - 1].id;
            if item_db::search(item_id).thrownconfirm == 1 {
                if confirm == 1 {
                    clif_parsethrow(pe);
                } else {
                    clif_throwconfirm(pe);
                }
            } else {
                clif_parsethrow(pe);
            }
        }
        0x18 => {
            clif_cancelafk(pe);
            clif_user_list(pe);
        }
        0x19 => {
            clif_cancelafk(pe);
            clif_parsewisp(pe);
        }
        0x1A => {
            clif_cancelafk(pe);
            clif_parseeatitem(pe);
        }
        0x1B => {
            if pe.read().loaded != 0 {
                let pe_fd = pe.fd;
                clif_changestatus(pe, rfifob(pe_fd, 6) as i32);
            }
        }
        0x1C => {
            clif_cancelafk(pe);
            clif_parseuseitem(pe);
        }
        0x1D => {
            clif_cancelafk(pe);
            pe.write().time += 1;
            if pe.read().time < 4 {
                clif_parseemotion(pe);
            }
        }
        0x1E => {
            clif_cancelafk(pe);
            pe.write().time += 1;
            if pe.read().time < 4 {
                clif_parsewield(pe);
            }
        }
        0x1F => {
            clif_cancelafk(pe);
            if pe.read().time < 4 {
                clif_parseunequip(pe);
            }
        }
        0x20 => {
            clif_cancelafk(pe);
            clif_open_sub(pe);
        }
        0x23 => {
            clif_paperpopupwrite_save(pe);
        }
        0x24 => {
            clif_cancelafk(pe);
            let pe_fd = pe.fd;
            clif_dropgold(pe, swap32(rfifol(pe_fd, 5)));
        }
        0x27 => {
            clif_cancelafk(pe);
            // reserved for quest tab — no-op
        }
        0x29 => {
            clif_cancelafk(pe);
            clif_handitem(pe);
        }
        0x2A => {
            clif_cancelafk(pe);
            clif_handgold(pe);
        }
        0x2D => {
            clif_cancelafk(pe);
            let pe_fd = pe.fd;
            if rfifob(pe_fd, 5) == 0 {
                clif_mystatus(pe);
            } else {
                clif_groupstatus(pe);
            }
        }
        0x2E => {
            clif_cancelafk(pe);
            clif_addgroup(pe);
        }
        0x30 => {
            clif_cancelafk(pe);
            let pe_fd = pe.fd;
            if rfifob(pe_fd, 5) == 1 {
                clif_parsechangespell(pe);
            } else {
                clif_parsechangepos(pe);
            }
        }
        0x32 => {
            clif_cancelafk(pe);
            clif_parsewalk(pe);
        }
        // NOTE: 0x34 falls through to 0x38 in the original C (missing break).
        // Preserved here: call both clif_postitem AND clif_refresh for 0x34.
        0x34 => {
            clif_cancelafk(pe);
            clif_postitem(pe);
            clif_refresh(pe);
        }
        0x38 => {
            clif_cancelafk(pe);
            clif_refresh(pe);
        }
        0x39 => {
            clif_cancelafk(pe);
            clif_handle_menuinput(pe);
        }
        0x3A => {
            clif_cancelafk(pe);
            clif_parsenpcdialog(pe);
        }
        0x3B => {
            clif_cancelafk(pe);
            clif_handle_boards(pe).await;
        }
        0x3F => {
            let pe_fd = pe.fd;
            pc_warp(
                pe,
                swap16(rfifow(pe_fd, 5)),
                swap16(rfifow(pe_fd, 7)),
                swap16(rfifow(pe_fd, 9)),
            );
        }
        0x41 => {
            clif_cancelafk(pe);
            clif_parseparcel(pe);
        }
        0x42 => {
            // client crash debug — no-op
        }
        0x43 => {
            clif_cancelafk(pe);
            clif_handle_clickgetinfo(pe).await;
        }
        0x4A => {
            clif_cancelafk(pe);
            clif_parse_exchange(pe);
        }
        0x4C => {
            clif_cancelafk(pe);
            clif_handle_powerboards(pe);
        }
        0x4F => {
            clif_cancelafk(pe);
            clif_changeprofile(pe);
        }
        0x60 => {
            // PING — no-op
        }
        0x66 => {
            clif_cancelafk(pe);
            clif_sendtowns(pe);
        }
        0x69 => {
            // obstruction — disabled
        }
        0x6B => {
            clif_cancelafk(pe);
            createdb_start(pe);
        }
        0x73 => {
            clif_cancelafk(pe);
            let pe_fd = pe.fd;
            if rfifob(pe_fd, 5) == 0x04 {
                clif_sendprofile(pe);
            }
            if rfifob(pe_fd, 5) == 0x00 {
                clif_sendboard(pe);
            }
        }
        0x75 => {
            // clif_parsewalkpong is in movement.rs
            crate::game::map_parse::movement::clif_parsewalkpong(pe);
        }
        0x77 => {
            clif_cancelafk(pe);
            let pe_fd = pe.fd;
            let name_ptr = rfifop(pe_fd, 5) as *const i8;
            let name_len = swap16(rfifow(pe_fd, 1)) as i32 - 5;
            clif_parsefriends(pe, name_ptr, name_len).await;
        }
        0x7B => {
            libc::printf(c"request: %u\n".as_ptr(), rfifob(pe.fd, 5) as u32);
            match rfifob(pe.fd, 5) {
                0 => {
                    send_meta(&mut *pe.write() as *mut MapSessionData);
                }
                1 => {
                    send_metalist(&mut *pe.write() as *mut MapSessionData);
                }
                _ => {}
            }
        }
        0x7C => {
            clif_cancelafk(pe);
            clif_sendminimap(pe);
        }
        0x7D => {
            clif_cancelafk(pe);
            match rfifob(fd, 5) {
                5 => {
                    clif_sendRewardInfo(pe, fd).await;
                }
                6 => {
                    clif_getReward(pe, fd).await;
                }
                _ => {
                    clif_parseranking(pe, fd).await;
                }
            }
        }
        0x82 => {
            clif_cancelafk(pe);
            clif_parseviewchange(pe);
        }
        0x83 => {
            // screenshots — no-op
        }
        0x84 => {
            clif_cancelafk(pe);
            clif_huntertoggle(pe).await;
        }
        0x85 => {
            clif_sendhunternote(pe).await;
            clif_cancelafk(pe);
        }
        _ => {
            let pe_fd = pe.fd;
            libc::printf(
                c"[Map] Unknown Packet ID: %02X\nPacket content:\n".as_ptr(),
                rfifob(pe_fd, 3) as u32,
            );
            clif_debug_u16(rfifop(pe_fd, 0), swap16(rfifow(pe_fd, 1)));
        }
    }

    rfifoskip(fd, len);
    0
}
