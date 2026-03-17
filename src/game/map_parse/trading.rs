//!
//! Exchange field types in `MapSessionData` (verified from `src/game/pc.rs`):
//! - `exchange_on: i32`  — flag (not used by these functions directly)
//! - `exchange: PcExchange` — embedded struct with:
//!     - `item: [Item; 52]`
//!     - `item_count: i32`
//!     - `exchange_done: i32`
//!     - `list_count: i32`
//!     - `gold: u32`
//!     - `target: u32` — character ID of exchange partner (NOT a pointer)

#![allow(non_snake_case, clippy::wildcard_imports)]


use crate::session::{session_exists, session_set_eof};
use crate::game::pc::{
    MapSessionData,
    BL_MOB, BL_NPC, BL_PC,
    FLAG_EXCHANGE,
};

// BL_ALL: all block-list types (from map_server.h enum)
const BL_ALL: i32 = 0x0F;
use crate::common::player::inventory::MAX_INVENTORY;

use super::packet::{
    encrypt,
    rfifob, rfifol,
    wfifob, wfifop, wfifow, wfifoset, wfifohead,
};

// ─── optFlag_stealth (from map_server.h) ─────────────────────────────────────

const OPT_FLAG_STEALTH: u64 = 32;


use crate::game::map_parse::chat::clif_sendminitext;
use crate::game::map_parse::player_state::clif_sendstatus;
use crate::game::block_grid;
use crate::game::pc::{
    pc_additem, pc_additemnolog,
    pc_delitem, pc_isinvenspace,
    pc_readglobalreg,
};
use crate::database::item_db;
use crate::database::class_db::name as classdb_name;

/// Dispatch a Lua event with two entity ID arguments.
fn sl_doscript_2(root: &str, method: Option<&str>, id1: u32, id2: u32) -> i32 {
    crate::game::scripting::doscript_blargs_id(root, method, &[id1, id2])
}

fn sl_doscript_coro_2(root: &str, method: Option<&str>, id1: u32, id2: u32) -> i32 {
    crate::game::scripting::doscript_coro_id(root, method, &[id1, id2])
}


// SFLAG_XPMONEY (from map_server.h)
const SFLAG_XPMONEY: i32 = 4;

// Item type constants (from item_db.h)
const ITM_SMOKE:  i32 = 23;
const ITM_BAG:    i32 = 25;
const ITM_MAP:    i32 = 24;
const ITM_QUIVER: i32 = 26;

// ─── string_truncate: mirror of C stringTruncate ─────────────────────────────

/// Truncate a fixed-size C-string buffer to `max_len` characters.
/// Mirrors `stringTruncate(buffer, maxLength)` from map_parse.c line 402.
unsafe fn string_truncate(buf: &mut [i8], max_len: usize) {
    if max_len < buf.len() {
        buf[max_len] = 0;
    }
}

// ─── clif_exchange_cleanup ───────────────────────────────────────────────────

/// Reset the exchange state on one side.  C lines 9316-9321.
pub unsafe fn clif_exchange_cleanup(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }
    let sd = &mut *sd;
    sd.exchange.exchange_done = 0;
    sd.exchange.gold = 0;
    sd.exchange.item_count = 0;
    0
}

// ─── clif_exchange_message ───────────────────────────────────────────────────

/// Send an exchange status message to one player.  C lines 9389-9412.
pub unsafe fn clif_exchange_message(
    sd:      *mut MapSessionData,
    message: *const i8,
    kind:    i32,
    extra:   i32,
) -> i32 {
    if sd.is_null() { return 0; }
    let sd = &*sd;
    let extra = if extra > 1 { 0 } else { extra };

    let msg_len = libc::strlen(message);
    let len = msg_len + 5;   // mirrors C: len = strlen(message) + 5

    if !session_exists(sd.fd) {
        return 0;
    }

    wfifohead(sd.fd, msg_len + 8);
    wfifob(sd.fd, 0, 0xAA);
    wfifob(sd.fd, 3, 0x42);
    wfifob(sd.fd, 4, 0x03);
    wfifob(sd.fd, 5, kind as u8);
    wfifob(sd.fd, 6, extra as u8);
    wfifob(sd.fd, 7, msg_len as u8);
    // copy message bytes into WFIFOP(sd->fd, 8)
    let dst = wfifop(sd.fd, 8) as *mut u8;
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(message as *const u8, dst, msg_len);
    }
    wfifow(sd.fd, 1, (len + 3) as u16);   // SWAP16(len + 3) — big-endian
    let p = wfifop(sd.fd, 1) as *mut u16;
    if !p.is_null() { p.write_unaligned(((len + 3) as u16).to_be()); }
    wfifoset(sd.fd, encrypt(sd.fd) as usize);
    0
}

// ─── clif_exchange_finalize ──────────────────────────────────────────────────

