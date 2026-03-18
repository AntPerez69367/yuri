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
use crate::game::pc::FLAG_EXCHANGE;
use crate::game::player::entity::PlayerEntity;
use crate::common::constants::entity::{BL_ALL, BL_MOB, BL_NPC, BL_PC};
use crate::common::player::inventory::MAX_INVENTORY;

use super::packet::{
    encrypt,
    rfifob, rfifol,
    wfifob, wfifop, wfifow, wfifoset, wfifohead,
};

use crate::common::constants::entity::player::OPT_FLAG_STEALTH;


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


use crate::common::constants::entity::player::SFLAG_XPMONEY;

// Item type constants (from item_db.h)
use crate::common::constants::entity::player::{ITM_SMOKE, ITM_BAG, ITM_MAP, ITM_QUIVER};

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
pub unsafe fn clif_exchange_cleanup(pe: &PlayerEntity) -> i32 {
    let mut g = pe.write();
    g.exchange.exchange_done = 0;
    g.exchange.gold = 0;
    g.exchange.item_count = 0;
    0
}

// ─── clif_exchange_message ───────────────────────────────────────────────────

/// Send an exchange status message to one player.  C lines 9389-9412.
pub unsafe fn clif_exchange_message(
    pe:      &PlayerEntity,
    message: *const i8,
    kind:    i32,
    extra:   i32,
) -> i32 {
    let extra = if extra > 1 { 0 } else { extra };

    let msg_len = libc::strlen(message);
    let len = msg_len + 5;   // mirrors C: len = strlen(message) + 5

    if !session_exists(pe.fd) {
        return 0;
    }

    wfifohead(pe.fd, msg_len + 8);
    wfifob(pe.fd, 0, 0xAA);
    wfifob(pe.fd, 3, 0x42);
    wfifob(pe.fd, 4, 0x03);
    wfifob(pe.fd, 5, kind as u8);
    wfifob(pe.fd, 6, extra as u8);
    wfifob(pe.fd, 7, msg_len as u8);
    // copy message bytes into WFIFOP(pe->fd, 8)
    let dst = wfifop(pe.fd, 8) as *mut u8;
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(message as *const u8, dst, msg_len);
    }
    wfifow(pe.fd, 1, (len + 3) as u16);   // SWAP16(len + 3) — big-endian
    let p = wfifop(pe.fd, 1) as *mut u16;
    if !p.is_null() { p.write_unaligned(((len + 3) as u16).to_be()); }
    wfifoset(pe.fd, encrypt(pe.fd) as usize);
    0
}

// ─── clif_exchange_finalize ──────────────────────────────────────────────────

/// Transfer items/gold between both sides and clean up.  C lines 9323-9387.
pub unsafe fn clif_exchange_finalize(
    pe:  &PlayerEntity,
    tpe: &PlayerEntity,
) -> i32 {
    let (pe_id, tpe_id) = (pe.id, tpe.id);
    sl_doscript_2("characterLog", Some("exchangeLogWrite"), pe_id, tpe_id);

    // Transfer pe's items to tpe
    let sd_item_count = pe.read().exchange.item_count as usize;
    for i in 0..sd_item_count {
        let it = pe.read().exchange.item[i];
        pc_additem(tpe, &it as *const _ as *mut _);
    }
    let pe_gold = pe.read().exchange.gold;
    tpe.write().player.inventory.money = tpe.read().player.inventory.money.saturating_add(pe_gold);
    pe.write().player.inventory.money  = pe.read().player.inventory.money.saturating_sub(pe_gold);
    pe.write().exchange.gold = 0;

    // Transfer tpe's items to pe
    let tsd_item_count = tpe.read().exchange.item_count as usize;
    for i in 0..tsd_item_count {
        let it = tpe.read().exchange.item[i];
        pc_additem(pe, &it as *const _ as *mut _);
    }
    let tpe_gold = tpe.read().exchange.gold;
    pe.write().player.inventory.money  = pe.read().player.inventory.money.saturating_add(tpe_gold);
    tpe.write().player.inventory.money = tpe.read().player.inventory.money.saturating_sub(tpe_gold);
    tpe.write().exchange.gold = 0;

    clif_sendstatus(pe,  SFLAG_XPMONEY);
    clif_sendstatus(tpe, SFLAG_XPMONEY);
    0
}

// ─── clif_exchange_sendok ────────────────────────────────────────────────────

