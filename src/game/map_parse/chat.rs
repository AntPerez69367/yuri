//! Covers broadcast, whisper, say, ignore list, and NPC speech callbacks.

use crate::database::map_db::raw_map_ptr;
use crate::session::{
    SessionId, session_exists, session_get_data,
};
use crate::game::npc::NpcData;
use crate::game::pc::{
    SdIgnoreList,
    FLAG_ADVICE, FLAG_SHOUT, FLAG_WHISPER,
    OPT_FLAG_STEALTH, U_FLAG_SILENCED,
    MAP_WHISPFAIL,
    groups,
    map_msg,
};
use crate::common::player::spells::MAX_SPELLS;
use crate::game::pc::MapSessionData;
use crate::game::player::entity::PlayerEntity;

use super::packet::{
    encrypt, wfifob, wfifohead, wfifol, wfifop, wfifoset, wfifow, wfifoheader,
    clif_send,
    SAMEMAP, SAMEAREA,
};
use crate::game::block::AreaType;
use crate::game::block_grid;
use crate::game::mob::{BL_PC, MobSpawnData};

/// Check that the session exists; if not, return false.
#[inline]
unsafe fn session_alive(fd: SessionId) -> bool {
    if session_exists(fd) { return true; }
    false
}

use crate::game::map_server::map_name2sd;
use crate::game::map_parse::combat::clif_sendaction_pc;
use crate::database::class_db::{name as classdb_name, chat as classdb_chat};
use crate::game::gm_command::is_command;
use crate::database::magic_db;
use crate::game::client::handlers::clif_Hacker;
use crate::game::client::BroadcastSrc;

// Alias for the async coroutine freeer (returns () in Rust, was i32 in C -- return unused).
#[inline]
unsafe fn sl_async_freeco(pe: &PlayerEntity) {
    let sd_ptr = &mut *pe.write() as *mut MapSessionData;
    crate::game::scripting::sl_async_freeco(sd_ptr);
}

/// Dispatch a Lua event with a single entity ID argument.
fn sl_doscript_simple(root: &str, method: Option<&str>, id: u32) -> i32 {
    crate::game::scripting::doscript_blargs_id(root, method, &[id])
}

fn sl_doscript_coro_2(root: &str, method: Option<&str>, id1: u32, id2: u32) -> i32 {
    crate::game::scripting::doscript_coro_id(root, method, &[id1, id2])
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
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendguidespecific(pe: &PlayerEntity, guide: i32) -> i32 {
    if !session_alive(pe.fd) { return 0; }

    wfifohead(pe.fd, 10);
    wfifob(pe.fd, 0, 0xAA);
    wfifow(pe.fd, 1, (0x07u16).to_be());
    wfifob(pe.fd, 3, 0x12);
    wfifob(pe.fd, 4, 0x03);
    wfifob(pe.fd, 5, 0x00);
    wfifob(pe.fd, 6, 0x02);
    wfifow(pe.fd, 7, (guide as u16).to_le());
    wfifob(pe.fd, 8, 0);
    wfifob(pe.fd, 9, 1);
    wfifoset(pe.fd, encrypt(pe.fd) as usize);
    0
}

// ─── clif_broadcast_sub ───────────────────────────────────────────────────────

/// foreachinarea callback: send a global broadcast to one player.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_broadcast_sub_inner(pe: &PlayerEntity, msg: *const i8) -> i32 {
    if !session_alive(pe.fd) { return 0; }

    let flag = pe.read().player.appearance.setting_flags & FLAG_SHOUT;
    if flag == 0 { return 0; }

    let len = libc_strlen(msg);

    wfifohead(pe.fd, len + 8);
    wfifob(pe.fd, 0, 0xAA);
    wfifob(pe.fd, 3, 0x0A);
    wfifob(pe.fd, 4, 0x03);
    wfifob(pe.fd, 5, 0x05);
    wfifow(pe.fd, 6, (len as u16).to_be());
    // copy msg bytes into wbuffer at offset 8
    let dst = wfifop(pe.fd, 8);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(msg as *const u8, dst, len);
    }
    wfifow(pe.fd, 1, ((len + 5) as u16).to_be());
    wfifoset(pe.fd, encrypt(pe.fd) as usize);
    0
}

// ─── clif_gmbroadcast_sub ─────────────────────────────────────────────────────

/// foreachinarea callback: send a GM broadcast to one player.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_gmbroadcast_sub_inner(pe: &PlayerEntity, msg: *const i8) -> i32 {
    if !session_alive(pe.fd) { return 0; }

    let len = libc_strlen(msg);

    wfifohead(pe.fd, len + 8);
    wfifob(pe.fd, 0, 0xAA);
    wfifob(pe.fd, 3, 0x0A);
    wfifob(pe.fd, 4, 0x03);
    wfifob(pe.fd, 5, 0x05);
    wfifow(pe.fd, 6, (len as u16).to_be());
    let dst = wfifop(pe.fd, 8);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(msg as *const u8, dst, len);
    }
    wfifow(pe.fd, 1, ((len + 5) as u16).to_be());
    wfifoset(pe.fd, encrypt(pe.fd) as usize);
    0
}

// ─── clif_broadcasttogm_sub ───────────────────────────────────────────────────

/// foreachinarea callback: send a broadcast only if the player is a GM.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_broadcasttogm_sub_inner(pe: &PlayerEntity, msg: *const i8) -> i32 {
    if pe.read().player.identity.gm_level != 0 {
        if !session_exists(pe.fd) {
            return 0;
        }

        let len = libc_strlen(msg);

        wfifohead(pe.fd, len + 8);
        wfifob(pe.fd, 0, 0xAA);
        wfifob(pe.fd, 3, 0x0A);
        wfifob(pe.fd, 4, 0x03);
        wfifob(pe.fd, 5, 0x05);
        wfifow(pe.fd, 6, (len as u16).to_be());
        let dst = wfifop(pe.fd, 8);
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(msg as *const u8, dst, len);
        }
        wfifow(pe.fd, 1, ((len + 5) as u16).to_be());
        wfifoset(pe.fd, encrypt(pe.fd) as usize);
    }
    0
}

// ─── clif_broadcast ───────────────────────────────────────────────────────────

