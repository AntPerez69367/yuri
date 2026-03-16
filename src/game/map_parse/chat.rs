//! Covers broadcast, whisper, say, ignore list, and NPC speech callbacks.

use crate::database::map_db::BlockList;
use crate::database::map_db::raw_map_ptr;
use crate::session::{
    SessionId, session_exists, session_get_data,
};
use crate::game::npc::NpcData;
use crate::game::pc::{
    MapSessionData, SdIgnoreList,
    FLAG_ADVICE, FLAG_SHOUT, FLAG_WHISPER,
    OPT_FLAG_STEALTH, U_FLAG_SILENCED,
    MAP_WHISPFAIL,
    groups,
    map_msg,
};
use crate::servers::char::charstatus::MAX_SPELLS;

use super::packet::{
    encrypt, wfifob, wfifohead, wfifol, wfifop, wfifoset, wfifow, wfifoheader,
    clif_send,
    SAMEMAP, SAMEAREA,
};
use crate::game::block::AreaType;
use crate::game::block_grid;

/// Check that the session exists; if not, return false.
#[inline]
unsafe fn session_alive(fd: SessionId) -> bool {
    if session_exists(fd) { return true; }
    false
}

use crate::game::map_server::map_name2sd;
use crate::game::map_parse::combat::clif_sendaction;
use crate::database::class_db::{name as classdb_name, chat as classdb_chat};
use crate::game::gm_command::is_command;
use crate::database::magic_db;
use crate::game::client::handlers::clif_Hacker;

// Alias for the async coroutine freeer (returns () in Rust, was i32 in C -- return unused).
#[inline]
unsafe fn sl_async_freeco(sd: *mut MapSessionData) {
    crate::game::scripting::sl_async_freeco(sd);
}

/// Dispatch a Lua event with a single block_list argument.
#[allow(dead_code)]
unsafe fn sl_doscript_simple(root: *const i8, method: *const i8, bl: *mut crate::database::map_db::BlockList) -> i32 {
    crate::game::scripting::doscript_blargs(root, method, &[bl as *mut _])
}

/// Dispatch a Lua event with two block_list arguments.
#[allow(dead_code)]
unsafe fn sl_doscript_2(root: *const i8, method: *const i8, bl1: *mut crate::database::map_db::BlockList, bl2: *mut crate::database::map_db::BlockList) -> i32 {
    crate::game::scripting::doscript_blargs(root, method, &[bl1 as *mut _, bl2 as *mut _])
}

/// Coroutine dispatch with two block_list arguments (for yielding handlers like onSayClick).
#[allow(dead_code)]
unsafe fn sl_doscript_coro_2(root: *const i8, method: *const i8, bl1: *mut crate::database::map_db::BlockList, bl2: *mut crate::database::map_db::BlockList) -> i32 {
    crate::game::scripting::doscript_coro(root, method, &[bl1 as *mut _, bl2 as *mut _])
}


// NPC subtype constant (from map_server.h)
const SCRIPT: u8 = 0;

// ─── inline helper: map_isloaded ─────────────────────────────────────────────

#[inline]
unsafe fn map_isloaded(m: usize) -> bool {
    !(*raw_map_ptr().add(m)).registry.is_null()
}

// ─── inline helper: write big-endian u16 into a local byte buffer ─────────────

#[inline]
fn buf_put_be16(buf: &mut [u8], pos: usize, val: u16) {
    let bytes = val.to_be_bytes();
    buf[pos]     = bytes[0];
    buf[pos + 1] = bytes[1];
}

#[inline]
fn buf_put_be32(buf: &mut [u8], pos: usize, val: u32) {
    let bytes = val.to_be_bytes();
    buf[pos]     = bytes[0];
    buf[pos + 1] = bytes[1];
    buf[pos + 2] = bytes[2];
    buf[pos + 3] = bytes[3];
}

// ─── clif_sendguidespecific ───────────────────────────────────────────────────

/// Send a guide popup packet to a single player.
///
pub unsafe fn clif_sendguidespecific(sd: *mut MapSessionData, guide: i32) -> i32 {
    if !session_alive((*sd).fd) { return 0; }

    wfifohead((*sd).fd, 10);
    wfifob((*sd).fd, 0, 0xAA);
    wfifow((*sd).fd, 1, (0x07u16).to_be());
    wfifob((*sd).fd, 3, 0x12);
    wfifob((*sd).fd, 4, 0x03);
    wfifob((*sd).fd, 5, 0x00);
    wfifob((*sd).fd, 6, 0x02);
    wfifow((*sd).fd, 7, (guide as u16).to_le());
    wfifob((*sd).fd, 8, 0);
    wfifob((*sd).fd, 9, 1);
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_broadcast_sub ───────────────────────────────────────────────────────

/// foreachinarea callback: send a global broadcast to one player.
///
pub unsafe fn clif_broadcast_sub_inner(bl: *const BlockList, msg: *const i8) -> i32 {
    let sd = bl as *const MapSessionData;
    if sd.is_null() { return 0; }

    if !session_alive((*sd).fd) { return 0; }

    let flag = (*sd).status.setting_flags & FLAG_SHOUT as u16;
    if flag == 0 { return 0; }

    let len = libc_strlen(msg);

    wfifohead((*sd).fd, len + 8);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 3, 0x0A);
    wfifob((*sd).fd, 4, 0x03);
    wfifob((*sd).fd, 5, 0x05);
    wfifow((*sd).fd, 6, (len as u16).to_be());
    // copy msg bytes into wbuffer at offset 8
    let dst = wfifop((*sd).fd, 8);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(msg as *const u8, dst, len);
    }
    wfifow((*sd).fd, 1, ((len + 5) as u16).to_be());
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_gmbroadcast_sub ─────────────────────────────────────────────────────

/// foreachinarea callback: send a GM broadcast to one player.
///
pub unsafe fn clif_gmbroadcast_sub_inner(bl: *const BlockList, msg: *const i8) -> i32 {
    let sd = bl as *const MapSessionData;
    if sd.is_null() { return 0; }

    if !session_alive((*sd).fd) { return 0; }

    let len = libc_strlen(msg);

    wfifohead((*sd).fd, len + 8);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 3, 0x0A);
    wfifob((*sd).fd, 4, 0x03);
    wfifob((*sd).fd, 5, 0x05);
    wfifow((*sd).fd, 6, (len as u16).to_be());
    let dst = wfifop((*sd).fd, 8);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(msg as *const u8, dst, len);
    }
    wfifow((*sd).fd, 1, ((len + 5) as u16).to_be());
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_broadcasttogm_sub ───────────────────────────────────────────────────