/// Handle one side confirming the exchange.  C lines 9414-9435.
pub unsafe fn clif_exchange_sendok(
    pe:  &PlayerEntity,
    tpe: &PlayerEntity,
) -> i32 {
    if tpe.read().exchange.exchange_done == 1 {
        clif_exchange_finalize(pe, tpe);

        let msg = b"You exchanged, and gave away ownership of the items.\0".as_ptr() as *const i8;
        clif_exchange_message(pe,  msg, 5, 0);
        clif_exchange_message(tpe, msg, 5, 0);

        clif_exchange_cleanup(pe);
        clif_exchange_cleanup(tpe);
    } else {
        pe.write().exchange.exchange_done = 1;
        let msg = b"You exchanged, and gave away ownership of the items.\0".as_ptr() as *const i8;
        clif_exchange_message(tpe, msg, 5, 1);
        clif_exchange_message(pe,  msg, 5, 1);
    }
    0
}

// ─── clif_startexchange ──────────────────────────────────────────────────────

/// Initiate a trade window between two players.  C lines 9545-9634.
pub unsafe fn clif_startexchange(
    pe:     &PlayerEntity,
    tpe:    &PlayerEntity,
) -> i32 {
    if tpe.id == pe.id {
        let msg = b"You move your items from one hand to another, but quickly get bored.\0".as_ptr() as *const i8;
        clif_sendminitext(pe, msg);
        return 0;
    }

    pe.write().exchange.target  = tpe.id;
    tpe.write().exchange.target = pe.id;

    if tpe.read().player.appearance.setting_flags as u32 & FLAG_EXCHANGE != 0 {
        let mut buff = [0i8; 256];

        // Build name string for pe (to send to tpe)
        let (tpe_class, tpe_mark, tpe_name, tpe_level) = {
            let g = tpe.read();
            (g.player.progression.class, g.player.progression.mark, g.player.identity.name.clone(), g.player.progression.level)
        };
        let tsd_class_name = classdb_name(tpe_class as i32, tpe_mark as i32);
        {
            let formatted = format!("{}({})\0", tpe_name, tsd_class_name);
            let copy_len = formatted.len().min(buff.len());
            std::ptr::copy_nonoverlapping(formatted.as_ptr() as *const i8, buff.as_mut_ptr(), copy_len);
        }

        if !session_exists(pe.fd) {
            return 0;
        }

        wfifohead(pe.fd, 512);
        wfifob(pe.fd, 0, 0xAA);
        wfifob(pe.fd, 3, 0x42);
        wfifob(pe.fd, 4, 0x03);
        wfifob(pe.fd, 5, 0x00);
        let p = wfifop(pe.fd, 6) as *mut u32;
        if !p.is_null() { p.write_unaligned(tpe.id.to_be()); }
        let mut len: usize = 4;
        let buf_len = libc::strlen(buff.as_ptr());
        wfifob(pe.fd, len + 6, buf_len as u8);
        let dst = wfifop(pe.fd, len + 7) as *mut u8;
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(buff.as_ptr() as *const u8, dst, buf_len);
        }
        len += buf_len + 1;
        let p2 = wfifop(pe.fd, len + 6) as *mut u16;
        if !p2.is_null() { p2.write_unaligned((tpe_level as u16).to_be()); }
        len += 2;
        let ph = wfifop(pe.fd, 1) as *mut u16;
        if !ph.is_null() { ph.write_unaligned(((len + 3) as u16).to_be()); }
        wfifoset(pe.fd, encrypt(pe.fd) as usize);

        if !session_exists(pe.fd) {
            return 0;
        }

        // Build name string for tpe (to send to pe)
        let (sd_class, sd_mark, sd_name, sd_level) = {
            let g = pe.read();
            (g.player.progression.class, g.player.progression.mark, g.player.identity.name.clone(), g.player.progression.level)
        };
        let sd_class_name = classdb_name(sd_class as i32, sd_mark as i32);
        {
            let formatted = format!("{}({})\0", sd_name, sd_class_name);
            let copy_len = formatted.len().min(buff.len());
            std::ptr::copy_nonoverlapping(formatted.as_ptr() as *const i8, buff.as_mut_ptr(), copy_len);
        }

        wfifohead(tpe.fd, 512);
        wfifob(tpe.fd, 0, 0xAA);
        wfifob(tpe.fd, 3, 0x42);
        wfifob(tpe.fd, 4, 0x03);
        wfifob(tpe.fd, 5, 0x00);
        let p3 = wfifop(tpe.fd, 6) as *mut u32;
        if !p3.is_null() { p3.write_unaligned(pe.id.to_be()); }
        let mut len: usize = 4;
        let buf_len = libc::strlen(buff.as_ptr());
        wfifob(tpe.fd, len + 6, buf_len as u8);
        let dst = wfifop(tpe.fd, len + 7) as *mut u8;
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(buff.as_ptr() as *const u8, dst, buf_len);
        }
        len += buf_len + 1;
        let p4 = wfifop(tpe.fd, len + 6) as *mut u16;
        if !p4.is_null() { p4.write_unaligned((sd_level as u16).to_be()); }
        len += 2;
        let ph2 = wfifop(tpe.fd, 1) as *mut u16;
        if !ph2.is_null() { ph2.write_unaligned(((len + 3) as u16).to_be()); }
        wfifoset(tpe.fd, encrypt(tpe.fd) as usize);

        pe.write().player.appearance.setting_flags ^= FLAG_EXCHANGE;
        tpe.write().player.appearance.setting_flags ^= FLAG_EXCHANGE;

        pe.write().exchange.item_count  = 0;
        tpe.write().exchange.item_count = 0;
        pe.write().exchange.list_count  = 0;
        tpe.write().exchange.list_count = 1;
    } else {
        let msg = b"They have refused to exchange with you\0".as_ptr() as *const i8;
        clif_sendminitext(pe, msg);
    }
    0
}

