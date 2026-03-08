//! Port of NPC/script dialog helpers from `c_src/map_parse.c`.
//!
//! Covers close-dialog, town list, countdown timer, NPC dialog/menu/input
//! display and parse, shop buy/sell dialogs, and the click-getinfo handler.
//! Functions declared `#[no_mangle] pub unsafe extern "C"` so they remain
//! callable from any remaining C code that has not yet been ported.

#![allow(non_snake_case, clippy::wildcard_imports)]

use std::ffi::{c_char, c_int, c_uint, c_void};

use crate::database::map_db::BlockList;
use crate::game::mob::MobSpawnData;
use crate::game::npc::NpcData;
use crate::game::pc::{
    MapSessionData,
    BL_NPC, BL_MOB, BL_PC,
    EQ_ARMOR, EQ_COAT, EQ_WEAP, EQ_SHIELD, EQ_HELM,
    EQ_FACEACC, EQ_CROWN, EQ_FACEACCTWO, EQ_MANTLE, EQ_NECKLACE, EQ_BOOTS,
};
use crate::servers::char::charstatus::Item;
use crate::ffi::session::{rust_session_exists, rust_session_set_eof};

use super::packet::{
    encrypt, wfifob, wfifohead, wfifol, wfifoset, wfifow,
    rfifob, rfifol, rfifop,
    swap16, swap32,
};

// ─── External C globals ───────────────────────────────────────────────────────

extern "C" {
    static login_ip: c_int;
    static login_port: c_int;
    static xor_key: [c_char; 10];
    static town_n: c_int;

    #[link_name = "towns"]
    static TOWNS: [TownData; 256];
}

// ─── C struct mirrors ─────────────────────────────────────────────────────────

/// Mirrors `struct town_data` from `c_src/config.h`.
#[repr(C)]
struct TownData {
    pub name: [c_char; 64],
}

// ─── External C functions not yet ported ─────────────────────────────────────