/// foreachinarea callback: send a broadcast only if the player is a GM.
///
pub unsafe fn clif_broadcasttogm_sub_inner(bl: *const BlockList, msg: *const i8) -> i32 {
    let sd = bl as *const MapSessionData;
    if sd.is_null() { return 0; }

    if (*sd).status.gm_level != 0 {
        if !session_exists((*sd).fd) {
            return 0;
        }

        let len = libc_strlen(msg);

        wfifohead((*sd).fd, len + 8);
        wfifob((*sd).fd, 0, 0xAA);
        wfifob((*sd).fd, 3, 0x0A);
        wfifob((*sd).fd, 4, 0x03);
        wfifob((*sd).fd, 5, 0x05);
        wfifow((*sd).fd, 6, (len as u16).to_be());
        let dst = wfifop((*sd).fd, 8);
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(msg as *const u8, dst, len);
        }
        wfifow((*sd).fd, 1, ((len + 5) as u16).to_be());
        wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    }
    0
}

// ─── clif_broadcast ───────────────────────────────────────────────────────────

/// Send a broadcast message to all players on a map (or all maps if m == -1).
///
pub unsafe fn clif_broadcast(msg: *const i8, m: i32) -> i32 {
    if m == -1 {
        for x in 0..65535usize {
            if map_isloaded(x) {
                if let Some(grid) = block_grid::get_grid(x) {
                    let slot = &*raw_map_ptr().add(x);
                    let ids = block_grid::ids_in_area(grid, 1, 1, AreaType::SameMap, slot.xs as i32, slot.ys as i32);
                    for id in ids {
                        if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                            clif_broadcast_sub_inner(&raw const pc_arc.read().bl, msg);
                        }
                    }
                }
            }
        }
    } else {
        if let Some(grid) = block_grid::get_grid(m as usize) {
            let slot = &*raw_map_ptr().add(m as usize);
            let ids = block_grid::ids_in_area(grid, 1, 1, AreaType::SameMap, slot.xs as i32, slot.ys as i32);
            for id in ids {
                if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_broadcast_sub_inner(&raw const pc_arc.read().bl, msg);
                }
            }
        }
    }
    0
}

// ─── clif_gmbroadcast ─────────────────────────────────────────────────────────

/// Send a GM broadcast message to all GMs on a map (or all maps if m == -1).
///
pub unsafe fn clif_gmbroadcast(msg: *const i8, m: i32) -> i32 {
    if m == -1 {
        for x in 0..65535usize {
            if map_isloaded(x) {
                if let Some(grid) = block_grid::get_grid(x) {
                    let slot = &*raw_map_ptr().add(x);
                    let ids = block_grid::ids_in_area(grid, 1, 1, AreaType::SameMap, slot.xs as i32, slot.ys as i32);
                    for id in ids {
                        if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                            clif_gmbroadcast_sub_inner(&raw const pc_arc.read().bl, msg);
                        }
                    }
                }
            }
        }
    } else {
        if let Some(grid) = block_grid::get_grid(m as usize) {
            let slot = &*raw_map_ptr().add(m as usize);
            let ids = block_grid::ids_in_area(grid, 1, 1, AreaType::SameMap, slot.xs as i32, slot.ys as i32);
            for id in ids {
                if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_gmbroadcast_sub_inner(&raw const pc_arc.read().bl, msg);
                }
            }
        }
    }
    0
}

// ─── clif_broadcasttogm ───────────────────────────────────────────────────────

/// Send a broadcast message to all GMs on a map (or all maps if m == -1).
///
pub unsafe fn clif_broadcasttogm(msg: *const i8, m: i32) -> i32 {
    if m == -1 {
        for x in 0..65535usize {
            if map_isloaded(x) {
                if let Some(grid) = block_grid::get_grid(x) {
                    let slot = &*raw_map_ptr().add(x);
                    let ids = block_grid::ids_in_area(grid, 1, 1, AreaType::SameMap, slot.xs as i32, slot.ys as i32);
                    for id in ids {
                        if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                            clif_broadcasttogm_sub_inner(&raw const pc_arc.read().bl, msg);
                        }
                    }
                }
            }
        }
    } else {
        if let Some(grid) = block_grid::get_grid(m as usize) {
            let slot = &*raw_map_ptr().add(m as usize);
            let ids = block_grid::ids_in_area(grid, 1, 1, AreaType::SameMap, slot.xs as i32, slot.ys as i32);
            for id in ids {
                if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_broadcasttogm_sub_inner(&raw const pc_arc.read().bl, msg);
                }
            }
        }
    }
    0
}

// ─── clif_guitextsd ───────────────────────────────────────────────────────────

/// Send a GUI text popup to a single player.
///
pub unsafe fn clif_guitextsd(msg: *const i8, sd: *mut MapSessionData) -> i32 {
    if !session_alive((*sd).fd) { return 0; }

    let mlen = libc_strlen(msg);

    wfifohead((*sd).fd, mlen + 8);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 1, 0x00);
    wfifob((*sd).fd, 3, 0x58);
    wfifob((*sd).fd, 5, 0x06);
    wfifow((*sd).fd, 6, (mlen as u16).to_be());
    let dst = wfifop((*sd).fd, 8);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(msg as *const u8, dst, mlen);
    }
    wfifow((*sd).fd, 1, ((8 + mlen + 3) as u16).to_be());
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_guitext ─────────────────────────────────────────────────────────────

/// foreachinarea callback: send a GUI text popup to one player.
///
pub unsafe fn clif_guitext_inner(bl: *const BlockList, msg: *const i8) -> i32 {
    let sd = bl as *const MapSessionData;
    if sd.is_null() { return 0; }

    if !session_alive((*sd).fd) { return 0; }

    let mlen = libc_strlen(msg);

    wfifohead((*sd).fd, mlen + 8);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 1, 0x00);
    wfifob((*sd).fd, 3, 0x58);
    wfifob((*sd).fd, 5, 0x06);
    wfifow((*sd).fd, 6, (mlen as u16).to_be());
    let dst = wfifop((*sd).fd, 8);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(msg as *const u8, dst, mlen);
    }
    wfifow((*sd).fd, 1, ((8 + mlen + 3) as u16).to_be());
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_parseemotion ────────────────────────────────────────────────────────

/// Handle an emotion packet from the client.
///
pub unsafe fn clif_parseemotion(sd: *mut MapSessionData) -> i32 {
    use super::packet::rfifob;
    if (*sd).status.state == 0 {
        clif_sendaction(
            &mut (*sd).bl,
            rfifob((*sd).fd, 5) as i32 + 11,
            0x4E,
            0,
        );
    }
    0
}

// ─── clif_sendmsg ─────────────────────────────────────────────────────────────