// ─── clif_exchange_additem_else ──────────────────────────────────────────────

/// Send a real_name (engrave) additem packet once per item.  C lines 9635-9694.
pub unsafe fn clif_exchange_additem_else(
    pe:  &PlayerEntity,
    tpe: &PlayerEntity,
    _id: i32,
) -> i32 {
    // nameof = pe->exchange.item[pe->exchange.item_count - 1].real_name, truncated to 15
    let item_idx = (pe.read().exchange.item_count - 1).max(0) as usize;
    let real_name = pe.read().exchange.item[item_idx].real_name;
    let mut nameof = [0i8; 255];
    let real_name_len = libc::strlen(real_name.as_ptr()).min(nameof.len() - 1);
    std::ptr::copy_nonoverlapping(real_name.as_ptr(), nameof.as_mut_ptr(), real_name_len);
    nameof[real_name_len] = 0;
    string_truncate(&mut nameof, 15);
    pe.write().exchange.list_count += 1;
    let list_count = pe.read().exchange.list_count;

    if !session_exists(pe.fd) {
        return 0;
    }

    let buf_len = libc::strlen(nameof.as_ptr());

    // Send to pe
    wfifohead(pe.fd, 2000);
    wfifob(pe.fd, 0, 0xAA);
    wfifob(pe.fd, 3, 0x42);
    wfifob(pe.fd, 4, 0x03);
    wfifob(pe.fd, 5, 0x02);
    wfifob(pe.fd, 6, 0x00);
    wfifob(pe.fd, 7, list_count as u8);
    let len: usize = 0;
    let pw = wfifop(pe.fd, len + 8) as *mut u16;
    if !pw.is_null() { pw.write_unaligned(0xFFFF_u16.to_le()); }
    wfifob(pe.fd, len + 10, 0x00);
    wfifob(pe.fd, len + 11, buf_len as u8);
    let dst = wfifop(pe.fd, len + 12) as *mut u8;
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(nameof.as_ptr() as *const u8, dst, buf_len);
    }
    let pkt_len = len + buf_len + 5;
    let ph = wfifop(pe.fd, 1) as *mut u16;
    if !ph.is_null() { ph.write_unaligned((pkt_len as u16).to_be()); }
    wfifoset(pe.fd, encrypt(pe.fd) as usize);

    if !session_exists(pe.fd) {
        return 0;
    }

    // Send to tpe
    wfifohead(tpe.fd, 2000);
    wfifob(tpe.fd, 0, 0xAA);
    wfifob(tpe.fd, 3, 0x42);
    wfifob(tpe.fd, 4, 0x03);
    wfifob(tpe.fd, 5, 0x02);
    wfifob(tpe.fd, 6, 0x01);
    wfifob(tpe.fd, 7, list_count as u8);
    let pw2 = wfifop(tpe.fd, 8) as *mut u16;
    if !pw2.is_null() { pw2.write_unaligned(0xFFFF_u16.to_le()); }
    wfifob(tpe.fd, 10, 0);
    wfifob(tpe.fd, 11, buf_len as u8);
    let dst2 = wfifop(tpe.fd, 12) as *mut u8;
    if !dst2.is_null() {
        std::ptr::copy_nonoverlapping(nameof.as_ptr() as *const u8, dst2, buf_len);
    }
    let tsd_pkt_len = buf_len + 1;
    let ph2 = wfifop(tpe.fd, 1) as *mut u16;
    if !ph2.is_null() { ph2.write_unaligned(((tsd_pkt_len + 8) as u16).to_be()); }
    wfifoset(tpe.fd, encrypt(tpe.fd) as usize);

    0
}

// ─── clif_exchange_additem ───────────────────────────────────────────────────