/// Transfer items/gold between both sides and clean up.  C lines 9323-9387.
pub unsafe fn clif_exchange_finalize(
    sd:  *mut MapSessionData,
    tsd: *mut MapSessionData,
) -> i32 {
    if sd.is_null() || tsd.is_null() { return 0; }

    {
        sl_doscript_2("characterLog", Some("exchangeLogWrite"), (*sd).id, (*tsd).id);
    }

    // Transfer sd's items to tsd
    let sd_item_count = (*sd).exchange.item_count as usize;
    for i in 0..sd_item_count {
        let it = (*sd).exchange.item[i];
        pc_additem(tsd, &it as *const _ as *mut _);
    }
    (*tsd).player.inventory.money = (*tsd).player.inventory.money.saturating_add((*sd).exchange.gold);
    (*sd).player.inventory.money  = (*sd).player.inventory.money.saturating_sub((*sd).exchange.gold);
    (*sd).exchange.gold = 0;

    // Transfer tsd's items to sd
    let tsd_item_count = (*tsd).exchange.item_count as usize;
    for i in 0..tsd_item_count {
        let it = (*tsd).exchange.item[i];
        pc_additem(sd, &it as *const _ as *mut _);
    }
    (*sd).player.inventory.money   = (*sd).player.inventory.money.saturating_add((*tsd).exchange.gold);
    (*tsd).player.inventory.money  = (*tsd).player.inventory.money.saturating_sub((*tsd).exchange.gold);
    (*tsd).exchange.gold = 0;

    clif_sendstatus(sd,  SFLAG_XPMONEY);
    clif_sendstatus(tsd, SFLAG_XPMONEY);
    0
}

// ─── clif_exchange_sendok ────────────────────────────────────────────────────

/// Handle one side confirming the exchange.  C lines 9414-9435.
pub unsafe fn clif_exchange_sendok(
    sd:  *mut MapSessionData,
    tsd: *mut MapSessionData,
) -> i32 {
    if sd.is_null() || tsd.is_null() { return 0; }

    if (*tsd).exchange.exchange_done == 1 {
        clif_exchange_finalize(sd, tsd);

        let msg = b"You exchanged, and gave away ownership of the items.\0".as_ptr() as *const i8;
        clif_exchange_message(sd,  msg, 5, 0);
        clif_exchange_message(tsd, msg, 5, 0);

        clif_exchange_cleanup(sd);
        clif_exchange_cleanup(tsd);
    } else {
        (*sd).exchange.exchange_done = 1;
        let msg = b"You exchanged, and gave away ownership of the items.\0".as_ptr() as *const i8;
        clif_exchange_message(tsd, msg, 5, 1);
        clif_exchange_message(sd,  msg, 5, 1);
    }
    0
}

// ─── clif_startexchange ──────────────────────────────────────────────────────

/// Initiate a trade window between two players.  C lines 9545-9634.
pub unsafe fn clif_startexchange(
    sd:     *mut MapSessionData,
    target: u32,
) -> i32 {
    if sd.is_null() { return 0; }

    if target == (*sd).id {
        let msg = b"You move your items from one hand to another, but quickly get bored.\0".as_ptr() as *const i8;
        clif_sendminitext(sd, msg);
        return 0;
    }

    let tsd_arc = match crate::game::map_server::map_id2sd_pc(target) {
        Some(a) => a, None => return 0,
    };
    let mut _tsd_guard = tsd_arc.write();
    let tsd = &mut *_tsd_guard as *mut MapSessionData;

    (*sd).exchange.target  = target;
    (*tsd).exchange.target = (*sd).id;

    if (*tsd).player.appearance.setting_flags as u32 & FLAG_EXCHANGE != 0 {
        let mut buff = [0i8; 256];

        // Build name string for sd (to send to tsd)
        let tsd_class_name = classdb_name((*tsd).player.progression.class as i32, (*tsd).player.progression.mark as i32);
        {
            let tsd_name = &(*tsd).player.identity.name;
            let formatted = format!("{}({})\0", tsd_name, tsd_class_name);
            let copy_len = formatted.len().min(buff.len());
            std::ptr::copy_nonoverlapping(formatted.as_ptr() as *const i8, buff.as_mut_ptr(), copy_len);
        }

        if !session_exists((*sd).fd) {
            return 0;
        }

        wfifohead((*sd).fd, 512);
        wfifob((*sd).fd, 0, 0xAA);
        wfifob((*sd).fd, 3, 0x42);
        wfifob((*sd).fd, 4, 0x03);
        wfifob((*sd).fd, 5, 0x00);
        // WFIFOL(sd->fd, 6) = SWAP32(tsd->bl.id)
        let p = wfifop((*sd).fd, 6) as *mut u32;
        if !p.is_null() { p.write_unaligned((*tsd).id.to_be()); }
        let mut len: usize = 4;
        let buf_len = libc::strlen(buff.as_ptr());
        wfifob((*sd).fd, len + 6, buf_len as u8);
        let dst = wfifop((*sd).fd, len + 7) as *mut u8;
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(buff.as_ptr() as *const u8, dst, buf_len);
        }
        len += buf_len + 1;
        // WFIFOW(sd->fd, len+6) = SWAP16(tsd->status.level)
        let p2 = wfifop((*sd).fd, len + 6) as *mut u16;
        if !p2.is_null() { p2.write_unaligned(((*tsd).player.progression.level as u16).to_be()); }
        len += 2;
        // WFIFOW(sd->fd, 1) = SWAP16(len + 3)
        let ph = wfifop((*sd).fd, 1) as *mut u16;
        if !ph.is_null() { ph.write_unaligned(((len + 3) as u16).to_be()); }
        wfifoset((*sd).fd, encrypt((*sd).fd) as usize);

        if !session_exists((*sd).fd) {
            return 0;
        }

        // Build name string for tsd (to send to sd)
        let sd_class_name = classdb_name((*sd).player.progression.class as i32, (*sd).player.progression.mark as i32);
        {
            let sd_name = &(*sd).player.identity.name;
            let formatted = format!("{}({})\0", sd_name, sd_class_name);
            let copy_len = formatted.len().min(buff.len());
            std::ptr::copy_nonoverlapping(formatted.as_ptr() as *const i8, buff.as_mut_ptr(), copy_len);
        }

        wfifohead((*tsd).fd, 512);
        wfifob((*tsd).fd, 0, 0xAA);
        wfifob((*tsd).fd, 3, 0x42);
        wfifob((*tsd).fd, 4, 0x03);
        wfifob((*tsd).fd, 5, 0x00);
        // WFIFOL(tsd->fd, 6) = SWAP32(sd->bl.id)
        let p3 = wfifop((*tsd).fd, 6) as *mut u32;
        if !p3.is_null() { p3.write_unaligned((*sd).id.to_be()); }
        let mut len: usize = 4;
        let buf_len = libc::strlen(buff.as_ptr());
        wfifob((*tsd).fd, len + 6, buf_len as u8);
        let dst = wfifop((*tsd).fd, len + 7) as *mut u8;
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(buff.as_ptr() as *const u8, dst, buf_len);
        }
        len += buf_len + 1;
        // WFIFOW(tsd->fd, len+6) = SWAP16(sd->status.level)
        let p4 = wfifop((*tsd).fd, len + 6) as *mut u16;
        if !p4.is_null() { p4.write_unaligned(((*sd).player.progression.level as u16).to_be()); }
        len += 2;
        let ph2 = wfifop((*tsd).fd, 1) as *mut u16;
        if !ph2.is_null() { ph2.write_unaligned(((len + 3) as u16).to_be()); }
        wfifoset((*tsd).fd, encrypt((*tsd).fd) as usize);

        (*sd).player.appearance.setting_flags ^= FLAG_EXCHANGE as u16;
        (*tsd).player.appearance.setting_flags ^= FLAG_EXCHANGE as u16;

        (*sd).exchange.item_count  = 0;
        (*tsd).exchange.item_count = 0;
        (*sd).exchange.list_count  = 0;
        (*tsd).exchange.list_count = 1;
    } else {
        let msg = b"They have refused to exchange with you\0".as_ptr() as *const i8;
        clif_sendminitext(sd, msg);
    }
    0
}