/// Send a typed chat message to a single player's socket.
///
/// Type 0 = wisp/blue, 3 = mini text, 5 = system, 11 = group/subpath, 12 = clan.
///
pub unsafe fn clif_sendmsg(
    sd: *mut MapSessionData,
    mut msg_type: i32,
    buf: *const i8,
) -> i32 {
    if buf.is_null() { return 0; }

    let advice_flag = (*sd).status.setting_flags & FLAG_ADVICE as u16;
    if msg_type == 99 && advice_flag != 0 {
        msg_type = 11;
    } else if msg_type == 99 {
        return 0;
    }

    let len = libc_strlen(buf);

    if !session_alive((*sd).fd) { return 0; }

    wfifohead((*sd).fd, 8 + len);
    wfifob((*sd).fd, 0, 0xAA);
    wfifow((*sd).fd, 1, ((5 + len) as u16).to_be());
    wfifob((*sd).fd, 3, 0x0A);
    wfifob((*sd).fd, 4, 0x03);
    wfifow((*sd).fd, 5, msg_type as u16);
    wfifob((*sd).fd, 7, len as u8);
    let dst = wfifop((*sd).fd, 8);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(buf as *const u8, dst, len);
    }
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_sendminitext ────────────────────────────────────────────────────────

/// Send a mini status-text message to a single player.
///
pub unsafe fn clif_sendminitext(sd: *mut MapSessionData, msg: *const i8) -> i32 {
    if sd.is_null() { return 0; }
    if libc_strlen(msg) == 0 { return 0; }
    clif_sendmsg(sd, 3, msg);
    0
}

// ─── clif_sendwisp ────────────────────────────────────────────────────────────

/// Deliver an incoming whisper to the destination player.
///
pub unsafe fn clif_sendwisp(
    sd: *mut MapSessionData,
    srcname: *const i8,
    msg: *const i8,
) -> i32 {
    let msglen = libc_strlen(msg);
    let srclen = libc_strlen(srcname);

    let src_sd = map_name2sd(srcname);
    if src_sd.is_null() { return 0; }

    let cn = classdb_name((*src_sd).status.class as i32, (*src_sd).status.mark as i32);
    let buf2: Vec<u8>;
    let newlen: usize;
    {
        let cn_bytes = cn.as_bytes();
        let mut tmp = Vec::with_capacity(cn_bytes.len() + 6);
        tmp.extend_from_slice(b"\" (");
        tmp.extend_from_slice(cn_bytes);
        tmp.extend_from_slice(b") ");
        newlen = tmp.len();
        buf2 = tmp;
    }

    let mut combined: Vec<u8> = Vec::with_capacity(srclen + newlen + msglen);
    combined.extend_from_slice(std::slice::from_raw_parts(srcname as *const u8, srclen));
    combined.extend_from_slice(&buf2);
    combined.extend_from_slice(std::slice::from_raw_parts(msg as *const u8, msglen));

    if (*raw_map_ptr().add((*sd).bl.m as usize)).cantalk == 1 && (*sd).status.gm_level == 0 {
        clif_sendminitext(sd, c"Your voice is carried away.".as_ptr());
        return 0;
    }

    // combined is not null-terminated — clif_sendmsg uses len, not CStr
    // Temporarily null-terminate for C call
    combined.push(0);
    clif_sendmsg(sd, 0, combined.as_ptr() as *const i8);
    0
}

// ─── clif_retrwisp ────────────────────────────────────────────────────────────

/// Echo a whisper back to the sender (shows "dstname> msg").
///
pub unsafe fn clif_retrwisp(
    sd: *mut MapSessionData,
    dstname: *mut i8,
    msg: *mut i8,
) -> i32 {
    let dst_str = std::ffi::CStr::from_ptr(dstname).to_bytes();
    let msg_str = std::ffi::CStr::from_ptr(msg).to_bytes();

    // format: "dstname> msg\0"
    let mut buf: Vec<u8> = Vec::with_capacity(dst_str.len() + 2 + msg_str.len() + 1);
    buf.extend_from_slice(dst_str);
    buf.extend_from_slice(b"> ");
    buf.extend_from_slice(msg_str);
    buf.push(0);

    clif_sendmsg(sd, 0, buf.as_ptr() as *const i8);
    0
}

// ─── clif_failwisp ────────────────────────────────────────────────────────────

/// Tell the player their whisper failed.
///
pub unsafe fn clif_failwisp(sd: *mut MapSessionData) -> i32 {
    clif_sendmsg(sd, 0, map_msg()[MAP_WHISPFAIL].message.as_ptr() as *const i8);
    0
}

// ─── clif_sendbluemessage ─────────────────────────────────────────────────────

/// Send a blue (whisper-style type 0) system message.
///
pub unsafe fn clif_sendbluemessage(sd: *mut MapSessionData, msg: *const i8) -> i32 {
    if !session_alive((*sd).fd) { return 0; }

    let mlen = libc_strlen(msg);

    wfifohead((*sd).fd, mlen + 8);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 3, 0x0A);
    wfifob((*sd).fd, 4, 0x03);
    wfifow((*sd).fd, 5, 0u16);
    wfifob((*sd).fd, 7, mlen as u8);
    let dst = wfifop((*sd).fd, 8);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(msg as *const u8, dst, mlen);
    }
    wfifow((*sd).fd, 1, ((mlen + 5) as u16).to_be());
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_playsound ───────────────────────────────────────────────────────────

/// Send a positional sound effect to all nearby clients.
///
pub unsafe fn clif_playsound(bl: *mut BlockList, sound: i32) -> i32 {
    let mut buf2 = [0u8; 32];

    buf2[0] = 0xAA;
    buf_put_be16(&mut buf2, 1, 0x14);
    buf2[3] = 0x19;
    buf2[4] = 0x03;
    buf_put_be16(&mut buf2, 5, 3);
    buf_put_be16(&mut buf2, 7, sound as u16);
    buf2[9] = 100;
    buf_put_be16(&mut buf2, 10, 4);
    buf_put_be32(&mut buf2, 12, (*bl).id);
    buf2[16] = 1;
    buf2[17] = 0;
    buf2[18] = 2;
    buf2[19] = 2;
    buf_put_be16(&mut buf2, 20, 4);
    buf2[22] = 0;

    clif_send(buf2.as_ptr(), 32, bl, SAMEAREA);
    0
}

// ─── ignorelist_add ───────────────────────────────────────────────────────────

