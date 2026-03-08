//! Port of the chat/social helpers from `c_src/map_parse.c`.
//!
//! Covers broadcast, whisper, say, ignore list, and NPC speech callbacks.
//! Functions declared `#[no_mangle] pub unsafe extern "C"` so they remain
//! callable from any remaining C code that has not yet been ported.

#![allow(non_snake_case, clippy::wildcard_imports)]

use std::ffi::{c_char, c_int, c_uint, c_ulong, c_void};

use crate::database::map_db::{BlockList, MapData};
use crate::ffi::map_db::map;
use crate::ffi::session::{
    rust_session_exists, rust_session_get_data, rust_session_set_eof,
};
use crate::game::npc::NpcData;
use crate::game::pc::{
    MapSessionData, SdIgnoreList,
    FLAG_ADVICE, FLAG_SHOUT, FLAG_WHISPER,
    OPT_FLAG_STEALTH, U_FLAG_SILENCED,
    MAP_WHISPFAIL,
    BL_PC, BL_MOB, BL_NPC,
    groups,
    map_msg,
};
use crate::servers::char::charstatus::MAX_SPELLS;

use super::packet::{
    encrypt, wfifob, wfifohead, wfifol, wfifoset, wfifow, wfifoheader,
    clif_send, map_foreachinarea,
    AREA, SAMEMAP, SAMEAREA, SELF,
};

// ─── External C globals ──────────────────────────────────────────────────────

extern "C" {
    static fd_max: c_int;
}

// ─── External C functions not yet ported ─────────────────────────────────────

extern "C" {
    fn map_name2sd(name: *const c_char) -> *mut MapSessionData;
    fn clif_sendaction(bl: *mut BlockList, action: c_int, unused: c_int, extra: c_int) -> c_int;
    fn rust_classdb_name(id: c_int, rank: c_int) -> *mut c_char;
    fn rust_classdb_chat(id: c_int) -> c_int;
    fn rust_is_command(sd: *mut MapSessionData, p: *const c_char, len: c_int) -> c_int;
    fn rust_magicdb_yname(id: c_int) -> *mut c_char;
    #[link_name = "rust_sl_async_freeco"]
    fn sl_async_freeco(sd: *mut c_void) -> c_int;
    fn Sql_EscapeString(handle: *mut c_void, out: *mut c_char, src: *const c_char);
    fn clif_Hacker(name: *mut c_char, reason: *const c_char) -> c_int;
}

/// Dispatch a Lua event with a single block_list argument.
#[cfg(not(test))]
#[allow(dead_code)]
unsafe fn sl_doscript_simple(root: *const std::ffi::c_char, method: *const std::ffi::c_char, bl: *mut crate::database::map_db::BlockList) -> std::ffi::c_int {
    crate::game::scripting::doscript_blargs(root, method, &[bl as *mut _])
}

/// Dispatch a Lua event with two block_list arguments.
#[cfg(not(test))]
#[allow(dead_code)]
unsafe fn sl_doscript_2(root: *const std::ffi::c_char, method: *const std::ffi::c_char, bl1: *mut crate::database::map_db::BlockList, bl2: *mut crate::database::map_db::BlockList) -> std::ffi::c_int {
    crate::game::scripting::doscript_blargs(root, method, &[bl1 as *mut _, bl2 as *mut _])
}


use crate::game::map_server::sql_handle;

// NPC subtype constant (from map_server.h)
const SCRIPT: u8 = 0;

// ─── inline helper: map_isloaded ─────────────────────────────────────────────