/// Send a broadcast message to all players on a map (or all maps if m == -1).
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_broadcast(msg: *const i8, m: i32) -> i32 {
    if m == -1 {
        for x in 0..65535usize {
            if map_isloaded(x) {
                if let Some(grid) = block_grid::get_grid(x) {
                    let slot = &*raw_map_ptr().add(x);
                    let ids = block_grid::ids_in_area(grid, 1, 1, AreaType::SameMap, slot.xs as i32, slot.ys as i32);
                    for id in ids {
                        if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                            clif_broadcast_sub_inner(pc_arc.as_ref(), msg);
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
                    clif_broadcast_sub_inner(pc_arc.as_ref(), msg);
                }
            }
        }
    }
    0
}

// ─── clif_gmbroadcast ─────────────────────────────────────────────────────────

/// Send a GM broadcast message to all GMs on a map (or all maps if m == -1).
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_gmbroadcast(msg: *const i8, m: i32) -> i32 {
    if m == -1 {
        for x in 0..65535usize {
            if map_isloaded(x) {
                if let Some(grid) = block_grid::get_grid(x) {
                    let slot = &*raw_map_ptr().add(x);
                    let ids = block_grid::ids_in_area(grid, 1, 1, AreaType::SameMap, slot.xs as i32, slot.ys as i32);
                    for id in ids {
                        if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                            clif_gmbroadcast_sub_inner(pc_arc.as_ref(), msg);
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
                    clif_gmbroadcast_sub_inner(pc_arc.as_ref(), msg);
                }
            }
        }
    }
    0
}

// ─── clif_broadcasttogm ───────────────────────────────────────────────────────

/// Send a broadcast message to all GMs on a map (or all maps if m == -1).
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_broadcasttogm(msg: *const i8, m: i32) -> i32 {
    if m == -1 {
        for x in 0..65535usize {
            if map_isloaded(x) {
                if let Some(grid) = block_grid::get_grid(x) {
                    let slot = &*raw_map_ptr().add(x);
                    let ids = block_grid::ids_in_area(grid, 1, 1, AreaType::SameMap, slot.xs as i32, slot.ys as i32);
                    for id in ids {
                        if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                            clif_broadcasttogm_sub_inner(pc_arc.as_ref(), msg);
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
                    clif_broadcasttogm_sub_inner(pc_arc.as_ref(), msg);
                }
            }
        }
    }
    0
}

// ─── clif_guitextsd ───────────────────────────────────────────────────────────

/// Send a GUI text popup to a single player.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_guitextsd(msg: *const i8, pe: &PlayerEntity) -> i32 {
    if !session_alive(pe.fd) { return 0; }

    let mlen = libc_strlen(msg);

    wfifohead(pe.fd, mlen + 8);
    wfifob(pe.fd, 0, 0xAA);
    wfifob(pe.fd, 1, 0x00);
    wfifob(pe.fd, 3, 0x58);
    wfifob(pe.fd, 5, 0x06);
    wfifow(pe.fd, 6, (mlen as u16).to_be());
    let dst = wfifop(pe.fd, 8);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(msg as *const u8, dst, mlen);
    }
    wfifow(pe.fd, 1, ((8 + mlen + 3) as u16).to_be());
    wfifoset(pe.fd, encrypt(pe.fd) as usize);
    0
}

// ─── clif_guitext ─────────────────────────────────────────────────────────────

/// foreachinarea callback: send a GUI text popup to one player.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_guitext_inner(pe: &PlayerEntity, msg: *const i8) -> i32 {
    if !session_alive(pe.fd) { return 0; }

    let mlen = libc_strlen(msg);

    wfifohead(pe.fd, mlen + 8);
    wfifob(pe.fd, 0, 0xAA);
    wfifob(pe.fd, 1, 0x00);
    wfifob(pe.fd, 3, 0x58);
    wfifob(pe.fd, 5, 0x06);
    wfifow(pe.fd, 6, (mlen as u16).to_be());
    let dst = wfifop(pe.fd, 8);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(msg as *const u8, dst, mlen);
    }
    wfifow(pe.fd, 1, ((8 + mlen + 3) as u16).to_be());
    wfifoset(pe.fd, encrypt(pe.fd) as usize);
    0
}

// ─── clif_parseemotion ────────────────────────────────────────────────────────

/// Handle an emotion packet from the client.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parseemotion(pe: &PlayerEntity) -> i32 {
    use super::packet::rfifob;
    if pe.read().player.combat.state == 0 {
        clif_sendaction_pc(
            &mut pe.write(),
            rfifob(pe.fd, 5) as i32 + 11,
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
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendmsg(
    pe: &PlayerEntity,
    mut msg_type: i32,
    buf: *const i8,
) -> i32 {
    if buf.is_null() { return 0; }

    let advice_flag = pe.read().player.appearance.setting_flags & FLAG_ADVICE;
    if msg_type == 99 && advice_flag != 0 {
        msg_type = 11;
    } else if msg_type == 99 {
        return 0;
    }

    let len = libc_strlen(buf);

    if !session_alive(pe.fd) { return 0; }

    wfifohead(pe.fd, 8 + len);
    wfifob(pe.fd, 0, 0xAA);
    wfifow(pe.fd, 1, ((5 + len) as u16).to_be());
    wfifob(pe.fd, 3, 0x0A);
    wfifob(pe.fd, 4, 0x03);
    wfifow(pe.fd, 5, msg_type as u16);
    wfifob(pe.fd, 7, len as u8);
    let dst = wfifop(pe.fd, 8);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(buf as *const u8, dst, len);
    }
    wfifoset(pe.fd, encrypt(pe.fd) as usize);
    0
}

// ─── clif_sendminitext ────────────────────────────────────────────────────────

/// Send a mini status-text message to a single player.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendminitext(pe: &PlayerEntity, msg: *const i8) -> i32 {
    if libc_strlen(msg) == 0 { return 0; }
    clif_sendmsg(pe, 3, msg);
    0
}

// ─── clif_sendwisp ────────────────────────────────────────────────────────────

