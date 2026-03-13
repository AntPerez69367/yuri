//! Covers close-dialog, town list, countdown timer, NPC dialog/menu/input
//! display and parse, shop buy/sell dialogs, and the click-getinfo handler.

#![allow(non_snake_case, clippy::wildcard_imports)]


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
use crate::session::{SessionId, session_exists};

use super::packet::{
    encrypt, wfifob, wfifohead, wfifol, wfifop, wfifoset, wfifow,
    rfifob, rfifol, rfifop,
    swap16, swap32,
};

// ─── External C globals ───────────────────────────────────────────────────────



use crate::game::map_server::map_id2bl_ref;
use crate::game::map_parse::chat::clif_sendminitext;
use crate::game::client::visual::clif_clickonplayer;
use crate::game::scripting::{
    rust_sl_resumedialog, rust_sl_resumemenuseq, rust_sl_resumeinputseq,
    rust_sl_resumebuy, rust_sl_resumesell, rust_sl_resumeinput, rust_sl_async_freeco,
};
use crate::database::item_db;
use crate::database::class_db::rust_classdb_name;

// map_id2sd_local: typed lookup returning raw pointer for use in unsafe context.
#[inline]
fn map_id2sd_local(id: u32) -> *mut MapSessionData {
    crate::game::map_server::map_id2sd_pc(id)
        .map(|arc| &mut *arc.write() as *mut MapSessionData)
        .unwrap_or(std::ptr::null_mut())
}
// map_id2npc_local: typed lookup returning raw pointer for use in unsafe context.
#[inline]
fn map_id2npc_local(id: u32) -> *mut crate::game::npc::NpcData {
    crate::game::map_server::map_id2npc_ref(id)
        .map(|arc| &mut *arc.write() as *mut crate::game::npc::NpcData)
        .unwrap_or(std::ptr::null_mut())
}
#[inline]
fn map_id2bl(id: u32) -> *mut BlockList {
    map_id2bl_ref(id)
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

/// Coroutine dispatch: wraps the first BL_PC arg and runs in a Lua coroutine.
/// Use for NPC click/dialog/menu interactions that may yield.
#[allow(dead_code)]
unsafe fn sl_doscript_coro(root: *const i8, method: *const i8, bl: *mut crate::database::map_db::BlockList) -> i32 {
    crate::game::scripting::doscript_coro(root, method, &[bl as *mut _])
}

/// Coroutine dispatch with two block_list arguments.
#[allow(dead_code)]
unsafe fn sl_doscript_coro_2(root: *const i8, method: *const i8, bl1: *mut crate::database::map_db::BlockList, bl2: *mut crate::database::map_db::BlockList) -> i32 {
    crate::game::scripting::doscript_coro(root, method, &[bl1 as *mut _, bl2 as *mut _])
}


// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Returns the byte length of a null-terminated C string (not counting the
/// null terminator).  Mirrors `strlen`.
#[inline]
unsafe fn cstrlen(p: *const i8) -> usize {
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
unsafe fn write_npc_equip_look(fd: SessionId, nd: *const NpcData, base_off: usize) {
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
unsafe fn write_npc_gfx_look(fd: SessionId, nd: *const NpcData, base_off: usize) {
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
pub unsafe fn clif_closeit(sd: *mut MapSessionData) -> i32 {
    let fd = (*sd).fd;

    if !session_exists(fd) {
        return 0;
    }

    wfifohead(fd, 255);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x03);
    let gc = crate::config_globals::global_config();
    wfifol(fd, 4, swap32(gc.login_ip as u32));
    wfifow(fd, 8, swap16(gc.login_port as u16));
    wfifob(fd, 10, 0x16);
    wfifow(fd, 11, swap16(9));
    // copy xor_key (9 chars + null) into WFIFOP(sd->fd, 13)
    libc::strcpy(
        wfifop(fd, 13) as *mut i8,
        gc.xor_key.as_ptr(),
    );
    let mut len = 11usize;
    let name_ptr = (*sd).status.name.as_ptr();
    let name_len = cstrlen(name_ptr as *const i8);
    wfifob(fd, len + 11, name_len as u8);
    libc::strcpy(
        wfifop(fd, len + 12) as *mut i8,
        name_ptr as *const i8,
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

/// Send town list dialog.
pub unsafe fn clif_sendtowns(sd: *mut MapSessionData) -> i32 {
    let fd = (*sd).fd;

    if !session_exists(fd) {
        return 0;
    }

    let gc = crate::config_globals::global_config();
    let n = gc.town_n as usize;

    wfifohead(fd, 0x59);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x59);
    wfifob(fd, 5, 64);
    wfifow(fd, 6, 0);
    wfifob(fd, 8, 34);
    wfifob(fd, 9, n as u8);

    let mut len = 0usize;
    for x in 0..n {
        let name_ptr = gc.towns[x].name.as_ptr();
        let name_len = cstrlen(name_ptr as *const i8);
        wfifob(fd, len + 10, x as u8);
        wfifob(fd, len + 11, name_len as u8);
        libc::strcpy(
            wfifop(fd, len + 12) as *mut i8,
            name_ptr as *const i8,
        );
        len += name_len + 2;
    }

    wfifow(fd, 1, swap16((len + 9) as u16));
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_send_timer ──────────────────────────────────────────────────────────

/// Send a countdown timer packet.  Mirrors `clif_send_timer` in
pub unsafe fn clif_send_timer(
    sd: *mut MapSessionData,
    timer_type: i8,
    length: u32,
) {
    let fd = (*sd).fd;

    if !session_exists(fd) {
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
pub unsafe fn clif_parsenpcdialog(sd: *mut MapSessionData) -> i32 {
    let fd = (*sd).fd;
    let npc_choice = rfifob(fd, 13) as u32;

    match rfifob(fd, 5) {
        0x01 => {
            // Dialog
            rust_sl_resumedialog(npc_choice, sd);
        }
        0x02 => {
            // Special menu
            let npc_menu = rfifob(fd, 15) as i32;
            rust_sl_resumemenuseq(npc_choice, npc_menu, sd);
        }
        0x04 => {
            // inputSeq returned input
            if rfifob(fd, 13) != 0x02 {
                rust_sl_async_freeco(sd);
                return 1;
            }
            let input_len = rfifob(fd, 15) as usize;
            let mut input = [0u8; 100];
            copy_rfifo_bytes(&mut input, rfifop(fd, 16), input_len);
            rust_sl_resumeinputseq(
                npc_choice,
                input.as_mut_ptr() as *mut i8,
                sd,
            );
        }
        _ => {}
    }

    0
}

// ─── clif_scriptmes ───────────────────────────────────────────────────────────

/// Send NPC dialog text.
pub unsafe fn clif_scriptmes(
    sd: *mut MapSessionData,
    id: i32,
    msg: *const i8,
    previous: i32,
    next: i32,
) -> i32 {
    let fd       = (*sd).fd;
    let graphic_id = (*sd).npc_g;
    let color    = (*sd).npc_gc;
    let nd       = map_id2npc_local(id as u32) as *mut NpcData;
    let dialog_type = (*sd).dialogtype;

    if !nd.is_null() {
        (*nd).lastaction = libc::time(std::ptr::null_mut()) as u32;
    }

    if !session_exists(fd) {
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
                wfifop(fd, 28) as *mut i8,
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
                wfifop(fd, 66) as *mut i8,
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
                wfifop(fd, 66) as *mut i8,
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

/// Send NPC dialog menu.
/// (Note: as of C source, this function appears not to be called anywhere.)
pub unsafe fn clif_scriptmenu(
    sd: *mut MapSessionData,
    id: i32,
    dialog: *mut i8,
    menu: *mut *mut i8,
    size: i32,
) -> i32 {
    let fd      = (*sd).fd;
    let graphic = (*sd).npc_g;
    let color   = (*sd).npc_gc;
    let nd      = map_id2npc_local(id as u32) as *mut NpcData;
    let dialog_type = (*sd).dialogtype;

    if !nd.is_null() {
        (*nd).lastaction = libc::time(std::ptr::null_mut()) as u32;
    }

    if !session_exists(fd) {
        return 0;
    }

    let dialog_len = cstrlen(dialog as *const i8);

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
                wfifop(fd, 22) as *mut i8,
                dialog as *const i8,
            );
            wfifob(fd, dialog_len + 22, size as u8);
            let mut len = dialog_len;
            for x in 1..=(size as usize) {
                let entry = *menu.add(x);
                let entry_len = cstrlen(entry as *const i8);
                wfifob(fd, len + 23, entry_len as u8);
                libc::strcpy(
                    wfifop(fd, len + 24) as *mut i8,
                    entry as *const i8,
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
                wfifop(fd, 60) as *mut i8,
                dialog as *const i8,
            );
            wfifob(fd, dialog_len + 60, size as u8);
            let mut len = dialog_len;
            for x in 1..=(size as usize) {
                let entry = *menu.add(x);
                let entry_len = cstrlen(entry as *const i8);
                wfifob(fd, len + 61, entry_len as u8);
                libc::strcpy(
                    wfifop(fd, len + 62) as *mut i8,
                    entry as *const i8,
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
                wfifop(fd, 60) as *mut i8,
                dialog as *const i8,
            );
            wfifob(fd, dialog_len + 60, size as u8);
            let mut len = dialog_len;
            for x in 1..=(size as usize) {
                let entry = *menu.add(x);
                let entry_len = cstrlen(entry as *const i8);
                wfifob(fd, len + 61, entry_len as u8);
                libc::strcpy(
                    wfifop(fd, len + 62) as *mut i8,
                    entry as *const i8,
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
pub unsafe fn clif_scriptmenuseq(
    sd: *mut MapSessionData,
    id: i32,
    dialog: *const i8,
    menu: *mut *const i8,
    size: i32,
    previous: i32,
    next: i32,
) -> i32 {
    let fd         = (*sd).fd;
    let graphic_id = (*sd).npc_g;
    let color      = (*sd).npc_gc;
    let nd         = map_id2npc_local(id as u32) as *mut NpcData;
    let dialog_type = (*sd).dialogtype;

    if !nd.is_null() {
        (*nd).lastaction = libc::time(std::ptr::null_mut()) as u32;
    }

    if !session_exists(fd) {
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
                wfifop(fd, 28) as *mut i8,
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
                    wfifop(fd, len + 28) as *mut i8,
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
                wfifop(fd, 68) as *mut i8,
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
                    wfifop(fd, len + 1) as *mut i8,
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
                wfifop(fd, 68) as *mut i8,
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
                    wfifop(fd, len + 1) as *mut i8,
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
pub unsafe fn clif_inputseq(
    sd: *mut MapSessionData,
    id: i32,
    dialog: *const i8,
    dialog2: *const i8,
    dialog3: *const i8,
    menu: *mut *const i8,
    size: i32,
    previous: i32,
    next: i32,
) -> i32 {
    let fd         = (*sd).fd;
    let graphic_id = (*sd).npc_g;
    let color      = (*sd).npc_gc;
    let nd         = map_id2npc_local(id as u32) as *mut NpcData;

    if !nd.is_null() {
        (*nd).lastaction = libc::time(std::ptr::null_mut()) as u32;
    }

    if !session_exists(fd) {
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
        wfifop(fd, 28) as *mut i8,
        dialog,
    );
    let mut len = dialog_len + 28;

    wfifob(fd, len, dialog2_len as u8);
    libc::strcpy(
        wfifop(fd, len + 1) as *mut i8,
        dialog2,
    );
    len += dialog2_len + 1;

    wfifob(fd, len, 42);
    len += 1;
    wfifob(fd, len, dialog3_len as u8);
    libc::strcpy(
        wfifop(fd, len + 1) as *mut i8,
        dialog3,
    );
    len += dialog3_len + 3;

    wfifow(fd, 1, swap16(len as u16));
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_handle_clickgetinfo ─────────────────────────────────────────────────

// FLOOR subtype constant — mirrors `enum { SCRIPT, FLOOR }` in map_parse.h
const FLOOR: i32 = 1;

/// Handle a click/getinfo request from the client.  Mirrors
pub async unsafe fn clif_handle_clickgetinfo(sd: *mut MapSessionData) -> i32 {
    let fd = (*sd).fd;

    let bl: *mut BlockList = if rfifol(fd, 6) == 0 {
        map_id2bl((*sd).last_click)
    } else {
        let raw_id = swap32(rfifol(fd, 6));
        if raw_id == 0xFFFFFFFE {
            // subpath chat toggle
            if (*sd).status.subpath_chat == 0 {
                (*sd).status.subpath_chat = 1;
                clif_sendminitext(sd, b"Subpath Chat: ON\0".as_ptr() as *const i8);
            } else {
                (*sd).status.subpath_chat = 0;
                clif_sendminitext(sd, b"Subpath Chat: OFF\0".as_ptr() as *const i8);
            }
            return 0;
        }
        map_id2bl(raw_id)
    };

    if bl.is_null() {
        return 0;
    }

    let sd_ref = &*sd;
    let bl_type = (*bl).bl_type as i32;

    if bl_type == BL_PC {
        let tsd = map_id2sd_local((*bl).id);
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
                    sl_doscript_coro(b"onClick\0".as_ptr() as *const i8, std::ptr::null(), &sd_ref.bl as *const _ as *mut BlockList);
                }
            }
        }
        clif_clickonplayer(sd, bl).await;
    } else if bl_type == BL_NPC {
        let nd = bl as *mut NpcData;
        let mut radius = 10i32;
        if (*bl).subtype as i32 == FLOOR { radius = 0; }

        // F1 NPC: map id 0 always accessible; otherwise check proximity
        let same_map_or_f1 = (*bl).m == 0
            || ((*bl).m == sd_ref.bl.m
                && (sd_ref.bl.x as i32 - (*bl).x as i32).abs() <= radius
                && (sd_ref.bl.y as i32 - (*bl).y as i32).abs() <= radius);

        if same_map_or_f1 {
            (*sd).last_click = (*bl).id;
            rust_sl_async_freeco(sd);

            if (*sd).status.karma <= -3.0f32 {
                let nd_name = (*nd).name.as_ptr();
                let is_f1npc = libc::strcmp(nd_name, b"f1npc\0".as_ptr() as *const i8) == 0;
                let is_totem = libc::strcmp(nd_name, b"totem_npc\0".as_ptr() as *const i8) == 0;
                if !is_f1npc && !is_totem {
                    clif_scriptmes(sd, (*bl).id as i32, b"Go away scum!\0".as_ptr() as *const i8, 0, 0);
                    return 0;
                }
            }

            sl_doscript_coro_2((*nd).name.as_ptr() as *const i8, b"click\0".as_ptr() as *const i8, &sd_ref.bl as *const _ as *mut BlockList, bl);
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
            rust_sl_async_freeco(sd);
            sl_doscript_coro_2(b"onLook\0".as_ptr() as *const i8, std::ptr::null(), &sd_ref.bl as *const _ as *mut BlockList, bl);
            if !(*mob).data.is_null() {
                sl_doscript_coro_2((*(*mob).data).yname.as_ptr() as *const i8, b"click\0".as_ptr() as *const i8, &sd_ref.bl as *const _ as *mut BlockList, bl);
            }
        }
    }

    0
}

// ─── clif_buydialog ───────────────────────────────────────────────────────────

/// Send NPC buy dialog.
pub unsafe fn clif_buydialog(
    sd: *mut MapSessionData,
    id: u32,
    dialog: *const i8,
    item: *mut Item,
    price: *mut i32,
    count: i32,
) -> i32 {
    let fd      = (*sd).fd;
    let graphic = (*sd).npc_g;
    let color   = (*sd).npc_gc;

    if !session_exists(fd) {
        // item points into caller's Vec — do not free here.
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
            wfifop(fd, 22) as *mut i8,
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

            let item = item_db::search(it.id);
            if it.custom_icon > 0 {
                wfifow(fd, len + 22, swap16((it.custom_icon + 49152) as u16));
                wfifob(fd, len + 24, it.custom_icon_color as u8);
            } else {
                wfifow(fd, len + 22, swap16(item.icon as u16));
                wfifob(fd, len + 24, item.icon_color as u8);
            }
            len += 3;
            wfifol(fd, len + 22, swap32(*price.add(x) as u32));
            len += 4;

            // Build display name
            if it.real_name[0] != 0 {
                libc::snprintf(
                    name_buf.as_mut_ptr() as *mut i8,
                    64,
                    b"%s\0".as_ptr() as *const i8,
                    it.real_name.as_ptr(),
                );
            } else {
                libc::snprintf(
                    name_buf.as_mut_ptr() as *mut i8,
                    64,
                    b"%s\0".as_ptr() as *const i8,
                    item.name.as_ptr(),
                );
            }
            if it.owner != 0 {
                let cur_len = libc::strlen(name_buf.as_ptr() as *const i8);
                libc::snprintf(
                    name_buf.as_mut_ptr().add(cur_len) as *mut i8,
                    64usize.saturating_sub(cur_len),
                    b" - BONDED\0".as_ptr() as *const i8,
                );
            }

            let name_len = libc::strlen(name_buf.as_ptr() as *const i8);
            wfifob(fd, len + 22, name_len as u8);
            libc::strcpy(
                wfifop(fd, len + 23) as *mut i8,
                name_buf.as_ptr() as *const i8,
            );
            len += name_len + 1;

            // Build buy-text / class description
            let mut buff_buf = [0u8; 64];
            if item.buytext[0] != 0 {
                libc::strcpy(buff_buf.as_mut_ptr() as *mut i8, item.buytext.as_ptr());
            } else if it.buytext[0] != 0 {
                libc::strcpy(
                    buff_buf.as_mut_ptr() as *mut i8,
                    it.buytext.as_ptr() as *const i8,
                );
            } else {
                let path = rust_classdb_name(
                    item.class as i32,
                    item.rank,
                );
                libc::snprintf(
                    buff_buf.as_mut_ptr() as *mut i8,
                    64,
                    b"%s level %u\0".as_ptr() as *const i8,
                    path,
                    item.level as u32,
                );
            }

            let buff_len = libc::strlen(buff_buf.as_ptr() as *const i8);
            wfifob(fd, len + 22, buff_len as u8);
            libc::memcpy(
                wfifop(fd, len + 23) as *mut std::ffi::c_void,
                buff_buf.as_ptr() as *const std::ffi::c_void,
                buff_len,
            );
            len += buff_len + 1;
        }

        wfifow(fd, 1, swap16((len + 19) as u16));
        wfifoset(fd, encrypt(fd) as usize);
    } else {
        // graphic == 0: show NPC equip look
        let nd = map_id2npc_local(id) as *mut NpcData;
        if nd.is_null() {
            // item points into caller's Vec — do not free here.
            return 0;
        }
        write_npc_equip_look(fd, nd, 11);

        wfifob(fd, 54, 1);
        wfifow(fd, 55, swap16(graphic as u16));
        wfifob(fd, 57, color as u8);

        wfifow(fd, 60, swap16(dialog_len as u16));
        libc::strcpy(
            wfifop(fd, 62) as *mut i8,
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

            let item = item_db::search(it.id);
            if it.custom_icon > 0 {
                wfifow(fd, len + 62, swap16(it.custom_icon as u16));
                wfifob(fd, len + 64, it.custom_icon_color as u8);
            } else {
                wfifow(fd, len + 62, swap16(item.icon as u16));
                wfifob(fd, len + 64, item.icon_color as u8);
            }
            len += 3;
            wfifol(fd, len + 62, swap32(*price.add(x) as u32));
            len += 4;

            if it.real_name[0] != 0 {
                libc::strcpy(
                    name_buf.as_mut_ptr() as *mut i8,
                    it.real_name.as_ptr(),
                );
            } else {
                libc::strcpy(
                    name_buf.as_mut_ptr() as *mut i8,
                    item.name.as_ptr(),
                );
            }
            if it.owner != 0 {
                let cur_len = libc::strlen(name_buf.as_ptr() as *const i8);
                libc::snprintf(
                    name_buf.as_mut_ptr().add(cur_len) as *mut i8,
                    64usize.saturating_sub(cur_len),
                    b" - BONDED\0".as_ptr() as *const i8,
                );
            }

            let name_len = libc::strlen(name_buf.as_ptr() as *const i8);
            wfifob(fd, len + 62, name_len as u8);
            libc::strcpy(
                wfifop(fd, len + 63) as *mut i8,
                name_buf.as_ptr() as *const i8,
            );
            len += name_len + 1;

            let mut buff_buf = [0u8; 64];
            if item.buytext[0] != 0 {
                libc::strcpy(buff_buf.as_mut_ptr() as *mut i8, item.buytext.as_ptr());
            } else if it.buytext[0] != 0 {
                libc::strcpy(
                    buff_buf.as_mut_ptr() as *mut i8,
                    it.buytext.as_ptr() as *const i8,
                );
            } else {
                let path = rust_classdb_name(
                    item.class as i32,
                    item.rank,
                );
                libc::snprintf(
                    buff_buf.as_mut_ptr() as *mut i8,
                    64,
                    b"%s level %u\0".as_ptr() as *const i8,
                    path,
                    item.level as u32,
                );
            }

            let buff_len = libc::strlen(buff_buf.as_ptr() as *const i8);
            wfifob(fd, len + 62, buff_len as u8);
            libc::memcpy(
                wfifop(fd, len + 63) as *mut std::ffi::c_void,
                buff_buf.as_ptr() as *const std::ffi::c_void,
                buff_len,
            );
            len += buff_len + 1;
        }
        wfifow(fd, 1, swap16((len + 63) as u16));
        wfifoset(fd, encrypt(fd) as usize);
    }

    // item points into caller's Vec — do not free here.
    0
}

// ─── clif_parsebuy ────────────────────────────────────────────────────────────

/// Parse a buy response packet.
pub unsafe fn clif_parsebuy(sd: *mut MapSessionData) -> i32 {
    let fd = (*sd).fd;
    let item_name_len = rfifob(fd, 12) as usize;
    let mut itemname = [0u8; 255];
    libc::memcpy(
        itemname.as_mut_ptr() as *mut std::ffi::c_void,
        rfifop(fd, 13) as *const std::ffi::c_void,
        item_name_len,
    );
    if itemname[0] != 0 {
        rust_sl_resumebuy(itemname.as_mut_ptr() as *mut i8, sd);
    }
    0
}

// ─── clif_selldialog ─────────────────────────────────────────────────────────

/// Send NPC sell dialog.
pub unsafe fn clif_selldialog(
    sd: *mut MapSessionData,
    id: u32,
    dialog: *const i8,
    item: *const i32,
    count: i32,
) -> i32 {
    let fd      = (*sd).fd;
    let graphic = (*sd).npc_g;
    let color   = (*sd).npc_gc;

    if !session_exists(fd) {
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
            wfifop(fd, 22) as *mut i8,
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
        let nd = map_id2npc_local(id) as *mut NpcData;
        if nd.is_null() { return 0; }
        write_npc_equip_look(fd, nd, 11);

        wfifob(fd, 54, 1);
        wfifow(fd, 55, swap16(graphic as u16));
        wfifob(fd, 57, color as u8);

        wfifow(fd, 60, swap16(dialog_len as u16));
        libc::strcpy(
            wfifop(fd, 62) as *mut i8,
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
pub unsafe fn clif_parsesell(sd: *mut MapSessionData) -> i32 {
    let fd = (*sd).fd;
    rust_sl_resumesell(rfifob(fd, 12) as u32, sd);
    0
}

// ─── clif_input ───────────────────────────────────────────────────────────────

/// Send NPC input dialog.
pub unsafe fn clif_input(
    sd: *mut MapSessionData,
    id: i32,
    dialog: *const i8,
    item: *const i8,
) -> i32 {
    let fd      = (*sd).fd;
    let graphic = (*sd).npc_g;
    let color   = (*sd).npc_gc;
    let nd      = map_id2npc_local(id as u32) as *mut NpcData;
    let dialog_type = (*sd).dialogtype;

    if !nd.is_null() {
        (*nd).lastaction = libc::time(std::ptr::null_mut()) as u32;
    }

    if !session_exists(fd) {
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
                wfifop(fd, 22) as *mut i8,
                dialog,
            );
            let mut len = dialog_len;
            wfifob(fd, len + 22, item_len as u8);
            len += 1;
            libc::strcpy(
                wfifop(fd, len + 23) as *mut i8,
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
                wfifop(fd, 60) as *mut i8,
                dialog,
            );
            let mut len = dialog_len;
            wfifob(fd, len + 60, item_len as u8);
            len += 1;
            libc::strcpy(
                wfifop(fd, len + 61) as *mut i8,
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
                wfifop(fd, 60) as *mut i8,
                dialog,
            );
            let mut len = dialog_len;
            wfifob(fd, len + 60, item_len as u8);
            len += 1;
            libc::strcpy(
                wfifop(fd, len + 61) as *mut i8,
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
pub unsafe fn clif_parseinput(sd: *mut MapSessionData) -> i32 {
    let fd = (*sd).fd;
    let mut output  = [0u8; 256];
    let mut output2 = [0u8; 256];

    let tag_len = rfifob(fd, 12) as usize;
    libc::memcpy(
        output.as_mut_ptr() as *mut std::ffi::c_void,
        rfifop(fd, 13) as *const std::ffi::c_void,
        tag_len,
    );
    let tlen = tag_len + 1;
    let inp_len = rfifob(fd, tlen + 12) as usize;
    libc::memcpy(
        output2.as_mut_ptr() as *mut std::ffi::c_void,
        rfifop(fd, tlen + 13) as *const std::ffi::c_void,
        inp_len,
    );

    rust_sl_resumeinput(
        output.as_mut_ptr() as *mut i8,
        output2.as_mut_ptr() as *mut i8,
        sd,
    );
    0
}