/// Add one inventory slot to the exchange offer.  C lines 9696-9851.
pub unsafe fn clif_exchange_additem(
    pe:     &PlayerEntity,
    tpe:    &PlayerEntity,
    id:     i32,
    amount: i32,
) -> i32 {
    let slot = id as usize;
    if slot >= pe.read().player.inventory.max_inv as usize {
        return 0;
    }
    let item_id = pe.read().player.inventory.inventory[slot].id;

    if item_id != 0 {
        if item_db::search(item_id).exchangeable != 0 {
            let msg = b"You cannot exchange that.\0".as_ptr() as *const i8;
            clif_sendminitext(pe, msg);
            return 0;
        }
    }

    // Check target has inventory space
    let (inv_owner, inv_real_name, inv_custom_look, inv_custom_look_color, inv_custom_icon, inv_custom_icon_color) = {
        let g = pe.read();
        let inv = &g.player.inventory.inventory[slot];
        (inv.owner, inv.real_name, inv.custom_look, inv.custom_look_color, inv.custom_icon, inv.custom_icon_color)
    };
    let space = pc_isinvenspace(
        tpe.data_ptr(), // TODO(phase6c): migrate
        item_id as i32,
        inv_owner as i32,
        inv_real_name.as_ptr(),
        inv_custom_look,
        inv_custom_look_color,
        inv_custom_icon,
        inv_custom_icon_color,
    );
    if space >= tpe.read().player.inventory.max_inv as i32 {
        let msg = b"Receiving player does not have enough inventory space.\0".as_ptr() as *const i8;
        clif_sendminitext(pe, msg);
        return 0;
    }

    // Copy item into exchange slot
    let xcount = {
        let g = pe.read();
        g.exchange.item_count as usize
    };
    {
        let mut g = pe.write();
        g.exchange.item[xcount] = g.player.inventory.inventory[slot];
        g.exchange.item[xcount].amount = amount;
    }

    // Build display name (nameof = itemdb_name, truncate to 15)
    let ex_item_id = pe.read().exchange.item[xcount].id;
    let ex_item_data = item_db::search(ex_item_id);
    let raw_name = ex_item_data.name.as_ptr();
    let mut nameof = [0i8; 255];
    if *raw_name != 0 {
        let name_len = libc::strlen(raw_name).min(nameof.len() - 1);
        std::ptr::copy_nonoverlapping(raw_name, nameof.as_mut_ptr(), name_len);
        nameof[name_len] = 0;
    }
    string_truncate(&mut nameof, 15);
    pe.write().exchange.list_count += 1;

    if !session_exists(pe.fd) {
        return 0;
    }

    // Snapshot exchange item fields before building packets
    let (ex_type, ex_dura, ex_custom_icon, ex_custom_icon_color, list_count) = {
        let g = pe.read();
        let ex_item = &g.exchange.item[xcount];
        (ex_item_data.typ as i32, ex_item.dura, ex_item.custom_icon, ex_item.custom_icon_color, g.exchange.list_count)
    };

    // Build buff string: name(amount) or name with durability annotation
    let mut buff = [0i8; 300];
    let i = xcount;

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

    // Send to pe (own side)
    wfifohead(pe.fd, 2000);
    wfifob(pe.fd, 0, 0xAA);
    wfifob(pe.fd, 3, 0x42);
    wfifob(pe.fd, 4, 0x03);
    wfifob(pe.fd, 5, 0x02);
    wfifob(pe.fd, 6, 0x00);
    wfifob(pe.fd, 7, list_count as u8);

    if ex_custom_icon != 0 {
        let icon_val = ex_custom_icon + 49152;
        let pw = wfifop(pe.fd, len + 8) as *mut u16;
        if !pw.is_null() { pw.write_unaligned((icon_val as u16).to_be()); }
        wfifob(pe.fd, len + 10, ex_custom_icon_color as u8);
    } else {
        let icon_val = ex_item_data.icon as u16;
        let pw = wfifop(pe.fd, len + 8) as *mut u16;
        if !pw.is_null() { pw.write_unaligned(icon_val.to_be()); }
        wfifob(pe.fd, len + 10, ex_item_data.icon_color as u8);
    }
    wfifob(pe.fd, len + 11, buf_len as u8);
    let dst = wfifop(pe.fd, len + 12) as *mut u8;
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(buff.as_ptr() as *const u8, dst, buf_len);
    }
    let sd_pkt_len = len + buf_len + 5;
    let ph = wfifop(pe.fd, 1) as *mut u16;
    if !ph.is_null() { ph.write_unaligned((sd_pkt_len as u16).to_be()); }
    wfifoset(pe.fd, encrypt(pe.fd) as usize);

    let len: usize = 0;

    if !session_exists(pe.fd) {
        return 0;
    }

    // Rebuild buff for the tpe side (same logic, slightly different format for amount>1)
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

    // Send to tpe (other side)
    wfifohead(tpe.fd, 2000);
    wfifob(tpe.fd, 0, 0xAA);
    wfifob(tpe.fd, 3, 0x42);
    wfifob(tpe.fd, 4, 0x03);
    wfifob(tpe.fd, 5, 0x02);
    wfifob(tpe.fd, 6, 0x01);
    wfifob(tpe.fd, 7, list_count as u8);

    if ex_custom_icon != 0 {
        let icon_val = ex_custom_icon + 49152;
        let pw = wfifop(tpe.fd, 8) as *mut u16;
        if !pw.is_null() { pw.write_unaligned((icon_val as u16).to_be()); }
        wfifob(tpe.fd, 10, ex_custom_icon_color as u8);
    } else {
        let icon_val = ex_item_data.icon as u16;
        let pw = wfifop(tpe.fd, 8) as *mut u16;
        if !pw.is_null() { pw.write_unaligned(icon_val.to_be()); }
        wfifob(tpe.fd, 10, ex_item_data.icon_color as u8);
    }
    wfifob(tpe.fd, 11, buf_len as u8);
    let dst2 = wfifop(tpe.fd, 12) as *mut u8;
    if !dst2.is_null() {
        std::ptr::copy_nonoverlapping(buff.as_ptr() as *const u8, dst2, buf_len);
    }
    let tsd_pkt_len = len + buf_len + 1;   // len += strlen(buff)+1; WFIFOW(tsd->fd,1)=SWAP16(len+8)
    let ph2 = wfifop(tpe.fd, 1) as *mut u16;
    if !ph2.is_null() { ph2.write_unaligned(((tsd_pkt_len + 8) as u16).to_be()); }
    wfifoset(tpe.fd, encrypt(tpe.fd) as usize);

    pe.write().exchange.item_count += 1;

    // Send engrave line if item has a real_name
    let has_real_name = libc::strlen(pe.read().exchange.item[i].real_name.as_ptr()) > 0;
    if has_real_name {
        clif_exchange_additem_else(pe, tpe, id);
    }
    pc_delitem(pe, id, amount, 9);
    0
}