/// Deliver an incoming whisper to the destination player.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendwisp(
    pe: &PlayerEntity,
    srcname: *const i8,
    msg: *const i8,
) -> i32 {
    let msglen = libc_strlen(msg);
    let srclen = libc_strlen(srcname);

    let src_sd = map_name2sd(srcname);
    if src_sd.is_null() { return 0; }

    let cn = classdb_name((*src_sd).player.progression.class as i32, (*src_sd).player.progression.mark as i32);
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

    let (sd_m, sd_gm_level) = {
        let g = pe.read();
        (g.m, g.player.identity.gm_level)
    };
    if (*raw_map_ptr().add(sd_m as usize)).cantalk == 1 && sd_gm_level == 0 {
        clif_sendminitext(pe, c"Your voice is carried away.".as_ptr());
        return 0;
    }

    // combined is not null-terminated — clif_sendmsg uses len, not CStr
    // Temporarily null-terminate for C call
    combined.push(0);
    clif_sendmsg(pe, 0, combined.as_ptr() as *const i8);
    0
}

// ─── clif_retrwisp ────────────────────────────────────────────────────────────

/// Echo a whisper back to the sender (shows "dstname> msg").
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_retrwisp(
    pe: &PlayerEntity,
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

    clif_sendmsg(pe, 0, buf.as_ptr() as *const i8);
    0
}

// ─── clif_failwisp ────────────────────────────────────────────────────────────

/// Tell the player their whisper failed.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_failwisp(pe: &PlayerEntity) -> i32 {
    clif_sendmsg(pe, 0, map_msg()[MAP_WHISPFAIL].message.as_ptr());
    0
}

// ─── clif_sendbluemessage ─────────────────────────────────────────────────────

/// Send a blue (whisper-style type 0) system message.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendbluemessage(pe: &PlayerEntity, msg: *const i8) -> i32 {
    if !session_alive(pe.fd) { return 0; }

    let mlen = libc_strlen(msg);

    wfifohead(pe.fd, mlen + 8);
    wfifob(pe.fd, 0, 0xAA);
    wfifob(pe.fd, 3, 0x0A);
    wfifob(pe.fd, 4, 0x03);
    wfifow(pe.fd, 5, 0u16);
    wfifob(pe.fd, 7, mlen as u8);
    let dst = wfifop(pe.fd, 8);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(msg as *const u8, dst, mlen);
    }
    wfifow(pe.fd, 1, ((mlen + 5) as u16).to_be());
    wfifoset(pe.fd, encrypt(pe.fd) as usize);
    0
}

// ─── clif_playsound ───────────────────────────────────────────────────────────

/// Send a positional sound effect to all nearby clients, given entity fields.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_playsound_entity(id: u32, m: u16, x: u16, y: u16, bl_type: u8, sound: i32) -> i32 {
    let mut buf2 = [0u8; 32];

    buf2[0] = 0xAA;
    buf_put_be16(&mut buf2, 1, 0x14);
    buf2[3] = 0x19;
    buf2[4] = 0x03;
    buf_put_be16(&mut buf2, 5, 3);
    buf_put_be16(&mut buf2, 7, sound as u16);
    buf2[9] = 100;
    buf_put_be16(&mut buf2, 10, 4);
    buf_put_be32(&mut buf2, 12, id);
    buf2[16] = 1;
    buf2[17] = 0;
    buf2[18] = 2;
    buf2[19] = 2;
    buf_put_be16(&mut buf2, 20, 4);
    buf2[22] = 0;

    clif_send(buf2.as_ptr(), 32, BroadcastSrc { id, m, x, y, bl_type }, SAMEAREA);
    0
}

// ─── ignorelist_add ───────────────────────────────────────────────────────────

/// Add a player name to the ignore list (no-op if already present).
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn ignorelist_add(pe: &PlayerEntity, name: *const i8) -> i32 {
    // Check if name is already on the list
    let mut current = pe.read().IgnoreList;
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

    let old_head = pe.read().IgnoreList;
    (*new_node).Next = old_head;
    pe.write().IgnoreList = new_node;
    0
}

// ─── ignorelist_remove ────────────────────────────────────────────────────────