/// Add a player name to the ignore list (no-op if already present).
///
pub unsafe fn ignorelist_add(sd: *mut MapSessionData, name: *const i8) -> i32 {
    // Check if name is already on the list
    let mut current = (*sd).IgnoreList;
    while !current.is_null() {
        if strcasecmp_cstr((*current).name.as_ptr(), name) == 0 {
            return 1;
        }
        current = (*current).Next;
    }

    // Allocate new node
    let new_node = Box::into_raw(Box::new(std::mem::zeroed::<SdIgnoreList>()));

    // Copy name (field is [i8; 100])
    let src = std::slice::from_raw_parts(name as *const u8, libc_strlen(name).min(99));
    let dst = std::slice::from_raw_parts_mut((*new_node).name.as_mut_ptr() as *mut u8, 100);
    dst[..src.len()].copy_from_slice(src);

    (*new_node).Next = (*sd).IgnoreList;
    (*sd).IgnoreList = new_node;
    0
}

// ─── ignorelist_remove ────────────────────────────────────────────────────────

/// Remove a player name from the ignore list.
///
/// Returns 0 on success, 1 if not found, 2 if list is empty.
///
pub unsafe fn ignorelist_remove(sd: *mut MapSessionData, name: *const i8) -> i32 {
    if (*sd).IgnoreList.is_null() {
        return 2;
    }

    let mut current = (*sd).IgnoreList;
    let mut prev: *mut SdIgnoreList = std::ptr::null_mut();

    while !current.is_null() {
        if strcasecmp_cstr((*current).name.as_ptr(), name) == 0 {
            // Found: unlink
            if !prev.is_null() {
                (*prev).Next = (*current).Next;
            } else {
                // Head-node removal: advance list pointer before freeing
                (*sd).IgnoreList = (*current).Next;
            }
            // Re-establish Box ownership and drop.
            // SAFETY: current was allocated via Box::into_raw in ignorelist_add.
            drop(Box::from_raw(current));
            return 0;
        }
        prev = current;
        current = (*current).Next;
    }

    1 // not found
}

// ─── clif_isignore ────────────────────────────────────────────────────────────

/// Check whether `sd` is ignoring `dst_sd` or vice versa. Returns 1 if
/// communication is allowed, 0 if blocked.
///
pub unsafe fn clif_isignore(
    sd: *mut MapSessionData,
    dst_sd: *mut MapSessionData,
) -> i32 {
    // Check if sd's name is in dst_sd's ignore list
    let mut current = (*dst_sd).IgnoreList;
    while !current.is_null() {
        if strcasecmp_cstr((*current).name.as_ptr(), (*sd).status.name.as_ptr()) == 0 {
            return 0;
        }
        current = (*current).Next;
    }

    // Check if dst_sd's name is in sd's ignore list
    let mut current = (*sd).IgnoreList;
    while !current.is_null() {
        if strcasecmp_cstr((*current).name.as_ptr(), (*dst_sd).status.name.as_ptr()) == 0 {
            return 0;
        }
        current = (*current).Next;
    }

    1
}

// ─── canwhisper ───────────────────────────────────────────────────────────────

/// Check whether `sd` is allowed to whisper `dst_sd`.
///
pub unsafe fn canwhisper(
    sd: *mut MapSessionData,
    dst_sd: *mut MapSessionData,
) -> i32 {
    if dst_sd.is_null() { return 0; }

    if (*sd).uFlags & U_FLAG_SILENCED != 0 {
        return 0;
    } else if (*sd).status.gm_level == 0
        && (*dst_sd).status.setting_flags & FLAG_WHISPER as u16 == 0
    {
        return 0;
    } else if (*sd).status.gm_level == 0 {
        return clif_isignore(sd, dst_sd);
    }

    1
}

// ─── clif_sendgroupmessage ────────────────────────────────────────────────────

/// Send a party chat message to all group members.
///
pub unsafe fn clif_sendgroupmessage(
    sd: *mut MapSessionData,
    msg: *mut u8,
    msglen: i32,
) -> i32 {
    if sd.is_null() { return 0; }

    if (*sd).uFlags & U_FLAG_SILENCED != 0 {
        clif_sendbluemessage(sd, c"You are silenced.".as_ptr());
        return 0;
    }

    if (*raw_map_ptr().add((*sd).bl.m as usize)).cantalk == 1 && (*sd).status.gm_level == 0 {
        clif_sendminitext(sd, c"Your voice is swept away by a strange wind.".as_ptr());
        return 0;
    }

    let mut message = [0i8; 256];
    let copy_len = (msglen as usize).min(255);
    std::ptr::copy_nonoverlapping(msg as *const i8, message.as_mut_ptr(), copy_len);

    let buf2 = format_chat_prefix(sd, b"[!", b"]", &message[..copy_len]);
    let buf2_c = std::ffi::CString::new(buf2).unwrap_or_default();

    let base = (*sd).groupid as usize * 256;
    let grp = groups();
    for i in 0..(*sd).group_count as usize {
        let idx = base + i;
        if idx >= grp.len() { break; }
        let tsd = session_get_data_checked(SessionId::from_raw(grp[idx] as i32));
        if !tsd.is_null() && clif_isignore(sd, tsd) != 0 {
            clif_sendmsg(tsd as *mut MapSessionData, 11, buf2_c.as_ptr());
        }
    }
    0
}

// ─── clif_sendsubpathmessage ──────────────────────────────────────────────────

/// Send a sub-path (class channel) chat message.
///
pub unsafe fn clif_sendsubpathmessage(
    sd: *mut MapSessionData,
    msg: *mut u8,
    msglen: i32,
) -> i32 {
    if sd.is_null() { return 0; }

    if (*sd).uFlags & U_FLAG_SILENCED != 0 {
        clif_sendbluemessage(sd, c"You are silenced.".as_ptr());
        return 0;
    }

    if (*raw_map_ptr().add((*sd).bl.m as usize)).cantalk == 1 && (*sd).status.gm_level == 0 {
        clif_sendminitext(sd, c"Your voice is swept away by a strange wind.".as_ptr());
        return 0;
    }

    let mut message = [0i8; 256];
    let copy_len = (msglen as usize).min(255);
    std::ptr::copy_nonoverlapping(msg as *const i8, message.as_mut_ptr(), copy_len);

    let buf2 = format_chat_prefix(sd, b"<@", b">", &message[..copy_len]);
    let buf2_c = std::ffi::CString::new(buf2).unwrap_or_default();

    for i in 0..crate::session::get_fd_max() {
        let fd = SessionId::from_raw(i);
        if !session_exists(fd) { continue; }
        let tsd = session_get_data(fd);
        if tsd.is_null() { continue; }
        if clif_isignore(sd, tsd) != 0
            && (*tsd).status.class == (*sd).status.class
            && (*tsd).status.subpath_chat != 0
        {
            clif_sendmsg(tsd, 11, buf2_c.as_ptr());
        }
    }
    0
}