extern "C" {
    fn map_id2npc(id: c_uint) -> *mut c_void;  // returns *mut NpcData; matches pc.rs decl
    fn map_id2sd(id: c_uint) -> *mut MapSessionData;
    fn map_id2bl(id: c_uint) -> *mut BlockList;
    fn clif_sendminitext(sd: *mut MapSessionData, msg: *const c_char) -> c_int;
    fn clif_clickonplayer(sd: *mut MapSessionData, bl: *mut BlockList) -> c_int;
    fn rust_sl_resumedialog(choice: c_uint, sd: *mut c_void);
    fn rust_sl_resumemenuseq(choice: c_uint, menu: c_int, sd: *mut c_void);
    fn rust_sl_resumeinputseq(choice: c_uint, input: *const c_char, sd: *mut c_void);
    fn rust_sl_resumebuy(items: *const c_char, sd: *mut c_void);
    fn rust_sl_resumesell(choice: c_uint, sd: *mut c_void);
    fn rust_sl_resumeinput(tag: *const c_char, input: *const c_char, sd: *mut c_void);
    fn rust_sl_async_freeco(user: *mut c_void);
    fn rust_itemdb_name(id: c_uint) -> *mut c_char;
    fn rust_itemdb_buytext(id: c_uint) -> *mut c_char;
    fn rust_itemdb_icon(id: c_uint) -> c_int;
    fn rust_itemdb_iconcolor(id: c_uint) -> c_int;
    fn rust_itemdb_class(id: c_uint) -> c_int;
    fn rust_itemdb_rank(id: c_uint) -> c_int;
    fn rust_itemdb_level(id: c_uint) -> c_int;
    fn rust_classdb_name(id: c_int, rank: c_int) -> *mut c_char;
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


// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Returns the byte length of a null-terminated C string (not counting the
/// null terminator).  Mirrors `strlen`.
#[inline]
unsafe fn cstrlen(p: *const c_char) -> usize {
    libc::strlen(p)
}

/// Copy `len` bytes from a C pointer into a fixed-size buffer, leaving the
/// rest zero. Mirrors `memcpy(dst, src, len)` with a null-terminated guarantee.
#[inline]
unsafe fn copy_rfifo_bytes(dst: &mut [u8], src: *const u8, len: usize) {
    let copy_len = len.min(dst.len().saturating_sub(1));
    std::ptr::copy_nonoverlapping(src, dst.as_mut_ptr(), copy_len);
}

/// Write an NPC equip-slot look block (type 1) starting at `base_off` into
/// the send buffer.  Used by `clif_scriptmes`, `clif_scriptmenu`,
/// `clif_scriptmenuseq`, `clif_input`, and `clif_buydialog`.
#[inline]
#[allow(clippy::too_many_arguments)]
unsafe fn write_npc_equip_look(fd: c_int, nd: *const NpcData, base_off: usize) {
    let nd = &*nd;
    wfifob(fd, base_off,      1);
    wfifow(fd, base_off + 1,  swap16(nd.sex));
    wfifob(fd, base_off + 3,  nd.state as u8);
    wfifob(fd, base_off + 4,  0);
    wfifow(fd, base_off + 5,  swap16(nd.equip[EQ_ARMOR as usize].id as u16));
    wfifob(fd, base_off + 7,  0);
    wfifob(fd, base_off + 8,  nd.face as u8);
    wfifob(fd, base_off + 9,  nd.hair as u8);
    wfifob(fd, base_off + 10, nd.hair_color as u8);
    wfifob(fd, base_off + 11, nd.face_color as u8);
    wfifob(fd, base_off + 12, nd.skin_color as u8);

    // armor (possibly overridden by coat)
    if nd.equip[EQ_ARMOR as usize].id == 0 {
        wfifow(fd, base_off + 13, 0xFFFF);
        wfifob(fd, base_off + 15, 0);
    } else {
        wfifow(fd, base_off + 13, swap16(nd.equip[EQ_ARMOR as usize].id as u16));
        if nd.armor_color != 0 {
            wfifob(fd, base_off + 15, nd.armor_color as u8);
        } else {
            wfifob(fd, base_off + 15, nd.equip[EQ_ARMOR as usize].custom_look_color as u8);
        }
    }
    // coat overrides armor slot
    if nd.equip[EQ_COAT as usize].id != 0 {
        wfifow(fd, base_off + 13, swap16(nd.equip[EQ_COAT as usize].id as u16));
        wfifob(fd, base_off + 15, nd.equip[EQ_COAT as usize].custom_look_color as u8);
    }

    // weap
    if nd.equip[EQ_WEAP as usize].id == 0 {
        wfifow(fd, base_off + 16, 0xFFFF);
        wfifob(fd, base_off + 18, 0);
    } else {
        wfifow(fd, base_off + 16, swap16(nd.equip[EQ_WEAP as usize].id as u16));
        wfifob(fd, base_off + 18, nd.equip[EQ_WEAP as usize].custom_look_color as u8);
    }

    // shield
    if nd.equip[EQ_SHIELD as usize].id == 0 {
        wfifow(fd, base_off + 19, 0xFFFF);
        wfifob(fd, base_off + 21, 0);
    } else {
        wfifow(fd, base_off + 19, swap16(nd.equip[EQ_SHIELD as usize].id as u16));
        wfifob(fd, base_off + 21, nd.equip[EQ_SHIELD as usize].custom_look_color as u8);
    }

    // helm
    if nd.equip[EQ_HELM as usize].id == 0 {
        wfifob(fd, base_off + 22, 0);
        wfifob(fd, base_off + 23, 0xFF);
        wfifob(fd, base_off + 24, 0);
    } else {
        wfifob(fd, base_off + 22, 1);
        wfifob(fd, base_off + 23, nd.equip[EQ_HELM as usize].id as u8);
        wfifob(fd, base_off + 24, nd.equip[EQ_HELM as usize].custom_look_color as u8);
    }

    // faceacc
    if nd.equip[EQ_FACEACC as usize].id == 0 {
        wfifow(fd, base_off + 25, 0xFFFF);
        wfifob(fd, base_off + 27, 0);
    } else {
        wfifow(fd, base_off + 25, swap16(nd.equip[EQ_FACEACC as usize].id as u16));
        wfifob(fd, base_off + 27, nd.equip[EQ_FACEACC as usize].custom_look_color as u8);
    }

    // crown (clears helm-present flag if crown present)
    if nd.equip[EQ_CROWN as usize].id == 0 {
        wfifow(fd, base_off + 28, 0xFFFF);
        wfifob(fd, base_off + 30, 0);
    } else {
        wfifob(fd, base_off + 22, 0); // matches C: clears helm-present flag
        wfifow(fd, base_off + 28, swap16(nd.equip[EQ_CROWN as usize].id as u16));
        wfifob(fd, base_off + 30, nd.equip[EQ_CROWN as usize].custom_look_color as u8);
    }

    // faceacctwo
    if nd.equip[EQ_FACEACCTWO as usize].id == 0 {
        wfifow(fd, base_off + 31, 0xFFFF);
        wfifob(fd, base_off + 33, 0);
    } else {
        wfifow(fd, base_off + 31, swap16(nd.equip[EQ_FACEACCTWO as usize].id as u16));
        wfifob(fd, base_off + 33, nd.equip[EQ_FACEACCTWO as usize].custom_look_color as u8);
    }

    // mantle
    if nd.equip[EQ_MANTLE as usize].id == 0 {
        wfifow(fd, base_off + 34, 0xFFFF);
        wfifob(fd, base_off + 36, 0);
    } else {
        wfifow(fd, base_off + 34, swap16(nd.equip[EQ_MANTLE as usize].id as u16));
        wfifob(fd, base_off + 36, nd.equip[EQ_MANTLE as usize].custom_look_color as u8);
    }

    // necklace
    if nd.equip[EQ_NECKLACE as usize].id == 0 {
        wfifow(fd, base_off + 37, 0xFFFF);
        wfifob(fd, base_off + 39, 0);
    } else {
        wfifow(fd, base_off + 37, swap16(nd.equip[EQ_NECKLACE as usize].id as u16));
        wfifob(fd, base_off + 39, nd.equip[EQ_NECKLACE as usize].custom_look_color as u8);
    }

    // boots (falls back to sex)
    if nd.equip[EQ_BOOTS as usize].id == 0 {
        wfifow(fd, base_off + 40, swap16(nd.sex));
        wfifob(fd, base_off + 42, 0);
    } else {
        wfifow(fd, base_off + 40, swap16(nd.equip[EQ_BOOTS as usize].id as u16));
        wfifob(fd, base_off + 42, nd.equip[EQ_BOOTS as usize].custom_look_color as u8);
    }
    // 43 bytes of equip data written starting at base_off
}

/// Write an NPC gfx-viewer look block (type 2) starting at `base_off`.
#[inline]
unsafe fn write_npc_gfx_look(fd: c_int, nd: *const NpcData, base_off: usize) {
    let nd = &*nd;
    let g = &nd.gfx;
    wfifob(fd, base_off,      1);
    wfifow(fd, base_off + 1,  swap16(nd.sex));
    wfifob(fd, base_off + 3,  nd.state as u8);
    wfifob(fd, base_off + 4,  0);
    wfifow(fd, base_off + 5,  swap16(g.armor));
    wfifob(fd, base_off + 7,  0);
    wfifob(fd, base_off + 8,  g.face);
    wfifob(fd, base_off + 9,  g.hair);
    wfifob(fd, base_off + 10, g.chair);
    wfifob(fd, base_off + 11, g.cface);
    wfifob(fd, base_off + 12, g.cskin);

    // armor
    wfifow(fd, base_off + 13, swap16(g.armor));
    wfifob(fd, base_off + 15, g.carmor);

    // weap
    wfifow(fd, base_off + 16, swap16(g.weapon));
    wfifob(fd, base_off + 18, g.cweapon);

    // shield
    wfifow(fd, base_off + 19, swap16(g.shield));
    wfifob(fd, base_off + 21, g.cshield);

    // helm
    if g.helm == 65535 {
        wfifob(fd, base_off + 22, 0);
        wfifob(fd, base_off + 23, 0xFF);
        wfifob(fd, base_off + 24, 0);
    } else {
        wfifob(fd, base_off + 22, 1);
        wfifob(fd, base_off + 23, g.helm as u8);
        wfifob(fd, base_off + 24, g.chelm);
    }

    // faceacc
    wfifow(fd, base_off + 25, swap16(g.face_acc));
    wfifob(fd, base_off + 27, g.cface_acc);

    // crown (clears helm-present flag if crown present)
    if g.crown == 65535 {
        wfifow(fd, base_off + 28, 0xFFFF);
        wfifob(fd, base_off + 30, 0);
    } else {
        wfifob(fd, base_off + 22, 0);
        wfifow(fd, base_off + 28, swap16(g.crown));
        wfifob(fd, base_off + 30, g.ccrown);
    }

    // faceacctwo
    wfifow(fd, base_off + 31, swap16(g.face_acc_t));
    wfifob(fd, base_off + 33, g.cface_acc_t);

    // mantle
    wfifow(fd, base_off + 34, swap16(g.mantle));
    wfifob(fd, base_off + 36, g.cmantle);

    // necklace
    wfifow(fd, base_off + 37, swap16(g.necklace));
    wfifob(fd, base_off + 39, g.cnecklace);

    // boots
    wfifow(fd, base_off + 40, swap16(g.boots));
    wfifob(fd, base_off + 42, g.cboots);
    // 43 bytes of gfx data written starting at base_off
}

// ─── clif_closeit ─────────────────────────────────────────────────────────────

/// Send a close-dialog packet to the client.  Mirrors `clif_closeit` in
/// `c_src/map_parse.c`.
#[no_mangle]
pub unsafe extern "C" fn clif_closeit(sd: *mut MapSessionData) -> c_int {
    let fd = (*sd).fd;

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    wfifohead(fd, 255);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x03);
    wfifol(fd, 4, swap32(login_ip as u32));
    wfifow(fd, 8, swap16(login_port as u16));
    wfifob(fd, 10, 0x16);
    wfifow(fd, 11, swap16(9));
    // copy xor_key (9 chars + null) into WFIFOP(sd->fd, 13)
    libc::strcpy(
        crate::ffi::session::rust_session_wdata_ptr(fd, 13) as *mut c_char,
        xor_key.as_ptr(),
    );
    let mut len = 11usize;
    let name_ptr = (*sd).status.name.as_ptr();
    let name_len = cstrlen(name_ptr as *const c_char);
    wfifob(fd, len + 11, name_len as u8);
    libc::strcpy(
        crate::ffi::session::rust_session_wdata_ptr(fd, len + 12) as *mut c_char,
        name_ptr as *const c_char,
    );
    len += name_len + 1;
    // WFIFOL(sd->fd,len+11)=SWAP32(sd->status.id);  // commented-out in C
    len += 4;
    wfifob(fd, 10, len as u8);
    wfifow(fd, 1, swap16((len + 8) as u16));
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_sendtowns ───────────────────────────────────────────────────────────

/// Send town list dialog.  Mirrors `clif_sendtowns` in `c_src/map_parse.c`.
#[no_mangle]
pub unsafe extern "C" fn clif_sendtowns(sd: *mut MapSessionData) -> c_int {
    let fd = (*sd).fd;

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    let n = town_n as usize;

    wfifohead(fd, 0x59);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x59);
    wfifob(fd, 5, 64);
    wfifow(fd, 6, 0);
    wfifob(fd, 8, 34);
    wfifob(fd, 9, n as u8);

    let mut len = 0usize;
    for x in 0..n {
        let name_ptr = TOWNS[x].name.as_ptr();
        let name_len = cstrlen(name_ptr as *const c_char);
        wfifob(fd, len + 10, x as u8);
        wfifob(fd, len + 11, name_len as u8);
        libc::strcpy(
            crate::ffi::session::rust_session_wdata_ptr(fd, len + 12) as *mut c_char,
            name_ptr as *const c_char,
        );
        len += name_len + 2;
    }

    wfifow(fd, 1, swap16((len + 9) as u16));
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_send_timer ──────────────────────────────────────────────────────────

/// Send a countdown timer packet.  Mirrors `clif_send_timer` in
/// `c_src/map_parse.c`.
#[no_mangle]
pub unsafe extern "C" fn clif_send_timer(
    sd: *mut MapSessionData,
    timer_type: c_char,
    length: c_uint,
) {
    let fd = (*sd).fd;

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return;
    }

    wfifohead(fd, 10);
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, swap16(7));
    wfifob(fd, 3, 0x67);
    wfifob(fd, 5, timer_type as u8);
    wfifol(fd, 6, swap32(length));
    wfifoset(fd, encrypt(fd) as usize);
}