/// Remove a player name from the ignore list.
///
/// Returns 0 on success, 1 if not found, 2 if list is empty.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn ignorelist_remove(pe: &PlayerEntity, name: *const i8) -> i32 {
    if pe.read().IgnoreList.is_null() {
        return 2;
    }

    let mut current = pe.read().IgnoreList;
    let mut prev: *mut SdIgnoreList = std::ptr::null_mut();

    while !current.is_null() {
        if strcasecmp_cstr((*current).name.as_ptr(), name) == 0 {
            // Found: unlink
            if !prev.is_null() {
                (*prev).Next = (*current).Next;
            } else {
                // Head-node removal: advance list pointer before freeing
                pe.write().IgnoreList = (*current).Next;
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
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_isignore(
    pe: &PlayerEntity,
    dst_pe: &PlayerEntity,
) -> i32 {
    // Check if pe's name is in dst_pe's ignore list
    let sd_name_cstr = std::ffi::CString::new(pe.read().player.identity.name.as_str()).unwrap_or_default();
    let dst_name_cstr = std::ffi::CString::new(dst_pe.read().player.identity.name.as_str()).unwrap_or_default();
    let mut current = dst_pe.read().IgnoreList;
    while !current.is_null() {
        if strcasecmp_cstr((*current).name.as_ptr(), sd_name_cstr.as_ptr()) == 0 {
            return 0;
        }
        current = (*current).Next;
    }

    // Check if dst_pe's name is in pe's ignore list
    let mut current = pe.read().IgnoreList;
    while !current.is_null() {
        if strcasecmp_cstr((*current).name.as_ptr(), dst_name_cstr.as_ptr()) == 0 {
            return 0;
        }
        current = (*current).Next;
    }

    1
}

// ─── canwhisper ───────────────────────────────────────────────────────────────

/// Check whether `sd` is allowed to whisper `dst_sd`.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn canwhisper(
    pe: &PlayerEntity,
    dst_pe: &PlayerEntity,
) -> i32 {
    let (uflags, gm_level) = {
        let g = pe.read();
        (g.uFlags, g.player.identity.gm_level)
    };
    let dst_whisper_flag = dst_pe.read().player.appearance.setting_flags & FLAG_WHISPER;

    if uflags & U_FLAG_SILENCED != 0 || (gm_level == 0 && dst_whisper_flag == 0) {
        return 0;
    } else if gm_level == 0 {
        return clif_isignore(pe, dst_pe);
    }

    1
}

// ─── clif_sendgroupmessage ────────────────────────────────────────────────────

/// Send a party chat message to all group members.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendgroupmessage(
    pe: &PlayerEntity,
    msg: *mut u8,
    msglen: i32,
) -> i32 {
    let (uflags, sd_m, gm_level, groupid, group_count) = {
        let g = pe.read();
        (g.uFlags, g.m, g.player.identity.gm_level, g.groupid, g.group_count)
    };

    if uflags & U_FLAG_SILENCED != 0 {
        clif_sendbluemessage(pe, c"You are silenced.".as_ptr());
        return 0;
    }

    if (*raw_map_ptr().add(sd_m as usize)).cantalk == 1 && gm_level == 0 {
        clif_sendminitext(pe, c"Your voice is swept away by a strange wind.".as_ptr());
        return 0;
    }

    let mut message = [0i8; 256];
    let copy_len = (msglen as usize).min(255);
    std::ptr::copy_nonoverlapping(msg as *const i8, message.as_mut_ptr(), copy_len);

    let buf2 = format_chat_prefix(pe, b"[!", b"]", &message[..copy_len]);
    let buf2_c = std::ffi::CString::new(buf2).unwrap_or_default();

    let base = groupid as usize * 256;
    let grp = groups();
    for i in 0..group_count as usize {
        let idx = base + i;
        if idx >= grp.len() { break; }
        let tsd = match session_get_data_checked(SessionId::from_raw(grp[idx] as i32)) {
            Some(a) => a,
            None => continue,
        };
        if clif_isignore(pe, tsd.as_ref()) != 0 {
            clif_sendmsg(tsd.as_ref(), 11, buf2_c.as_ptr());
        }
    }
    0
}

// ─── clif_sendsubpathmessage ──────────────────────────────────────────────────

/// Send a sub-path (class channel) chat message.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendsubpathmessage(
    pe: &PlayerEntity,
    msg: *mut u8,
    msglen: i32,
) -> i32 {
    let (uflags, sd_m, gm_level, sd_class) = {
        let g = pe.read();
        (g.uFlags, g.m, g.player.identity.gm_level, g.player.progression.class)
    };

    if uflags & U_FLAG_SILENCED != 0 {
        clif_sendbluemessage(pe, c"You are silenced.".as_ptr());
        return 0;
    }

    if (*raw_map_ptr().add(sd_m as usize)).cantalk == 1 && gm_level == 0 {
        clif_sendminitext(pe, c"Your voice is swept away by a strange wind.".as_ptr());
        return 0;
    }

    let mut message = [0i8; 256];
    let copy_len = (msglen as usize).min(255);
    std::ptr::copy_nonoverlapping(msg as *const i8, message.as_mut_ptr(), copy_len);

    let buf2 = format_chat_prefix(pe, b"<@", b">", &message[..copy_len]);
    let buf2_c = std::ffi::CString::new(buf2).unwrap_or_default();

    for i in 0..crate::session::get_fd_max() {
        let fd = SessionId::from_raw(i);
        if !session_exists(fd) { continue; }
        let tsd = match session_get_data(fd) { Some(a) => a, None => continue };
        let (tsd_class, tsd_subpath_chat) = {
            let r = tsd.read();
            (r.player.progression.class, r.player.social.subpath_chat)
        };
        if tsd_class == sd_class && tsd_subpath_chat != 0
            && clif_isignore(pe, tsd.as_ref()) != 0 {
                clif_sendmsg(tsd.as_ref(), 11, buf2_c.as_ptr());
            }
    }
    0
}

// ─── clif_sendclanmessage ─────────────────────────────────────────────────────

/// Send a clan chat message to all clan members.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendclanmessage(
    pe: &PlayerEntity,
    msg: *mut u8,
    msglen: i32,
) -> i32 {
    let (uflags, sd_m, gm_level, sd_clan) = {
        let g = pe.read();
        (g.uFlags, g.m, g.player.identity.gm_level, g.player.social.clan)
    };

    if uflags & U_FLAG_SILENCED != 0 {
        clif_sendbluemessage(pe, c"You are silenced.".as_ptr());
        return 0;
    }

    if (*raw_map_ptr().add(sd_m as usize)).cantalk == 1 && gm_level == 0 {
        clif_sendminitext(pe, c"Your voice is swept away by a strange wind.".as_ptr());
        return 0;
    }

    let mut message = [0i8; 256];
    let copy_len = (msglen as usize).min(255);
    std::ptr::copy_nonoverlapping(msg as *const i8, message.as_mut_ptr(), copy_len);

    let buf2 = format_chat_prefix(pe, b"<!", b">", &message[..copy_len]);
    let buf2_c = std::ffi::CString::new(buf2).unwrap_or_default();

    for i in 0..crate::session::get_fd_max() {
        let fd = SessionId::from_raw(i);
        if !session_exists(fd) { continue; }
        let tsd = match session_get_data(fd) { Some(a) => a, None => continue };
        let (tsd_clan, tsd_clan_chat) = {
            let r = tsd.read();
            (r.player.social.clan, r.player.social.clan_chat)
        };
        if tsd_clan == sd_clan && tsd_clan_chat != 0
            && clif_isignore(pe, tsd.as_ref()) != 0 {
                clif_sendmsg(tsd.as_ref(), 12, buf2_c.as_ptr());
            }
    }
    0
}

// ─── clif_sendnovicemessage ───────────────────────────────────────────────────