// ─── clif_sendclanmessage ─────────────────────────────────────────────────────

/// Send a clan chat message to all clan members.
///
pub unsafe fn clif_sendclanmessage(
    sd: *mut MapSessionData,
    msg: *mut u8,
    msglen: i32,
) -> i32 {
    if (*sd).uFlags & U_FLAG_SILENCED != 0 {
        clif_sendbluemessage(sd, c"You are silenced.".as_ptr());
        return 0;
    }

    if (*raw_map_ptr().add((*sd).bl.m as usize)).cantalk == 1 && (*sd).status.gm_level == 0 {
        clif_sendminitext(sd, c"Your voice is swept away by a strange wind.".as_ptr());
        return 0;
    }

    let mut message = [0i8; 256];
    let copy_len = (msglen as usize).min(255);
    std::ptr::copy_nonoverlapping(msg as *const i8, message.as_mut_ptr(), copy_len);

    let buf2 = format_chat_prefix(sd, b"<!", b">", &message[..copy_len]);
    let buf2_c = std::ffi::CString::new(buf2).unwrap_or_default();

    for i in 0..crate::session::get_fd_max() {
        let fd = SessionId::from_raw(i);
        if !session_exists(fd) { continue; }
        let tsd = session_get_data(fd);
        if tsd.is_null() { continue; }
        if clif_isignore(sd, tsd) != 0
            && (*tsd).status.clan == (*sd).status.clan
            && (*tsd).status.clan_chat != 0
        {
            clif_sendmsg(tsd, 12, buf2_c.as_ptr());
        }
    }
    0
}

// ─── clif_sendnovicemessage ───────────────────────────────────────────────────

/// Send a novice/tutor channel message.
///
pub unsafe fn clif_sendnovicemessage(
    sd: *mut MapSessionData,
    msg: *mut u8,
    msglen: i32,
) -> i32 {
    if (*sd).uFlags & U_FLAG_SILENCED != 0 {
        clif_sendbluemessage(sd, c"You are silenced.".as_ptr());
        return 0;
    }

    if (*raw_map_ptr().add((*sd).bl.m as usize)).cantalk == 1 && (*sd).status.gm_level == 0 {
        clif_sendminitext(sd, c"Your voice is swept away by a strange wind.".as_ptr());
        return 0;
    }

    let mut message = [0i8; 256];
    let copy_len = (msglen as usize).min(255);
    std::ptr::copy_nonoverlapping(msg as *const i8, message.as_mut_ptr(), copy_len);
    let msg_str = i8_slice_to_str(&message[..copy_len]);

    let class_str = classdb_name((*sd).status.class as i32, (*sd).status.mark as i32);

    let name_str = i8_slice_to_str(&(*sd).status.name);
    let buf2 = format!(
        "[Novice]({}) {}> {}\0",
        class_str, name_str, msg_str
    );

    // Non-tutors get a copy of their own message (so it appears on their screen)
    if (*sd).status.tutor == 0 {
        clif_sendmsg(sd, 11, buf2.as_ptr() as *const i8);
    }

    for i in 0..crate::session::get_fd_max() {
        let fd = SessionId::from_raw(i);
        if !session_exists(fd) { continue; }
        let tsd = session_get_data(fd);
        if tsd.is_null() { continue; }
        if clif_isignore(sd, tsd) != 0
            && ((*tsd).status.tutor != 0 || (*tsd).status.gm_level > 0)
            && (*tsd).status.novice_chat != 0
        {
            clif_sendmsg(tsd, 12, buf2.as_ptr() as *const i8);
        }
    }
    0
}

// ─── clif_parsewisp ───────────────────────────────────────────────────────────