// ─── clif_parsenpcdialog ──────────────────────────────────────────────────────

/// Parse an NPC dialog response packet.  Mirrors `clif_parsenpcdialog` in
/// `c_src/map_parse.c`.
#[no_mangle]
pub unsafe extern "C" fn clif_parsenpcdialog(sd: *mut MapSessionData) -> c_int {
    let fd = (*sd).fd;
    let npc_choice = rfifob(fd, 13) as c_uint;

    match rfifob(fd, 5) {
        0x01 => {
            // Dialog
            rust_sl_resumedialog(npc_choice, sd as *mut c_void);
        }
        0x02 => {
            // Special menu
            let npc_menu = rfifob(fd, 15) as c_int;
            rust_sl_resumemenuseq(npc_choice, npc_menu, sd as *mut c_void);
        }
        0x04 => {
            // inputSeq returned input
            if rfifob(fd, 13) != 0x02 {
                rust_sl_async_freeco(sd as *mut c_void);
                return 1;
            }
            let input_len = rfifob(fd, 15) as usize;
            let mut input = [0u8; 100];
            copy_rfifo_bytes(&mut input, rfifop(fd, 16), input_len);
            rust_sl_resumeinputseq(
                npc_choice,
                input.as_ptr() as *const c_char,
                sd as *mut c_void,
            );
        }
        _ => {}
    }

    0
}

// ─── clif_scriptmes ───────────────────────────────────────────────────────────

/// Send NPC dialog text.  Mirrors `clif_scriptmes` in `c_src/map_parse.c`.
#[no_mangle]
pub unsafe extern "C" fn clif_scriptmes(
    sd: *mut MapSessionData,
    id: c_int,
    msg: *const c_char,
    previous: c_int,
    next: c_int,
) -> c_int {
    let fd       = (*sd).fd;
    let graphic_id = (*sd).npc_g;
    let color    = (*sd).npc_gc;
    let nd       = map_id2npc(id as c_uint) as *mut NpcData;
    let dialog_type = (*sd).dialogtype;

    if !nd.is_null() {
        (*nd).lastaction = libc::time(std::ptr::null_mut()) as c_uint;
    }

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    let msg_len = cstrlen(msg);

    wfifohead(fd, 1024);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x30);
    wfifow(fd, 5, swap16(1));
    wfifol(fd, 7, swap32(id as u32));

    match dialog_type {
        0 => {
            // graphic-only look
            if graphic_id == 0 {
                wfifob(fd, 11, 0);
            } else if graphic_id >= 49152 {
                wfifob(fd, 11, 2);
            } else {
                wfifob(fd, 11, 1);
            }
            wfifob(fd, 12, 1);
            wfifow(fd, 13, swap16(graphic_id as u16));
            wfifob(fd, 15, color as u8);
            wfifob(fd, 16, 1);
            wfifow(fd, 17, swap16(graphic_id as u16));
            wfifob(fd, 19, color as u8);
            wfifol(fd, 20, swap32(1));
            wfifob(fd, 24, previous as u8);
            wfifob(fd, 25, next as u8);
            wfifow(fd, 26, swap16(msg_len as u16));
            libc::strcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, 28) as *mut c_char,
                msg,
            );
            wfifow(fd, 1, swap16((msg_len + 25) as u16));
        }
        1 => {
            // NPC equip look
            if nd.is_null() { return 0; }
            write_npc_equip_look(fd, nd, 11);
            // after the 43 bytes of equip data (base 11):
            // offset 11+43 = 54: graphic block
            wfifob(fd, 54, 1);
            wfifow(fd, 55, swap16(graphic_id as u16));
            wfifob(fd, 57, color as u8);
            wfifol(fd, 58, swap32(1));
            wfifob(fd, 62, previous as u8);
            wfifob(fd, 63, next as u8);
            wfifow(fd, 64, swap16(msg_len as u16));
            libc::strcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, 66) as *mut c_char,
                msg,
            );
            wfifow(fd, 1, swap16((msg_len + 63) as u16));
        }
        2 => {
            // NPC gfx look
            if nd.is_null() { return 0; }
            write_npc_gfx_look(fd, nd, 11);
            wfifob(fd, 54, 1);
            wfifow(fd, 55, swap16(graphic_id as u16));
            wfifob(fd, 57, color as u8);
            wfifol(fd, 58, swap32(1));
            wfifob(fd, 62, previous as u8);
            wfifob(fd, 63, next as u8);
            wfifow(fd, 64, swap16(msg_len as u16));
            libc::strcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, 66) as *mut c_char,
                msg,
            );
            wfifow(fd, 1, swap16((msg_len + 63) as u16));
        }
        _ => {}
    }

    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_scriptmenu ──────────────────────────────────────────────────────────