/// Send a novice/tutor channel message.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendnovicemessage(
    pe: &PlayerEntity,
    msg: *mut u8,
    msglen: i32,
) -> i32 {
    let (uflags, sd_m, gm_level, sd_class, sd_mark, sd_tutor) = {
        let g = pe.read();
        (g.uFlags, g.m, g.player.identity.gm_level, g.player.progression.class, g.player.progression.mark, g.player.social.tutor)
    };

    if uflags & U_FLAG_SILENCED != 0 {
        clif_sendbluemessage(pe, c"You are silenced.".as_ptr());
        return 0;
    }

    if (*raw_map_ptr().add(sd_m as usize)).cantalk == 1 && gm_level == 0 {
        clif_sendminitext(pe, c"Your voice is swept away by a strange wind.".as_ptr());
        return 0;
    }

    let mut message = [0i8; 256];
    let copy_len = (msglen as usize).min(255);
    std::ptr::copy_nonoverlapping(msg as *const i8, message.as_mut_ptr(), copy_len);
    let msg_str = i8_slice_to_str(&message[..copy_len]);

    let class_str = classdb_name(sd_class as i32, sd_mark as i32);

    let name_str = pe.read().player.identity.name.clone();
    let buf2 = format!(
        "[Novice]({}) {}> {}\0",
        class_str, name_str, msg_str
    );

    // Non-tutors get a copy of their own message (so it appears on their screen)
    if sd_tutor == 0 {
        clif_sendmsg(pe, 11, buf2.as_ptr() as *const i8);
    }

    for i in 0..crate::session::get_fd_max() {
        let fd = SessionId::from_raw(i);
        if !session_exists(fd) { continue; }
        let tsd = match session_get_data(fd) { Some(a) => a, None => continue };
        let (tsd_tutor, tsd_tgm, tsd_novice_chat) = {
            let r = tsd.read();
            (r.player.social.tutor, r.player.identity.gm_level, r.player.social.novice_chat)
        };
        if (tsd_tutor != 0 || tsd_tgm > 0) && tsd_novice_chat != 0
            && clif_isignore(pe, tsd.as_ref()) != 0 {
                clif_sendmsg(tsd.as_ref(), 12, buf2.as_ptr() as *const i8);
            }
    }
    0
}

// ─── clif_parsewisp ───────────────────────────────────────────────────────────