/// Parse an incoming whisper packet and dispatch it.
///
pub unsafe fn clif_parsewisp(sd: *mut MapSessionData) -> i32 {
    use super::packet::{rfifob, rfifop, rfiforest, rfifow};

    if (*sd).status.setting_flags & FLAG_WHISPER as u16 == 0 && (*sd).status.gm_level == 0 {
        clif_sendbluemessage(sd, c"You have whispering turned off.".as_ptr());
        return 0;
    }

    if (*sd).uFlags & U_FLAG_SILENCED != 0 {
        clif_sendbluemessage(sd, c"You are silenced.".as_ptr());
        return 0;
    }

    if (*raw_map_ptr().add((*sd).bl.m as usize)).cantalk == 1 && (*sd).status.gm_level == 0 {
        clif_sendminitext(sd, c"Your voice is swept away by a strange wind.".as_ptr());
        return 0;
    }

    let dstlen = rfifob((*sd).fd, 5) as usize;
    let msglen = rfifob((*sd).fd, 6 + dstlen) as usize;

    // rfifow already returns host-order u16; no additional swap needed
    let pkt_size = rfifow((*sd).fd, 1) as usize;

    if msglen > 80
        || dstlen > 80
        || dstlen > rfiforest((*sd).fd) as usize
        || dstlen > pkt_size
        || msglen > rfiforest((*sd).fd) as usize
        || msglen > pkt_size
    {
        clif_Hacker((*sd).status.name.as_mut_ptr() as *mut i8, c"Whisper packet".as_ptr());
        return 0;
    }

    let mut dst_name = [0u8; 100];
    let mut msg_buf = [0u8; 100];

    let src_dst = rfifop((*sd).fd, 6);
    std::ptr::copy_nonoverlapping(src_dst, dst_name.as_mut_ptr(), dstlen.min(99));

    let src_msg = rfifop((*sd).fd, 7 + dstlen);
    std::ptr::copy_nonoverlapping(src_msg, msg_buf.as_mut_ptr(), msglen.min(80));
    msg_buf[80] = 0;

    let dst_name_c = dst_name.as_ptr() as *const i8;
    let msg_c = msg_buf.as_ptr() as *const i8;

    // "!" → clan chat
    if dst_name[0] == b'!' && dst_name[1] == 0 {
        if (*sd).status.clan == 0 {
            clif_sendbluemessage(sd, c"You are not in a clan".as_ptr());
        } else if (*sd).status.clan_chat != 0 {
            crate::game::scripting::doscript_strings(c"characterLog".as_ptr(), c"clanChatLog".as_ptr(), &[(*sd).status.name.as_ptr(), msg_c]);
            clif_sendclanmessage(sd, rfifop((*sd).fd, 7 + dstlen) as *mut u8, msglen as i32);
        } else {
            clif_sendbluemessage(sd, c"Clan chat is off.".as_ptr());
        }
    // "!!" → group chat
    } else if dst_name[0] == b'!' && dst_name[1] == b'!' && dst_name[2] == 0 {
        if (*sd).group_count == 0 {
            clif_sendbluemessage(sd, c"You are not in a group".as_ptr());
        } else {
            crate::game::scripting::doscript_strings(c"characterLog".as_ptr(), c"groupChatLog".as_ptr(), &[(*sd).status.name.as_ptr(), msg_c]);
            clif_sendgroupmessage(sd, rfifop((*sd).fd, 7 + dstlen) as *mut u8, msglen as i32);
        }
    // "@" → subpath chat
    } else if dst_name[0] == b'@' && dst_name[1] == 0 {
        if classdb_chat((*sd).status.class as i32) != 0 {
            crate::game::scripting::doscript_strings(c"characterLog".as_ptr(), c"subPathChatLog".as_ptr(), &[(*sd).status.name.as_ptr(), msg_c]);
            clif_sendsubpathmessage(sd, rfifop((*sd).fd, 7 + dstlen) as *mut u8, msglen as i32);
        } else {
            clif_sendbluemessage(sd, c"You cannot do that.".as_ptr());
        }
    // "?" → novice chat
    } else if dst_name[0] == b'?' && dst_name[1] == 0 {
        if (*sd).status.tutor == 0 && (*sd).status.gm_level == 0 {
            if (*sd).status.level < 99 {
                crate::game::scripting::doscript_strings(c"characterLog".as_ptr(), c"noviceChatLog".as_ptr(), &[(*sd).status.name.as_ptr(), msg_c]);
                clif_sendnovicemessage(sd, rfifop((*sd).fd, 7 + dstlen) as *mut u8, msglen as i32);
            } else {
                clif_sendbluemessage(sd, c"You cannot do that.".as_ptr());
            }
        } else {
            crate::game::scripting::doscript_strings(c"characterLog".as_ptr(), c"noviceChatLog".as_ptr(), &[(*sd).status.name.as_ptr(), msg_c]);
            clif_sendnovicemessage(sd, rfifop((*sd).fd, 7 + dstlen) as *mut u8, msglen as i32);
        }
    // named whisper
    } else {
        let dst_sd = map_name2sd(dst_name_c);
        if dst_sd.is_null() {
            let target = std::ffi::CStr::from_ptr(dst_name_c).to_string_lossy();
            let nf = format!("{} is nowhere to be found.\0", target);
            clif_sendbluemessage(sd, nf.as_ptr() as *const i8);
        } else if canwhisper(sd, dst_sd) != 0 {
            if (*dst_sd).afk != 0 {
                let afk_msg = std::ffi::CStr::from_ptr((*dst_sd).afkmessage.as_ptr());
                let has_afk_msg = !afk_msg.to_bytes().is_empty();

                crate::game::scripting::doscript_strings(c"characterLog".as_ptr(), c"whisperLog".as_ptr(), &[(*dst_sd).status.name.as_ptr(), (*sd).status.name.as_ptr(), msg_c]);

                clif_sendwisp(dst_sd, (*sd).status.name.as_ptr() as *const i8, msg_c);
                clif_sendwisp(dst_sd, (*dst_sd).status.name.as_ptr() as *const i8, (*dst_sd).afkmessage.as_ptr() as *const i8);

                if (*sd).status.gm_level == 0 && (*dst_sd).optFlags & OPT_FLAG_STEALTH != 0 {
                    // don't reveal their presence — strText was formatted but C never sent it here
                } else {
                    clif_retrwisp(sd, (*dst_sd).status.name.as_mut_ptr() as *mut i8, msg_buf.as_mut_ptr() as *mut i8);
                    clif_retrwisp(sd, (*dst_sd).status.name.as_mut_ptr() as *mut i8, (*dst_sd).afkmessage.as_mut_ptr() as *mut i8);
                }
                let _ = has_afk_msg;
            } else {
                crate::game::scripting::doscript_strings(c"characterLog".as_ptr(), c"whisperLog".as_ptr(), &[(*dst_sd).status.name.as_ptr(), (*sd).status.name.as_ptr(), msg_c]);

                clif_sendwisp(dst_sd, (*sd).status.name.as_ptr() as *const i8, msg_c);

                if (*sd).status.gm_level == 0 && (*dst_sd).optFlags & OPT_FLAG_STEALTH != 0 {
                    let target = std::ffi::CStr::from_ptr(dst_name_c).to_string_lossy();
                    let nf = format!("{} is nowhere to be found.\0", target);
                    clif_sendbluemessage(sd, nf.as_ptr() as *const i8);
                } else {
                    clif_retrwisp(sd, (*dst_sd).status.name.as_mut_ptr() as *mut i8, msg_buf.as_mut_ptr() as *mut i8);
                }
            }
        } else {
            clif_sendbluemessage(sd, c"They cannot hear you right now.".as_ptr());
        }
    }
    0
}

// ─── clif_sendsay ─────────────────────────────────────────────────────────────

/// Broadcast a player say/shout and fire NPC speech callbacks.
///
pub unsafe fn clif_sendsay(
    sd: *mut MapSessionData,
    msg: *mut i8,
    msglen: i32,
    say_type: i32,
) -> i32 {
    let namelen = libc_strlen((*sd).status.name.as_ptr() as *const i8);

    if say_type == 1 {
        (*sd).talktype = 1;
        let src = std::slice::from_raw_parts(msg as *const u8, (msglen as usize).min(254));
        let dst = std::slice::from_raw_parts_mut((*sd).speech.as_mut_ptr() as *mut u8, 255);
        dst[..src.len()].copy_from_slice(src);
        dst[src.len()] = 0;
    } else {
        (*sd).talktype = 0;
        let src = std::slice::from_raw_parts(msg as *const u8, (msglen as usize).min(254));
        let dst = std::slice::from_raw_parts_mut((*sd).speech.as_mut_ptr() as *mut u8, 255);
        dst[..src.len()].copy_from_slice(src);
        dst[src.len()] = 0;
    }

    for i in 0..MAX_SPELLS {
        if (*sd).status.skill[i] > 0 {
            let yname = (*magic_db::search((*sd).status.skill[i] as i32)).yname.as_ptr();
            sl_doscript_simple(yname, c"on_say".as_ptr(), &raw mut (*sd).bl);
        }
    }
    sl_doscript_simple(c"onSay".as_ptr(), std::ptr::null(), &raw mut (*sd).bl);
    0
}

// ─── clif_sendscriptsay ───────────────────────────────────────────────────────