#[inline]
unsafe fn map_isloaded(m: usize) -> bool {
    !(*map.add(m)).registry.is_null()
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
/// Mirrors `clif_sendguidespecific` from `c_src/map_parse.c` ~line 690.
#[no_mangle]
pub unsafe extern "C" fn clif_sendguidespecific(sd: *mut MapSessionData, guide: c_int) -> c_int {
    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

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
/// Mirrors `clif_broadcast_sub` from `c_src/map_parse.c` ~line 710.
#[no_mangle]
pub unsafe extern "C" fn clif_broadcast_sub(bl: *mut BlockList, mut ap: ...) -> c_int {
    let sd = bl as *mut MapSessionData;
    if sd.is_null() { return 0; }

    let msg: *const c_char = ap.arg::<*const c_char>();

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

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
    let dst = crate::ffi::session::rust_session_wdata_ptr((*sd).fd, 8);
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
/// Mirrors `clif_gmbroadcast_sub` from `c_src/map_parse.c` ~line 740.
#[no_mangle]
pub unsafe extern "C" fn clif_gmbroadcast_sub(bl: *mut BlockList, mut ap: ...) -> c_int {
    let sd = bl as *mut MapSessionData;
    if sd.is_null() { return 0; }

    let msg: *const c_char = ap.arg::<*const c_char>();

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    let len = libc_strlen(msg);

    wfifohead((*sd).fd, len + 8);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 3, 0x0A);
    wfifob((*sd).fd, 4, 0x03);
    wfifob((*sd).fd, 5, 0x05);
    wfifow((*sd).fd, 6, (len as u16).to_be());
    let dst = crate::ffi::session::rust_session_wdata_ptr((*sd).fd, 8);
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
/// Mirrors `clif_broadcasttogm_sub` from `c_src/map_parse.c` ~line 768.
#[no_mangle]
pub unsafe extern "C" fn clif_broadcasttogm_sub(bl: *mut BlockList, mut ap: ...) -> c_int {
    let sd = bl as *mut MapSessionData;
    if sd.is_null() { return 0; }

    if (*sd).status.gm_level != 0 {
        let msg: *const c_char = ap.arg::<*const c_char>();

        if rust_session_exists((*sd).fd) == 0 {
            rust_session_set_eof((*sd).fd, 8);
            return 0;
        }

        let len = libc_strlen(msg);

        wfifohead((*sd).fd, len + 8);
        wfifob((*sd).fd, 0, 0xAA);
        wfifob((*sd).fd, 3, 0x0A);
        wfifob((*sd).fd, 4, 0x03);
        wfifob((*sd).fd, 5, 0x05);
        wfifow((*sd).fd, 6, (len as u16).to_be());
        let dst = crate::ffi::session::rust_session_wdata_ptr((*sd).fd, 8);
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
/// Mirrors `clif_broadcast` from `c_src/map_parse.c` ~line 797.
#[no_mangle]
pub unsafe extern "C" fn clif_broadcast(msg: *const c_char, m: c_int) -> c_int {
    if m == -1 {
        for x in 0..65535usize {
            if map_isloaded(x) {
                map_foreachinarea(clif_broadcast_sub, x as c_int, 1, 1, SAMEMAP, BL_PC, msg);
            }
        }
    } else {
        map_foreachinarea(clif_broadcast_sub, m, 1, 1, SAMEMAP, BL_PC, msg);
    }
    0
}

// ─── clif_gmbroadcast ─────────────────────────────────────────────────────────

/// Send a GM broadcast message to all GMs on a map (or all maps if m == -1).
///
/// Mirrors `clif_gmbroadcast` from `c_src/map_parse.c` ~line 811.
#[no_mangle]
pub unsafe extern "C" fn clif_gmbroadcast(msg: *const c_char, m: c_int) -> c_int {
    if m == -1 {
        for x in 0..65535usize {
            if map_isloaded(x) {
                map_foreachinarea(clif_gmbroadcast_sub, x as c_int, 1, 1, SAMEMAP, BL_PC, msg);
            }
        }
    } else {
        map_foreachinarea(clif_gmbroadcast_sub, m, 1, 1, SAMEMAP, BL_PC, msg);
    }
    0
}

// ─── clif_broadcasttogm ───────────────────────────────────────────────────────

/// Send a broadcast message to all GMs on a map (or all maps if m == -1).
///
/// Mirrors `clif_broadcasttogm` from `c_src/map_parse.c` ~line 824.
#[no_mangle]
pub unsafe extern "C" fn clif_broadcasttogm(msg: *const c_char, m: c_int) -> c_int {
    if m == -1 {
        for x in 0..65535usize {
            if map_isloaded(x) {
                map_foreachinarea(clif_broadcasttogm_sub, x as c_int, 1, 1, SAMEMAP, BL_PC, msg);
            }
        }
    } else {
        map_foreachinarea(clif_broadcasttogm_sub, m, 1, 1, SAMEMAP, BL_PC, msg);
    }
    0
}

// ─── clif_guitextsd ───────────────────────────────────────────────────────────

/// Send a GUI text popup to a single player.
///
/// Mirrors `clif_guitextsd` from `c_src/map_parse.c` ~line 4363.
#[no_mangle]
pub unsafe extern "C" fn clif_guitextsd(msg: *const c_char, sd: *mut MapSessionData) -> c_int {
    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    let mlen = libc_strlen(msg);

    wfifohead((*sd).fd, mlen + 8);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 1, 0x00);
    wfifob((*sd).fd, 3, 0x58);
    wfifob((*sd).fd, 5, 0x06);
    wfifow((*sd).fd, 6, (mlen as u16).to_be());
    let dst = crate::ffi::session::rust_session_wdata_ptr((*sd).fd, 8);
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
/// Mirrors `clif_guitext` from `c_src/map_parse.c` ~line 4388.
#[no_mangle]
pub unsafe extern "C" fn clif_guitext(bl: *mut BlockList, mut ap: ...) -> c_int {
    let sd = bl as *mut MapSessionData;
    if sd.is_null() { return 0; }

    let msg: *const c_char = ap.arg::<*const c_char>();

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    let mlen = libc_strlen(msg);

    wfifohead((*sd).fd, mlen + 8);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 1, 0x00);
    wfifob((*sd).fd, 3, 0x58);
    wfifob((*sd).fd, 5, 0x06);
    wfifow((*sd).fd, 6, (mlen as u16).to_be());
    let dst = crate::ffi::session::rust_session_wdata_ptr((*sd).fd, 8);
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
/// Mirrors `clif_parseemotion` from `c_src/map_parse.c` ~line 5867.
#[no_mangle]
pub unsafe extern "C" fn clif_parseemotion(sd: *mut MapSessionData) -> c_int {
    use super::packet::rfifob;
    if (*sd).status.state == 0 {
        clif_sendaction(
            &raw mut (*sd).bl,
            rfifob((*sd).fd, 5) as c_int + 11,
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
/// Mirrors `clif_sendmsg` from `c_src/map_parse.c` ~line 5874.
#[no_mangle]
pub unsafe extern "C" fn clif_sendmsg(
    sd: *mut MapSessionData,
    mut msg_type: c_int,
    buf: *const c_char,
) -> c_int {
    if buf.is_null() { return 0; }

    let advice_flag = (*sd).status.setting_flags & FLAG_ADVICE as u16;
    if msg_type == 99 && advice_flag != 0 {
        msg_type = 11;
    } else if msg_type == 99 {
        return 0;
    }

    let len = libc_strlen(buf);

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    wfifohead((*sd).fd, 8 + len);
    wfifob((*sd).fd, 0, 0xAA);
    wfifow((*sd).fd, 1, ((5 + len) as u16).to_be());
    wfifob((*sd).fd, 3, 0x0A);
    wfifob((*sd).fd, 4, 0x03);
    wfifow((*sd).fd, 5, msg_type as u16);
    wfifob((*sd).fd, 7, len as u8);
    let dst = crate::ffi::session::rust_session_wdata_ptr((*sd).fd, 8);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(buf as *const u8, dst, len);
    }
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_sendminitext ────────────────────────────────────────────────────────

/// Send a mini status-text message to a single player.
///
/// Mirrors `clif_sendminitext` from `c_src/map_parse.c` ~line 5913.
#[no_mangle]
pub unsafe extern "C" fn clif_sendminitext(sd: *mut MapSessionData, msg: *const c_char) -> c_int {
    if sd.is_null() { return 0; }
    if libc_strlen(msg) == 0 { return 0; }
    clif_sendmsg(sd, 3, msg);
    0
}

// ─── clif_sendwisp ────────────────────────────────────────────────────────────

/// Deliver an incoming whisper to the destination player.
///
/// Mirrors `clif_sendwisp` from `c_src/map_parse.c` ~line 5920.
#[no_mangle]
pub unsafe extern "C" fn clif_sendwisp(
    sd: *mut MapSessionData,
    srcname: *const c_char,
    msg: *const c_char,
) -> c_int {
    let msglen = libc_strlen(msg);
    let srclen = libc_strlen(srcname);

    let src_sd = map_name2sd(srcname);
    if src_sd.is_null() { return 0; }

    let class_name = rust_classdb_name((*src_sd).status.class as c_int, (*src_sd).status.mark as c_int);
    let buf2: Vec<u8>;
    let newlen: usize;
    if !class_name.is_null() {
        let cn_str = std::ffi::CStr::from_ptr(class_name).to_bytes();
        // format: `" (classname) "`
        let mut tmp = Vec::with_capacity(cn_str.len() + 6);
        tmp.extend_from_slice(b"\" (");
        tmp.extend_from_slice(cn_str);
        tmp.extend_from_slice(b") ");
        newlen = tmp.len();
        buf2 = tmp;
        // free the CString returned by rust_classdb_name
        drop(std::ffi::CString::from_raw(class_name));
    } else {
        buf2 = b"\" () ".to_vec();
        newlen = buf2.len();
    }

    let mut combined: Vec<u8> = Vec::with_capacity(srclen + newlen + msglen);
    combined.extend_from_slice(std::slice::from_raw_parts(srcname as *const u8, srclen));
    combined.extend_from_slice(&buf2);
    combined.extend_from_slice(std::slice::from_raw_parts(msg as *const u8, msglen));

    if (*map.add((*sd).bl.m as usize)).cantalk == 1 && (*sd).status.gm_level == 0 {
        let cant = b"Your voice is carried away.\0";
        clif_sendminitext(sd, cant.as_ptr() as *const c_char);
        return 0;
    }

    // combined is not null-terminated — clif_sendmsg uses len, not CStr
    // Temporarily null-terminate for C call
    combined.push(0);
    clif_sendmsg(sd, 0, combined.as_ptr() as *const c_char);
    0
}

// ─── clif_retrwisp ────────────────────────────────────────────────────────────

/// Echo a whisper back to the sender (shows "dstname> msg").
///
/// Mirrors `clif_retrwisp` from `c_src/map_parse.c` ~line 5955.
#[no_mangle]
pub unsafe extern "C" fn clif_retrwisp(
    sd: *mut MapSessionData,
    dstname: *mut c_char,
    msg: *mut c_char,
) -> c_int {
    let dst_str = std::ffi::CStr::from_ptr(dstname).to_bytes();
    let msg_str = std::ffi::CStr::from_ptr(msg).to_bytes();

    // format: "dstname> msg\0"
    let mut buf: Vec<u8> = Vec::with_capacity(dst_str.len() + 2 + msg_str.len() + 1);
    buf.extend_from_slice(dst_str);
    buf.extend_from_slice(b"> ");
    buf.extend_from_slice(msg_str);
    buf.push(0);

    clif_sendmsg(sd, 0, buf.as_ptr() as *const c_char);
    0
}

// ─── clif_failwisp ────────────────────────────────────────────────────────────

/// Tell the player their whisper failed.
///
/// Mirrors `clif_failwisp` from `c_src/map_parse.c` ~line 5975.
#[no_mangle]
pub unsafe extern "C" fn clif_failwisp(sd: *mut MapSessionData) -> c_int {
    clif_sendmsg(sd, 0, map_msg[MAP_WHISPFAIL].message.as_ptr() as *const c_char);
    0
}

// ─── clif_sendbluemessage ─────────────────────────────────────────────────────

/// Send a blue (whisper-style type 0) system message.
///
/// Mirrors `clif_sendbluemessage` from `c_src/map_parse.c` ~line 7651.
#[no_mangle]
pub unsafe extern "C" fn clif_sendbluemessage(sd: *mut MapSessionData, msg: *mut c_char) -> c_int {
    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    let mlen = libc_strlen(msg as *const c_char);

    wfifohead((*sd).fd, mlen + 8);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 3, 0x0A);
    wfifob((*sd).fd, 4, 0x03);
    wfifow((*sd).fd, 5, 0u16);
    wfifob((*sd).fd, 7, mlen as u8);
    let dst = crate::ffi::session::rust_session_wdata_ptr((*sd).fd, 8);
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
/// Mirrors `clif_playsound` from `c_src/map_parse.c` ~line 7669.
#[no_mangle]
pub unsafe extern "C" fn clif_playsound(bl: *mut BlockList, sound: c_int) -> c_int {
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
/// Mirrors `ignorelist_add` from `c_src/map_parse.c` ~line 6617.
#[no_mangle]
pub unsafe extern "C" fn ignorelist_add(sd: *mut MapSessionData, name: *const c_char) -> c_int {
    // Check if name is already on the list
    let mut current = (*sd).IgnoreList;
    while !current.is_null() {
        if strcasecmp_cstr((*current).name.as_ptr(), name) == 0 {
            return 1;
        }
        current = (*current).Next;
    }

    // Allocate new node
    let new_node = libc::calloc(1, std::mem::size_of::<SdIgnoreList>()) as *mut SdIgnoreList;
    if new_node.is_null() { return 0; }

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
/// Mirrors `ignorelist_remove` from `c_src/map_parse.c` ~line 6643.
#[no_mangle]
pub unsafe extern "C" fn ignorelist_remove(sd: *mut MapSessionData, name: *const c_char) -> c_int {
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
                libc::free(current as *mut c_void);
            } else {
                // Head-node removal: advance list pointer before freeing
                (*sd).IgnoreList = (*current).Next;
                libc::free(current as *mut c_void);
            }
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
/// Mirrors `clif_isignore` from `c_src/map_parse.c` ~line 6687.
#[no_mangle]
pub unsafe extern "C" fn clif_isignore(
    sd: *mut MapSessionData,
    dst_sd: *mut MapSessionData,
) -> c_int {
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
/// Mirrors `canwhisper` from `c_src/map_parse.c` ~line 6716.
#[no_mangle]
pub unsafe extern "C" fn canwhisper(
    sd: *mut MapSessionData,
    dst_sd: *mut MapSessionData,
) -> c_int {
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
/// Mirrors `clif_sendgroupmessage` from `c_src/map_parse.c` ~line 6458.
#[no_mangle]
pub unsafe extern "C" fn clif_sendgroupmessage(
    sd: *mut MapSessionData,
    msg: *mut u8,
    msglen: c_int,
) -> c_int {
    if sd.is_null() { return 0; }

    if (*sd).uFlags & U_FLAG_SILENCED != 0 {
        let cant = b"You are silenced.\0";
        clif_sendbluemessage(sd, cant.as_ptr() as *mut c_char);
        return 0;
    }

    if (*map.add((*sd).bl.m as usize)).cantalk == 1 && (*sd).status.gm_level == 0 {
        let cant = b"Your voice is swept away by a strange wind.\0";
        clif_sendminitext(sd, cant.as_ptr() as *const c_char);
        return 0;
    }

    let mut message = [0i8; 256];
    let copy_len = (msglen as usize).min(255);
    std::ptr::copy_nonoverlapping(msg as *const i8, message.as_mut_ptr(), copy_len);

    let buf2 = format_chat_prefix(sd, b"[!", b"]", &message[..copy_len]);
    let buf2_c = std::ffi::CString::new(buf2).unwrap_or_default();

    let base = (*sd).groupid as usize * 256;
    for i in 0..(*sd).group_count as usize {
        let idx = base + i;
        if idx >= groups.len() { break; }
        let tsd = rust_session_get_data_checked(groups[idx] as c_int);
        if !tsd.is_null() && clif_isignore(sd, tsd) != 0 {
            clif_sendmsg(tsd as *mut MapSessionData, 11, buf2_c.as_ptr());
        }
    }
    0
}

// ─── clif_sendsubpathmessage ──────────────────────────────────────────────────

/// Send a sub-path (class channel) chat message.
///
/// Mirrors `clif_sendsubpathmessage` from `c_src/map_parse.c` ~line 6493.
#[no_mangle]
pub unsafe extern "C" fn clif_sendsubpathmessage(
    sd: *mut MapSessionData,
    msg: *mut u8,
    msglen: c_int,
) -> c_int {
    if sd.is_null() { return 0; }

    if (*sd).uFlags & U_FLAG_SILENCED != 0 {
        let cant = b"You are silenced.\0";
        clif_sendbluemessage(sd, cant.as_ptr() as *mut c_char);
        return 0;
    }

    if (*map.add((*sd).bl.m as usize)).cantalk == 1 && (*sd).status.gm_level == 0 {
        let cant = b"Your voice is swept away by a strange wind.\0";
        clif_sendminitext(sd, cant.as_ptr() as *const c_char);
        return 0;
    }

    let mut message = [0i8; 256];
    let copy_len = (msglen as usize).min(255);
    std::ptr::copy_nonoverlapping(msg as *const i8, message.as_mut_ptr(), copy_len);

    let buf2 = format_chat_prefix(sd, b"<@", b">", &message[..copy_len]);
    let buf2_c = std::ffi::CString::new(buf2).unwrap_or_default();

    for i in 0..fd_max {
        if rust_session_exists(i) == 0 { continue; }
        let tsd = rust_session_get_data(i) as *mut MapSessionData;
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
/// Mirrors `clif_sendclanmessage` from `c_src/map_parse.c` ~line 6534.
#[no_mangle]
pub unsafe extern "C" fn clif_sendclanmessage(
    sd: *mut MapSessionData,
    msg: *mut u8,
    msglen: c_int,
) -> c_int {
    if (*sd).uFlags & U_FLAG_SILENCED != 0 {
        let cant = b"You are silenced.\0";
        clif_sendbluemessage(sd, cant.as_ptr() as *mut c_char);
        return 0;
    }

    if (*map.add((*sd).bl.m as usize)).cantalk == 1 && (*sd).status.gm_level == 0 {
        let cant = b"Your voice is swept away by a strange wind.\0";
        clif_sendminitext(sd, cant.as_ptr() as *const c_char);
        return 0;
    }

    let mut message = [0i8; 256];
    let copy_len = (msglen as usize).min(255);
    std::ptr::copy_nonoverlapping(msg as *const i8, message.as_mut_ptr(), copy_len);

    let buf2 = format_chat_prefix(sd, b"<!", b">", &message[..copy_len]);
    let buf2_c = std::ffi::CString::new(buf2).unwrap_or_default();

    for i in 0..fd_max {
        if rust_session_exists(i) == 0 { continue; }
        let tsd = rust_session_get_data(i) as *mut MapSessionData;
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
/// Mirrors `clif_sendnovicemessage` from `c_src/map_parse.c` ~line 6574.
#[no_mangle]
pub unsafe extern "C" fn clif_sendnovicemessage(
    sd: *mut MapSessionData,
    msg: *mut u8,
    msglen: c_int,
) -> c_int {
    if (*sd).uFlags & U_FLAG_SILENCED != 0 {
        let cant = b"You are silenced.\0";
        clif_sendbluemessage(sd, cant.as_ptr() as *mut c_char);
        return 0;
    }

    if (*map.add((*sd).bl.m as usize)).cantalk == 1 && (*sd).status.gm_level == 0 {
        let cant = b"Your voice is swept away by a strange wind.\0";
        clif_sendminitext(sd, cant.as_ptr() as *const c_char);
        return 0;
    }

    let mut message = [0i8; 256];
    let copy_len = (msglen as usize).min(255);
    std::ptr::copy_nonoverlapping(msg as *const i8, message.as_mut_ptr(), copy_len);
    let msg_str = i8_slice_to_str(&message[..copy_len]);

    let class_name = rust_classdb_name((*sd).status.class as c_int, (*sd).status.mark as c_int);
    let class_str = if !class_name.is_null() {
        let s = std::ffi::CStr::from_ptr(class_name).to_string_lossy().into_owned();
        drop(std::ffi::CString::from_raw(class_name));
        s
    } else {
        String::new()
    };

    let name_str = i8_slice_to_str(&(*sd).status.name);
    let buf2 = format!(
        "[Novice]({}) {}> {}\0",
        class_str, name_str, msg_str
    );

    // Non-tutors get a copy of their own message (so it appears on their screen)
    if (*sd).status.tutor == 0 {
        clif_sendmsg(sd, 11, buf2.as_ptr() as *const c_char);
    }

    for i in 0..fd_max {
        if rust_session_exists(i) == 0 { continue; }
        let tsd = rust_session_get_data(i) as *mut MapSessionData;
        if tsd.is_null() { continue; }
        if clif_isignore(sd, tsd) != 0
            && ((*tsd).status.tutor != 0 || (*tsd).status.gm_level > 0)
            && (*tsd).status.novice_chat != 0
        {
            clif_sendmsg(tsd, 12, buf2.as_ptr() as *const c_char);
        }
    }
    0
}

// ─── clif_parsewisp ───────────────────────────────────────────────────────────

/// Parse an incoming whisper packet and dispatch it.
///
/// Mirrors `clif_parsewisp` from `c_src/map_parse.c` ~line 6731.
#[no_mangle]
pub unsafe extern "C" fn clif_parsewisp(sd: *mut MapSessionData) -> c_int {
    use super::packet::{rfifob, rfifop, rfiforest, rfifow};

    if (*sd).status.setting_flags & FLAG_WHISPER as u16 == 0 && (*sd).status.gm_level == 0 {
        let msg = b"You have whispering turned off.\0";
        clif_sendbluemessage(sd, msg.as_ptr() as *mut c_char);
        return 0;
    }

    if (*sd).uFlags & U_FLAG_SILENCED != 0 {
        let msg = b"You are silenced.\0";
        clif_sendbluemessage(sd, msg.as_ptr() as *mut c_char);
        return 0;
    }

    if (*map.add((*sd).bl.m as usize)).cantalk == 1 && (*sd).status.gm_level == 0 {
        let msg = b"Your voice is swept away by a strange wind.\0";
        clif_sendminitext(sd, msg.as_ptr() as *const c_char);
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
        clif_Hacker((*sd).status.name.as_mut_ptr() as *mut c_char, b"Whisper packet\0".as_ptr() as *const c_char);
        return 0;
    }

    let mut dst_name = [0u8; 100];
    let mut msg_buf = [0u8; 100];

    let src_dst = rfifop((*sd).fd, 6);
    std::ptr::copy_nonoverlapping(src_dst, dst_name.as_mut_ptr(), dstlen.min(99));

    let src_msg = rfifop((*sd).fd, 7 + dstlen);
    std::ptr::copy_nonoverlapping(src_msg, msg_buf.as_mut_ptr(), msglen.min(80));
    msg_buf[80] = 0;

    let dst_name_c = dst_name.as_ptr() as *const c_char;
    let msg_c = msg_buf.as_ptr() as *const c_char;

    // Sql_EscapeString for the msg
    let mut escape = [0i8; 255];
    Sql_EscapeString(sql_handle as *mut c_void, escape.as_mut_ptr(), msg_c);

    // "!" → clan chat
    if dst_name[0] == b'!' && dst_name[1] == 0 {
        if (*sd).status.clan == 0 {
            let m = b"You are not in a clan\0";
            clif_sendbluemessage(sd, m.as_ptr() as *mut c_char);
        } else if (*sd).status.clan_chat != 0 {
            crate::game::scripting::doscript_strings(b"characterLog\0".as_ptr() as *const c_char, b"clanChatLog\0".as_ptr() as *const c_char, &[(*sd).status.name.as_ptr(), msg_c]);
            clif_sendclanmessage(sd, rfifop((*sd).fd, 7 + dstlen) as *mut u8, msglen as c_int);
        } else {
            let m = b"Clan chat is off.\0";
            clif_sendbluemessage(sd, m.as_ptr() as *mut c_char);
        }
    // "!!" → group chat
    } else if dst_name[0] == b'!' && dst_name[1] == b'!' && dst_name[2] == 0 {
        if (*sd).group_count == 0 {
            let m = b"You are not in a group\0";
            clif_sendbluemessage(sd, m.as_ptr() as *mut c_char);
        } else {
            crate::game::scripting::doscript_strings(b"characterLog\0".as_ptr() as *const c_char, b"groupChatLog\0".as_ptr() as *const c_char, &[(*sd).status.name.as_ptr(), msg_c]);
            clif_sendgroupmessage(sd, rfifop((*sd).fd, 7 + dstlen) as *mut u8, msglen as c_int);
        }
    // "@" → subpath chat
    } else if dst_name[0] == b'@' && dst_name[1] == 0 {
        if rust_classdb_chat((*sd).status.class as c_int) != 0 {
            crate::game::scripting::doscript_strings(b"characterLog\0".as_ptr() as *const c_char, b"subPathChatLog\0".as_ptr() as *const c_char, &[(*sd).status.name.as_ptr(), msg_c]);
            clif_sendsubpathmessage(sd, rfifop((*sd).fd, 7 + dstlen) as *mut u8, msglen as c_int);
        } else {
            let m = b"You cannot do that.\0";
            clif_sendbluemessage(sd, m.as_ptr() as *mut c_char);
        }
    // "?" → novice chat
    } else if dst_name[0] == b'?' && dst_name[1] == 0 {
        if (*sd).status.tutor == 0 && (*sd).status.gm_level == 0 {
            if (*sd).status.level < 99 {
                crate::game::scripting::doscript_strings(b"characterLog\0".as_ptr() as *const c_char, b"noviceChatLog\0".as_ptr() as *const c_char, &[(*sd).status.name.as_ptr(), msg_c]);
                clif_sendnovicemessage(sd, rfifop((*sd).fd, 7 + dstlen) as *mut u8, msglen as c_int);
            } else {
                let m = b"You cannot do that.\0";
                clif_sendbluemessage(sd, m.as_ptr() as *mut c_char);
            }
        } else {
            crate::game::scripting::doscript_strings(b"characterLog\0".as_ptr() as *const c_char, b"noviceChatLog\0".as_ptr() as *const c_char, &[(*sd).status.name.as_ptr(), msg_c]);
            clif_sendnovicemessage(sd, rfifop((*sd).fd, 7 + dstlen) as *mut u8, msglen as c_int);
        }
    // named whisper
    } else {
        let dst_sd = map_name2sd(dst_name_c);
        if dst_sd.is_null() {
            let target = std::ffi::CStr::from_ptr(dst_name_c).to_string_lossy();
            let nf = format!("{} is nowhere to be found.\0", target);
            clif_sendbluemessage(sd, nf.as_ptr() as *mut c_char);
        } else if canwhisper(sd, dst_sd) != 0 {
            if (*dst_sd).afk != 0 {
                let afk_msg = std::ffi::CStr::from_ptr((*dst_sd).afkmessage.as_ptr());
                let has_afk_msg = !afk_msg.to_bytes().is_empty();

                crate::game::scripting::doscript_strings(b"characterLog\0".as_ptr() as *const c_char, b"whisperLog\0".as_ptr() as *const c_char, &[(*dst_sd).status.name.as_ptr(), (*sd).status.name.as_ptr(), msg_c]);

                clif_sendwisp(dst_sd, (*sd).status.name.as_ptr() as *const c_char, msg_c);
                clif_sendwisp(dst_sd, (*dst_sd).status.name.as_ptr() as *const c_char, (*dst_sd).afkmessage.as_ptr() as *const c_char);

                if (*sd).status.gm_level == 0 && (*dst_sd).optFlags & OPT_FLAG_STEALTH != 0 {
                    // don't reveal their presence — strText was formatted but C never sent it here
                } else {
                    clif_retrwisp(sd, (*dst_sd).status.name.as_mut_ptr() as *mut c_char, msg_buf.as_mut_ptr() as *mut c_char);
                    clif_retrwisp(sd, (*dst_sd).status.name.as_mut_ptr() as *mut c_char, (*dst_sd).afkmessage.as_mut_ptr() as *mut c_char);
                }
                let _ = has_afk_msg;
            } else {
                crate::game::scripting::doscript_strings(b"characterLog\0".as_ptr() as *const c_char, b"whisperLog\0".as_ptr() as *const c_char, &[(*dst_sd).status.name.as_ptr(), (*sd).status.name.as_ptr(), msg_c]);

                clif_sendwisp(dst_sd, (*sd).status.name.as_ptr() as *const c_char, msg_c);

                if (*sd).status.gm_level == 0 && (*dst_sd).optFlags & OPT_FLAG_STEALTH != 0 {
                    let target = std::ffi::CStr::from_ptr(dst_name_c).to_string_lossy();
                    let nf = format!("{} is nowhere to be found.\0", target);
                    clif_sendbluemessage(sd, nf.as_ptr() as *mut c_char);
                } else {
                    clif_retrwisp(sd, (*dst_sd).status.name.as_mut_ptr() as *mut c_char, msg_buf.as_mut_ptr() as *mut c_char);
                }
            }
        } else {
            let m = b"They cannot hear you right now.\0";
            clif_sendbluemessage(sd, m.as_ptr() as *mut c_char);
        }
    }
    0
}

// ─── clif_sendsay ─────────────────────────────────────────────────────────────

/// Broadcast a player say/shout and fire NPC speech callbacks.
///
/// Mirrors `clif_sendsay` from `c_src/map_parse.c` ~line 6869.
#[no_mangle]
pub unsafe extern "C" fn clif_sendsay(
    sd: *mut MapSessionData,
    msg: *mut c_char,
    msglen: c_int,
    say_type: c_int,
) -> c_int {
    let namelen = libc_strlen((*sd).status.name.as_ptr() as *const c_char);

    if say_type == 1 {
        (*sd).talktype = 1;
        let src = std::slice::from_raw_parts(msg as *const u8, (msglen as usize).min(254));
        let dst = std::slice::from_raw_parts_mut((*sd).speech.as_mut_ptr() as *mut u8, 255);
        dst[..src.len()].copy_from_slice(src);
    } else {
        (*sd).talktype = 0;
        let src = std::slice::from_raw_parts(msg as *const u8, (msglen as usize).min(254));
        let dst = std::slice::from_raw_parts_mut((*sd).speech.as_mut_ptr() as *mut u8, 255);
        dst[..src.len()].copy_from_slice(src);
    }

    for i in 0..MAX_SPELLS {
        if (*sd).status.skill[i] > 0 {
            let yname = rust_magicdb_yname((*sd).status.skill[i] as c_int);
            sl_doscript_simple(yname, b"on_say\0".as_ptr() as *const c_char, &raw mut (*sd).bl);
        }
    }
    sl_doscript_simple(b"onSay\0".as_ptr() as *const c_char, std::ptr::null(), &raw mut (*sd).bl);
    0
}

// ─── clif_sendscriptsay ───────────────────────────────────────────────────────

/// Broadcast a player's scripted say and log it; handles language channels.
///
/// Mirrors `clif_sendscriptsay` from `c_src/map_parse.c` ~line 6893.
#[no_mangle]
pub unsafe extern "C" fn clif_sendscriptsay(
    sd: *mut MapSessionData,
    msg: *const c_char,
    msglen: c_int,
    say_type: c_int,
) -> c_int {
    let namelen = libc_strlen((*sd).status.name.as_ptr() as *const c_char);

    if (*map.add((*sd).bl.m as usize)).cantalk == 1 && (*sd).status.gm_level == 0 {
        let m = b"Your voice is swept away by a strange wind.\0";
        clif_sendminitext(sd, m.as_ptr() as *const c_char);
        return 0;
    }

    let mut escape = [0i8; 255];
    Sql_EscapeString(sql_handle as *mut c_void, escape.as_mut_ptr(), msg);

    if rust_is_command(sd, msg, msglen) != 0 {
        return 0;
    }

    if (*sd).uFlags & U_FLAG_SILENCED != 0 {
        let m = b"Shut up for now. ^^\0";
        clif_sendminitext(sd, m.as_ptr() as *const c_char);
        return 0;
    }

    let msg_bytes = std::slice::from_raw_parts(msg as *const u8, msglen as usize);

    if say_type >= 10 {
        let ext_namelen = namelen + 4; // prefix like "EN[" + name + "]"

        if rust_session_exists((*sd).fd) == 0 {
            rust_session_set_eof((*sd).fd, 8);
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

        clif_send(buf.as_ptr(), buf_size as c_int, &raw mut (*sd).bl, SAMEAREA);
    } else {
        if rust_session_exists((*sd).fd) == 0 {
            rust_session_set_eof((*sd).fd, 8);
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
        clif_send(buf.as_ptr(), buf_size as c_int, &raw mut (*sd).bl, send_target);
    }

    // Copy msg to speech
    let src = std::slice::from_raw_parts(msg as *const u8, (msglen as usize).min(254));
    let dst = std::slice::from_raw_parts_mut((*sd).speech.as_mut_ptr() as *mut u8, 255);
    dst[..src.len()].copy_from_slice(src);

    if say_type == 1 {
        map_foreachinarea(clif_sendnpcyell, (*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int, SAMEMAP,
                          BL_NPC, msg, sd);
        map_foreachinarea(clif_sendmobyell, (*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int, SAMEMAP,
                          BL_MOB, msg, sd);
    } else {
        map_foreachinarea(clif_sendnpcsay, (*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int, AREA,
                          BL_NPC, msg, sd);
        map_foreachinarea(clif_sendmobsay, (*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int, AREA,
                          BL_MOB, msg, sd);
    }
    0
}

// ─── clif_sendnpcsay ──────────────────────────────────────────────────────────

/// foreachinarea callback: fire NPC speech handler if player is nearby.
///
/// Mirrors `clif_sendnpcsay` from `c_src/map_parse.c` ~line 7089.
#[no_mangle]
pub unsafe extern "C" fn clif_sendnpcsay(bl: *mut BlockList, mut ap: ...) -> c_int {
    if (*bl).subtype != SCRIPT { return 0; }

    let _msg: *const c_char = ap.arg::<*const c_char>();
    let sd_arg: *mut MapSessionData = ap.arg::<*mut MapSessionData>();
    if sd_arg.is_null() { return 0; }

    let nd = bl as *mut NpcData;
    if nd.is_null() { return 0; }

    if clif_distance(bl, &raw mut (*sd_arg).bl) <= 10 {
        (*sd_arg).last_click = (*bl).id;
        sl_async_freeco(sd_arg as *mut c_void);
        sl_doscript_2((*nd).name.as_ptr() as *const c_char, b"onSayClick\0".as_ptr() as *const c_char, &raw mut (*sd_arg).bl, bl);
    }
    0
}

// ─── clif_sendmobsay ──────────────────────────────────────────────────────────

/// foreachinarea callback: mob speech handler (currently a no-op in C).
///
/// Mirrors `clif_sendmobsay` from `c_src/map_parse.c` ~line 7108.
#[no_mangle]
pub unsafe extern "C" fn clif_sendmobsay(bl: *mut BlockList, mut ap: ...) -> c_int {
    let _: *const c_char = ap.arg::<*const c_char>();
    let _: *mut MapSessionData = ap.arg::<*mut MapSessionData>();
    0
}

// ─── clif_sendnpcyell ─────────────────────────────────────────────────────────

/// foreachinarea callback: fire NPC speech handler (yell range = 20).
///
/// Mirrors `clif_sendnpcyell` from `c_src/map_parse.c` ~line 7134.
#[no_mangle]
pub unsafe extern "C" fn clif_sendnpcyell(bl: *mut BlockList, mut ap: ...) -> c_int {
    if (*bl).subtype != SCRIPT { return 0; }

    let _msg: *const c_char = ap.arg::<*const c_char>();
    let sd_arg: *mut MapSessionData = ap.arg::<*mut MapSessionData>();
    if sd_arg.is_null() { return 0; }

    let nd = bl as *mut NpcData;
    if nd.is_null() { return 0; }

    if clif_distance(bl, &raw mut (*sd_arg).bl) <= 20 {
        (*sd_arg).last_click = (*bl).id;
        sl_async_freeco(sd_arg as *mut c_void);
        sl_doscript_2((*nd).name.as_ptr() as *const c_char, b"onSayClick\0".as_ptr() as *const c_char, &raw mut (*sd_arg).bl, bl);
    }
    0
}

// ─── clif_sendmobyell ─────────────────────────────────────────────────────────

/// foreachinarea callback: mob yell handler (currently a no-op in C).
///
/// Mirrors `clif_sendmobyell` from `c_src/map_parse.c` ~line 7156.
#[no_mangle]
pub unsafe extern "C" fn clif_sendmobyell(bl: *mut BlockList, mut ap: ...) -> c_int {
    let _: *const c_char = ap.arg::<*const c_char>();
    let _: *mut MapSessionData = ap.arg::<*mut MapSessionData>();
    0
}

// ─── clif_speak ───────────────────────────────────────────────────────────────

/// Send an NPC/object speech-bubble packet to one player.
///
/// Mirrors `clif_speak` from `c_src/map_parse.c` ~line 7183.
#[no_mangle]
pub unsafe extern "C" fn clif_speak(bl: *mut BlockList, mut ap: ...) -> c_int {
    let msg: *const c_char = ap.arg::<*const c_char>();
    let nd: *mut BlockList = ap.arg::<*mut BlockList>();
    let speak_type: c_int = ap.arg::<c_int>();
    let sd = bl as *mut MapSessionData;
    if sd.is_null() || nd.is_null() { return 0; }

    let len = libc_strlen(msg);

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    wfifohead((*sd).fd, len + 11);
    wfifob((*sd).fd, 5, speak_type as u8);
    wfifol((*sd).fd, 6, ((*nd).id).to_be());
    wfifob((*sd).fd, 10, len as u8);
    // len for header = len + 8
    let hdr_len = (len + 8) as u16;
    wfifoheader((*sd).fd, 0x0D, hdr_len);
    // copy msg after header at offset 11
    let dst = crate::ffi::session::rust_session_wdata_ptr((*sd).fd, 11);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(msg as *const u8, dst, len);
    }
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_parseignore ─────────────────────────────────────────────────────────

/// Handle an ignore-list add/remove packet from the client.
///
/// Mirrors `clif_parseignore` from `c_src/map_parse.c` ~line 7213.
#[no_mangle]
pub unsafe extern "C" fn clif_parseignore(sd: *mut MapSessionData) -> c_int {
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
                ignorelist_add(sd, name_buf.as_ptr() as *const c_char);
            }
            0x03 => {
                // Remove
                let src = super::packet::rfifop((*sd).fd, 7);
                std::ptr::copy_nonoverlapping(src as *const i8, name_buf.as_mut_ptr(), nlen.min(31));
                ignorelist_remove(sd, name_buf.as_ptr() as *const c_char);
            }
            _ => {}
        }
    }
    0
}

// ─── clif_parsesay ────────────────────────────────────────────────────────────

/// Parse an incoming say packet from the client.
///
/// Mirrors `clif_parsesay` from `c_src/map_parse.c` ~line 7241.
#[no_mangle]
pub unsafe extern "C" fn clif_parsesay(sd: *mut MapSessionData) -> c_int {
    use super::packet::{rfifob, rfifop};

    let msg = rfifop((*sd).fd, 7) as *const c_char;

    (*sd).talktype = rfifob((*sd).fd, 5);

    if (*sd).talktype > 1 || rfifob((*sd).fd, 6) > 100 {
        let m = b"I just told the GM on you!\0";
        clif_sendminitext(sd, m.as_ptr() as *const c_char);
        libc::printf(b"Talk Hacker: %s\n\0".as_ptr() as *const c_char, (*sd).status.name.as_ptr());
        return 0;
    }

    if rust_is_command(sd, msg, rfifob((*sd).fd, 6) as c_int) != 0 {
        return 0;
    }

    // Copy msg into speech
    let src = std::slice::from_raw_parts(msg as *const u8, (rfifob((*sd).fd, 6) as usize).min(254));
    let dst = std::slice::from_raw_parts_mut((*sd).speech.as_mut_ptr() as *mut u8, 255);
    dst[..src.len()].copy_from_slice(src);

    for i in 0..MAX_SPELLS {
        if (*sd).status.skill[i] > 0 {
            let yname = rust_magicdb_yname((*sd).status.skill[i] as c_int);
            sl_doscript_simple(yname, b"on_say\0".as_ptr() as *const c_char, &raw mut (*sd).bl);
        }
    }
    sl_doscript_simple(b"onSay\0".as_ptr() as *const c_char, std::ptr::null(), &raw mut (*sd).bl);
    0
}

// ─── Private helpers ──────────────────────────────────────────────────────────

/// Measure a C string length (mirrors `strlen`).
#[inline]
unsafe fn libc_strlen(s: *const c_char) -> usize {
    if s.is_null() { return 0; }
    let mut p = s as *const u8;
    let mut n = 0usize;
    while *p != 0 { p = p.add(1); n += 1; }
    n
}

/// Case-insensitive comparison of a `[i8]` slice against a `*const c_char`.
unsafe fn strcasecmp_cstr(a: *const i8, b: *const c_char) -> c_int {
    libc::strcasecmp(a as *const c_char, b)
}

/// Convert an `i8` slice (null-terminated or not) to a `&str`.
unsafe fn i8_slice_to_str(s: &[i8]) -> &str {
    let bytes = std::slice::from_raw_parts(s.as_ptr() as *const u8, s.len());
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end]).unwrap_or("")
}

/// Build `"[prefix name][(classname)] message"` for group/clan/subpath chat.
unsafe fn format_chat_prefix(sd: *mut MapSessionData, open: &[u8], close: &[u8], msg: &[i8]) -> Vec<u8> {
    let name = i8_slice_to_str(&(*sd).status.name);
    let class_name = rust_classdb_name((*sd).status.class as c_int, (*sd).status.mark as c_int);
    let class_str = if !class_name.is_null() {
        let s = std::ffi::CStr::from_ptr(class_name).to_string_lossy().into_owned();
        drop(std::ffi::CString::from_raw(class_name));
        s
    } else {
        String::new()
    };
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
unsafe fn clif_distance(bl: *const BlockList, bl2: *const BlockList) -> c_int {
    let dx = ((*bl).x as i32) - ((*bl2).x as i32);
    let dy = ((*bl).y as i32) - ((*bl2).y as i32);
    dx.abs() + dy.abs()
}

/// Retrieve session data for fd, returning null if session does not exist.
#[inline]
unsafe fn rust_session_get_data_checked(fd: c_int) -> *mut MapSessionData {
    if rust_session_exists(fd) != 0 {
        rust_session_get_data(fd) as *mut MapSessionData
    } else {
        std::ptr::null_mut()
    }
}