/// Parse an incoming whisper packet and dispatch it.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parsewisp(pe: &PlayerEntity) -> i32 {
    use super::packet::{rfifob, rfifop, rfiforest, rfifow};

    let (whisper_flag, gm_level, sd_m, uflags, sd_clan, clan_chat, group_count, sd_class, sd_tutor, sd_level) = {
        let g = pe.read();
        (
            g.player.appearance.setting_flags & FLAG_WHISPER,
            g.player.identity.gm_level,
            g.m,
            g.uFlags,
            g.player.social.clan,
            g.player.social.clan_chat,
            g.group_count,
            g.player.progression.class,
            g.player.social.tutor,
            g.player.progression.level,
        )
    };

    if whisper_flag == 0 && gm_level == 0 {
        clif_sendbluemessage(pe, c"You have whispering turned off.".as_ptr());
        return 0;
    }

    if uflags & U_FLAG_SILENCED != 0 {
        clif_sendbluemessage(pe, c"You are silenced.".as_ptr());
        return 0;
    }

    if (*raw_map_ptr().add(sd_m as usize)).cantalk == 1 && gm_level == 0 {
        clif_sendminitext(pe, c"Your voice is swept away by a strange wind.".as_ptr());
        return 0;
    }

    let dstlen = rfifob(pe.fd, 5) as usize;
    let msglen = rfifob(pe.fd, 6 + dstlen) as usize;

    // rfifow already returns host-order u16; no additional swap needed
    let pkt_size = rfifow(pe.fd, 1) as usize;

    if msglen > 80
        || dstlen > 80
        || dstlen > rfiforest(pe.fd) as usize
        || dstlen > pkt_size
        || msglen > rfiforest(pe.fd) as usize
        || msglen > pkt_size
    {
        let mut sd_name_buf: Vec<u8> = pe.read().player.identity.name.as_bytes().to_vec();
        sd_name_buf.push(0);
        clif_Hacker(sd_name_buf.as_mut_ptr() as *mut i8, c"Whisper packet".as_ptr());
        return 0;
    }

    let mut dst_name = [0u8; 100];
    let mut msg_buf = [0u8; 100];

    let src_dst = rfifop(pe.fd, 6);
    std::ptr::copy_nonoverlapping(src_dst, dst_name.as_mut_ptr(), dstlen.min(99));

    let src_msg = rfifop(pe.fd, 7 + dstlen);
    std::ptr::copy_nonoverlapping(src_msg, msg_buf.as_mut_ptr(), msglen.min(80));
    msg_buf[80] = 0;

    let dst_name_c = dst_name.as_ptr() as *const i8;
    let msg_c = msg_buf.as_ptr() as *const i8;

    // Build null-terminated CStrings for the player names used in C API calls.
    let sd_name_cstr = std::ffi::CString::new(pe.read().player.identity.name.as_str()).unwrap_or_default();

    // "!" → clan chat
    if dst_name[0] == b'!' && dst_name[1] == 0 {
        if sd_clan == 0 {
            clif_sendbluemessage(pe, c"You are not in a clan".as_ptr());
        } else if clan_chat != 0 {
            crate::game::scripting::doscript_strings(c"characterLog".as_ptr(), c"clanChatLog".as_ptr(), &[sd_name_cstr.as_ptr(), msg_c]);
            clif_sendclanmessage(pe, rfifop(pe.fd, 7 + dstlen) as *mut u8, msglen as i32);
        } else {
            clif_sendbluemessage(pe, c"Clan chat is off.".as_ptr());
        }
    // "!!" → group chat
    } else if dst_name[0] == b'!' && dst_name[1] == b'!' && dst_name[2] == 0 {
        if group_count == 0 {
            clif_sendbluemessage(pe, c"You are not in a group".as_ptr());
        } else {
            crate::game::scripting::doscript_strings(c"characterLog".as_ptr(), c"groupChatLog".as_ptr(), &[sd_name_cstr.as_ptr(), msg_c]);
            clif_sendgroupmessage(pe, rfifop(pe.fd, 7 + dstlen) as *mut u8, msglen as i32);
        }
    // "@" → subpath chat
    } else if dst_name[0] == b'@' && dst_name[1] == 0 {
        if classdb_chat(sd_class as i32) != 0 {
            crate::game::scripting::doscript_strings(c"characterLog".as_ptr(), c"subPathChatLog".as_ptr(), &[sd_name_cstr.as_ptr(), msg_c]);
            clif_sendsubpathmessage(pe, rfifop(pe.fd, 7 + dstlen) as *mut u8, msglen as i32);
        } else {
            clif_sendbluemessage(pe, c"You cannot do that.".as_ptr());
        }
    // "?" → novice chat
    } else if dst_name[0] == b'?' && dst_name[1] == 0 {
        if sd_tutor == 0 && gm_level == 0 {
            if sd_level < 99 {
                crate::game::scripting::doscript_strings(c"characterLog".as_ptr(), c"noviceChatLog".as_ptr(), &[sd_name_cstr.as_ptr(), msg_c]);
                clif_sendnovicemessage(pe, rfifop(pe.fd, 7 + dstlen) as *mut u8, msglen as i32);
            } else {
                clif_sendbluemessage(pe, c"You cannot do that.".as_ptr());
            }
        } else {
            crate::game::scripting::doscript_strings(c"characterLog".as_ptr(), c"noviceChatLog".as_ptr(), &[sd_name_cstr.as_ptr(), msg_c]);
            clif_sendnovicemessage(pe, rfifop(pe.fd, 7 + dstlen) as *mut u8, msglen as i32);
        }
    // named whisper
    } else {
        let dst_sd = map_name2sd(dst_name_c);
        if dst_sd.is_null() {
            let target = std::ffi::CStr::from_ptr(dst_name_c).to_string_lossy();
            let nf = format!("{} is nowhere to be found.\0", target);
            clif_sendbluemessage(pe, nf.as_ptr() as *const i8);
        } else {
            let dst_id = (*dst_sd).id;
            if let Some(dst_pe_arc) = crate::game::map_server::map_id2sd_pc(dst_id) {
                if canwhisper(pe, dst_pe_arc.as_ref()) != 0 {
                    let (dst_afk, dst_opt_flags) = {
                        let dg = dst_pe_arc.read();
                        (dg.afk, dg.optFlags)
                    };
                    let dst_name_cstr = std::ffi::CString::new(dst_pe_arc.read().player.identity.name.as_str()).unwrap_or_default();
                    if dst_afk != 0 {
                        let afk_msg_ptr = dst_pe_arc.read().afkmessage.as_ptr();
                        crate::game::scripting::doscript_strings(c"characterLog".as_ptr(), c"whisperLog".as_ptr(), &[dst_name_cstr.as_ptr(), sd_name_cstr.as_ptr(), msg_c]);

                        clif_sendwisp(dst_pe_arc.as_ref(), sd_name_cstr.as_ptr(), msg_c);
                        // afk message needs a stable pointer — copy it out
                        let afk_msg_bytes = dst_pe_arc.read().afkmessage;
                        clif_sendwisp(dst_pe_arc.as_ref(), dst_name_cstr.as_ptr(), afk_msg_bytes.as_ptr());

                        if gm_level == 0 && dst_opt_flags & OPT_FLAG_STEALTH != 0 {
                            // don't reveal their presence
                        } else {
                            let mut dst_name_buf: Vec<u8> = dst_pe_arc.read().player.identity.name.as_bytes().to_vec();
                            dst_name_buf.push(0);
                            clif_retrwisp(pe, dst_name_buf.as_mut_ptr() as *mut i8, msg_buf.as_mut_ptr() as *mut i8);
                            let mut afk_msg_buf: Vec<u8> = afk_msg_bytes.iter().map(|&b| b as u8).collect();
                            if let Some(z) = afk_msg_buf.iter().position(|&b| b == 0) { afk_msg_buf.truncate(z); }
                            afk_msg_buf.push(0);
                            clif_retrwisp(pe, dst_name_buf.as_mut_ptr() as *mut i8, afk_msg_buf.as_mut_ptr() as *mut i8);
                        }
                        let _ = afk_msg_ptr;
                    } else {
                        crate::game::scripting::doscript_strings(c"characterLog".as_ptr(), c"whisperLog".as_ptr(), &[dst_name_cstr.as_ptr(), sd_name_cstr.as_ptr(), msg_c]);

                        clif_sendwisp(dst_pe_arc.as_ref(), sd_name_cstr.as_ptr(), msg_c);

                        if gm_level == 0 && dst_opt_flags & OPT_FLAG_STEALTH != 0 {
                            let target = std::ffi::CStr::from_ptr(dst_name_c).to_string_lossy();
                            let nf = format!("{} is nowhere to be found.\0", target);
                            clif_sendbluemessage(pe, nf.as_ptr() as *const i8);
                        } else {
                            let mut dst_name_buf: Vec<u8> = dst_pe_arc.read().player.identity.name.as_bytes().to_vec();
                            dst_name_buf.push(0);
                            clif_retrwisp(pe, dst_name_buf.as_mut_ptr() as *mut i8, msg_buf.as_mut_ptr() as *mut i8);
                        }
                    }
                } else {
                    clif_sendbluemessage(pe, c"They cannot hear you right now.".as_ptr());
                }
            } else {
                let target = std::ffi::CStr::from_ptr(dst_name_c).to_string_lossy();
                let nf = format!("{} is nowhere to be found.\0", target);
                clif_sendbluemessage(pe, nf.as_ptr() as *const i8);
            }
        }
    }
    0
}

// ─── clif_sendsay ─────────────────────────────────────────────────────────────

/// Broadcast a player say/shout and fire NPC speech callbacks.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendsay(
    pe: &PlayerEntity,
    msg: *mut i8,
    msglen: i32,
    say_type: i32,
) -> i32 {
    let src = std::slice::from_raw_parts(msg as *const u8, (msglen as usize).min(254));
    {
        let mut g = pe.write();
        g.talktype = if say_type == 1 { 1 } else { 0 };
        let dst = std::slice::from_raw_parts_mut(g.speech.as_mut_ptr() as *mut u8, 255);
        dst[..src.len()].copy_from_slice(src);
        dst[src.len()] = 0;
    }

    let skills: Vec<u16> = pe.read().player.spells.skills.clone();
    for skill in skills.iter().take(MAX_SPELLS) {
        if *skill > 0 {
            let spell = magic_db::search(*skill as i32);
            let yname = crate::game::scripting::carray_to_str(&spell.yname);
            sl_doscript_simple(yname, Some("on_say"), pe.id);
        }
    }
    sl_doscript_simple("onSay", None, pe.id);
    0
}