/// Send NPC dialog menu.  Mirrors `clif_scriptmenu` in `c_src/map_parse.c`.
/// (Note: as of C source, this function appears not to be called anywhere.)
#[no_mangle]
pub unsafe extern "C" fn clif_scriptmenu(
    sd: *mut MapSessionData,
    id: c_int,
    dialog: *mut c_char,
    menu: *mut *mut c_char,
    size: c_int,
) -> c_int {
    let fd      = (*sd).fd;
    let graphic = (*sd).npc_g;
    let color   = (*sd).npc_gc;
    let nd      = map_id2npc(id as c_uint) as *mut NpcData;
    let dialog_type = (*sd).dialogtype;

    if !nd.is_null() {
        (*nd).lastaction = libc::time(std::ptr::null_mut()) as c_uint;
    }

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    let dialog_len = cstrlen(dialog as *const c_char);

    wfifohead(fd, 65535);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x2F);
    wfifow(fd, 5, swap16(1));
    wfifol(fd, 7, swap32(id as u32));

    match dialog_type {
        0 => {
            if graphic == 0 {
                wfifob(fd, 11, 0);
            } else if graphic >= 49152 {
                wfifob(fd, 11, 2);
            } else {
                wfifob(fd, 11, 1);
            }
            wfifob(fd, 12, 1);
            wfifow(fd, 13, swap16(graphic as u16));
            wfifob(fd, 15, color as u8);
            wfifob(fd, 16, 1);
            wfifow(fd, 17, swap16(graphic as u16));
            wfifob(fd, 19, color as u8);
            wfifow(fd, 20, swap16(dialog_len as u16));
            libc::strcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, 22) as *mut c_char,
                dialog as *const c_char,
            );
            wfifob(fd, dialog_len + 22, size as u8);
            let mut len = dialog_len;
            for x in 1..=(size as usize) {
                let entry = *menu.add(x);
                let entry_len = cstrlen(entry as *const c_char);
                wfifob(fd, len + 23, entry_len as u8);
                libc::strcpy(
                    crate::ffi::session::rust_session_wdata_ptr(fd, len + 24) as *mut c_char,
                    entry as *const c_char,
                );
                len += entry_len + 1;
                wfifow(fd, len + 23, swap16(x as u16));
                len += 2;
            }
            wfifow(fd, 1, swap16((len + 20) as u16));
        }
        1 => {
            if nd.is_null() { return 0; }
            write_npc_equip_look(fd, nd, 11);
            wfifob(fd, 54, 1);
            wfifow(fd, 55, swap16(graphic as u16));
            wfifob(fd, 57, color as u8);
            wfifow(fd, 58, swap16(dialog_len as u16));
            libc::strcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, 60) as *mut c_char,
                dialog as *const c_char,
            );
            wfifob(fd, dialog_len + 60, size as u8);
            let mut len = dialog_len;
            for x in 1..=(size as usize) {
                let entry = *menu.add(x);
                let entry_len = cstrlen(entry as *const c_char);
                wfifob(fd, len + 61, entry_len as u8);
                libc::strcpy(
                    crate::ffi::session::rust_session_wdata_ptr(fd, len + 62) as *mut c_char,
                    entry as *const c_char,
                );
                len += entry_len + 1;
                wfifow(fd, len + 61, swap16(x as u16));
                len += 2;
            }
            wfifow(fd, 1, swap16((len + 58) as u16));
        }
        2 => {
            if nd.is_null() { return 0; }
            write_npc_gfx_look(fd, nd, 11);
            wfifob(fd, 54, 1);
            wfifow(fd, 55, swap16(graphic as u16));
            wfifob(fd, 57, color as u8);
            wfifow(fd, 58, swap16(dialog_len as u16));
            libc::strcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, 60) as *mut c_char,
                dialog as *const c_char,
            );
            wfifob(fd, dialog_len + 60, size as u8);
            let mut len = dialog_len;
            for x in 1..=(size as usize) {
                let entry = *menu.add(x);
                let entry_len = cstrlen(entry as *const c_char);
                wfifob(fd, len + 61, entry_len as u8);
                libc::strcpy(
                    crate::ffi::session::rust_session_wdata_ptr(fd, len + 62) as *mut c_char,
                    entry as *const c_char,
                );
                len += entry_len + 1;
                wfifow(fd, len + 61, swap16(x as u16));
                len += 2;
            }
            wfifow(fd, 1, swap16((len + 58) as u16));
        }
        _ => {}
    }

    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_scriptmenuseq ───────────────────────────────────────────────────────