// ─── clif_exchange_additem_else ──────────────────────────────────────────────

/// Send a real_name (engrave) additem packet once per item.  C lines 9635-9694.
pub unsafe fn clif_exchange_additem_else(
    sd:  *mut MapSessionData,
    tsd: *mut MapSessionData,
    _id: i32,
) -> i32 {
    if sd.is_null()  { return 0; }
    if tsd.is_null() { return 0; }

    // nameof = sd->exchange.item[sd->exchange.item_count - 1].real_name, truncated to 15
    let item_idx = ((*sd).exchange.item_count - 1).max(0) as usize;
    let mut nameof = [0i8; 255];
    let real_name_ptr = (*sd).exchange.item[item_idx].real_name.as_ptr();
    let real_name_len = libc::strlen(real_name_ptr).min(nameof.len() - 1);
    std::ptr::copy_nonoverlapping(real_name_ptr, nameof.as_mut_ptr(), real_name_len);
    nameof[real_name_len] = 0;
    string_truncate(&mut nameof, 15);
    (*sd).exchange.list_count += 1;

    if !session_exists((*sd).fd) {
        return 0;
    }

    let buf_len = libc::strlen(nameof.as_ptr());

    // Send to sd
    wfifohead((*sd).fd, 2000);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 3, 0x42);
    wfifob((*sd).fd, 4, 0x03);
    wfifob((*sd).fd, 5, 0x02);
    wfifob((*sd).fd, 6, 0x00);
    wfifob((*sd).fd, 7, (*sd).exchange.list_count as u8);
    let len: usize = 0;
    // WFIFOW(sd->fd, len+8) = 0xFFFF
    let pw = wfifop((*sd).fd, len + 8) as *mut u16;
    if !pw.is_null() { pw.write_unaligned(0xFFFF_u16.to_le()); }
    wfifob((*sd).fd, len + 10, 0x00);
    wfifob((*sd).fd, len + 11, buf_len as u8);
    let dst = wfifop((*sd).fd, len + 12) as *mut u8;
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(nameof.as_ptr() as *const u8, dst, buf_len);
    }
    let pkt_len = len + buf_len + 5;   // WFIFOW(sd->fd,1) = SWAP16(len+5)
    let ph = wfifop((*sd).fd, 1) as *mut u16;
    if !ph.is_null() { ph.write_unaligned(((pkt_len) as u16).to_be()); }
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);

    if !session_exists((*sd).fd) {
        return 0;
    }

    // Send to tsd
    wfifohead((*tsd).fd, 2000);
    wfifob((*tsd).fd, 0, 0xAA);
    wfifob((*tsd).fd, 3, 0x42);
    wfifob((*tsd).fd, 4, 0x03);
    wfifob((*tsd).fd, 5, 0x02);
    wfifob((*tsd).fd, 6, 0x01);
    wfifob((*tsd).fd, 7, (*sd).exchange.list_count as u8);
    let pw2 = wfifop((*tsd).fd, 8) as *mut u16;
    if !pw2.is_null() { pw2.write_unaligned(0xFFFF_u16.to_le()); }
    wfifob((*tsd).fd, 10, 0);
    wfifob((*tsd).fd, 11, buf_len as u8);
    let dst2 = wfifop((*tsd).fd, 12) as *mut u8;
    if !dst2.is_null() {
        std::ptr::copy_nonoverlapping(nameof.as_ptr() as *const u8, dst2, buf_len);
    }
    let tsd_pkt_len = buf_len + 1;   // len += strlen(buff)+1; WFIFOW(tsd->fd,1)=SWAP16(len+8)
    let ph2 = wfifop((*tsd).fd, 1) as *mut u16;
    if !ph2.is_null() { ph2.write_unaligned(((tsd_pkt_len + 8) as u16).to_be()); }
    wfifoset((*tsd).fd, encrypt((*tsd).fd) as usize);

    0
}