// ─── clif_sendscriptsay ───────────────────────────────────────────────────────

/// Broadcast a player's scripted say and log it; handles language channels.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendscriptsay(
    pe: &PlayerEntity,
    msg: *const i8,
    msglen: i32,
    say_type: i32,
) -> i32 {
    let (sd_m, gm_level, uflags, sd_name, sd_id, sd_x, sd_y) = {
        let g = pe.read();
        (g.m, g.player.identity.gm_level, g.uFlags, g.player.identity.name.clone(), g.player.identity.id, g.x, g.y)
    };
    let namelen = sd_name.len();

    if (*raw_map_ptr().add(sd_m as usize)).cantalk == 1 && gm_level == 0 {
        clif_sendminitext(pe, c"Your voice is swept away by a strange wind.".as_ptr());
        return 0;
    }

    if is_command(&mut *pe.write() as *mut MapSessionData, msg, msglen) != 0 {
        return 0;
    }

    if uflags & U_FLAG_SILENCED != 0 {
        clif_sendminitext(pe, c"Shut up for now. ^^".as_ptr());
        return 0;
    }

    let msg_bytes = std::slice::from_raw_parts(msg as *const u8, msglen as usize);

    if say_type >= 10 {
        let ext_namelen = namelen + 4; // prefix like "EN[" + name + "]"

        if !session_exists(pe.fd) {
            return 0;
        }

        let buf_size = 16 + ext_namelen + msglen as usize;
        let mut buf = vec![0u8; buf_size];

        buf[0] = 0xAA;
        buf_put_be16(&mut buf, 1, (10 + ext_namelen + msglen as usize) as u16);
        buf[3] = 0x0D;
        buf[5] = say_type as u8;
        buf_put_be32(&mut buf, 6, sd_id);
        buf[10] = (ext_namelen + msglen as usize + 2) as u8;

        // Build prefixed name
        let prefixed = match say_type {
            10 => format!("EN[{}]", sd_name),
            11 => format!("ES[{}]", sd_name),
            12 => format!("FR[{}]", sd_name),
            13 => format!("CN[{}]", sd_name),
            14 => format!("PT[{}]", sd_name),
            15 => format!("ID[{}]", sd_name),
            _  => sd_name.to_string(),
        };
        let pname = prefixed.as_bytes();
        buf[11..11 + pname.len()].copy_from_slice(pname);
        buf[11 + pname.len()] = b':';
        buf[12 + pname.len()] = b' ';
        buf[13 + pname.len()..13 + pname.len() + msglen as usize].copy_from_slice(msg_bytes);

        clif_send(buf.as_ptr(), buf_size as i32, BroadcastSrc { id: pe.id, m: sd_m, x: sd_x, y: sd_y, bl_type: BL_PC as u8 }, SAMEAREA);
    } else {
        if !session_exists(pe.fd) {
            return 0;
        }

        let buf_size = 16 + namelen + msglen as usize;
        let mut buf = vec![0u8; buf_size];

        buf[0] = 0xAA;
        buf_put_be16(&mut buf, 1, (10 + namelen + msglen as usize) as u16);
        buf[3] = 0x0D;
        buf[5] = say_type as u8;
        buf_put_be32(&mut buf, 6, sd_id);
        buf[10] = (namelen + msglen as usize + 2) as u8;

        let name_bytes = sd_name.as_bytes();
        buf[11..11 + namelen].copy_from_slice(name_bytes);
        buf[11 + namelen] = if say_type == 1 { b'!' } else { b':' };
        buf[12 + namelen] = b' ';
        buf[13 + namelen..13 + namelen + msglen as usize].copy_from_slice(msg_bytes);

        let send_target = if say_type == 1 { SAMEMAP } else { SAMEAREA };
        clif_send(buf.as_ptr(), buf_size as i32, BroadcastSrc { id: pe.id, m: sd_m, x: sd_x, y: sd_y, bl_type: BL_PC as u8 }, send_target);
    }

    // Copy msg to speech
    let src = std::slice::from_raw_parts(msg as *const u8, (msglen as usize).min(254));
    {
        let mut g = pe.write();
        let dst = std::slice::from_raw_parts_mut(g.speech.as_mut_ptr() as *mut u8, 255);
        dst[..src.len()].copy_from_slice(src);
        dst[src.len()] = 0;
    }

    let m = sd_m as i32;
    let bx = sd_x as i32;
    let by = sd_y as i32;
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let slot = &*raw_map_ptr().add(m as usize);
        let area = if say_type == 1 { AreaType::SameMap } else { AreaType::Area };
        let ids = block_grid::ids_in_area(grid, bx, by, area, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if let Some(npc_arc) = crate::game::map_server::map_id2npc_ref(id) {
                if say_type == 1 {
                    clif_sendnpcyell_inner(&*npc_arc.read(), msg, pe);
                } else {
                    clif_sendnpcsay_inner(&*npc_arc.read(), msg, pe);
                }
            } else if let Some(mob_arc) = crate::game::map_server::map_id2mob_ref(id) {
                if say_type == 1 {
                    clif_sendmobyell_inner(&*mob_arc.read(), msg, pe);
                } else {
                    clif_sendmobsay_inner(&*mob_arc.read(), msg, pe);
                }
            }
        }
    }
    0
}

// ─── clif_sendnpcsay ──────────────────────────────────────────────────────────

/// foreachinarea callback: fire NPC speech handler if player is nearby.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendnpcsay_inner(nd: *const NpcData, _msg: *const i8, pe: &PlayerEntity) -> i32 {
    if (*nd).subtype != SCRIPT { return 0; }

    let (sd_x, sd_y) = { let g = pe.read(); (g.x, g.y) };
    if clif_distance((*nd).x, (*nd).y, sd_x, sd_y) <= 10 {
        pe.write().last_click = (*nd).id;
        sl_async_freeco(pe);
        sl_doscript_coro_2(crate::game::scripting::carray_to_str(&(*nd).name), Some("onSayClick"), pe.id, (*nd).id);
    }
    0
}