/// Send sequential NPC menu dialog.  Mirrors `clif_scriptmenuseq` in
/// `c_src/map_parse.c`.
#[no_mangle]
pub unsafe extern "C" fn clif_scriptmenuseq(
    sd: *mut MapSessionData,
    id: c_int,
    dialog: *const c_char,
    menu: *mut *const c_char,
    size: c_int,
    previous: c_int,
    next: c_int,
) -> c_int {
    let fd         = (*sd).fd;
    let graphic_id = (*sd).npc_g;
    let color      = (*sd).npc_gc;
    let nd         = map_id2npc(id as c_uint) as *mut NpcData;
    let dialog_type = (*sd).dialogtype;

    if !nd.is_null() {
        (*nd).lastaction = libc::time(std::ptr::null_mut()) as c_uint;
    }

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    let dialog_len = cstrlen(dialog);

    wfifohead(fd, 65535);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x30);
    wfifob(fd, 4, 0x03);
    wfifob(fd, 5, 0x02);
    wfifob(fd, 6, 0x02);
    wfifol(fd, 7, swap32(id as u32));

    match dialog_type {
        0 => {
            if graphic_id == 0 {
                wfifob(fd, 11, 0);
            } else if graphic_id >= 49152 {
                wfifob(fd, 11, 2);
            } else {
                wfifob(fd, 11, 1);
            }
            wfifob(fd, 12, 1);
            wfifow(fd, 13, swap16(graphic_id as u16));
            wfifob(fd, 15, color as u8);
            wfifob(fd, 16, 1);
            wfifow(fd, 17, swap16(graphic_id as u16));
            wfifob(fd, 19, color as u8);
            wfifol(fd, 20, swap32(1));
            wfifob(fd, 24, previous as u8);
            wfifob(fd, 25, next as u8);
            wfifow(fd, 26, swap16(dialog_len as u16));
            libc::strcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, 28) as *mut c_char,
                dialog,
            );
            let mut len = dialog_len + 1;
            wfifob(fd, len + 27, size as u8);
            len += 1;
            for x in 1..=(size as usize) {
                let entry = *menu.add(x);
                let entry_len = cstrlen(entry);
                wfifob(fd, len + 27, entry_len as u8);
                libc::strcpy(
                    crate::ffi::session::rust_session_wdata_ptr(fd, len + 28) as *mut c_char,
                    entry,
                );
                len += entry_len + 1;
            }
            wfifow(fd, 1, swap16((len + 24) as u16));
        }
        1 => {
            if nd.is_null() { return 0; }
            write_npc_equip_look(fd, nd, 11);
            // type==1 sequential menu uses slightly different offsets than type==0
            // C writes: [55]=0, [56]=1, [55]=graphic (swap), [59]=color, [60..]=dialog etc.
            wfifob(fd, 55, 0);
            wfifob(fd, 56, 1);
            wfifow(fd, 55, swap16(graphic_id as u16));
            wfifob(fd, 59, color as u8);
            wfifol(fd, 60, swap32(1));
            wfifob(fd, 64, previous as u8);
            wfifob(fd, 65, next as u8);
            wfifow(fd, 66, swap16(dialog_len as u16));
            libc::strcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, 68) as *mut c_char,
                dialog,
            );
            let mut len = dialog_len + 68;
            wfifob(fd, len, size as u8);
            len += 1;
            for x in 1..=(size as usize) {
                let entry = *menu.add(x);
                let entry_len = cstrlen(entry);
                wfifob(fd, len, entry_len as u8);
                libc::strcpy(
                    crate::ffi::session::rust_session_wdata_ptr(fd, len + 1) as *mut c_char,
                    entry,
                );
                len += entry_len + 1;
            }
            wfifow(fd, 1, swap16((len + 68) as u16));
        }
        2 => {
            // type==2: player gfx look
            let sd_ref = &*sd;
            let g = &sd_ref.gfx;
            wfifob(fd, 11, 1);
            wfifow(fd, 12, swap16(sd_ref.status.sex as u16));
            wfifob(fd, 14, sd_ref.status.state as u8);
            wfifob(fd, 15, 0);
            wfifow(fd, 16, swap16(g.armor));
            wfifob(fd, 18, 0);
            wfifob(fd, 19, g.face);
            wfifob(fd, 20, g.hair);
            wfifob(fd, 21, g.chair);
            wfifob(fd, 22, g.cface);
            wfifob(fd, 23, g.cskin);

            // armor
            wfifow(fd, 24, swap16(g.armor));
            wfifob(fd, 26, g.carmor);
            // weap
            wfifow(fd, 27, swap16(g.weapon));
            wfifob(fd, 29, g.cweapon);
            // shield
            wfifow(fd, 30, swap16(g.shield));
            wfifob(fd, 32, g.cshield);
            // helm
            if g.helm == 65535 {
                wfifob(fd, 33, 0);
                wfifob(fd, 34, 0xFF);
                wfifob(fd, 35, 0);
            } else {
                wfifob(fd, 33, 1);
                wfifob(fd, 34, g.helm as u8);
                wfifob(fd, 35, g.chelm);
            }
            // faceacc
            wfifow(fd, 36, swap16(g.face_acc));
            wfifob(fd, 38, g.cface_acc);
            // crown
            if g.crown == 65535 {
                wfifow(fd, 39, 0xFFFF);
                wfifob(fd, 41, 0);
            } else {
                wfifob(fd, 33, 0);
                wfifow(fd, 39, swap16(g.crown));
                wfifob(fd, 41, g.ccrown);
            }
            // faceacctwo
            wfifow(fd, 42, swap16(g.face_acc_t));
            wfifob(fd, 44, g.cface_acc_t);
            // mantle
            wfifow(fd, 45, swap16(g.mantle));
            wfifob(fd, 47, g.cmantle);
            // necklace
            wfifow(fd, 48, swap16(g.necklace));
            wfifob(fd, 50, g.cnecklace);
            // boots
            wfifow(fd, 51, swap16(g.boots));
            wfifob(fd, 53, g.cboots);

            wfifob(fd, 55, 0);
            wfifob(fd, 56, 1);
            wfifow(fd, 55, swap16(graphic_id as u16));
            wfifob(fd, 59, color as u8);
            wfifol(fd, 60, swap32(1));
            wfifob(fd, 64, previous as u8);
            wfifob(fd, 65, next as u8);
            wfifow(fd, 66, swap16(dialog_len as u16));
            libc::strcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, 68) as *mut c_char,
                dialog,
            );
            let mut len = dialog_len + 68;
            wfifob(fd, len, size as u8);
            len += 1;
            for x in 1..=(size as usize) {
                let entry = *menu.add(x);
                let entry_len = cstrlen(entry);
                wfifob(fd, len, entry_len as u8);
                libc::strcpy(
                    crate::ffi::session::rust_session_wdata_ptr(fd, len + 1) as *mut c_char,
                    entry,
                );
                len += entry_len + 1;
            }
            wfifow(fd, 1, swap16((len + 68) as u16));
        }
        _ => {}
    }

    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_inputseq ────────────────────────────────────────────────────────────

/// Send sequential NPC input dialog.  Mirrors `clif_inputseq` in
/// `c_src/map_parse.c`.
#[no_mangle]
pub unsafe extern "C" fn clif_inputseq(
    sd: *mut MapSessionData,
    id: c_int,
    dialog: *const c_char,
    dialog2: *const c_char,
    dialog3: *const c_char,
    menu: *mut *const c_char,
    size: c_int,
    previous: c_int,
    next: c_int,
) -> c_int {
    let fd         = (*sd).fd;
    let graphic_id = (*sd).npc_g;
    let color      = (*sd).npc_gc;
    let nd         = map_id2npc(id as c_uint) as *mut NpcData;

    if !nd.is_null() {
        (*nd).lastaction = libc::time(std::ptr::null_mut()) as c_uint;
    }

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    let dialog_len  = cstrlen(dialog);
    let dialog2_len = cstrlen(dialog2);
    let dialog3_len = cstrlen(dialog3);
    let _ = (menu, size); // these are declared in C but not used in this path

    wfifohead(fd, 65535);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x30);
    wfifob(fd, 4, 0x5C);
    wfifol(fd, 7, swap32(id as u32));
    wfifob(fd, 5, 0x04);
    wfifob(fd, 6, 0x04);

    if graphic_id == 0 {
        wfifob(fd, 11, 0);
    } else if graphic_id >= 49152 {
        wfifob(fd, 11, 2);
    } else {
        wfifob(fd, 11, 1);
    }

    wfifob(fd, 12, 1);
    wfifow(fd, 13, swap16(graphic_id as u16));
    wfifob(fd, 15, color as u8);
    wfifob(fd, 16, 1);
    wfifow(fd, 17, swap16(graphic_id as u16));
    wfifob(fd, 19, color as u8);
    wfifol(fd, 20, swap32(1));
    wfifob(fd, 24, previous as u8);
    wfifob(fd, 25, next as u8);

    wfifow(fd, 26, swap16(dialog_len as u16));
    libc::strcpy(
        crate::ffi::session::rust_session_wdata_ptr(fd, 28) as *mut c_char,
        dialog,
    );
    let mut len = dialog_len + 28;

    wfifob(fd, len, dialog2_len as u8);
    libc::strcpy(
        crate::ffi::session::rust_session_wdata_ptr(fd, len + 1) as *mut c_char,
        dialog2,
    );
    len += dialog2_len + 1;

    wfifob(fd, len, 42);
    len += 1;
    wfifob(fd, len, dialog3_len as u8);
    libc::strcpy(
        crate::ffi::session::rust_session_wdata_ptr(fd, len + 1) as *mut c_char,
        dialog3,
    );
    len += dialog3_len + 3;

    wfifow(fd, 1, swap16(len as u16));
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_handle_clickgetinfo ─────────────────────────────────────────────────

// FLOOR subtype constant — mirrors `enum { SCRIPT, FLOOR }` in map_parse.h
const FLOOR: c_int = 1;