// ─── clif_exchange_additem ───────────────────────────────────────────────────

/// Add one inventory slot to the exchange offer.  C lines 9696-9851.
pub unsafe fn clif_exchange_additem(
    sd:     *mut MapSessionData,
    tsd:    *mut MapSessionData,
    id:     i32,
    amount: i32,
) -> i32 {
    if sd.is_null()  { return 0; }
    if tsd.is_null() { return 0; }

    let slot = id as usize;
    if slot >= (*sd).player.inventory.max_inv as usize {
        return 0;
    }
    let item_id = (&(*sd).player.inventory.inventory)[slot].id;

    if item_id != 0 {
        if item_db::search(item_id).exchangeable != 0 {
            let msg = b"You cannot exchange that.\0".as_ptr() as *const i8;
            clif_sendminitext(sd, msg);
            return 0;
        }
    }

    // Check target has inventory space
    let inv = &(&(*sd).player.inventory.inventory)[slot];
    let space = pc_isinvenspace(
        tsd,
        item_id as i32,
        inv.owner as i32,
        inv.real_name.as_ptr(),
        inv.custom_look,
        inv.custom_look_color,
        inv.custom_icon,
        inv.custom_icon_color,
    );
    if space >= (*tsd).player.inventory.max_inv as i32 {
        let msg = b"Receiving player does not have enough inventory space.\0".as_ptr() as *const i8;
        clif_sendminitext(sd, msg);
        return 0;
    }

    // Copy item into exchange slot
    let xcount = (*sd).exchange.item_count as usize;
    (*sd).exchange.item[xcount] = (&(*sd).player.inventory.inventory)[slot];
    (*sd).exchange.item[xcount].amount = amount;

    // Build display name (nameof = itemdb_name, truncate to 15)
    let ex_item_data = item_db::search((*sd).exchange.item[xcount].id);
    let raw_name = ex_item_data.name.as_ptr();
    let mut nameof = [0i8; 255];
    if *raw_name != 0 {
        let name_len = libc::strlen(raw_name).min(nameof.len() - 1);
        std::ptr::copy_nonoverlapping(raw_name, nameof.as_mut_ptr(), name_len);
        nameof[name_len] = 0;
    }
    string_truncate(&mut nameof, 15);
    (*sd).exchange.list_count += 1;

    if !session_exists((*sd).fd) {
        return 0;
    }

    // Build buff string: name(amount) or name with durability annotation
    let mut buff = [0i8; 300];
    let i = xcount;
    let ex_item = &(*sd).exchange.item[i];
    let ex_type = ex_item_data.typ as i32;
    let ex_dura = ex_item.dura;

    if amount > 1 {
        libc::snprintf(
            buff.as_mut_ptr(), buff.len(),
            b"%s(%d)\0".as_ptr() as *const i8,
            nameof.as_ptr(), amount,
        );
    } else {
        libc::snprintf(
            buff.as_mut_ptr(), buff.len(),
            b"%s\0".as_ptr() as *const i8,
            nameof.as_ptr(),
        );
    }

    if ex_type > 2 && ex_type < 17 {
        let max_dura = ex_item_data.dura;
        let percentage = if max_dura > 0 {
            (ex_dura as f32 / max_dura as f32) * 100.0
        } else { 0.0 };
        libc::snprintf(
            buff.as_mut_ptr(), buff.len(),
            b"%s (%d%%)\0".as_ptr() as *const i8,
            nameof.as_ptr(), percentage as i32,
        );
    } else if ex_type == ITM_SMOKE {
        libc::snprintf(
            buff.as_mut_ptr(), buff.len(),
            b"%s [%d %s]\0".as_ptr() as *const i8,
            nameof.as_ptr(), ex_dura, ex_item_data.text.as_ptr(),
        );
    } else if ex_type == ITM_BAG {
        libc::snprintf(
            buff.as_mut_ptr(), buff.len(),
            b"%s [%d]\0".as_ptr() as *const i8,
            nameof.as_ptr(), ex_dura,
        );
    } else if ex_type == ITM_MAP {
        libc::snprintf(
            buff.as_mut_ptr(), buff.len(),
            b"[T%d] %s\0".as_ptr() as *const i8,
            ex_dura, nameof.as_ptr(),
        );
    } else if ex_type == ITM_QUIVER {
        libc::snprintf(
            buff.as_mut_ptr(), buff.len(),
            b"%s [%d]\0".as_ptr() as *const i8,
            nameof.as_ptr(), ex_dura,
        );
    }

    let buf_len = libc::strlen(buff.as_ptr());
    let len: usize = 0;

    // Send to sd (own side)
    wfifohead((*sd).fd, 2000);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 3, 0x42);
    wfifob((*sd).fd, 4, 0x03);
    wfifob((*sd).fd, 5, 0x02);
    wfifob((*sd).fd, 6, 0x00);
    wfifob((*sd).fd, 7, (*sd).exchange.list_count as u8);

    if ex_item.custom_icon != 0 {
        let icon_val = ex_item.custom_icon + 49152;
        let pw = wfifop((*sd).fd, len + 8) as *mut u16;
        if !pw.is_null() { pw.write_unaligned((icon_val as u16).to_be()); }
        wfifob((*sd).fd, len + 10, ex_item.custom_icon_color as u8);
    } else {
        let icon_val = ex_item_data.icon as u16;
        let pw = wfifop((*sd).fd, len + 8) as *mut u16;
        if !pw.is_null() { pw.write_unaligned(icon_val.to_be()); }
        wfifob((*sd).fd, len + 10, ex_item_data.icon_color as u8);
    }
    wfifob((*sd).fd, len + 11, buf_len as u8);
    let dst = wfifop((*sd).fd, len + 12) as *mut u8;
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(buff.as_ptr() as *const u8, dst, buf_len);
    }
    let sd_pkt_len = len + buf_len + 5;
    let ph = wfifop((*sd).fd, 1) as *mut u16;
    if !ph.is_null() { ph.write_unaligned((sd_pkt_len as u16).to_be()); }
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);

    let len: usize = 0;

    if !session_exists((*sd).fd) {
        return 0;
    }

    // Rebuild buff for the tsd side (same logic, slightly different format for amount>1)
    let ex_item = &(*sd).exchange.item[i];
    if amount > 1 {
        libc::snprintf(
            buff.as_mut_ptr(), buff.len(),
            b"%s (%d)\0".as_ptr() as *const i8,
            nameof.as_ptr(), amount,
        );
    } else {
        libc::snprintf(
            buff.as_mut_ptr(), buff.len(),
            b"%s\0".as_ptr() as *const i8,
            nameof.as_ptr(),
        );
    }

    if ex_type > 2 && ex_type < 17 {
        let max_dura = ex_item_data.dura;
        let percentage = if max_dura > 0 {
            (ex_dura as f32 / max_dura as f32) * 100.0
        } else { 0.0 };
        libc::snprintf(
            buff.as_mut_ptr(), buff.len(),
            b"%s (%d%%)\0".as_ptr() as *const i8,
            nameof.as_ptr(), percentage as i32,
        );
    } else if ex_type == ITM_SMOKE {
        libc::snprintf(
            buff.as_mut_ptr(), buff.len(),
            b"%s [%d %s]\0".as_ptr() as *const i8,
            nameof.as_ptr(), ex_dura, ex_item_data.text.as_ptr(),
        );
    } else if ex_type == ITM_BAG {
        libc::snprintf(
            buff.as_mut_ptr(), buff.len(),
            b"%s [%d]\0".as_ptr() as *const i8,
            nameof.as_ptr(), ex_dura,
        );
    } else if ex_type == ITM_MAP {
        libc::snprintf(
            buff.as_mut_ptr(), buff.len(),
            b"[T%d] %s\0".as_ptr() as *const i8,
            ex_dura, nameof.as_ptr(),
        );
    } else if ex_type == ITM_QUIVER {
        libc::snprintf(
            buff.as_mut_ptr(), buff.len(),
            b"%s [%d]\0".as_ptr() as *const i8,
            nameof.as_ptr(), ex_dura,
        );
    }

    let buf_len = libc::strlen(buff.as_ptr());

    // Send to tsd (other side)
    wfifohead((*tsd).fd, 2000);
    wfifob((*tsd).fd, 0, 0xAA);
    wfifob((*tsd).fd, 3, 0x42);
    wfifob((*tsd).fd, 4, 0x03);
    wfifob((*tsd).fd, 5, 0x02);
    wfifob((*tsd).fd, 6, 0x01);
    wfifob((*tsd).fd, 7, (*sd).exchange.list_count as u8);

    let ex_item = &(*sd).exchange.item[i];
    if ex_item.custom_icon != 0 {
        let icon_val = ex_item.custom_icon + 49152;
        let pw = wfifop((*tsd).fd, 8) as *mut u16;
        if !pw.is_null() { pw.write_unaligned((icon_val as u16).to_be()); }
        wfifob((*tsd).fd, 10, ex_item.custom_icon_color as u8);
    } else {
        let icon_val = ex_item_data.icon as u16;
        let pw = wfifop((*tsd).fd, 8) as *mut u16;
        if !pw.is_null() { pw.write_unaligned(icon_val.to_be()); }
        wfifob((*tsd).fd, 10, ex_item_data.icon_color as u8);
    }
    wfifob((*tsd).fd, 11, buf_len as u8);
    let dst2 = wfifop((*tsd).fd, 12) as *mut u8;
    if !dst2.is_null() {
        std::ptr::copy_nonoverlapping(buff.as_ptr() as *const u8, dst2, buf_len);
    }
    let tsd_pkt_len = len + buf_len + 1;   // len += strlen(buff)+1; WFIFOW(tsd->fd,1)=SWAP16(len+8)
    let ph2 = wfifop((*tsd).fd, 1) as *mut u16;
    if !ph2.is_null() { ph2.write_unaligned(((tsd_pkt_len + 8) as u16).to_be()); }
    wfifoset((*tsd).fd, encrypt((*tsd).fd) as usize);

    (*sd).exchange.item_count += 1;

    // Send engrave line if item has a real_name
    if libc::strlen((*sd).exchange.item[i].real_name.as_ptr()) > 0 {
        clif_exchange_additem_else(sd, tsd, id);
    }
    pc_delitem(sd, id, amount, 9);
    0
}

