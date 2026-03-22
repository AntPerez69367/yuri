//! Covers close-dialog, town list, countdown timer, NPC dialog/menu/input
//! display and parse, shop buy/sell dialogs, and the click-getinfo handler.

#![allow(non_snake_case, clippy::wildcard_imports)]

use super::packet::{
    encrypt, rfifob, rfifol, rfifop, swap16, swap32, wfifob, wfifohead, wfifol, wfifop, wfifoset,
    wfifow,
};
use crate::common::traits::LegacyEntity;
use crate::common::types::Item;
use crate::game::lua::dispatch::dispatch_coro;
use crate::game::npc::NpcData;
use crate::game::pc::{
    MapSessionData, BL_MOB, BL_NPC, BL_PC, EQ_ARMOR, EQ_BOOTS, EQ_COAT, EQ_CROWN, EQ_FACEACC,
    EQ_FACEACCTWO, EQ_HELM, EQ_MANTLE, EQ_NECKLACE, EQ_SHIELD, EQ_WEAP,
};
use crate::game::player::entity::PlayerEntity;
use crate::session::{session_exists, SessionId};

// ─── External C globals ───────────────────────────────────────────────────────

use crate::database::class_db::name as classdb_name;
use crate::database::item_db;
use crate::game::client::visual::clif_clickonplayer;
use crate::game::map_parse::chat::clif_sendminitext;
use crate::game::scripting::{
    sl_async_freeco, sl_resumebuy, sl_resumedialog, sl_resumeinput, sl_resumeinputseq,
    sl_resumemenuseq, sl_resumesell,
};

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
fn sl_doscript_coro(root: &str, method: Option<&str>, id: u32) -> bool {
    dispatch_coro(root, method, &[id])
}