/// Handle a click/getinfo request from the client.  Mirrors
/// `clif_handle_clickgetinfo` in `c_src/map_parse.c`.
#[no_mangle]
pub unsafe extern "C" fn clif_handle_clickgetinfo(sd: *mut MapSessionData) -> c_int {
    let fd = (*sd).fd;

    let bl: *mut BlockList = if rfifol(fd, 6) == 0 {
        map_id2bl((*sd).last_click)
    } else {
        let raw_id = swap32(rfifol(fd, 6));
        if raw_id == 0xFFFFFFFE {
            // subpath chat toggle
            if (*sd).status.subpath_chat == 0 {
                (*sd).status.subpath_chat = 1;
                clif_sendminitext(sd, b"Subpath Chat: ON\0".as_ptr() as *const c_char);
            } else {
                (*sd).status.subpath_chat = 0;
                clif_sendminitext(sd, b"Subpath Chat: OFF\0".as_ptr() as *const c_char);
            }
            return 0;
        }
        map_id2bl(raw_id)
    };

    if bl.is_null() {
        return 0;
    }

    let sd_ref = &*sd;
    let bl_type = (*bl).bl_type as c_int;

    if bl_type == BL_PC {
        let tsd = map_id2sd((*bl).id);
        if !tsd.is_null() {
            let tsd_ref = &*tsd;
            // CheckProximity: same map, within 21 tiles
            if (*bl).m == sd_ref.bl.m
                && (sd_ref.bl.x as i32 - tsd_ref.bl.x as i32).abs() <= 21
                && (sd_ref.bl.y as i32 - tsd_ref.bl.y as i32).abs() <= 21
            {
                if sd_ref.status.gm_level != 0
                    || (tsd_ref.optFlags & 64 == 0      // !optFlag_noclick
                        && tsd_ref.optFlags & 32 == 0)  // !optFlag_stealth
                {
                    sl_doscript_simple(b"onClick\0".as_ptr() as *const c_char, std::ptr::null(), &sd_ref.bl as *const _ as *mut BlockList);
                }
            }
        }
        clif_clickonplayer(sd, bl);
    } else if bl_type == BL_NPC {
        let nd = bl as *mut NpcData;
        let mut radius = 10i32;
        if (*bl).subtype as c_int == FLOOR { radius = 0; }

        // F1 NPC: map id 0 always accessible; otherwise check proximity
        let same_map_or_f1 = (*bl).m == 0
            || ((*bl).m == sd_ref.bl.m
                && (sd_ref.bl.x as i32 - (*bl).x as i32).abs() <= radius
                && (sd_ref.bl.y as i32 - (*bl).y as i32).abs() <= radius);

        if same_map_or_f1 {
            (*sd).last_click = (*bl).id;
            rust_sl_async_freeco(sd as *mut c_void);

            if (*sd).status.karma <= -3.0f32 {
                let nd_name = (*nd).name.as_ptr();
                let is_f1npc = libc::strcmp(nd_name, b"f1npc\0".as_ptr() as *const c_char) == 0;
                let is_totem = libc::strcmp(nd_name, b"totem_npc\0".as_ptr() as *const c_char) == 0;
                if !is_f1npc && !is_totem {
                    clif_scriptmes(sd, (*bl).id as c_int, b"Go away scum!\0".as_ptr() as *const c_char, 0, 0);
                    return 0;
                }
            }

            sl_doscript_2((*nd).name.as_ptr() as *const c_char, b"click\0".as_ptr() as *const c_char, &sd_ref.bl as *const _ as *mut BlockList, bl);
        }
    } else if bl_type == BL_MOB {
        // cast block_list* → MobSpawnData* (bl is always first field)
        let mob = bl as *mut MobSpawnData;
        let mut radius = 10i32;
        // mob->data->type == 3 → radius 0
        if !(*mob).data.is_null() && (*(*mob).data).mobtype == 3 {
            radius = 0;
        }

        // proximity check: same map, within radius tiles
        if (*bl).m == sd_ref.bl.m
            && (sd_ref.bl.x as i32 - (*bl).x as i32).abs() <= radius
            && (sd_ref.bl.y as i32 - (*bl).y as i32).abs() <= radius
        {
            (*sd).last_click = (*bl).id;
            rust_sl_async_freeco(sd as *mut c_void);
            sl_doscript_2(b"onLook\0".as_ptr() as *const c_char, std::ptr::null(), &sd_ref.bl as *const _ as *mut BlockList, bl);
            if !(*mob).data.is_null() {
                sl_doscript_2((*(*mob).data).yname.as_ptr() as *const c_char, b"click\0".as_ptr() as *const c_char, &sd_ref.bl as *const _ as *mut BlockList, bl);
            }
        }
    }

    0
}

// ─── clif_buydialog ───────────────────────────────────────────────────────────