// ─── clif_exchange_money ─────────────────────────────────────────────────────

/// Broadcast the current gold offer to both sides.  C lines 9853-9904.
pub unsafe fn clif_exchange_money(
    sd:  *mut MapSessionData,
    tsd: *mut MapSessionData,
) -> i32 {
    if sd.is_null()  { return 0; }
    if tsd.is_null() { return 0; }

    if !session_exists((*sd).fd) {
        return 0;
    }
    if !session_exists((*tsd).fd) {
        return 0;
    }

    wfifohead((*sd).fd,  11);
    wfifohead((*tsd).fd, 11);

    // sd side: own gold offer
    wfifob((*sd).fd, 0, 0xAA);
    {
        let p = wfifop((*sd).fd, 1) as *mut u16;
        if !p.is_null() { p.write_unaligned(8_u16.to_be()); }
    }
    wfifob((*sd).fd, 3, 0x42);
    wfifob((*sd).fd, 4, 0x03);
    wfifob((*sd).fd, 5, 0x03);
    wfifob((*sd).fd, 6, 0x00);
    {
        let p = wfifop((*sd).fd, 7) as *mut u32;
        if !p.is_null() { p.write_unaligned((*sd).exchange.gold.to_be()); }
    }
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);

    // tsd side: sd's gold offer visible to partner
    wfifob((*tsd).fd, 0, 0xAA);
    {
        let p = wfifop((*tsd).fd, 1) as *mut u16;
        if !p.is_null() { p.write_unaligned(8_u16.to_be()); }
    }
    wfifob((*tsd).fd, 3, 0x42);
    wfifob((*tsd).fd, 4, 0x03);
    wfifob((*tsd).fd, 5, 0x03);
    wfifob((*tsd).fd, 6, 0x01);
    {
        let p = wfifop((*tsd).fd, 7) as *mut u32;
        if !p.is_null() { p.write_unaligned((*sd).exchange.gold.to_be()); }
    }
    wfifoset((*tsd).fd, encrypt((*tsd).fd) as usize);

    0
}