// ─── clif_exchange_money ─────────────────────────────────────────────────────

/// Broadcast the current gold offer to both sides.  C lines 9853-9904.
pub unsafe fn clif_exchange_money(
    pe:  &PlayerEntity,
    tpe: &PlayerEntity,
) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }
    if !session_exists(tpe.fd) {
        return 0;
    }

    let pe_gold = pe.read().exchange.gold;

    wfifohead(pe.fd,  11);
    wfifohead(tpe.fd, 11);

    // pe side: own gold offer
    wfifob(pe.fd, 0, 0xAA);
    {
        let p = wfifop(pe.fd, 1) as *mut u16;
        if !p.is_null() { p.write_unaligned(8_u16.to_be()); }
    }
    wfifob(pe.fd, 3, 0x42);
    wfifob(pe.fd, 4, 0x03);
    wfifob(pe.fd, 5, 0x03);
    wfifob(pe.fd, 6, 0x00);
    {
        let p = wfifop(pe.fd, 7) as *mut u32;
        if !p.is_null() { p.write_unaligned(pe_gold.to_be()); }
    }
    wfifoset(pe.fd, encrypt(pe.fd) as usize);

    // tpe side: pe's gold offer visible to partner
    wfifob(tpe.fd, 0, 0xAA);
    {
        let p = wfifop(tpe.fd, 1) as *mut u16;
        if !p.is_null() { p.write_unaligned(8_u16.to_be()); }
    }
    wfifob(tpe.fd, 3, 0x42);
    wfifob(tpe.fd, 4, 0x03);
    wfifob(tpe.fd, 5, 0x03);
    wfifob(tpe.fd, 6, 0x01);
    {
        let p = wfifop(tpe.fd, 7) as *mut u32;
        if !p.is_null() { p.write_unaligned(pe_gold.to_be()); }
    }
    wfifoset(tpe.fd, encrypt(tpe.fd) as usize);

    0
}

// ─── clif_exchange_close ─────────────────────────────────────────────────────

/// Cancel the exchange and return all held items.  C lines 9906-9926.
pub unsafe fn clif_exchange_close(pe: &PlayerEntity) -> i32 {
    pe.write().exchange.target = 0;

    let item_count = pe.read().exchange.item_count as usize;
    for i in 0..item_count {
        let it = pe.read().exchange.item[i];
        pc_additemnolog(pe, &it as *const _ as *mut _);
    }
    clif_exchange_cleanup(pe);
    0
}