/// Send NPC buy dialog.  Mirrors `clif_buydialog` in `c_src/map_parse.c`.
#[no_mangle]
pub unsafe extern "C" fn clif_buydialog(
    sd: *mut MapSessionData,
    id: c_uint,
    dialog: *const c_char,
    item: *mut Item,
    price: *mut c_int,
    count: c_int,
) -> c_int {
    let fd      = (*sd).fd;
    let graphic = (*sd).npc_g;
    let color   = (*sd).npc_gc;

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        libc::free(item as *mut libc::c_void);
        return 0;
    }

    let dialog_len = cstrlen(dialog);

    wfifohead(fd, 65535);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x2F);
    wfifob(fd, 5, 4);
    wfifob(fd, 6, 2);
    wfifol(fd, 7, swap32(id));

    if graphic > 0 {
        if graphic > 49152 {
            wfifob(fd, 11, 2);
        } else {
            wfifob(fd, 11, 1);
        }
        wfifob(fd, 12, 1);
        wfifow(fd, 13, swap16(graphic as u16));
        wfifob(fd, 15, color as u8);
        wfifob(fd, 16, 1);
        wfifow(fd, 17, swap16(graphic as u16));
        wfifob(fd, 19, color as u8);

        wfifow(fd, 20, swap16(dialog_len as u16));
        libc::strcpy(
            crate::ffi::session::rust_session_wdata_ptr(fd, 22) as *mut c_char,
            dialog,
        );
        let mut len = dialog_len;
        wfifow(fd, len + 22, dialog_len as u16); // NOTE: C writes strlen() without SWAP16 here
        len += 2;
        wfifow(fd, len + 22, swap16(count as u16));
        len += 2;

        for x in 0..(count as usize) {
            let it = &*item.add(x);
            let mut name_buf = [0u8; 64];

            if it.custom_icon > 0 {
                wfifow(fd, len + 22, swap16((it.custom_icon + 49152) as u16));
                wfifob(fd, len + 24, it.custom_icon_color as u8);
            } else {
                wfifow(fd, len + 22, swap16(rust_itemdb_icon(it.id) as u16));
                wfifob(fd, len + 24, rust_itemdb_iconcolor(it.id) as u8);
            }
            len += 3;
            wfifol(fd, len + 22, swap32(*price.add(x) as u32));
            len += 4;

            // Build display name
            if it.real_name[0] != 0 {
                libc::snprintf(
                    name_buf.as_mut_ptr() as *mut c_char,
                    64,
                    b"%s\0".as_ptr() as *const c_char,
                    it.real_name.as_ptr(),
                );
            } else {
                libc::snprintf(
                    name_buf.as_mut_ptr() as *mut c_char,
                    64,
                    b"%s\0".as_ptr() as *const c_char,
                    rust_itemdb_name(it.id),
                );
            }
            if it.owner != 0 {
                let cur_len = libc::strlen(name_buf.as_ptr() as *const c_char);
                libc::snprintf(
                    name_buf.as_mut_ptr().add(cur_len) as *mut c_char,
                    64usize.saturating_sub(cur_len),
                    b" - BONDED\0".as_ptr() as *const c_char,
                );
            }

            let name_len = libc::strlen(name_buf.as_ptr() as *const c_char);
            wfifob(fd, len + 22, name_len as u8);
            libc::strcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, len + 23) as *mut c_char,
                name_buf.as_ptr() as *const c_char,
            );
            len += name_len + 1;

            // Build buy-text / class description
            let mut buff_buf = [0u8; 64];
            let buytext_ptr = rust_itemdb_buytext(it.id);
            if !buytext_ptr.is_null() && *buytext_ptr != 0 {
                libc::strcpy(buff_buf.as_mut_ptr() as *mut c_char, buytext_ptr);
            } else if it.buytext[0] != 0 {
                libc::strcpy(
                    buff_buf.as_mut_ptr() as *mut c_char,
                    it.buytext.as_ptr() as *const c_char,
                );
            } else {
                let path = rust_classdb_name(
                    rust_itemdb_class(it.id),
                    rust_itemdb_rank(it.id),
                );
                libc::snprintf(
                    buff_buf.as_mut_ptr() as *mut c_char,
                    64,
                    b"%s level %u\0".as_ptr() as *const c_char,
                    path,
                    rust_itemdb_level(it.id) as c_uint,
                );
            }

            let buff_len = libc::strlen(buff_buf.as_ptr() as *const c_char);
            wfifob(fd, len + 22, buff_len as u8);
            libc::memcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, len + 23) as *mut c_void,
                buff_buf.as_ptr() as *const c_void,
                buff_len,
            );
            len += buff_len + 1;
        }

        wfifow(fd, 1, swap16((len + 19) as u16));
        wfifoset(fd, encrypt(fd) as usize);
    } else {
        // graphic == 0: show NPC equip look
        let nd = map_id2npc(id) as *mut NpcData;
        if nd.is_null() {
            libc::free(item as *mut libc::c_void);
            return 0;
        }
        write_npc_equip_look(fd, nd, 11);

        wfifob(fd, 54, 1);
        wfifow(fd, 55, swap16(graphic as u16));
        wfifob(fd, 57, color as u8);

        wfifow(fd, 60, swap16(dialog_len as u16));
        libc::strcpy(
            crate::ffi::session::rust_session_wdata_ptr(fd, 62) as *mut c_char,
            dialog,
        );
        let mut len = dialog_len;
        wfifow(fd, len + 62, dialog_len as u16);
        len += 2;
        wfifow(fd, len + 62, swap16(count as u16));
        len += 2;

        for x in 0..(count as usize) {
            let it = &*item.add(x);
            let mut name_buf = [0u8; 64];

            if it.custom_icon > 0 {
                wfifow(fd, len + 62, swap16(it.custom_icon as u16));
                wfifob(fd, len + 64, it.custom_icon_color as u8);
            } else {
                wfifow(fd, len + 62, swap16(rust_itemdb_icon(it.id) as u16));
                wfifob(fd, len + 64, rust_itemdb_iconcolor(it.id) as u8);
            }
            len += 3;
            wfifol(fd, len + 62, swap32(*price.add(x) as u32));
            len += 4;

            if it.real_name[0] != 0 {
                libc::strcpy(
                    name_buf.as_mut_ptr() as *mut c_char,
                    it.real_name.as_ptr(),
                );
            } else {
                libc::strcpy(
                    name_buf.as_mut_ptr() as *mut c_char,
                    rust_itemdb_name(it.id),
                );
            }
            if it.owner != 0 {
                let cur_len = libc::strlen(name_buf.as_ptr() as *const c_char);
                libc::snprintf(
                    name_buf.as_mut_ptr().add(cur_len) as *mut c_char,
                    64usize.saturating_sub(cur_len),
                    b" - BONDED\0".as_ptr() as *const c_char,
                );
            }

            let name_len = libc::strlen(name_buf.as_ptr() as *const c_char);
            wfifob(fd, len + 62, name_len as u8);
            libc::strcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, len + 63) as *mut c_char,
                name_buf.as_ptr() as *const c_char,
            );
            len += name_len + 1;

            let mut buff_buf = [0u8; 64];
            let buytext_ptr = rust_itemdb_buytext(it.id);
            if !buytext_ptr.is_null() && *buytext_ptr != 0 {
                libc::strcpy(buff_buf.as_mut_ptr() as *mut c_char, buytext_ptr);
            } else if it.buytext[0] != 0 {
                libc::strcpy(
                    buff_buf.as_mut_ptr() as *mut c_char,
                    it.buytext.as_ptr() as *const c_char,
                );
            } else {
                let path = rust_classdb_name(
                    rust_itemdb_class(it.id),
                    rust_itemdb_rank(it.id),
                );
                libc::snprintf(
                    buff_buf.as_mut_ptr() as *mut c_char,
                    64,
                    b"%s level %u\0".as_ptr() as *const c_char,
                    path,
                    rust_itemdb_level(it.id) as c_uint,
                );
            }

            let buff_len = libc::strlen(buff_buf.as_ptr() as *const c_char);
            wfifob(fd, len + 62, buff_len as u8);
            libc::memcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, len + 63) as *mut c_void,
                buff_buf.as_ptr() as *const c_void,
                buff_len,
            );
            len += buff_len + 1;
        }
        wfifow(fd, 1, swap16((len + 63) as u16));
        wfifoset(fd, encrypt(fd) as usize);
    }

    libc::free(item as *mut libc::c_void);
    0
}

// ─── clif_parsebuy ────────────────────────────────────────────────────────────

/// Parse a buy response packet.  Mirrors `clif_parsebuy` in `c_src/map_parse.c`.
#[no_mangle]
pub unsafe extern "C" fn clif_parsebuy(sd: *mut MapSessionData) -> c_int {
    let fd = (*sd).fd;
    let item_name_len = rfifob(fd, 12) as usize;
    let mut itemname = [0u8; 255];
    libc::memcpy(
        itemname.as_mut_ptr() as *mut c_void,
        rfifop(fd, 13) as *const c_void,
        item_name_len,
    );
    if itemname[0] != 0 {
        rust_sl_resumebuy(itemname.as_ptr() as *const c_char, sd as *mut c_void);
    }
    0
}

// ─── clif_selldialog ─────────────────────────────────────────────────────────