// ─── clif_exchange_close ─────────────────────────────────────────────────────

/// Cancel the exchange and return all held items.  C lines 9906-9926.
pub unsafe fn clif_exchange_close(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }

    (*sd).exchange.target = 0;

    let item_count = (*sd).exchange.item_count as usize;
    for i in 0..item_count {
        let it = (*sd).exchange.item[i];
        pc_additemnolog(sd, &it as *const _ as *mut _);
    }
    clif_exchange_cleanup(sd);
    0
}

// ─── clif_handgold ───────────────────────────────────────────────────────────

/// Handle a "hand gold" packet — offer gold from adjacent cell.  C lines 9090-9155.
pub unsafe fn clif_handgold(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }

    let gold = {
        // SWAP32(RFIFOL(sd->fd, 5)) — network big-endian
        let raw = rfifol((*sd).fd, 5);
        u32::from_be_bytes(raw.to_le_bytes())   // raw is LE from rfifol; SWAP32 makes BE → flip
    };

    // C: if (gold < 0) gold = 0; (gold is unsigned so this is a no-op, but kept for fidelity)
    let gold = gold;
    if gold == 0 { return 0; }
    let gold = gold.min((*sd).player.inventory.money);

    // Compute adjacent cell based on facing direction
    let (x, y) = side_cell(&*sd);

    let bl = block_grid::first_in_cell((*sd).m as usize, x as u16, y as u16, BL_ALL)
        .and_then(|id| { let p = crate::game::map_server::map_id2bl_ref(id); if p.is_null() { None } else { Some(p) } });

    (*sd).exchange.gold = gold;

    if let Some(bl) = bl {
        if (*bl).bl_type as i32 == BL_PC {
            if let Some(tsd_arc) = crate::game::map_server::map_id2sd_pc((*bl).id) {
                let mut tsd_guard = tsd_arc.write();
                let tsd = &mut *tsd_guard as *mut MapSessionData;
                if (*tsd).player.appearance.setting_flags as u32 & FLAG_EXCHANGE != 0 {
                    clif_startexchange(sd, (*bl).id);
                    clif_exchange_money(sd, tsd);
                } else {
                    let msg = b"They have refused to exchange with you\0".as_ptr() as *const i8;
                    clif_sendminitext(sd, msg);
                }
            }
        }
    }
    0
}

// ─── clif_handitem ───────────────────────────────────────────────────────────