// ─── clif_handgold ───────────────────────────────────────────────────────────

/// Handle a "hand gold" packet — offer gold from adjacent cell.  C lines 9090-9155.
pub unsafe fn clif_handgold(pe: &PlayerEntity) -> i32 {
    let gold = {
        // SWAP32(RFIFOL(sd->fd, 5)) — network big-endian
        let raw = rfifol(pe.fd, 5);
        u32::from_be_bytes(raw.to_le_bytes())   // raw is LE from rfifol; SWAP32 makes BE → flip
    };

    // C: if (gold < 0) gold = 0; (gold is unsigned so this is a no-op, but kept for fidelity)
    if gold == 0 { return 0; }
    let gold = gold.min(pe.read().player.inventory.money);

    // Compute adjacent cell based on facing direction
    let (x, y) = side_cell(pe);

    let (map_id,) = { let g = pe.read(); (g.m,) };
    let target_id = block_grid::first_in_cell(map_id as usize, x as u16, y as u16, BL_ALL);

    pe.write().exchange.gold = gold;

    if let Some(target_id) = target_id {
        if let Some((_, bl_type)) = crate::game::map_server::entity_position(target_id) {
            if bl_type as i32 == BL_PC {
                if let Some(tpe_arc) = crate::game::map_server::map_id2sd_pc(target_id) {
                    let tpe = tpe_arc.as_ref();
                    if tpe.read().player.appearance.setting_flags as u32 & FLAG_EXCHANGE != 0 {
                        clif_startexchange(pe, tpe);
                        clif_exchange_money(pe, tpe);
                    } else {
                        let msg = b"They have refused to exchange with you\0".as_ptr() as *const i8;
                        clif_sendminitext(pe, msg);
                    }
                }
            }
        }
    }
    0
}

// ─── clif_handitem ───────────────────────────────────────────────────────────

/// Handle a "hand item" packet — offer/give item from adjacent cell.  C lines 9206-9314.
pub unsafe fn clif_handitem(pe: &PlayerEntity) -> i32 {
    let slot      = rfifob(pe.fd, 5).saturating_sub(1) as usize;
    let handgive  = rfifob(pe.fd, 6);
    let amount: i32 = if handgive == 0 {
        1
    } else {
        pe.read().player.inventory.inventory[slot].amount
    };

    let (x, y) = side_cell(pe);

    pe.write().invslot = slot as u8;

    let map_id = pe.read().m;
    let target_id = match block_grid::first_in_cell(map_id as usize, x as u16, y as u16, BL_ALL) {
        Some(id) => id,
        None => return 0,
    };
    let bl_type = match crate::game::map_server::entity_position(target_id) {
        Some((_, t)) => t as i32,
        None => return 0,
    };

    if bl_type == BL_PC {
        if let Some(tpe_arc) = crate::game::map_server::map_id2sd_pc(target_id) {
            let tpe = tpe_arc.as_ref();
            if tpe.read().player.appearance.setting_flags as u32 & FLAG_EXCHANGE != 0 {
                clif_startexchange(pe, tpe);
                clif_exchange_additem(pe, tpe, slot as i32, amount);
            } else {
                let msg = b"They have refused to exchange with you\0".as_ptr() as *const i8;
                clif_sendminitext(pe, msg);
            }
        }
    }

    if bl_type == BL_MOB {
        let mob_arc = match crate::game::map_server::map_id2mob_ref(target_id) {
            Some(a) => a, None => return 0,
        };
        let mut mob_guard = mob_arc.write();
        let mob = &mut *mob_guard as *mut crate::game::mob::MobSpawnData;

        let inv_id = pe.read().player.inventory.inventory[slot].id;
        if item_db::search(inv_id).exchangeable == 1 { return 0; }

        let (inv_dura, inv_own, inv_prot) = {
            let g = pe.read();
            let inv = &g.player.inventory.inventory[slot];
            (inv.dura, inv.owner, inv.protected)
        };

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
        pc_delitem(pe, slot as i32, amount, 9);
    }

    if bl_type == BL_NPC {
        let nd_arc = match crate::game::map_server::map_id2npc_ref(target_id) {
            Some(a) => a, None => return 0,
        };
        let mut nd_guard = nd_arc.write();
        let nd = &mut *nd_guard as *mut crate::game::npc::NpcData;

        let inv_id = pe.read().player.inventory.inventory[slot].id;
        let inv_item = item_db::search(inv_id);
        if inv_item.exchangeable != 0 || inv_item.droppable != 0 {
            return 0;
        }

        if (*nd).receive_item == 1 {
            sl_doscript_coro_2(crate::game::scripting::carray_to_str(&(*nd).name), Some("handItem"), pe.id, (*nd).id);
        } else {
            let item_name = item_db::search(inv_id).name.as_ptr();
            let mut msg = [0i8; 128];
            libc::snprintf(
                msg.as_mut_ptr(), msg.len(),
                b"What are you trying to do? Keep your junky %s with you!\0".as_ptr() as *const i8,
                item_name,
            );
            let msg_len = libc::strlen(msg.as_ptr());

            if !session_exists(pe.fd) {
                return 0;
            }

            wfifohead(pe.fd, msg_len + 11);
            wfifob(pe.fd, 5, 0);
            {
                let p = wfifop(pe.fd, 6) as *mut u32;
                if !p.is_null() { p.write_unaligned(target_id.to_be()); }
            }
            wfifob(pe.fd, 10, msg_len as u8);
            let dst = wfifop(pe.fd, 11) as *mut u8;
            if !dst.is_null() {
                std::ptr::copy_nonoverlapping(msg.as_ptr() as *const u8, dst, msg_len);
            }
            wfifob(pe.fd, 0, 0xAA);
            wfifob(pe.fd, 3, 0x0D);
            wfifob(pe.fd, 4, 0); // increment placeholder (WFIFOHEADER sets it)
            {
                let ph = wfifop(pe.fd, 1) as *mut u16;
                if !ph.is_null() { ph.write_unaligned(((msg_len + 11) as u16).to_be()); }
            }
            wfifoset(pe.fd, encrypt(pe.fd) as usize);
        }
    }
    0
}