/// Send NPC sell dialog.  Mirrors `clif_selldialog` in `c_src/map_parse.c`.
#[no_mangle]
pub unsafe extern "C" fn clif_selldialog(
    sd: *mut MapSessionData,
    id: c_uint,
    dialog: *const c_char,
    item: *const c_int,
    count: c_int,
) -> c_int {
    let fd      = (*sd).fd;
    let graphic = (*sd).npc_g;
    let color   = (*sd).npc_gc;

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    let dialog_len = cstrlen(dialog);

    wfifohead(fd, 65535);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x2F);
    wfifob(fd, 4, 3);
    wfifob(fd, 5, 5);
    wfifob(fd, 6, 4);
    wfifol(fd, 7, swap32(id));

    if graphic > 0 {
        if graphic > 49152 {
            wfifob(fd, 11, 2);
        } else {
            wfifob(fd, 11, 1);
        }
        wfifob(fd, 12, 1);
        wfifow(fd, 13, swap16(graphic as u16));
        wfifob(fd, 15, color as u8);
        wfifob(fd, 16, 1);
        wfifow(fd, 17, swap16(graphic as u16));
        wfifob(fd, 19, color as u8);

        wfifow(fd, 20, swap16(dialog_len as u16));
        libc::strcpy(
            crate::ffi::session::rust_session_wdata_ptr(fd, 22) as *mut c_char,
            dialog,
        );
        let mut len = dialog_len + 2;
        wfifow(fd, len + 20, swap16(dialog_len as u16));
        len += 2;
        wfifob(fd, len + 20, count as u8);
        len += 1;
        for i in 0..(count as usize) {
            wfifob(fd, len + 20, (*item.add(i) + 1) as u8);
            len += 1;
        }
        wfifow(fd, 1, swap16((len + 17) as u16));
        wfifoset(fd, encrypt(fd) as usize);
    } else {
        let nd = map_id2npc(id) as *mut NpcData;
        if nd.is_null() { return 0; }
        write_npc_equip_look(fd, nd, 11);

        wfifob(fd, 54, 1);
        wfifow(fd, 55, swap16(graphic as u16));
        wfifob(fd, 57, color as u8);

        wfifow(fd, 60, swap16(dialog_len as u16));
        libc::strcpy(
            crate::ffi::session::rust_session_wdata_ptr(fd, 62) as *mut c_char,
            dialog,
        );
        let mut len = dialog_len;
        wfifow(fd, len + 62, dialog_len as u16);
        len += 2;
        wfifob(fd, len + 62, count as u8);
        len += 1;
        for i in 0..(count as usize) {
            wfifob(fd, len + 62, (*item.add(i) + 1) as u8);
            len += 1;
        }
        wfifow(fd, 1, swap16((len + 62) as u16));
        wfifoset(fd, encrypt(fd) as usize);
    }

    0
}

// ─── clif_parsesell ───────────────────────────────────────────────────────────

/// Parse a sell response packet.  Mirrors `clif_parsesell` in
/// `c_src/map_parse.c`.
#[no_mangle]
pub unsafe extern "C" fn clif_parsesell(sd: *mut MapSessionData) -> c_int {
    let fd = (*sd).fd;
    rust_sl_resumesell(rfifob(fd, 12) as c_uint, sd as *mut c_void);
    0
}

// ─── clif_input ───────────────────────────────────────────────────────────────

/// Send NPC input dialog.  Mirrors `clif_input` in `c_src/map_parse.c`.
#[no_mangle]
pub unsafe extern "C" fn clif_input(
    sd: *mut MapSessionData,
    id: c_int,
    dialog: *const c_char,
    item: *const c_char,
) -> c_int {
    let fd      = (*sd).fd;
    let graphic = (*sd).npc_g;
    let color   = (*sd).npc_gc;
    let nd      = map_id2npc(id as c_uint) as *mut NpcData;
    let dialog_type = (*sd).dialogtype;

    if !nd.is_null() {
        (*nd).lastaction = libc::time(std::ptr::null_mut()) as c_uint;
    }

    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    let dialog_len = cstrlen(dialog);
    let item_len   = cstrlen(item);

    wfifohead(fd, 1000);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x2F);
    wfifob(fd, 5, 3);
    wfifob(fd, 6, 3);
    wfifol(fd, 7, swap32(id as u32));

    match dialog_type {
        0 => {
            if graphic == 0 {
                wfifob(fd, 11, 0);
            } else if graphic >= 49152 {
                wfifob(fd, 11, 2);
            } else {
                wfifob(fd, 11, 1);
            }
            wfifob(fd, 12, 1);
            wfifow(fd, 13, swap16(graphic as u16));
            wfifob(fd, 15, color as u8);
            wfifob(fd, 16, 1);
            wfifow(fd, 17, swap16(graphic as u16));
            wfifob(fd, 19, color as u8);

            wfifow(fd, 20, swap16(dialog_len as u16));
            libc::strcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, 22) as *mut c_char,
                dialog,
            );
            let mut len = dialog_len;
            wfifob(fd, len + 22, item_len as u8);
            len += 1;
            libc::strcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, len + 23) as *mut c_char,
                item,
            );
            len += item_len + 1;
            wfifow(fd, len + 22, swap16(76));
            len += 2;

            wfifow(fd, 1, swap16((len + 19) as u16));
            wfifoset(fd, encrypt(fd) as usize);
        }
        1 => {
            if nd.is_null() { return 0; }
            write_npc_equip_look(fd, nd, 11);
            wfifob(fd, 54, 1);
            wfifow(fd, 55, swap16(graphic as u16));
            wfifob(fd, 57, color as u8);
            wfifow(fd, 58, swap16(dialog_len as u16));
            libc::strcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, 60) as *mut c_char,
                dialog,
            );
            let mut len = dialog_len;
            wfifob(fd, len + 60, item_len as u8);
            len += 1;
            libc::strcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, len + 61) as *mut c_char,
                item,
            );
            len += item_len + 1;
            wfifow(fd, len + 60, swap16(76));
            len += 2;

            wfifow(fd, 1, swap16((len + 57) as u16));
            wfifoset(fd, encrypt(fd) as usize);
        }
        2 => {
            if nd.is_null() { return 0; }
            write_npc_gfx_look(fd, nd, 11);
            wfifob(fd, 54, 1);
            wfifow(fd, 55, swap16(graphic as u16));
            wfifob(fd, 57, color as u8);
            wfifow(fd, 58, swap16(dialog_len as u16));
            libc::strcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, 60) as *mut c_char,
                dialog,
            );
            let mut len = dialog_len;
            wfifob(fd, len + 60, item_len as u8);
            len += 1;
            libc::strcpy(
                crate::ffi::session::rust_session_wdata_ptr(fd, len + 61) as *mut c_char,
                item,
            );
            len += item_len + 1;
            wfifow(fd, len + 60, swap16(76));
            len += 2;

            wfifow(fd, 1, swap16((len + 57) as u16));
            wfifoset(fd, encrypt(fd) as usize);
        }
        _ => {}
    }

    0
}

// ─── clif_parseinput ─────────────────────────────────────────────────────────

/// Parse an input response packet.  Mirrors `clif_parseinput` in
/// `c_src/map_parse.c`.
#[no_mangle]
pub unsafe extern "C" fn clif_parseinput(sd: *mut MapSessionData) -> c_int {
    let fd = (*sd).fd;
    let mut output  = [0u8; 256];
    let mut output2 = [0u8; 256];

    let tag_len = rfifob(fd, 12) as usize;
    libc::memcpy(
        output.as_mut_ptr() as *mut c_void,
        rfifop(fd, 13) as *const c_void,
        tag_len,
    );
    let tlen = tag_len + 1;
    let inp_len = rfifob(fd, tlen + 12) as usize;
    libc::memcpy(
        output2.as_mut_ptr() as *mut c_void,
        rfifop(fd, tlen + 13) as *const c_void,
        inp_len,
    );

    rust_sl_resumeinput(
        output.as_ptr() as *const c_char,
        output2.as_ptr() as *const c_char,
        sd as *mut c_void,
    );
    0
}