/// Handle a "hand item" packet — offer/give item from adjacent cell.  C lines 9206-9314.
pub unsafe fn clif_handitem(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }

    let slot      = rfifob((*sd).fd, 5).saturating_sub(1) as usize;
    let handgive  = rfifob((*sd).fd, 6);
    let amount: i32 = if handgive == 0 {
        1
    } else {
        (&(*sd).player.inventory.inventory)[slot].amount
    };

    let (x, y) = side_cell(&*sd);

    (*sd).invslot = slot as u8;

    let bl = match block_grid::first_in_cell((*sd).m as usize, x as u16, y as u16, BL_ALL)
        .and_then(|id| { let p = crate::game::map_server::map_id2bl_ref(id); if p.is_null() { None } else { Some(p) } }) {
        Some(p) => p,
        None => return 0,
    };

    if (*bl).bl_type as i32 == BL_PC {
        if let Some(tsd_arc) = crate::game::map_server::map_id2sd_pc((*bl).id) {
            let mut tsd_guard = tsd_arc.write();
            let tsd = &mut *tsd_guard as *mut MapSessionData;
            if (*tsd).player.appearance.setting_flags as u32 & FLAG_EXCHANGE != 0 {
                clif_startexchange(sd, (*bl).id);
                clif_exchange_additem(sd, tsd, slot as i32, amount);
            } else {
                let msg = b"They have refused to exchange with you\0".as_ptr() as *const i8;
                clif_sendminitext(sd, msg);
            }
        }
    }

    if (*bl).bl_type as i32 == BL_MOB {
        let mob_arc = match crate::game::map_server::map_id2mob_ref((*bl).id) {
            Some(a) => a, None => return 0,
        };
        let mut mob_guard = mob_arc.write();
        let mob = &mut *mob_guard as *mut crate::game::mob::MobSpawnData;

        if item_db::search((&(*sd).player.inventory.inventory)[slot].id).exchangeable == 1 { return 0; }

        let inv_id   = (&(*sd).player.inventory.inventory)[slot].id;
        let inv_dura = (&(*sd).player.inventory.inventory)[slot].dura;
        let inv_own  = (&(*sd).player.inventory.inventory)[slot].owner;
        let inv_prot = (&(*sd).player.inventory.inventory)[slot].protected;

        let mut found = false;
        for i in 0..MAX_INVENTORY {
            let mob_slot = &mut (*mob).inventory[i];
            if mob_slot.id == inv_id
                && mob_slot.dura    == inv_dura
                && mob_slot.owner   == inv_own
                && mob_slot.protected == inv_prot
            {
                mob_slot.amount += amount;
                found = true;
                break;
            } else if mob_slot.id == 0 {
                mob_slot.id     = inv_id;
                mob_slot.amount = amount;
                mob_slot.owner  = inv_own;
                mob_slot.dura   = inv_dura;
                mob_slot.protected = inv_prot;
                found = true;
                break;
            }
        }
        let _ = found;
        pc_delitem(sd, slot as i32, amount, 9);
    }

    if (*bl).bl_type as i32 == BL_NPC {
        let nd_arc = match crate::game::map_server::map_id2npc_ref((*bl).id) {
            Some(a) => a, None => return 0,
        };
        let mut nd_guard = nd_arc.write();
        let nd = &mut *nd_guard as *mut crate::game::npc::NpcData;

        let inv_id = (&(*sd).player.inventory.inventory)[slot].id;
        let inv_item = item_db::search(inv_id);
        if inv_item.exchangeable != 0 || inv_item.droppable != 0 {
            return 0;
        }

        if (*nd).receive_item == 1 {
            sl_doscript_coro_2(crate::game::scripting::carray_to_str(&(*nd).name), Some("handItem"), (*sd).id, (*nd).id);
        } else {
            let item_name = item_db::search(inv_id).name.as_ptr();
            let mut msg = [0i8; 128];
            libc::snprintf(
                msg.as_mut_ptr(), msg.len(),
                b"What are you trying to do? Keep your junky %s with you!\0".as_ptr() as *const i8,
                item_name,
            );
            let msg_len = libc::strlen(msg.as_ptr());

            if !session_exists((*sd).fd) {
                return 0;
            }

            wfifohead((*sd).fd, msg_len + 11);
            wfifob((*sd).fd, 5, 0);
            {
                let p = wfifop((*sd).fd, 6) as *mut u32;
                if !p.is_null() { p.write_unaligned((*bl).id.to_be()); }
            }
            wfifob((*sd).fd, 10, msg_len as u8);
            let dst = wfifop((*sd).fd, 11) as *mut u8;
            if !dst.is_null() {
                std::ptr::copy_nonoverlapping(msg.as_ptr() as *const u8, dst, msg_len);
            }
            wfifob((*sd).fd, 0, 0xAA);
            wfifob((*sd).fd, 3, 0x0D);
            wfifob((*sd).fd, 4, 0); // increment placeholder (WFIFOHEADER sets it)
            {
                let ph = wfifop((*sd).fd, 1) as *mut u16;
                if !ph.is_null() { ph.write_unaligned(((msg_len + 11) as u16).to_be()); }
            }
            wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
        }
    }
    0
}

// ─── clif_parse_exchange ─────────────────────────────────────────────────────