/// Broadcast a player's scripted say and log it; handles language channels.
///
pub unsafe fn clif_sendscriptsay(
    sd: *mut MapSessionData,
    msg: *const i8,
    msglen: i32,
    say_type: i32,
) -> i32 {
    let namelen = libc_strlen((*sd).status.name.as_ptr() as *const i8);

    if (*raw_map_ptr().add((*sd).bl.m as usize)).cantalk == 1 && (*sd).status.gm_level == 0 {
        clif_sendminitext(sd, c"Your voice is swept away by a strange wind.".as_ptr());
        return 0;
    }

    if is_command(sd, msg, msglen) != 0 {
        return 0;
    }

    if (*sd).uFlags & U_FLAG_SILENCED != 0 {
        clif_sendminitext(sd, c"Shut up for now. ^^".as_ptr());
        return 0;
    }

    let msg_bytes = std::slice::from_raw_parts(msg as *const u8, msglen as usize);

    if say_type >= 10 {
        let ext_namelen = namelen + 4; // prefix like "EN[" + name + "]"

        if !session_exists((*sd).fd) {
            return 0;
        }

        let buf_size = 16 + ext_namelen + msglen as usize;
        let mut buf = vec![0u8; buf_size];

        buf[0] = 0xAA;
        buf_put_be16(&mut buf, 1, (10 + ext_namelen + msglen as usize) as u16);
        buf[3] = 0x0D;
        buf[5] = say_type as u8;
        buf_put_be32(&mut buf, 6, (*sd).status.id);
        buf[10] = (ext_namelen + msglen as usize + 2) as u8;

        // Build prefixed name
        let name_str = i8_slice_to_str(&(*sd).status.name);
        let prefixed = match say_type {
            10 => format!("EN[{}]", name_str),
            11 => format!("ES[{}]", name_str),
            12 => format!("FR[{}]", name_str),
            13 => format!("CN[{}]", name_str),
            14 => format!("PT[{}]", name_str),
            15 => format!("ID[{}]", name_str),
            _  => name_str.to_string(),
        };
        let pname = prefixed.as_bytes();
        buf[11..11 + pname.len()].copy_from_slice(pname);
        buf[11 + pname.len()] = b':';
        buf[12 + pname.len()] = b' ';
        buf[13 + pname.len()..13 + pname.len() + msglen as usize].copy_from_slice(msg_bytes);

        clif_send(buf.as_ptr(), buf_size as i32, &raw mut (*sd).bl, SAMEAREA);
    } else {
        if !session_exists((*sd).fd) {
            return 0;
        }

        let buf_size = 16 + namelen + msglen as usize;
        let mut buf = vec![0u8; buf_size];

        buf[0] = 0xAA;
        buf_put_be16(&mut buf, 1, (10 + namelen + msglen as usize) as u16);
        buf[3] = 0x0D;
        buf[5] = say_type as u8;
        buf_put_be32(&mut buf, 6, (*sd).status.id);
        buf[10] = (namelen + msglen as usize + 2) as u8;

        let name_bytes = std::slice::from_raw_parts(
            (*sd).status.name.as_ptr() as *const u8,
            namelen,
        );
        buf[11..11 + namelen].copy_from_slice(name_bytes);
        buf[11 + namelen] = if say_type == 1 { b'!' } else { b':' };
        buf[12 + namelen] = b' ';
        buf[13 + namelen..13 + namelen + msglen as usize].copy_from_slice(msg_bytes);

        let send_target = if say_type == 1 { SAMEMAP } else { SAMEAREA };
        clif_send(buf.as_ptr(), buf_size as i32, &raw mut (*sd).bl, send_target);
    }

    // Copy msg to speech
    let src = std::slice::from_raw_parts(msg as *const u8, (msglen as usize).min(254));
    let dst = std::slice::from_raw_parts_mut((*sd).speech.as_mut_ptr() as *mut u8, 255);
    dst[..src.len()].copy_from_slice(src);
    dst[src.len()] = 0;

    let m = (*sd).bl.m as i32;
    let bx = (*sd).bl.x as i32;
    let by = (*sd).bl.y as i32;
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let slot = &*raw_map_ptr().add(m as usize);
        let area = if say_type == 1 { AreaType::SameMap } else { AreaType::Area };
        let ids = block_grid::ids_in_area(grid, bx, by, area, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if let Some(npc_arc) = crate::game::map_server::map_id2npc_ref(id) {
                if say_type == 1 {
                    clif_sendnpcyell_inner(&raw mut npc_arc.write().bl, msg, sd);
                } else {
                    clif_sendnpcsay_inner(&raw mut npc_arc.write().bl, msg, sd);
                }
            } else if let Some(mob_arc) = crate::game::map_server::map_id2mob_ref(id) {
                if say_type == 1 {
                    clif_sendmobyell_inner(&raw mut mob_arc.write().bl, msg, sd);
                } else {
                    clif_sendmobsay_inner(&raw mut mob_arc.write().bl, msg, sd);
                }
            }
        }
    }
    0
}

// ─── clif_sendnpcsay ──────────────────────────────────────────────────────────

/// foreachinarea callback: fire NPC speech handler if player is nearby.
///
pub unsafe fn clif_sendnpcsay_inner(bl: *mut BlockList, _msg: *const i8, sd_arg: *mut MapSessionData) -> i32 {
    if (*bl).subtype != SCRIPT { return 0; }

    if sd_arg.is_null() { return 0; }

    let nd = bl as *mut NpcData;
    if nd.is_null() { return 0; }

    if clif_distance(&*bl, &(*sd_arg).bl) <= 10 {
        (*sd_arg).last_click = (*bl).id;
        sl_async_freeco(sd_arg);
        sl_doscript_coro_2((*nd).name.as_ptr() as *const i8, c"onSayClick".as_ptr(), &raw mut (*sd_arg).bl, bl);
    }
    0
}

// ─── clif_sendmobsay ──────────────────────────────────────────────────────────

/// foreachinarea callback: mob speech handler (currently a no-op in C).
///
pub unsafe fn clif_sendmobsay_inner(_bl: *mut BlockList, _msg: *const i8, _sd: *mut MapSessionData) -> i32 {
    0
}

// ─── clif_sendnpcyell ─────────────────────────────────────────────────────────

/// foreachinarea callback: fire NPC speech handler (yell range = 20).
///
pub unsafe fn clif_sendnpcyell_inner(bl: *mut BlockList, _msg: *const i8, sd_arg: *mut MapSessionData) -> i32 {
    if (*bl).subtype != SCRIPT { return 0; }

    if sd_arg.is_null() { return 0; }

    let nd = bl as *mut NpcData;
    if nd.is_null() { return 0; }

    if clif_distance(&*bl, &(*sd_arg).bl) <= 20 {
        (*sd_arg).last_click = (*bl).id;
        sl_async_freeco(sd_arg);
        sl_doscript_coro_2((*nd).name.as_ptr() as *const i8, c"onSayClick".as_ptr(), &raw mut (*sd_arg).bl, bl);
    }
    0
}