// ─── clif_parse_exchange ─────────────────────────────────────────────────────

/// Dispatch incoming exchange sub-packet by type byte.  C lines 9438-9543.
pub unsafe fn clif_parse_exchange(pe: &PlayerEntity) -> i32 {
    let kind = rfifob(pe.fd, 5) as i32;

    let reg_str = b"goldbardupe\0".as_ptr() as *const i8;
    let _dupe_times = pc_readglobalreg(pe.data_ptr(), reg_str); // TODO(phase6c): migrate
    // C has a commented-out quarantine block here; no-op as in C.

    match kind {
        0 => {
            // Initiation: type 0
            let raw = rfifol(pe.fd, 6);
            let target_id = u32::from_be_bytes(raw.to_le_bytes());
            let tpe_arc = match crate::game::map_server::map_id2sd_pc(target_id) {
                Some(a) => a, None => return 0,
            };
            let (pe_m, pe_gm_level) = {
                let g = pe.read();
                (g.m, g.player.identity.gm_level)
            };
            let (tpe_m, tpe_bl_type, tpe_opt_flags) = {
                let g = tpe_arc.read();
                (g.m, g.bl_type, g.optFlags)
            };
            if pe_m != tpe_m || tpe_bl_type as i32 != BL_PC {
                return 0;
            }
            if pe_gm_level != 0 || (tpe_opt_flags & OPT_FLAG_STEALTH) == 0 {
                clif_startexchange(pe, tpe_arc.as_ref());
            }
        }
        1 => {
            // Add item — check if it needs an amount prompt
            let id = rfifob(pe.fd, 10).saturating_sub(1) as usize;
            if id >= pe.read().player.inventory.max_inv as usize {
                return 0;
            }
            let (inv_amount, inv_id) = {
                let g = pe.read();
                (g.player.inventory.inventory[id].amount, g.player.inventory.inventory[id].id)
            };
            if inv_amount > 1 {
                if !session_exists(pe.fd) {
                    return 0;
                }
                wfifohead(pe.fd, 7);
                wfifob(pe.fd, 0, 0xAA);
                {
                    let p = wfifop(pe.fd, 1) as *mut u16;
                    if !p.is_null() { p.write_unaligned(4_u16.to_be()); }
                }
                wfifob(pe.fd, 3, 0x42);
                wfifob(pe.fd, 4, 0x03);
                wfifob(pe.fd, 5, 0x01);
                wfifob(pe.fd, 6, (id + 1) as u8);
                wfifoset(pe.fd, encrypt(pe.fd) as usize);
            } else if inv_id != 0 {
                let raw = rfifol(pe.fd, 6);
                let target_id = u32::from_be_bytes(raw.to_le_bytes());
                let tpe_arc = match crate::game::map_server::map_id2sd_pc(target_id) {
                    Some(a) => a, None => return 0,
                };
                clif_exchange_additem(pe, tpe_arc.as_ref(), id as i32, 1);
            }
            // else: blank slot hack attempt — do nothing (matching C)
        }
        2 => {
            // Add item with explicit amount
            let id     = rfifob(pe.fd, 10).saturating_sub(1) as usize;
            if id >= pe.read().player.inventory.max_inv as usize {
                return 0;
            }
            let amount = rfifob(pe.fd, 11) as i32;
            let raw = rfifol(pe.fd, 6);
            let target_id = u32::from_be_bytes(raw.to_le_bytes());
            let tpe_arc = match crate::game::map_server::map_id2sd_pc(target_id) {
                Some(a) => a, None => return 0,
            };
            let (inv_id, inv_amount) = {
                let g = pe.read();
                (g.player.inventory.inventory[id].id, g.player.inventory.inventory[id].amount)
            };
            if amount > 0 && inv_id != 0 && amount <= inv_amount {
                clif_exchange_additem(pe, tpe_arc.as_ref(), id as i32, amount);
            }
            // else: blank slot or zero amount — do nothing
        }
        3 => {
            // Exchange gold
            let raw_target = rfifol(pe.fd, 6);
            let target_id  = u32::from_be_bytes(raw_target.to_le_bytes());
            let raw_amount = rfifol(pe.fd, 10);
            let amount     = u32::from_be_bytes(raw_amount.to_le_bytes());
            let tpe_arc = match crate::game::map_server::map_id2sd_pc(target_id) {
                Some(a) => a, None => return 0,
            };
            let tpe = tpe_arc.as_ref();
            if amount > pe.read().player.inventory.money {
                clif_exchange_money(pe, tpe);
            } else {
                pe.write().exchange.gold = amount;
                clif_exchange_money(pe, tpe);
            }
        }
        4 => {
            // Quit exchange
            let raw = rfifol(pe.fd, 6);
            let target_id = u32::from_be_bytes(raw.to_le_bytes());
            let msg = b"Exchange cancelled.\0".as_ptr() as *const i8;
            clif_exchange_message(pe, msg, 4, 0);
            if let Some(tpe_arc) = crate::game::map_server::map_id2sd_pc(target_id) {
                let tpe = tpe_arc.as_ref();
                clif_exchange_message(tpe, msg, 4, 0);
                clif_exchange_close(tpe);
            }
            clif_exchange_close(pe);
        }
        5 => {
            // Finish exchange
            let raw = rfifol(pe.fd, 6);
            let target_id = u32::from_be_bytes(raw.to_le_bytes());
            let tpe_arc = crate::game::map_server::map_id2sd_pc(target_id);

            let (pe_exchange_target, pe_id, pe_exchange_gold, pe_money, pe_fd) = {
                let g = pe.read();
                (g.exchange.target, g.id, g.exchange.gold, g.player.inventory.money, pe.fd)
            };

            if pe_exchange_target != target_id {
                clif_exchange_close(pe);
                let msg = b"Exchange cancelled.\0".as_ptr() as *const i8;
                clif_exchange_message(pe, msg, 4, 0);
                if let Some(ref arc) = tpe_arc {
                    let tpe = arc.as_ref();
                    if tpe.read().exchange.target == pe_id {
                        clif_exchange_message(tpe, msg, 4, 0);
                        clif_exchange_close(tpe);
                        session_set_eof(pe_fd, 10);
                    }
                }
                return 0;
            }
            let msg_no_gold = b"You do not have that amount.\0".as_ptr() as *const i8;
            let msg_cancel  = b"Exchange cancelled.\0".as_ptr() as *const i8;
            if pe_exchange_gold > pe_money {
                clif_exchange_message(pe, msg_no_gold, 4, 0);
                if let Some(ref arc) = tpe_arc {
                    let tpe = arc.as_ref();
                    clif_exchange_message(tpe, msg_cancel, 4, 0);
                    clif_exchange_close(tpe);
                }
                clif_exchange_close(pe);
            } else if let Some(ref arc) = tpe_arc {
                clif_exchange_sendok(pe, arc.as_ref());
            } else {
                clif_exchange_close(pe);
            }
        }
        _ => {}
    }
    0
}

// ─── side_cell: compute adjacent cell from facing direction ──────────────────

/// Return the (x, y) of the cell in front of the player based on `player.combat.side`.
/// Matches the repeated side==0..3 pattern in clif_handgold / clif_handitem.
unsafe fn side_cell(pe: &PlayerEntity) -> (i32, i32) {
    let (side, x, y) = {
        let g = pe.read();
        (g.player.combat.side, g.x, g.y)
    };
    match side {
        0 => (x as i32,     y as i32 - 1),
        1 => (x as i32 + 1, y as i32),
        2 => (x as i32,     y as i32 + 1),
        3 => (x as i32 - 1, y as i32),
        _ => (x as i32,     y as i32),
    }
}