/// Dispatch incoming exchange sub-packet by type byte.  C lines 9438-9543.
pub unsafe fn clif_parse_exchange(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }

    let kind = rfifob((*sd).fd, 5) as i32;

    let reg_str = b"goldbardupe\0".as_ptr() as *const i8;
    let _dupe_times = pc_readglobalreg(sd, reg_str);
    // C has a commented-out quarantine block here; no-op as in C.

    match kind {
        0 => {
            // Initiation: type 0
            let raw = rfifol((*sd).fd, 6);
            let target_id = u32::from_be_bytes(raw.to_le_bytes());
            let tsd_arc = match crate::game::map_server::map_id2sd_pc(target_id) {
                Some(a) => a, None => return 0,
            };
            let _tsd_guard = tsd_arc.read();
            let tsd = &*_tsd_guard;
            if (*sd).m != tsd.m || tsd.bl_type as i32 != BL_PC {
                return 0;
            }
            if (*sd).player.identity.gm_level != 0 || (tsd.optFlags & OPT_FLAG_STEALTH) == 0 {
                drop(_tsd_guard);
                clif_startexchange(sd, target_id);
            }
        }
        1 => {
            // Add item — check if it needs an amount prompt
            let id = rfifob((*sd).fd, 10).saturating_sub(1) as usize;
            if id >= (*sd).player.inventory.max_inv as usize {
                return 0;
            }
            if (&(*sd).player.inventory.inventory)[id].amount > 1 {
                if !session_exists((*sd).fd) {
                    return 0;
                }
                wfifohead((*sd).fd, 7);
                wfifob((*sd).fd, 0, 0xAA);
                {
                    let p = wfifop((*sd).fd, 1) as *mut u16;
                    if !p.is_null() { p.write_unaligned(4_u16.to_be()); }
                }
                wfifob((*sd).fd, 3, 0x42);
                wfifob((*sd).fd, 4, 0x03);
                wfifob((*sd).fd, 5, 0x01);
                wfifob((*sd).fd, 6, (id + 1) as u8);
                wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
            } else if (&(*sd).player.inventory.inventory)[id].id != 0 {
                let raw = rfifol((*sd).fd, 6);
                let target_id = u32::from_be_bytes(raw.to_le_bytes());
                let tsd_arc = match crate::game::map_server::map_id2sd_pc(target_id) {
                    Some(a) => a, None => return 0,
                };
                let mut _tsd_guard = tsd_arc.write();
                let tsd = &mut *_tsd_guard as *mut MapSessionData;
                clif_exchange_additem(sd, tsd, id as i32, 1);
            }
            // else: blank slot hack attempt — do nothing (matching C)
        }
        2 => {
            // Add item with explicit amount
            let id     = rfifob((*sd).fd, 10).saturating_sub(1) as usize;
            if id >= (*sd).player.inventory.max_inv as usize {
                return 0;
            }
            let amount = rfifob((*sd).fd, 11) as i32;
            let raw = rfifol((*sd).fd, 6);
            let target_id = u32::from_be_bytes(raw.to_le_bytes());
            let tsd_arc = match crate::game::map_server::map_id2sd_pc(target_id) {
                Some(a) => a, None => return 0,
            };
            let mut _tsd_guard = tsd_arc.write();
            let tsd = &mut *_tsd_guard as *mut MapSessionData;
            if amount > 0
                && (&(*sd).player.inventory.inventory)[id].id != 0
                && amount <= (&(*sd).player.inventory.inventory)[id].amount
            {
                clif_exchange_additem(sd, tsd, id as i32, amount);
            }
            // else: blank slot or zero amount — do nothing
        }
        3 => {
            // Exchange gold
            let raw_target = rfifol((*sd).fd, 6);
            let target_id  = u32::from_be_bytes(raw_target.to_le_bytes());
            let raw_amount = rfifol((*sd).fd, 10);
            let amount     = u32::from_be_bytes(raw_amount.to_le_bytes());
            let tsd_arc = match crate::game::map_server::map_id2sd_pc(target_id) {
                Some(a) => a, None => return 0,
            };
            let mut _tsd_guard = tsd_arc.write();
            let tsd = &mut *_tsd_guard as *mut MapSessionData;
            if amount > (*sd).player.inventory.money {
                clif_exchange_money(sd, tsd);
            } else {
                (*sd).exchange.gold = amount;
                clif_exchange_money(sd, tsd);
            }
        }
        4 => {
            // Quit exchange
            let raw = rfifol((*sd).fd, 6);
            let target_id = u32::from_be_bytes(raw.to_le_bytes());
            let msg = b"Exchange cancelled.\0".as_ptr() as *const i8;
            clif_exchange_message(sd, msg, 4, 0);
            if let Some(tsd_arc) = crate::game::map_server::map_id2sd_pc(target_id) {
                let mut _tsd_guard = tsd_arc.write();
                let tsd = &mut *_tsd_guard as *mut MapSessionData;
                clif_exchange_message(tsd, msg, 4, 0);
                clif_exchange_close(tsd);
            }
            clif_exchange_close(sd);
        }
        5 => {
            // Finish exchange
            let raw = rfifol((*sd).fd, 6);
            let target_id = u32::from_be_bytes(raw.to_le_bytes());
            let tsd_arc = crate::game::map_server::map_id2sd_pc(target_id);

            if (*sd).exchange.target != target_id {
                clif_exchange_close(sd);
                let msg = b"Exchange cancelled.\0".as_ptr() as *const i8;
                clif_exchange_message(sd, msg, 4, 0);
                if let Some(ref arc) = tsd_arc {
                    let mut guard = arc.write();
                    let tsd = &mut *guard as *mut MapSessionData;
                    if (*tsd).exchange.target == (*sd).id {
                        clif_exchange_message(tsd, msg, 4, 0);
                        clif_exchange_close(tsd);
                        session_set_eof((*sd).fd, 10);
                    }
                }
                return 0;
            }
            let msg_no_gold = b"You do not have that amount.\0".as_ptr() as *const i8;
            let msg_cancel  = b"Exchange cancelled.\0".as_ptr() as *const i8;
            if (*sd).exchange.gold > (*sd).player.inventory.money {
                clif_exchange_message(sd, msg_no_gold, 4, 0);
                if let Some(ref arc) = tsd_arc {
                    let mut guard = arc.write();
                    let tsd = &mut *guard as *mut MapSessionData;
                    clif_exchange_message(tsd, msg_cancel, 4, 0);
                    clif_exchange_close(tsd);
                }
                clif_exchange_close(sd);
            } else if let Some(ref arc) = tsd_arc {
                let mut guard = arc.write();
                let tsd = &mut *guard as *mut MapSessionData;
                clif_exchange_sendok(sd, tsd);
            } else {
                clif_exchange_close(sd);
            }
        }
        _ => {}
    }
    0
}

// ─── side_cell: compute adjacent cell from facing direction ──────────────────

/// Return the (x, y) of the cell in front of the player based on `player.combat.side`.
/// Matches the repeated side==0..3 pattern in clif_handgold / clif_handitem.
unsafe fn side_cell(sd: &MapSessionData) -> (i32, i32) {
    match sd.player.combat.side {
        0 => (sd.x as i32,     sd.y as i32 - 1),
        1 => (sd.x as i32 + 1, sd.y as i32),
        2 => (sd.x as i32,     sd.y as i32 + 1),
        3 => (sd.x as i32 - 1, sd.y as i32),
        _ => (sd.x as i32,     sd.y as i32),
    }
}