// ─── clif_sendmobyell ─────────────────────────────────────────────────────────

/// foreachinarea callback: mob yell handler (currently a no-op in C).
///
pub unsafe fn clif_sendmobyell_inner(_bl: *mut BlockList, _msg: *const i8, _sd: *mut MapSessionData) -> i32 {
    0
}

// ─── clif_speak ───────────────────────────────────────────────────────────────

/// Send an NPC/object speech-bubble packet to one player.
///
pub unsafe fn clif_speak_inner(bl: *const BlockList, msg: *const i8, nd: *const BlockList, speak_type: i32) -> i32 {
    let sd = bl as *const MapSessionData;
    if sd.is_null() || nd.is_null() { return 0; }

    let len = libc_strlen(msg);

    if !session_alive((*sd).fd) { return 0; }

    wfifohead((*sd).fd, len + 11);
    wfifob((*sd).fd, 5, speak_type as u8);
    wfifol((*sd).fd, 6, ((*nd).id).to_be());
    wfifob((*sd).fd, 10, len as u8);
    // len for header = len + 8
    let hdr_len = (len + 8) as u16;
    wfifoheader((*sd).fd, 0x0D, hdr_len);
    // copy msg after header at offset 11
    let dst = wfifop((*sd).fd, 11);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(msg as *const u8, dst, len);
    }
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_parseignore ─────────────────────────────────────────────────────────

/// Handle an ignore-list add/remove packet from the client.
///
pub unsafe fn clif_parseignore(sd: *mut MapSessionData) -> i32 {
    use super::packet::rfifob;

    let icmd = rfifob((*sd).fd, 5);
    let nlen = rfifob((*sd).fd, 6) as usize;

    if nlen <= 16 {
        let mut name_buf = [0i8; 32];
        match icmd {
            0x02 => {
                // Add
                let src = super::packet::rfifop((*sd).fd, 7);
                std::ptr::copy_nonoverlapping(src as *const i8, name_buf.as_mut_ptr(), nlen.min(31));
                ignorelist_add(sd, name_buf.as_ptr() as *const i8);
            }
            0x03 => {
                // Remove
                let src = super::packet::rfifop((*sd).fd, 7);
                std::ptr::copy_nonoverlapping(src as *const i8, name_buf.as_mut_ptr(), nlen.min(31));
                ignorelist_remove(sd, name_buf.as_ptr() as *const i8);
            }
            _ => {}
        }
    }
    0
}

// ─── clif_parsesay ────────────────────────────────────────────────────────────

/// Parse an incoming say packet from the client.
///
pub unsafe fn clif_parsesay(sd: *mut MapSessionData) -> i32 {
    use super::packet::{rfifob, rfifop};

    let msg = rfifop((*sd).fd, 7) as *const i8;

    (*sd).talktype = rfifob((*sd).fd, 5);

    if (*sd).talktype > 1 || rfifob((*sd).fd, 6) > 100 {
        clif_sendminitext(sd, c"I just told the GM on you!".as_ptr());
        tracing::warn!("[chat] Talk Hacker: {}", i8_slice_to_str(&(*sd).status.name));
        return 0;
    }

    if is_command(sd, msg, rfifob((*sd).fd, 6) as i32) != 0 {
        return 0;
    }

    // Copy msg into speech
    let src = std::slice::from_raw_parts(msg as *const u8, (rfifob((*sd).fd, 6) as usize).min(254));
    let dst = std::slice::from_raw_parts_mut((*sd).speech.as_mut_ptr() as *mut u8, 255);
    dst[..src.len()].copy_from_slice(src);
    dst[src.len()] = 0;

    for i in 0..MAX_SPELLS {
        if (*sd).status.skill[i] > 0 {
            let yname = (*magic_db::search((*sd).status.skill[i] as i32)).yname.as_ptr();
            sl_doscript_simple(yname, c"on_say".as_ptr(), &raw mut (*sd).bl);
        }
    }
    sl_doscript_simple(c"onSay".as_ptr(), std::ptr::null(), &raw mut (*sd).bl);
    0
}

// ─── Private helpers ──────────────────────────────────────────────────────────

/// Measure a C string length (mirrors `strlen`).
#[inline]
unsafe fn libc_strlen(s: *const i8) -> usize {
    if s.is_null() { return 0; }
    let mut p = s as *const u8;
    let mut n = 0usize;
    while *p != 0 { p = p.add(1); n += 1; }
    n
}

/// Case-insensitive comparison of two C strings. Returns 0 if equal.
unsafe fn strcasecmp_cstr(a: *const i8, b: *const i8) -> i32 {
    let a_str = std::ffi::CStr::from_ptr(a);
    let b_str = std::ffi::CStr::from_ptr(b);
    // Compare byte-by-byte, case-insensitive (ASCII only — player names are ASCII).
    let a_bytes = a_str.to_bytes();
    let b_bytes = b_str.to_bytes();
    if a_bytes.len() != b_bytes.len() { return 1; }
    for (&x, &y) in a_bytes.iter().zip(b_bytes.iter()) {
        if x.to_ascii_lowercase() != y.to_ascii_lowercase() { return 1; }
    }
    0
}

/// Convert an `i8` slice (null-terminated or not) to a `&str`.
fn i8_slice_to_str(s: &[i8]) -> &str {
    let bytes = unsafe { &*(s as *const [i8] as *const [u8]) };
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end]).unwrap_or("")
}

/// Build `"[prefix name][(classname)] message"` for group/clan/subpath chat.
unsafe fn format_chat_prefix(sd: *mut MapSessionData, open: &[u8], close: &[u8], msg: &[i8]) -> Vec<u8> {
    let name = i8_slice_to_str(&(*sd).status.name);
    let class_str = classdb_name((*sd).status.class as i32, (*sd).status.mark as i32);
    let msg_str = i8_slice_to_str(msg);

    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(open);
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(close);
    out.extend_from_slice(b" (");
    out.extend_from_slice(class_str.as_bytes());
    out.extend_from_slice(b") ");
    out.extend_from_slice(msg_str.as_bytes());
    out
}

/// Manhattan distance between two block_list entries.
fn clif_distance(bl: &BlockList, bl2: &BlockList) -> i32 {
    let dx = (bl.x as i32) - (bl2.x as i32);
    let dy = (bl.y as i32) - (bl2.y as i32);
    dx.abs() + dy.abs()
}

/// Retrieve session data for fd, returning null if session does not exist.
#[inline]
unsafe fn session_get_data_checked(fd: SessionId) -> *mut MapSessionData {
    if session_exists(fd) {
        session_get_data(fd)
    } else {
        std::ptr::null_mut()
    }
}