fn sl_doscript_coro_2(root: &str, method: Option<&str>, id1: u32, id2: u32) -> bool {
    dispatch_coro(root, method, &[id1, id2])
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
    wfifob(fd, base_off, 1);
    wfifow(fd, base_off + 1, swap16(nd.sex));
    wfifob(fd, base_off + 3, nd.state as u8);
    wfifob(fd, base_off + 4, 0);
    wfifow(
        fd,
        base_off + 5,
        swap16(nd.equip[EQ_ARMOR as usize].id as u16),
    );
    wfifob(fd, base_off + 7, 0);
    wfifob(fd, base_off + 8, nd.face as u8);
    wfifob(fd, base_off + 9, nd.hair as u8);
    wfifob(fd, base_off + 10, nd.hair_color as u8);
    wfifob(fd, base_off + 11, nd.face_color as u8);
    wfifob(fd, base_off + 12, nd.skin_color as u8);

    // armor (possibly overridden by coat)
    if nd.equip[EQ_ARMOR as usize].id == 0 {
        wfifow(fd, base_off + 13, 0xFFFF);
        wfifob(fd, base_off + 15, 0);
    } else {
        wfifow(
            fd,
            base_off + 13,
            swap16(nd.equip[EQ_ARMOR as usize].id as u16),
        );
        if nd.armor_color != 0 {
            wfifob(fd, base_off + 15, nd.armor_color as u8);
        } else {
            wfifob(
                fd,
                base_off + 15,
                nd.equip[EQ_ARMOR as usize].custom_look_color as u8,
            );
        }
    }
    // coat overrides armor slot
    if nd.equip[EQ_COAT as usize].id != 0 {
        wfifow(
            fd,
            base_off + 13,
            swap16(nd.equip[EQ_COAT as usize].id as u16),
        );
        wfifob(
            fd,
            base_off + 15,
            nd.equip[EQ_COAT as usize].custom_look_color as u8,
        );
    }

    // weap
    if nd.equip[EQ_WEAP as usize].id == 0 {
        wfifow(fd, base_off + 16, 0xFFFF);
        wfifob(fd, base_off + 18, 0);
    } else {
        wfifow(
            fd,
            base_off + 16,
            swap16(nd.equip[EQ_WEAP as usize].id as u16),
        );
        wfifob(
            fd,
            base_off + 18,
            nd.equip[EQ_WEAP as usize].custom_look_color as u8,
        );
    }

    // shield
    if nd.equip[EQ_SHIELD as usize].id == 0 {
        wfifow(fd, base_off + 19, 0xFFFF);
        wfifob(fd, base_off + 21, 0);
    } else {
        wfifow(
            fd,
            base_off + 19,
            swap16(nd.equip[EQ_SHIELD as usize].id as u16),
        );
        wfifob(
            fd,
            base_off + 21,
            nd.equip[EQ_SHIELD as usize].custom_look_color as u8,
        );
    }

    // helm
    if nd.equip[EQ_HELM as usize].id == 0 {
        wfifob(fd, base_off + 22, 0);
        wfifob(fd, base_off + 23, 0xFF);
        wfifob(fd, base_off + 24, 0);
    } else {
        wfifob(fd, base_off + 22, 1);
        wfifob(fd, base_off + 23, nd.equip[EQ_HELM as usize].id as u8);
        wfifob(
            fd,
            base_off + 24,
            nd.equip[EQ_HELM as usize].custom_look_color as u8,
        );
    }

    // faceacc
    if nd.equip[EQ_FACEACC as usize].id == 0 {
        wfifow(fd, base_off + 25, 0xFFFF);
        wfifob(fd, base_off + 27, 0);
    } else {
        wfifow(
            fd,
            base_off + 25,
            swap16(nd.equip[EQ_FACEACC as usize].id as u16),
        );
        wfifob(
            fd,
            base_off + 27,
            nd.equip[EQ_FACEACC as usize].custom_look_color as u8,
        );
    }

    // crown (clears helm-present flag if crown present)
    if nd.equip[EQ_CROWN as usize].id == 0 {
        wfifow(fd, base_off + 28, 0xFFFF);
        wfifob(fd, base_off + 30, 0);
    } else {
        wfifob(fd, base_off + 22, 0); // matches C: clears helm-present flag
        wfifow(
            fd,
            base_off + 28,
            swap16(nd.equip[EQ_CROWN as usize].id as u16),
        );
        wfifob(
            fd,
            base_off + 30,
            nd.equip[EQ_CROWN as usize].custom_look_color as u8,
        );
    }

    // faceacctwo
    if nd.equip[EQ_FACEACCTWO as usize].id == 0 {
        wfifow(fd, base_off + 31, 0xFFFF);
        wfifob(fd, base_off + 33, 0);
    } else {
        wfifow(
            fd,
            base_off + 31,
            swap16(nd.equip[EQ_FACEACCTWO as usize].id as u16),
        );
        wfifob(
            fd,
            base_off + 33,
            nd.equip[EQ_FACEACCTWO as usize].custom_look_color as u8,
        );
    }

    // mantle
    if nd.equip[EQ_MANTLE as usize].id == 0 {
        wfifow(fd, base_off + 34, 0xFFFF);
        wfifob(fd, base_off + 36, 0);
    } else {
        wfifow(
            fd,
            base_off + 34,
            swap16(nd.equip[EQ_MANTLE as usize].id as u16),
        );
        wfifob(
            fd,
            base_off + 36,
            nd.equip[EQ_MANTLE as usize].custom_look_color as u8,
        );
    }

    // necklace
    if nd.equip[EQ_NECKLACE as usize].id == 0 {
        wfifow(fd, base_off + 37, 0xFFFF);
        wfifob(fd, base_off + 39, 0);
    } else {
        wfifow(
            fd,
            base_off + 37,
            swap16(nd.equip[EQ_NECKLACE as usize].id as u16),
        );
        wfifob(
            fd,
            base_off + 39,
            nd.equip[EQ_NECKLACE as usize].custom_look_color as u8,
        );
    }

    // boots (falls back to sex)
    if nd.equip[EQ_BOOTS as usize].id == 0 {
        wfifow(fd, base_off + 40, swap16(nd.sex));
        wfifob(fd, base_off + 42, 0);
    } else {
        wfifow(
            fd,
            base_off + 40,
            swap16(nd.equip[EQ_BOOTS as usize].id as u16),
        );
        wfifob(
            fd,
            base_off + 42,
            nd.equip[EQ_BOOTS as usize].custom_look_color as u8,
        );
    }
    // 43 bytes of equip data written starting at base_off
}