// ─── clif_sendmobsay ──────────────────────────────────────────────────────────

/// foreachinarea callback: mob speech handler (currently a no-op in C).
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendmobsay_inner(_md: *const MobSpawnData, _msg: *const i8, _pe: &PlayerEntity) -> i32 {
    0
}

// ─── clif_sendnpcyell ─────────────────────────────────────────────────────────

/// foreachinarea callback: fire NPC speech handler (yell range = 20).
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendnpcyell_inner(nd: *const NpcData, _msg: *const i8, pe: &PlayerEntity) -> i32 {
    if (*nd).subtype != SCRIPT { return 0; }

    let (sd_x, sd_y) = { let g = pe.read(); (g.x, g.y) };
    if clif_distance((*nd).x, (*nd).y, sd_x, sd_y) <= 20 {
        pe.write().last_click = (*nd).id;
        sl_async_freeco(pe);
        sl_doscript_coro_2(crate::game::scripting::carray_to_str(&(*nd).name), Some("onSayClick"), pe.id, (*nd).id);
    }
    0
}

// ─── clif_sendmobyell ─────────────────────────────────────────────────────────

/// foreachinarea callback: mob yell handler (currently a no-op in C).
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendmobyell_inner(_md: *const MobSpawnData, _msg: *const i8, _pe: &PlayerEntity) -> i32 {
    0
}

// ─── clif_speak ───────────────────────────────────────────────────────────────

/// Send an NPC/object speech-bubble packet to one player.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_speak_inner(viewer_fd: SessionId, msg: *const i8, speaker_id: u32, speak_type: i32) -> i32 {
    if speaker_id == 0 { return 0; }

    let len = libc_strlen(msg);

    if !session_alive(viewer_fd) { return 0; }

    wfifohead(viewer_fd, len + 11);
    wfifob(viewer_fd, 5, speak_type as u8);
    wfifol(viewer_fd, 6, speaker_id.to_be());
    wfifob(viewer_fd, 10, len as u8);
    let hdr_len = (len + 8) as u16;
    wfifoheader(viewer_fd, 0x0D, hdr_len);
    let dst = wfifop(viewer_fd, 11);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(msg as *const u8, dst, len);
    }
    wfifoset(viewer_fd, encrypt(viewer_fd) as usize);
    0
}

// ─── clif_parseignore ─────────────────────────────────────────────────────────

/// Handle an ignore-list add/remove packet from the client.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parseignore(pe: &PlayerEntity) -> i32 {
    use super::packet::rfifob;

    let icmd = rfifob(pe.fd, 5);
    let nlen = rfifob(pe.fd, 6) as usize;

    if nlen <= 16 {
        let mut name_buf = [0i8; 32];
        match icmd {
            0x02 => {
                // Add
                let src = super::packet::rfifop(pe.fd, 7);
                std::ptr::copy_nonoverlapping(src as *const i8, name_buf.as_mut_ptr(), nlen.min(31));
                ignorelist_add(pe, name_buf.as_ptr());
            }
            0x03 => {
                // Remove
                let src = super::packet::rfifop(pe.fd, 7);
                std::ptr::copy_nonoverlapping(src as *const i8, name_buf.as_mut_ptr(), nlen.min(31));
                ignorelist_remove(pe, name_buf.as_ptr());
            }
            _ => {}
        }
    }
    0
}

// ─── clif_parsesay ────────────────────────────────────────────────────────────

/// Parse an incoming say packet from the client.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parsesay(pe: &PlayerEntity) -> i32 {
    use super::packet::{rfifob, rfifop};

    let msg = rfifop(pe.fd, 7) as *const i8;

    let talktype = rfifob(pe.fd, 5);
    pe.write().talktype = talktype;

    if talktype > 1 || rfifob(pe.fd, 6) > 100 {
        clif_sendminitext(pe, c"I just told the GM on you!".as_ptr());
        tracing::warn!("[chat] Talk Hacker: {}", pe.read().player.identity.name);
        return 0;
    }

    if is_command(&mut *pe.write() as *mut MapSessionData, msg, rfifob(pe.fd, 6) as i32) != 0 {
        return 0;
    }

    // Copy msg into speech
    let msglen = rfifob(pe.fd, 6) as usize;
    let src = std::slice::from_raw_parts(msg as *const u8, msglen.min(254));
    {
        let mut g = pe.write();
        let dst = std::slice::from_raw_parts_mut(g.speech.as_mut_ptr() as *mut u8, 255);
        dst[..src.len()].copy_from_slice(src);
        dst[src.len()] = 0;
    }

    let skills: Vec<u16> = pe.read().player.spells.skills.clone();
    for skill in skills.iter().take(MAX_SPELLS) {
        if *skill > 0 {
            let spell = magic_db::search(*skill as i32);
            let yname = crate::game::scripting::carray_to_str(&spell.yname);
            sl_doscript_simple(yname, Some("on_say"), pe.id);
        }
    }
    sl_doscript_simple("onSay", None, pe.id);
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
        if !x.eq_ignore_ascii_case(&y) { return 1; }
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
unsafe fn format_chat_prefix(pe: &PlayerEntity, open: &[u8], close: &[u8], msg: &[i8]) -> Vec<u8> {
    let (name, sd_class, sd_mark) = {
        let g = pe.read();
        (g.player.identity.name.clone(), g.player.progression.class, g.player.progression.mark)
    };
    let class_str = classdb_name(sd_class as i32, sd_mark as i32);
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

/// Manhattan distance between two coordinate pairs.
fn clif_distance(x1: u16, y1: u16, x2: u16, y2: u16) -> i32 {
    let dx = (x1 as i32) - (x2 as i32);
    let dy = (y1 as i32) - (y2 as i32);
    dx.abs() + dy.abs()
}

/// Retrieve session data for fd, returning None if session does not exist.
#[inline]
fn session_get_data_checked(fd: SessionId) -> Option<std::sync::Arc<crate::game::player::entity::PlayerEntity>> {
    if session_exists(fd) {
        session_get_data(fd)
    } else {
        None
    }
}