/// Write an NPC gfx-viewer look block (type 2) starting at `base_off`.
#[inline]
unsafe fn write_npc_gfx_look(fd: SessionId, nd: *const NpcData, base_off: usize) {
    let nd = &*nd;
    let g = &nd.gfx;
    wfifob(fd, base_off, 1);
    wfifow(fd, base_off + 1, swap16(nd.sex));
    wfifob(fd, base_off + 3, nd.state as u8);
    wfifob(fd, base_off + 4, 0);
    wfifow(fd, base_off + 5, swap16(g.armor));
    wfifob(fd, base_off + 7, 0);
    wfifob(fd, base_off + 8, g.face);
    wfifob(fd, base_off + 9, g.hair);
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
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_closeit(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;

    if !session_exists(fd) {
        return 0;
    }

    wfifohead(fd, 255);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x03);
    let cfg = crate::config::config();
    let login_ip: u32 = cfg
        .login_ip
        .parse::<std::net::Ipv4Addr>()
        .map(|a| u32::from_le_bytes(a.octets()))
        .unwrap_or(0);
    wfifol(fd, 4, swap32(login_ip));
    wfifow(fd, 8, swap16(cfg.login_port as u16));
    wfifob(fd, 10, 0x16);
    wfifow(fd, 11, swap16(9));
    // copy xor_key (up to 9 chars + null) into WFIFOP(sd->fd, 13)
    let xor_bytes = cfg.xor_key.as_bytes();
    let dst = wfifop(fd, 13);
    let xor_len = xor_bytes.len().min(9);
    std::ptr::copy_nonoverlapping(xor_bytes.as_ptr(), dst, xor_len);
    *dst.add(xor_len) = 0;
    let mut len = 11usize;
    let name = pe.read().player.identity.name.clone();
    let name_ptr = name.as_ptr();
    let name_len = cstrlen(name_ptr as *const i8);
    wfifob(fd, len + 11, name_len as u8);
    libc::strcpy(wfifop(fd, len + 12) as *mut i8, name_ptr as *const i8);
    len += name_len + 1;
    // WFIFOL(sd->fd,len+11)=SWAP32(sd->status.id);  // commented-out in C
    len += 4;
    wfifob(fd, 10, len as u8);
    wfifow(fd, 1, swap16((len + 8) as u16));
    // No encryption and no set_packet_indexes — matches original C behavior.
    // The client expects this redirect packet raw.
    wfifoset(fd, len + 11);
    0
}

// ─── clif_sendtowns ───────────────────────────────────────────────────────────

/// Send town list dialog.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendtowns(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;

    if !session_exists(fd) {
        return 0;
    }

    let cfg = crate::config::config();
    let n = cfg.town.len().min(255);

    wfifohead(fd, 0x59);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x59);
    wfifob(fd, 5, 64);
    wfifow(fd, 6, 0);
    wfifob(fd, 8, 34);
    wfifob(fd, 9, n as u8);

    let mut len = 0usize;
    for x in 0..n {
        let town_bytes = cfg.town[x].as_bytes();
        let name_len = town_bytes.len();
        wfifob(fd, len + 10, x as u8);
        wfifob(fd, len + 11, name_len as u8);
        let dst = wfifop(fd, len + 12);
        std::ptr::copy_nonoverlapping(town_bytes.as_ptr(), dst, name_len);
        *dst.add(name_len) = 0;
        len += name_len + 2;
    }

    wfifow(fd, 1, swap16((len + 9) as u16));
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_send_timer ──────────────────────────────────────────────────────────

/// Send a countdown timer packet.  Mirrors `clif_send_timer` in
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_send_timer(pe: &PlayerEntity, timer_type: i8, length: u32) {
    let fd = pe.fd;

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
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parsenpcdialog(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;
    let npc_choice = rfifob(fd, 13) as u32;

    match rfifob(fd, 5) {
        0x01 => {
            // Dialog
            sl_resumedialog(npc_choice, &mut *pe.write() as *mut MapSessionData);
        }
        0x02 => {
            // Special menu
            let npc_menu = rfifob(fd, 15) as i32;
            sl_resumemenuseq(
                npc_choice,
                npc_menu,
                &mut *pe.write() as *mut MapSessionData,
            );
        }
        0x04 => {
            // inputSeq returned input
            if rfifob(fd, 13) != 0x02 {
                sl_async_freeco(&mut *pe.write() as *mut MapSessionData);
                return 1;
            }
            let input_len = rfifob(fd, 15) as usize;
            let mut input = [0u8; 100];
            copy_rfifo_bytes(&mut input, rfifop(fd, 16), input_len);
            sl_resumeinputseq(
                npc_choice,
                input.as_mut_ptr() as *mut i8,
                &mut *pe.write() as *mut MapSessionData,
            );
        }
        _ => {}
    }

    0
}

// ─── clif_scriptmes ───────────────────────────────────────────────────────────

/// Send NPC dialog text.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_scriptmes(
    pe: &PlayerEntity,
    id: i32,
    msg: *const i8,
    previous: i32,
    next: i32,
) -> i32 {
    let fd = pe.fd;
    let graphic_id = pe.read().npc_g;
    let color = pe.read().npc_gc;
    let nd = map_id2npc_local(id as u32);
    let dialog_type = pe.read().dialogtype;

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
            libc::strcpy(wfifop(fd, 28) as *mut i8, msg);
            wfifow(fd, 1, swap16((msg_len + 25) as u16));
        }
        1 => {
            // NPC equip look
            if nd.is_null() {
                return 0;
            }
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
            libc::strcpy(wfifop(fd, 66) as *mut i8, msg);
            wfifow(fd, 1, swap16((msg_len + 63) as u16));
        }
        2 => {
            // NPC gfx look
            if nd.is_null() {
                return 0;
            }
            write_npc_gfx_look(fd, nd, 11);
            wfifob(fd, 54, 1);
            wfifow(fd, 55, swap16(graphic_id as u16));
            wfifob(fd, 57, color as u8);
            wfifol(fd, 58, swap32(1));
            wfifob(fd, 62, previous as u8);
            wfifob(fd, 63, next as u8);
            wfifow(fd, 64, swap16(msg_len as u16));
            libc::strcpy(wfifop(fd, 66) as *mut i8, msg);
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
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_scriptmenu(
    pe: &PlayerEntity,
    id: i32,
    dialog: *mut i8,
    menu: *mut *mut i8,
    size: i32,
) -> i32 {
    let fd = pe.fd;
    let graphic = pe.read().npc_g;
    let color = pe.read().npc_gc;
    let nd = map_id2npc_local(id as u32);
    let dialog_type = pe.read().dialogtype;

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
            libc::strcpy(wfifop(fd, 22) as *mut i8, dialog as *const i8);
            wfifob(fd, dialog_len + 22, size as u8);
            let mut len = dialog_len;
            for x in 1..=(size as usize) {
                let entry = *menu.add(x);
                let entry_len = cstrlen(entry as *const i8);
                wfifob(fd, len + 23, entry_len as u8);
                libc::strcpy(wfifop(fd, len + 24) as *mut i8, entry as *const i8);
                len += entry_len + 1;
                wfifow(fd, len + 23, swap16(x as u16));
                len += 2;
            }
            wfifow(fd, 1, swap16((len + 20) as u16));
        }
        1 => {
            if nd.is_null() {
                return 0;
            }
            write_npc_equip_look(fd, nd, 11);
            wfifob(fd, 54, 1);
            wfifow(fd, 55, swap16(graphic as u16));
            wfifob(fd, 57, color as u8);
            wfifow(fd, 58, swap16(dialog_len as u16));
            libc::strcpy(wfifop(fd, 60) as *mut i8, dialog as *const i8);
            wfifob(fd, dialog_len + 60, size as u8);
            let mut len = dialog_len;
            for x in 1..=(size as usize) {
                let entry = *menu.add(x);
                let entry_len = cstrlen(entry as *const i8);
                wfifob(fd, len + 61, entry_len as u8);
                libc::strcpy(wfifop(fd, len + 62) as *mut i8, entry as *const i8);
                len += entry_len + 1;
                wfifow(fd, len + 61, swap16(x as u16));
                len += 2;
            }
            wfifow(fd, 1, swap16((len + 58) as u16));
        }
        2 => {
            if nd.is_null() {
                return 0;
            }
            write_npc_gfx_look(fd, nd, 11);
            wfifob(fd, 54, 1);
            wfifow(fd, 55, swap16(graphic as u16));
            wfifob(fd, 57, color as u8);
            wfifow(fd, 58, swap16(dialog_len as u16));
            libc::strcpy(wfifop(fd, 60) as *mut i8, dialog as *const i8);
            wfifob(fd, dialog_len + 60, size as u8);
            let mut len = dialog_len;
            for x in 1..=(size as usize) {
                let entry = *menu.add(x);
                let entry_len = cstrlen(entry as *const i8);
                wfifob(fd, len + 61, entry_len as u8);
                libc::strcpy(wfifop(fd, len + 62) as *mut i8, entry as *const i8);
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
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_scriptmenuseq(
    pe: &PlayerEntity,
    id: i32,
    dialog: *const i8,
    menu: *mut *const i8,
    size: i32,
    previous: i32,
    next: i32,
) -> i32 {
    let fd = pe.fd;
    let graphic_id = pe.read().npc_g;
    let color = pe.read().npc_gc;
    let nd = map_id2npc_local(id as u32);
    let dialog_type = pe.read().dialogtype;

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
            libc::strcpy(wfifop(fd, 28) as *mut i8, dialog);
            let mut len = dialog_len + 1;
            wfifob(fd, len + 27, size as u8);
            len += 1;
            for x in 1..=(size as usize) {
                let entry = *menu.add(x);
                let entry_len = cstrlen(entry);
                wfifob(fd, len + 27, entry_len as u8);
                libc::strcpy(wfifop(fd, len + 28) as *mut i8, entry);
                len += entry_len + 1;
            }
            wfifow(fd, 1, swap16((len + 24) as u16));
        }
        1 => {
            if nd.is_null() {
                return 0;
            }
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
            libc::strcpy(wfifop(fd, 68) as *mut i8, dialog);
            let mut len = dialog_len + 68;
            wfifob(fd, len, size as u8);
            len += 1;
            for x in 1..=(size as usize) {
                let entry = *menu.add(x);
                let entry_len = cstrlen(entry);
                wfifob(fd, len, entry_len as u8);
                libc::strcpy(wfifop(fd, len + 1) as *mut i8, entry);
                len += entry_len + 1;
            }
            wfifow(fd, 1, swap16((len + 68) as u16));
        }
        2 => {
            // type==2: player gfx look
            let sd_ref = pe.read();
            let g = sd_ref.gfx;
            wfifob(fd, 11, 1);
            wfifow(fd, 12, swap16(sd_ref.player.identity.sex as u16));
            wfifob(fd, 14, sd_ref.player.combat.state as u8);
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
            libc::strcpy(wfifop(fd, 68) as *mut i8, dialog);
            let mut len = dialog_len + 68;
            wfifob(fd, len, size as u8);
            len += 1;
            for x in 1..=(size as usize) {
                let entry = *menu.add(x);
                let entry_len = cstrlen(entry);
                wfifob(fd, len, entry_len as u8);
                libc::strcpy(wfifop(fd, len + 1) as *mut i8, entry);
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

/// Dialog content pointers passed to `clif_inputseq`.
pub struct DialogContent {
    pub dialog: *const i8,
    pub dialog2: *const i8,
    pub dialog3: *const i8,
    pub menu: *mut *const i8,
    pub size: i32,
    pub previous: i32,
    pub next: i32,
}

/// Send sequential NPC input dialog.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_inputseq(pe: &PlayerEntity, id: i32, content: DialogContent) -> i32 {
    let DialogContent {
        dialog,
        dialog2,
        dialog3,
        menu,
        size,
        previous,
        next,
    } = content;
    let fd = pe.fd;
    let graphic_id = pe.read().npc_g;
    let color = pe.read().npc_gc;
    let nd = map_id2npc_local(id as u32);

    if !nd.is_null() {
        (*nd).lastaction = libc::time(std::ptr::null_mut()) as u32;
    }

    if !session_exists(fd) {
        return 0;
    }

    let dialog_len = cstrlen(dialog);
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
    libc::strcpy(wfifop(fd, 28) as *mut i8, dialog);
    let mut len = dialog_len + 28;

    wfifob(fd, len, dialog2_len as u8);
    libc::strcpy(wfifop(fd, len + 1) as *mut i8, dialog2);
    len += dialog2_len + 1;

    wfifob(fd, len, 42);
    len += 1;
    wfifob(fd, len, dialog3_len as u8);
    libc::strcpy(wfifop(fd, len + 1) as *mut i8, dialog3);
    len += dialog3_len + 3;

    wfifow(fd, 1, swap16(len as u16));
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_handle_clickgetinfo ─────────────────────────────────────────────────

/// Handle a click/getinfo request from the client.  Mirrors
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn clif_handle_clickgetinfo(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;

    let target_id = if rfifol(fd, 6) == 0 {
        pe.read().last_click
    } else {
        let raw_id = swap32(rfifol(fd, 6));
        if raw_id == 0xFFFFFFFE {
            // subpath chat toggle
            if pe.read().player.social.subpath_chat == 0 {
                pe.write().player.social.subpath_chat = 1;
                clif_sendminitext(pe, c"Subpath Chat: ON".as_ptr());
            } else {
                pe.write().player.social.subpath_chat = 0;
                clif_sendminitext(pe, c"Subpath Chat: OFF".as_ptr());
            }
            return 0;
        }
        raw_id
    };

    let Some((pos, bl_type)) = crate::game::map_server::entity_position(target_id) else {
        return 0;
    };

    let bl_type = bl_type as i32;

    if bl_type == BL_PC {
        let tsd = map_id2sd_local(target_id);
        if !tsd.is_null() {
            let tsd_ref = &*tsd;
            // CheckProximity: same map, within 21 tiles
            let pe_m = pe.read().m;
            let pe_x = pe.read().x;
            let pe_y = pe.read().y;
            let pe_gm_level = pe.read().player.identity.gm_level;
            if pos.m == pe_m
                && (pe_x as i32 - tsd_ref.x as i32).abs() <= 21
                && (pe_y as i32 - tsd_ref.y as i32).abs() <= 21
                && (pe_gm_level != 0
                    || (tsd_ref.optFlags & 64 == 0      // !optFlag_noclick
                        && tsd_ref.optFlags & 32 == 0))
            // !optFlag_stealth
            {
                sl_doscript_coro("onClick", None, pe.id);
            }
        }
        clif_clickonplayer(pe, target_id).await;
    } else if bl_type == BL_NPC {
        let Some(arc) = crate::game::map_server::map_id2npc_ref(target_id) else {
            return 0;
        };
        let nd = arc.read();
        let mut radius = 10i32;
        if nd.subtype as i32 == crate::common::constants::entity::SUBTYPE_FLOOR as i32 {
            radius = 0;
        }

        // F1 NPC: map id 0 always accessible; otherwise check proximity
        let pe_m = pe.read().m;
        let pe_x = pe.read().x;
        let pe_y = pe.read().y;
        let same_map_or_f1 = pos.m == 0
            || (pos.m == pe_m
                && (pe_x as i32 - pos.x as i32).abs() <= radius
                && (pe_y as i32 - pos.y as i32).abs() <= radius);

        if same_map_or_f1 {
            pe.write().last_click = target_id;
            sl_async_freeco(&mut *pe.write() as *mut MapSessionData);

            if pe.read().player.social.karma <= -3.0f32 {
                let nd_name = nd.name.as_ptr();
                let is_f1npc = libc::strcmp(nd_name, c"f1npc".as_ptr()) == 0;
                let is_totem = libc::strcmp(nd_name, c"totem_npc".as_ptr()) == 0;
                if !is_f1npc && !is_totem {
                    clif_scriptmes(pe, target_id as i32, c"Go away scum!".as_ptr(), 0, 0);
                    return 0;
                }
            }

            sl_doscript_coro_2(
                crate::game::scripting::carray_to_str(&nd.name),
                Some("click"),
                pe.id,
                nd.id,
            );
        }
    } else if bl_type == BL_MOB {
        let Some(arc) = crate::game::map_server::map_id2mob_ref(target_id) else {
            return 0;
        };
        // Extract needed values under the read guard, then drop before Lua calls.
        let (radius, mob_yname) = {
            let mob = arc.read();
            let mut radius = 10i32;
            let yname: Option<String> = if !mob.data.is_null() {
                if (*mob.data).mobtype == 3 {
                    radius = 0;
                }
                Some(crate::game::scripting::carray_to_str(&(*mob.data).yname).to_owned())
            } else {
                None
            };
            (radius, yname)
        };

        // proximity check: same map, within radius tiles
        let pe_m = pe.read().m;
        let pe_x = pe.read().x;
        let pe_y = pe.read().y;
        if pos.m == pe_m
            && (pe_x as i32 - pos.x as i32).abs() <= radius
            && (pe_y as i32 - pos.y as i32).abs() <= radius
        {
            pe.write().last_click = target_id;
            sl_async_freeco(&mut *pe.write() as *mut MapSessionData);
            sl_doscript_coro_2("onLook", None, pe.id, target_id);
            if let Some(ref yname) = mob_yname {
                sl_doscript_coro_2(yname, Some("click"), pe.id, target_id);
            }
        }
    }

    0
}

// ─── clif_buydialog ───────────────────────────────────────────────────────────

/// Send NPC buy dialog.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_buydialog(
    pe: &PlayerEntity,
    id: u32,
    dialog: *const i8,
    item: *mut Item,
    price: *mut i32,
    count: i32,
) -> i32 {
    let fd = pe.fd;
    let graphic = pe.read().npc_g;
    let color = pe.read().npc_gc;

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
        libc::strcpy(wfifop(fd, 22) as *mut i8, dialog);
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
                    c"%s".as_ptr(),
                    it.real_name.as_ptr(),
                );
            } else {
                libc::snprintf(
                    name_buf.as_mut_ptr() as *mut i8,
                    64,
                    c"%s".as_ptr(),
                    item.name.as_ptr(),
                );
            }
            if it.owner != 0 {
                let cur_len = libc::strlen(name_buf.as_ptr() as *const i8);
                libc::snprintf(
                    name_buf.as_mut_ptr().add(cur_len) as *mut i8,
                    64usize.saturating_sub(cur_len),
                    c" - BONDED".as_ptr(),
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
                let cn = classdb_name(item.class as i32, item.rank);
                let formatted = format!("{} level {}\0", cn, item.level as u32);
                let copy = formatted.len().min(64);
                std::ptr::copy_nonoverlapping(formatted.as_ptr(), buff_buf.as_mut_ptr(), copy);
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
        let nd = map_id2npc_local(id);
        if nd.is_null() {
            // item points into caller's Vec — do not free here.
            return 0;
        }
        write_npc_equip_look(fd, nd, 11);

        wfifob(fd, 54, 1);
        wfifow(fd, 55, swap16(graphic as u16));
        wfifob(fd, 57, color as u8);

        wfifow(fd, 60, swap16(dialog_len as u16));
        libc::strcpy(wfifop(fd, 62) as *mut i8, dialog);
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
                libc::strcpy(name_buf.as_mut_ptr() as *mut i8, it.real_name.as_ptr());
            } else {
                libc::strcpy(name_buf.as_mut_ptr() as *mut i8, item.name.as_ptr());
            }
            if it.owner != 0 {
                let cur_len = libc::strlen(name_buf.as_ptr() as *const i8);
                libc::snprintf(
                    name_buf.as_mut_ptr().add(cur_len) as *mut i8,
                    64usize.saturating_sub(cur_len),
                    c" - BONDED".as_ptr(),
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
                let cn = classdb_name(item.class as i32, item.rank);
                let formatted = format!("{} level {}\0", cn, item.level as u32);
                let copy = formatted.len().min(64);
                std::ptr::copy_nonoverlapping(formatted.as_ptr(), buff_buf.as_mut_ptr(), copy);
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
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parsebuy(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;
    let item_name_len = rfifob(fd, 12) as usize;
    let mut itemname = [0u8; 255];
    libc::memcpy(
        itemname.as_mut_ptr() as *mut std::ffi::c_void,
        rfifop(fd, 13) as *const std::ffi::c_void,
        item_name_len,
    );
    if itemname[0] != 0 {
        sl_resumebuy(
            itemname.as_mut_ptr() as *mut i8,
            &mut *pe.write() as *mut MapSessionData,
        );
    }
    0
}

// ─── clif_selldialog ─────────────────────────────────────────────────────────

/// Send NPC sell dialog.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_selldialog(
    pe: &PlayerEntity,
    id: u32,
    dialog: *const i8,
    item: *const i32,
    count: i32,
) -> i32 {
    let fd = pe.fd;
    let graphic = pe.read().npc_g;
    let color = pe.read().npc_gc;

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
        libc::strcpy(wfifop(fd, 22) as *mut i8, dialog);
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
        let nd = map_id2npc_local(id);
        if nd.is_null() {
            return 0;
        }
        write_npc_equip_look(fd, nd, 11);

        wfifob(fd, 54, 1);
        wfifow(fd, 55, swap16(graphic as u16));
        wfifob(fd, 57, color as u8);

        wfifow(fd, 60, swap16(dialog_len as u16));
        libc::strcpy(wfifop(fd, 62) as *mut i8, dialog);
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
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parsesell(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;
    sl_resumesell(
        rfifob(fd, 12) as u32,
        &mut *pe.write() as *mut MapSessionData,
    );
    0
}

// ─── clif_input ───────────────────────────────────────────────────────────────

/// Send NPC input dialog.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_input(pe: &PlayerEntity, id: i32, dialog: *const i8, item: *const i8) -> i32 {
    let fd = pe.fd;
    let graphic = pe.read().npc_g;
    let color = pe.read().npc_gc;
    let nd = map_id2npc_local(id as u32);
    let dialog_type = pe.read().dialogtype;

    if !nd.is_null() {
        (*nd).lastaction = libc::time(std::ptr::null_mut()) as u32;
    }

    if !session_exists(fd) {
        return 0;
    }

    let dialog_len = cstrlen(dialog);
    let item_len = cstrlen(item);

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
            libc::strcpy(wfifop(fd, 22) as *mut i8, dialog);
            let mut len = dialog_len;
            wfifob(fd, len + 22, item_len as u8);
            len += 1;
            libc::strcpy(wfifop(fd, len + 23) as *mut i8, item);
            len += item_len + 1;
            wfifow(fd, len + 22, swap16(76));
            len += 2;

            wfifow(fd, 1, swap16((len + 19) as u16));
            wfifoset(fd, encrypt(fd) as usize);
        }
        1 => {
            if nd.is_null() {
                return 0;
            }
            write_npc_equip_look(fd, nd, 11);
            wfifob(fd, 54, 1);
            wfifow(fd, 55, swap16(graphic as u16));
            wfifob(fd, 57, color as u8);
            wfifow(fd, 58, swap16(dialog_len as u16));
            libc::strcpy(wfifop(fd, 60) as *mut i8, dialog);
            let mut len = dialog_len;
            wfifob(fd, len + 60, item_len as u8);
            len += 1;
            libc::strcpy(wfifop(fd, len + 61) as *mut i8, item);
            len += item_len + 1;
            wfifow(fd, len + 60, swap16(76));
            len += 2;

            wfifow(fd, 1, swap16((len + 57) as u16));
            wfifoset(fd, encrypt(fd) as usize);
        }
        2 => {
            if nd.is_null() {
                return 0;
            }
            write_npc_gfx_look(fd, nd, 11);
            wfifob(fd, 54, 1);
            wfifow(fd, 55, swap16(graphic as u16));
            wfifob(fd, 57, color as u8);
            wfifow(fd, 58, swap16(dialog_len as u16));
            libc::strcpy(wfifop(fd, 60) as *mut i8, dialog);
            let mut len = dialog_len;
            wfifob(fd, len + 60, item_len as u8);
            len += 1;
            libc::strcpy(wfifop(fd, len + 61) as *mut i8, item);
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
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parseinput(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;
    let mut output = [0u8; 256];
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

    sl_resumeinput(
        output.as_mut_ptr() as *mut i8,
        output2.as_mut_ptr() as *mut i8,
        &mut *pe.write() as *mut MapSessionData,
    );
    0
}
