//! Item and equipment packet handlers.

#![allow(non_snake_case, clippy::wildcard_imports)]

use crate::database::map_db::raw_map_ptr;
use crate::database::map_db::{WarpList, BLOCK_SIZE};
use crate::common::traits::LegacyEntity;
use crate::game::mob::MOB_DEAD;
use crate::game::pc::MapSessionData;
use crate::game::pc::{
    map_msg, BL_PC, EQ_ARMOR, EQ_BOOTS, EQ_COAT, EQ_CROWN, EQ_FACEACC, EQ_HELM, EQ_LEFT, EQ_MANTLE,
    EQ_NECKLACE, EQ_RIGHT, EQ_SHIELD, EQ_SUBLEFT, EQ_SUBRIGHT, EQ_WEAP, SFLAG_FULLSTATS,
    SFLAG_HPMP, SFLAG_XPMONEY,
};
use crate::game::player::entity::PlayerEntity;
use crate::game::player::prelude::*;
use crate::session::session_exists;

// MAP_EQ* message indices
use crate::common::constants::entity::player::{
    MAP_EQARMOR, MAP_EQBOOTS, MAP_EQCOAT, MAP_EQCROWN, MAP_EQFACEACC, MAP_EQHELM, MAP_EQLEFT,
    MAP_EQMANTLE, MAP_EQNECKLACE, MAP_EQRIGHT, MAP_EQSHIELD, MAP_EQSUBLEFT, MAP_EQSUBRIGHT,
    MAP_EQWEAP,
};

use crate::common::player::inventory::MAX_INVENTORY;
use crate::common::player::spells::MAX_MAGIC_TIMERS;
use crate::game::scripting::types::floor::FloorItemData;

use super::packet::{
    clif_send, encrypt, rfifob, wfifob, wfifoheader, wfifol, wfifop, wfifoset, wfifow, SAMEAREA,
};

use crate::common::constants::entity::player::OPT_FLAG_STEALTH;

// SCRIPT subtype constant (enum { SCRIPT=0, FLOOR=1 } in map_server.h)
const SCRIPT: u8 = 0;

use crate::database::item_db;
use crate::database::magic_db;
use crate::game::client::visual::{broadcast_update_state, clif_getequiptype};
use crate::game::client::BroadcastSrc;
use crate::game::map_parse::chat::{clif_sendminitext, clif_sendmsg};
use crate::game::map_parse::combat::clif_sendaction_pc;
use crate::game::map_parse::movement::{clif_object_canmove, clif_object_canmove_from};
use crate::game::map_parse::player_state::clif_sendstatus;
use crate::game::map_server::{map_additem, map_id2name};
use crate::game::pc::{
    pc_delitem, pc_loadmagic, pc_readglobalreg, pc_reload_aether, pc_unequip, pc_useitem,
};

use crate::game::block::AreaType;
use crate::game::block_grid;
use crate::game::lua::dispatch::dispatch;
use crate::game::map_parse::visual::clif_object_look2_item;
use crate::game::pc::pc_addtocurrent_inner;
// ─── Lua dispatch helpers ─────────────────────────────────────────────────────

fn sl_doscript_simple(root: &str, method: Option<&str>, id: u32) -> bool {
    dispatch(root, method, &[id])
}

fn sl_doscript_2(root: &str, method: Option<&str>, id1: u32, id2: u32) -> bool {
    dispatch(root, method, &[id1, id2])
}

// ─── libc helpers ─────────────────────────────────────────────────────────────

unsafe fn strcasecmp_rs(a: *const i8, b: *const u8) -> i32 {
    libc::strcasecmp(a, b.cast())
}

unsafe fn strlen_cstr(p: *const i8) -> usize {
    libc::strlen(p)
}

unsafe fn strcpy_cstr(dst: *mut u8, src: *const i8) {
    libc::strcpy(dst.cast(), src);
}

unsafe fn sprintf_buf(dst: &mut [i8; 128], fmt: &[u8], arg: *const i8) {
    // Used only for formatting name strings into fixed buffers.
    // We call libc snprintf for safety.
    libc::snprintf(dst.as_mut_ptr(), 128, fmt.as_ptr().cast(), arg);
}

// ─── clif_checkinvbod ─────────────────────────────────────────────────────────

/// Validate inventory on death: break or restore items as needed.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_checkinvbod(pe: &PlayerEntity) -> i32 {
    for x in 0..MAX_INVENTORY {
        pe.write().invslot = x as u8;

        let (item_id, combat_state, inv_protected) = {
            let g = pe.read();
            (
                g.player.inventory.inventory[x].id,
                g.player.combat.state,
                g.player.inventory.inventory[x].protected,
            )
        };

        if item_id == 0 {
            continue;
        }

        let item = item_db::search(item_id);

        if combat_state == 1 && item.bod == 1 {
            if item.protected != 0 || inv_protected >= 1 {
                {
                    let mut g = pe.write();
                    g.player.inventory.inventory[x].protected =
                        g.player.inventory.inventory[x].protected.saturating_sub(1);
                    g.player.inventory.inventory[x].dura = item.dura;
                }

                let mut buf = [0i8; 256];
                libc::snprintf(
                    buf.as_mut_ptr(),
                    256,
                    c"Your %s has been restored!".as_ptr(),
                    item.name.as_ptr(),
                );
                clif_sendstatus(pe, SFLAG_FULLSTATS | SFLAG_HPMP);
                clif_sendmsg(pe, 5, buf.as_ptr());
                sl_doscript_simple("characterLog", Some("invRestore"), pe.id);
                return 0;
            }

            // Copy item into boditems before clearing it
            let (inv_item, bod_count) = {
                let g = pe.read();
                (g.player.inventory.inventory[x], g.boditems.bod_count)
            };
            let bod_idx = bod_count as usize;
            if bod_idx < 52 {
                let mut g = pe.write();
                g.boditems.item[bod_idx] = inv_item;
                g.boditems.bod_count += 1;
            }

            let mut buf = [0i8; 256];
            libc::snprintf(
                buf.as_mut_ptr(),
                256,
                c"Your %s was destroyed!".as_ptr(),
                item.name.as_ptr(),
            );
            sl_doscript_simple("characterLog", Some("invBreak"), pe.id);

            pe.write().breakid = item_id;
            sl_doscript_simple("onBreak", None, pe.id);
            sl_doscript_simple(
                crate::game::scripting::carray_to_str(&item.yname),
                Some("on_break"),
                pe.id,
            );

            pc_delitem(pe, x as i32, 1, 9);
            clif_sendmsg(pe, 5, buf.as_ptr());
        }

        broadcast_update_state(pe);
    }

    sl_doscript_simple("characterLog", Some("bodLog"), pe.id);
    pe.write().boditems.bod_count = 0;

    0
}

// ─── clif_senddelitem ─────────────────────────────────────────────────────────

/// Remove an item from the client inventory view.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_senddelitem(pe: &PlayerEntity, num: i32, r#type: i32) -> i32 {
    let n = num as usize;
    {
        let mut g = pe.write();
        g.player.inventory.inventory[n].id = 0;
        g.player.inventory.inventory[n].dura = 0;
        g.player.inventory.inventory[n].protected = 0;
        g.player.inventory.inventory[n].amount = 0;
        g.player.inventory.inventory[n].owner = 0;
        g.player.inventory.inventory[n].custom = 0;
        g.player.inventory.inventory[n].custom_look = 0;
        g.player.inventory.inventory[n].custom_look_color = 0;
        g.player.inventory.inventory[n].custom_icon = 0;
        g.player.inventory.inventory[n].custom_icon_color = 0;
        g.player.inventory.inventory[n].traps_table = [0u32; 100];
        g.player.inventory.inventory[n].time = 0;
        g.player.inventory.inventory[n].real_name[0] = 0;
    }

    if !session_exists(pe.fd) {
        return 0;
    }

    let fd = pe.fd;
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 1, 0x00);
    wfifob(fd, 2, 0x06);
    wfifob(fd, 3, 0x10);
    wfifob(fd, 5, (num + 1) as u8);
    wfifob(fd, 6, r#type as u8);
    wfifob(fd, 7, 0x00);
    wfifob(fd, 8, 0x00);
    wfifoset(fd, encrypt(fd) as usize);

    0
}

// ─── clif_sendadditem ─────────────────────────────────────────────────────────

/// Send an inventory item to the client.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendadditem(pe: &PlayerEntity, num: i32) -> i32 {
    let n = num as usize;
    let id = pe.read().player.inventory.inventory[n].id;

    let blank_item = crate::common::types::Item {
        id: 0,
        owner: 0,
        custom: 0,
        time: 0,
        dura: 0,
        amount: 0,
        pos: 0,
        _pad0: [0; 3],
        custom_look: 0,
        custom_icon: 0,
        custom_look_color: 0,
        custom_icon_color: 0,
        protected: 0,
        traps_table: [0; 100],
        buytext: [0; 64],
        note: [0; 300],
        repair: 0,
        real_name: [0; 64],
        _pad1: [0; 3],
    };

    if id < 4 {
        pe.write().player.inventory.inventory[n] = blank_item;
        return 0;
    }

    let item = item_db::search(id);
    let item_name = item.name.as_ptr();
    if id > 0 && strcasecmp_rs(item_name, c"??".as_ptr() as *const u8) == 0 {
        pe.write().player.inventory.inventory[n] = blank_item;
        return 0;
    }

    // Snapshot inventory slot fields needed for display name and packet
    let (
        inv_real_name_0,
        inv_real_name,
        inv_dura,
        inv_amount,
        inv_custom_icon,
        inv_custom_icon_color,
        inv_protected,
        inv_owner,
    ) = {
        let g = pe.read();
        let slot = &g.player.inventory.inventory[n];
        (
            slot.real_name[0],
            slot.real_name,
            slot.dura,
            slot.amount,
            slot.custom_icon,
            slot.custom_icon_color,
            slot.protected,
            slot.owner,
        )
    };

    // Choose display name
    let name_ptr: *const i8 = if inv_real_name_0 != 0 {
        inv_real_name.as_ptr()
    } else {
        item_name
    };

    // Build display name string into a fixed buffer
    let mut buf = [0i8; 128];
    {
        let item_type = item.typ as i32;
        let dura = inv_dura;
        let amount = inv_amount;
        if amount > 1 {
            libc::snprintf(buf.as_mut_ptr(), 128, c"%s (%d)".as_ptr(), name_ptr, amount);
        } else if item_type == 2 {
            libc::snprintf(
                buf.as_mut_ptr(),
                128,
                c"%s [%d %s]".as_ptr(),
                name_ptr,
                dura,
                item.text.as_ptr(),
            );
        } else if item_type == 21 {
            libc::snprintf(buf.as_mut_ptr(), 128, c"%s [%d]".as_ptr(), name_ptr, dura);
        } else if item_type == 22 {
            libc::snprintf(buf.as_mut_ptr(), 128, c"[T%d] %s".as_ptr(), dura, name_ptr);
        } else if item_type == 23 {
            libc::snprintf(buf.as_mut_ptr(), 128, c"%s [%d]".as_ptr(), name_ptr, dura);
        } else {
            libc::snprintf(buf.as_mut_ptr(), 128, c"%s".as_ptr(), name_ptr);
        }
    }

    if !session_exists(pe.fd) {
        return 0;
    }

    let fd = pe.fd;
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x0F);
    wfifob(fd, 5, (num + 1) as u8);

    // icon
    if inv_custom_icon != 0 {
        wfifow(fd, 6, ((inv_custom_icon + 49152) as u16).swap_bytes());
        wfifob(fd, 8, inv_custom_icon_color as u8);
    } else {
        wfifow(fd, 6, (item.icon as u16).swap_bytes());
        wfifob(fd, 8, item.icon_color);
    }

    // display name
    let buf_len = strlen_cstr(buf.as_ptr());
    wfifob(fd, 9, buf_len as u8);
    {
        let dst = wfifop(fd, 10);
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(buf.as_ptr().cast::<u8>(), dst, buf_len);
        }
    }
    let mut len = buf_len + 10;

    // base item name
    let base_name = item.name.as_ptr();
    let base_len = strlen_cstr(base_name);
    wfifob(fd, len, base_len as u8);
    {
        let dst = wfifop(fd, len + 1);
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(base_name.cast::<u8>(), dst, base_len);
        }
    }
    len += base_len + 1;

    // amount (big-endian u32)
    wfifol(fd, len, (inv_amount as u32).swap_bytes());
    len += 4;

    // dura/protected block
    let item_type = item.typ as i32;
    let db_prot = item.protected as u32;
    let final_prot = if inv_protected >= db_prot {
        inv_protected
    } else {
        db_prot
    };
    if (3..=17).contains(&item_type) {
        wfifob(fd, len, 0);
        wfifol(fd, len + 1, (inv_dura as u32).swap_bytes());
        wfifob(fd, len + 5, final_prot as u8);
        len += 6;
    } else {
        if item.stack_amount > 1 {
            wfifob(fd, len, 1);
        } else {
            wfifob(fd, len, 0);
        }
        wfifol(fd, len + 1, 0);
        wfifob(fd, len + 5, final_prot as u8);
        len += 6;
    }

    // owner name
    if inv_owner != 0 {
        let owner_name: String =
            crate::database::blocking_run_async(async move { map_id2name(inv_owner).await });
        let bytes = owner_name.as_bytes();
        let owner_len = bytes.len();
        wfifob(fd, len, owner_len as u8);
        {
            let dst = wfifop(fd, len + 1);
            if !dst.is_null() {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, owner_len);
            }
        }
        len += owner_len + 1;
    } else {
        wfifob(fd, len, 0);
        len += 1;
    }

    wfifow(fd, len, 0x00);
    len += 2;
    wfifob(fd, len, 0x00);
    len += 1;

    wfifow(fd, 1, (len as u16).swap_bytes());
    wfifoset(fd, encrypt(fd) as usize);

    0
}

// ─── clif_equipit ─────────────────────────────────────────────────────────────

/// Send an equip slot item to the client.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_equipit(pe: &PlayerEntity, id: i32) -> i32 {
    let slot = id as usize;

    let (eq_id, eq_real_name_0, eq_real_name, eq_custom_icon, eq_custom_icon_color, eq_dura) = {
        let g = pe.read();
        let eq = &g.player.inventory.equip[slot];
        (
            eq.id,
            eq.real_name[0],
            eq.real_name,
            eq.custom_icon,
            eq.custom_icon_color,
            eq.dura,
        )
    };

    let eq_item = item_db::search(eq_id);

    let nameof: *const i8 = if eq_real_name_0 != 0 {
        eq_real_name.as_ptr()
    } else {
        eq_item.name.as_ptr()
    };

    if !session_exists(pe.fd) {
        return 0;
    }

    let fd = pe.fd;
    wfifob(fd, 5, clif_getequiptype(id) as u8);

    if eq_custom_icon != 0 {
        wfifow(fd, 6, ((eq_custom_icon + 49152) as u16).swap_bytes());
        wfifob(fd, 8, eq_custom_icon_color as u8);
    } else {
        wfifow(fd, 6, (eq_item.icon as u16).swap_bytes());
        wfifob(fd, 8, eq_item.icon_color);
    }

    let nameof_len = strlen_cstr(nameof);
    wfifob(fd, 9, nameof_len as u8);
    {
        let dst = wfifop(fd, 10);
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(nameof.cast::<u8>(), dst, nameof_len);
        }
    }
    let mut len = nameof_len + 1;

    let base_name = eq_item.name.as_ptr();
    let base_len = strlen_cstr(base_name);
    wfifob(fd, len + 9, base_len as u8);
    {
        let dst = wfifop(fd, len + 10);
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(base_name.cast::<u8>(), dst, base_len);
        }
    }
    len += base_len + 1;

    wfifol(fd, len + 9, (eq_dura as u32).swap_bytes());
    len += 4;
    wfifow(fd, len + 9, 0x0000);
    len += 2;
    wfifoheader(fd, 0x37, (len + 6) as u16);
    wfifoset(fd, encrypt(fd) as usize);

    0
}

// ─── clif_sendequip ───────────────────────────────────────────────────────────

/// Equip an item and send a confirmation message to the client.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendequip(pe: &PlayerEntity, id: i32) -> i32 {
    let slot = id as usize;

    let msgnum: usize = match id {
        EQ_HELM => MAP_EQHELM,
        EQ_WEAP => MAP_EQWEAP,
        EQ_ARMOR => MAP_EQARMOR,
        EQ_SHIELD => MAP_EQSHIELD,
        EQ_RIGHT => MAP_EQRIGHT,
        EQ_LEFT => MAP_EQLEFT,
        EQ_SUBLEFT => MAP_EQSUBLEFT,
        EQ_SUBRIGHT => MAP_EQSUBRIGHT,
        EQ_FACEACC => MAP_EQFACEACC,
        EQ_CROWN => MAP_EQCROWN,
        EQ_BOOTS => MAP_EQBOOTS,
        EQ_MANTLE => MAP_EQMANTLE,
        EQ_COAT => MAP_EQCOAT,
        EQ_NECKLACE => MAP_EQNECKLACE,
        _ => return -1,
    };

    let (eq_id, eq_real_name_0, eq_real_name) = {
        let g = pe.read();
        let eq = &g.player.inventory.equip[slot];
        (eq.id, eq.real_name[0], eq.real_name)
    };

    let eq_item = item_db::search(eq_id);

    if eq_id > 0 && strcasecmp_rs(eq_item.name.as_ptr(), c"??".as_ptr() as *const u8) == 0 {
        pe.write().player.inventory.equip[slot] = crate::common::types::Item {
            id: 0,
            owner: 0,
            custom: 0,
            time: 0,
            dura: 0,
            amount: 0,
            pos: 0,
            _pad0: [0; 3],
            custom_look: 0,
            custom_icon: 0,
            custom_look_color: 0,
            custom_icon_color: 0,
            protected: 0,
            traps_table: [0; 100],
            buytext: [0; 64],
            note: [0; 300],
            repair: 0,
            real_name: [0; 64],
            _pad1: [0; 3],
        };
        return 0;
    }

    let name: *const i8 = if eq_real_name_0 != 0 {
        eq_real_name.as_ptr()
    } else {
        eq_item.name.as_ptr()
    };

    let mut buff = [0i8; 256];
    libc::snprintf(
        buff.as_mut_ptr(),
        256,
        map_msg()[msgnum].message.as_ptr(),
        name,
    );
    clif_equipit(pe, id);
    clif_sendminitext(pe, buff.as_ptr());

    0
}

// ─── clif_parseuseitem ────────────────────────────────────────────────────────

/// Handle a use-item packet from the client.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parseuseitem(pe: &PlayerEntity) -> i32 {
    pc_useitem(pe, rfifob(pe.fd, 5) as i32 - 1);
    0
}

// ─── clif_parseeatitem ────────────────────────────────────────────────────────

/// Handle an eat-item packet; only processes items of type ITM_EAT.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parseeatitem(pe: &PlayerEntity) -> i32 {
    let slot = rfifob(pe.fd, 5) as usize - 1;
    let id = pe.read().player.inventory.inventory[slot].id;
    // ITM_EAT = 0 (first entry in item_db.h enum)
    if item_db::search(id).typ as i32 == 0 {
        pc_useitem(pe, slot as i32);
    } else {
        clif_sendminitext(pe, c"That item is not edible.".as_ptr());
    }
    0
}

// ─── clif_parsegetitem ────────────────────────────────────────────────────────

/// Handle a pick-up-item packet from the client.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parsegetitem(pe: &PlayerEntity) -> i32 {
    let combat_state = pe.read().player.combat.state;
    if combat_state == 1 || combat_state == 3 {
        return 0; // dead can't pick up
    }

    if combat_state == 2 {
        pe.write().player.combat.state = 0;
        sl_doscript_simple("invis_rogue", Some("uncast"), pe.id);
        broadcast_update_state(pe);
    }

    clif_sendaction_pc(&mut pe.write(), 4, 40, 0);

    pe.write().pickuptype = rfifob(pe.fd, 5);

    let dura_aether = pe.read().player.spells.dura_aether.clone();
    for da in dura_aether.iter().take(MAX_MAGIC_TIMERS) {
        if da.id > 0 && da.duration > 0 {
            sl_doscript_simple(
                crate::game::scripting::carray_to_str(&magic_db::search(da.id as i32).yname),
                Some("on_pickup_while_cast"),
                pe.id,
            );
        }
    }

    sl_doscript_simple("onPickUp", None, pe.id);

    0
}

// ─── clif_unequipit ───────────────────────────────────────────────────────────

/// Send an unequip confirmation to the client.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_unequipit(pe: &PlayerEntity, spot: i32) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }

    let fd = pe.fd;
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, 4u16.swap_bytes());
    wfifob(fd, 3, 0x38);
    wfifob(fd, 4, 0x03);
    wfifob(fd, 5, spot as u8);
    wfifob(fd, 6, 0x00);
    wfifoset(fd, encrypt(fd) as usize);

    0
}

// ─── clif_parseunequip ────────────────────────────────────────────────────────

/// Handle an unequip packet from the client.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parseunequip(pe: &PlayerEntity) -> i32 {
    let slot_byte = rfifob(pe.fd, 5) as i32;
    let eq_type: i32 = match slot_byte {
        0x01 => EQ_WEAP,
        0x02 => EQ_ARMOR,
        0x03 => EQ_SHIELD,
        0x04 => EQ_HELM,
        0x06 => EQ_NECKLACE,
        0x07 => EQ_LEFT,
        0x08 => EQ_RIGHT,
        13 => EQ_BOOTS,
        14 => EQ_MANTLE,
        16 => EQ_COAT,
        20 => EQ_SUBLEFT,
        21 => EQ_SUBRIGHT,
        22 => EQ_FACEACC,
        23 => EQ_CROWN,
        _ => return 0,
    };

    let (eq_id, gm_level, maxinv) = {
        let g = pe.read();
        (
            g.player.inventory.equip[eq_type as usize].id,
            g.player.identity.gm_level,
            g.player.inventory.max_inv as usize,
        )
    };

    if item_db::search(eq_id).unequip as i32 == 1 && gm_level == 0 {
        clif_sendminitext(pe, c"You are unable to unequip that.".as_ptr());
        return 0;
    }

    for x in 0..maxinv {
        if pe.read().player.inventory.inventory[x].id == 0 {
            pc_unequip(&mut *pe.write() as *mut MapSessionData, eq_type);
            clif_unequipit(pe, slot_byte);
            return 0;
        }
    }

    clif_sendminitext(pe, c"Your inventory is full.".as_ptr());

    0
}

// ─── clif_parsewield ──────────────────────────────────────────────────────────

/// Handle a wield (equip) packet from the client.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parsewield(pe: &PlayerEntity) -> i32 {
    let pos = rfifob(pe.fd, 5) as usize - 1;
    let id = pe.read().player.inventory.inventory[pos].id;
    let item_type = item_db::search(id).typ as i32;

    if (3..=16).contains(&item_type) {
        pc_useitem(pe, pos as i32);
    } else {
        clif_sendminitext(pe, c"You cannot wield that!".as_ptr());
    }

    0
}

// ─── clif_addtocurrent ────────────────────────────────────────────────────────

/// foreach_in_cell callback: add gold to an existing floor item.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_addtocurrent_inner(
    fl: *mut FloorItemData,
    def: *mut i32,
    amount: u32,
    _pe: Option<&PlayerEntity>,
) -> i32 {
    if fl.is_null() {
        return 0;
    }

    if !def.is_null() && *def != 0 {
        return 0;
    }

    if (*fl).data.id <= 3 {
        (*fl).data.amount = ((*fl).data.amount as i64 + amount as i64) as i32;
        if !def.is_null() {
            *def = 1;
        }
    }

    0
}

// ─── clif_dropgold ────────────────────────────────────────────────────────────

/// Drop gold coins onto the current cell.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_dropgold(pe: &PlayerEntity, amounts: u32) -> i32 {
    let reg_str = b"goldbardupe\0";
    let dupe_times = pc_readglobalreg(
        &mut *pe.write() as *mut MapSessionData,
        reg_str.as_ptr().cast(),
    );
    if dupe_times != 0 {
        return 0;
    }

    let (gm_level, combat_state, money, sd_m, sd_x, sd_y) = {
        let g = pe.read();
        (
            g.player.identity.gm_level,
            g.player.combat.state,
            g.player.inventory.money,
            g.m,
            g.x,
            g.y,
        )
    };

    if gm_level == 0 {
        if combat_state == 1 {
            clif_sendminitext(pe, c"Spirits can't do that.".as_ptr());
            return 0;
        }
        if combat_state == 3 {
            clif_sendminitext(pe, c"You cannot do that while riding a mount.".as_ptr());
            return 0;
        }
        if combat_state == 4 {
            clif_sendminitext(pe, c"You cannot do that while transformed.".as_ptr());
            return 0;
        }
    }

    if money == 0 {
        return 0;
    }
    if amounts == 0 {
        return 0;
    }

    let mut amount = amounts;

    clif_sendaction_pc(&mut pe.write(), 5, 20, 0);

    let mut fl = Box::new(unsafe { std::mem::zeroed::<FloorItemData>() });
    fl.m = sd_m;
    fl.x = sd_x;
    fl.y = sd_y;

    if money < amount {
        amount = money;
        pe.write().player.inventory.money = 0;
    } else {
        pe.write().player.inventory.money -= amount;
    }

    fl.data.id = match amount {
        1 => 0u32,
        2..=99 => 1u32,
        100..=999 => 2u32,
        _ => 3u32,
    };
    fl.data.amount = amount as i32;

    pe.write().fakeDrop = 0;

    sl_doscript_2("on_drop_gold", None, pe.id, fl.id);

    let dura_aether = pe.read().player.spells.dura_aether.clone();
    for da in dura_aether.iter().take(MAX_MAGIC_TIMERS) {
        if da.id > 0 && da.duration > 0 {
            sl_doscript_2(
                crate::game::scripting::carray_to_str(&magic_db::search(da.id as i32).yname),
                Some("on_drop_gold_while_cast"),
                pe.id,
                fl.id,
            );
        }
    }

    for da in dura_aether.iter().take(MAX_MAGIC_TIMERS) {
        if da.id > 0 && da.aether > 0 {
            sl_doscript_2(
                crate::game::scripting::carray_to_str(&magic_db::search(da.id as i32).yname),
                Some("on_drop_gold_while_aether"),
                pe.id,
                fl.id,
            );
        }
    }

    if pe.read().fakeDrop != 0 {
        return 0;
    }

    let mut mini = [0i8; 64];
    libc::snprintf(
        mini.as_mut_ptr(),
        64,
        c"You dropped %d coins".as_ptr(),
        fl.data.amount,
    );
    clif_sendminitext(pe, mini.as_ptr());

    let mut def = [0i32; 1];
    if let Some(grid) = block_grid::get_grid(sd_m as usize) {
        let cell_ids = grid.ids_at_tile(sd_x, sd_y);
        for id in cell_ids {
            if let Some(fl_arc) = crate::game::map_server::map_id2fl_ref(id) {
                let fl = &mut *fl_arc.write();
                clif_addtocurrent_inner(fl as *mut FloorItemData, def.as_mut_ptr(), amount, None);
            }
        }
    }

    if def[0] == 0 {
        let fl_raw = Box::into_raw(fl);
        map_additem(fl_raw);

        sl_doscript_2("after_drop_gold", None, pe.id, (*fl_raw).id);

        let dura_aether = pe.read().player.spells.dura_aether.clone();
        for da in dura_aether.iter().take(MAX_MAGIC_TIMERS) {
            if da.id > 0 && da.duration > 0 {
                sl_doscript_2(
                    crate::game::scripting::carray_to_str(&magic_db::search(da.id as i32).yname),
                    Some("after_drop_gold_while_cast"),
                    pe.id,
                    (*fl_raw).id,
                );
            }
        }

        for da in dura_aether.iter().take(MAX_MAGIC_TIMERS) {
            if da.id > 0 && da.aether > 0 {
                sl_doscript_2(
                    crate::game::scripting::carray_to_str(&magic_db::search(da.id as i32).yname),
                    Some("after_drop_gold_while_aether"),
                    pe.id,
                    (*fl_raw).id,
                );
            }
        }

        sl_doscript_2("characterLog", Some("dropWrite"), pe.id, (*fl_raw).id);

        if let Some(grid) = block_grid::get_grid(sd_m as usize) {
            let slot = &*raw_map_ptr().add(sd_m as usize);
            let ids = block_grid::ids_in_area(
                grid,
                sd_x as i32,
                sd_y as i32,
                AreaType::Area,
                slot.xs as i32,
                slot.ys as i32,
            );
            for id in ids {
                if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_object_look2_item(pc_arc.fd, pc_arc.read().player.identity.id, &*fl_raw);
                }
            }
        }
    } else {
        drop(fl);
    }

    clif_sendstatus(pe, SFLAG_XPMONEY);

    0
}

// ─── clif_open_sub ────────────────────────────────────────────────────────────

/// Trigger the onOpen script hook.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_open_sub(pe: &PlayerEntity) -> i32 {
    sl_doscript_simple("onOpen", None, pe.id);
    0
}

// ─── clif_removespell ─────────────────────────────────────────────────────────

/// Send a remove-spell packet to the client.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_removespell(pe: &PlayerEntity, pos: i32) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }

    let fd = pe.fd;
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, 3u16.swap_bytes());
    wfifob(fd, 3, 0x18);
    wfifob(fd, 4, 0x03);
    wfifob(fd, 5, (pos + 1) as u8);
    wfifoset(fd, encrypt(fd) as usize);

    0
}

// ─── clif_parsechangespell ────────────────────────────────────────────────────

/// Handle a swap-spell-slots packet from the client.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parsechangespell(pe: &PlayerEntity) -> i32 {
    let start_pos = rfifob(pe.fd, 6) as usize - 1;
    let stop_pos = rfifob(pe.fd, 7) as usize - 1;

    let (start_id, stop_id) = {
        let g = pe.read();
        (
            g.player.spells.skills[start_pos],
            g.player.spells.skills[stop_pos],
        )
    };

    clif_removespell(pe, start_pos as i32);
    clif_removespell(pe, stop_pos as i32);

    {
        let mut g = pe.write();
        g.player.spells.skills[start_pos] = stop_id;
        g.player.spells.skills[stop_pos] = start_id;
    }

    pc_loadmagic(&mut *pe.write() as *mut MapSessionData);
    pc_reload_aether(&mut *pe.write() as *mut MapSessionData);

    0
}

// ─── clif_throwitem_sub ───────────────────────────────────────────────────────

/// Execute a throw: fire the onThrow script with source/destination floor items.
///
/// Note: this is NOT a foreachinarea callback; it is called directly.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_throwitem_sub(pe: &PlayerEntity, id: i32, _type: i32, x: i32, y: i32) -> i32 {
    let (inv_id, inv_amount, sd_m) = {
        let g = pe.read();
        (
            g.player.inventory.inventory[id as usize].id,
            g.player.inventory.inventory[id as usize].amount,
            g.m,
        )
    };
    if inv_id == 0 {
        return 0;
    }

    if inv_amount <= 0 {
        clif_senddelitem(pe, id, 4);
        return 0;
    }

    let mut fl = Box::new(unsafe { std::mem::zeroed::<FloorItemData>() });
    fl.m = sd_m;
    fl.x = x as u16;
    fl.y = y as u16;

    // memcpy(&fl->data, &sd->status.inventory[id], sizeof(struct item))
    let inv_item = pe.read().player.inventory.inventory[id as usize];
    std::ptr::copy_nonoverlapping(
        &inv_item as *const _ as *const u8,
        &raw mut fl.data as *mut u8,
        std::mem::size_of::<crate::common::types::Item>(),
    );

    {
        let mut g = pe.write();
        g.invslot = id as u8;
        g.throwx = x as u16;
        g.throwy = y as u16;
    }

    sl_doscript_2("onThrow", None, pe.id, fl.id);

    // fl is dropped here — it was a temporary used only to pass data to the script.
    drop(fl);
    0
}

// ─── clif_throwitem_script ────────────────────────────────────────────────────

/// Complete a throw action after script approval.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_throwitem_script(pe: &PlayerEntity) -> i32 {
    let (id, x, y, sd_m, sd_x, sd_y) = {
        let g = pe.read();
        (
            g.invslot as usize,
            g.throwx as i32,
            g.throwy as i32,
            g.m,
            g.x,
            g.y,
        )
    };
    let item_type = 0i32;

    let mut fl = Box::new(unsafe { std::mem::zeroed::<FloorItemData>() });
    fl.m = sd_m;
    fl.x = x as u16;
    fl.y = y as u16;

    let inv_item = pe.read().player.inventory.inventory[id];
    std::ptr::copy_nonoverlapping(
        &inv_item as *const _ as *const u8,
        &raw mut fl.data as *mut u8,
        std::mem::size_of::<crate::common::types::Item>(),
    );

    let mut def = [0i32; 1];

    if fl.data.dura == item_db::search(fl.data.id).dura {
        if let Some(grid) = block_grid::get_grid(sd_m as usize) {
            let cell_ids = grid.ids_at_tile(x as u16, y as u16);
            for cid in cell_ids {
                if let Some(fl_ref_arc) = crate::game::map_server::map_id2fl_ref(cid) {
                    let fl_ref = &mut *fl_ref_arc.write();
                    pc_addtocurrent_inner(
                        &mut *fl_ref as *mut FloorItemData,
                        def.as_mut_ptr(),
                        id as i32,
                        item_type,
                        &mut *pe.write() as *mut MapSessionData,
                    );
                }
            }
        }
    }

    pe.write().player.inventory.inventory[id].amount -= 1;

    let inv_amount = pe.read().player.inventory.inventory[id].amount;
    if item_type != 0 || inv_amount == 0 {
        pe.write().player.inventory.inventory[id] = crate::common::types::Item {
            id: 0,
            owner: 0,
            custom: 0,
            time: 0,
            dura: 0,
            amount: 0,
            pos: 0,
            _pad0: [0; 3],
            custom_look: 0,
            custom_icon: 0,
            custom_look_color: 0,
            custom_icon_color: 0,
            protected: 0,
            traps_table: [0; 100],
            buytext: [0; 64],
            note: [0; 300],
            repair: 0,
            real_name: [0; 64],
            _pad1: [0; 3],
        };
        clif_senddelitem(pe, id as i32, 4);
    } else {
        fl.data.amount = 1;
        clif_sendadditem(pe, id as i32);
    }

    if sd_x as i32 != x {
        let mut sndbuf = [0u8; 48];
        sndbuf[0] = 0xAA;
        let len_be = 0x1Bu16.to_be_bytes();
        sndbuf[1] = len_be[0];
        sndbuf[2] = len_be[1];
        sndbuf[3] = 0x16;
        sndbuf[4] = 0x03;
        let id_be = pe.id.to_be_bytes();
        sndbuf[5] = id_be[0];
        sndbuf[6] = id_be[1];
        sndbuf[7] = id_be[2];
        sndbuf[8] = id_be[3];

        if fl.data.custom_icon != 0 {
            let icon_be = ((fl.data.custom_icon + 49152) as u16).to_be_bytes();
            sndbuf[9] = icon_be[0];
            sndbuf[10] = icon_be[1];
            sndbuf[11] = fl.data.custom_icon_color as u8;
        } else {
            let fl_item = item_db::search(fl.data.id);
            let icon_be = (fl_item.icon as u16).to_be_bytes();
            sndbuf[9] = icon_be[0];
            sndbuf[10] = icon_be[1];
            sndbuf[11] = fl_item.icon_color;
        }

        let fl_id_be = if def[0] != 0 {
            (def[0] as u32).to_be_bytes()
        } else {
            (fl.id).to_be_bytes()
        };
        sndbuf[12] = fl_id_be[0];
        sndbuf[13] = fl_id_be[1];
        sndbuf[14] = fl_id_be[2];
        sndbuf[15] = fl_id_be[3];

        let sx_be = sd_x.to_be_bytes();
        sndbuf[16] = sx_be[0];
        sndbuf[17] = sx_be[1];
        let sy_be = sd_y.to_be_bytes();
        sndbuf[18] = sy_be[0];
        sndbuf[19] = sy_be[1];
        let dx_be = (x as u16).to_be_bytes();
        sndbuf[20] = dx_be[0];
        sndbuf[21] = dx_be[1];
        let dy_be = (y as u16).to_be_bytes();
        sndbuf[22] = dy_be[0];
        sndbuf[23] = dy_be[1];
        // bytes 24..27 already 0
        sndbuf[28] = 0x02;
        sndbuf[29] = 0x00;

        clif_send(
            sndbuf.as_ptr(),
            48,
            BroadcastSrc {
                id: pe.id,
                m: sd_m,
                x: sd_x,
                y: sd_y,
                bl_type: BL_PC as u8,
            },
            SAMEAREA,
        );
    } else {
        clif_sendaction_pc(&mut pe.write(), 2, 30, 0);
    }

    if def[0] == 0 {
        let fl_raw = Box::into_raw(fl);
        map_additem(fl_raw);
        if let Some(grid) = block_grid::get_grid(sd_m as usize) {
            let slot = &*raw_map_ptr().add(sd_m as usize);
            let ids = block_grid::ids_in_area(
                grid,
                sd_x as i32,
                sd_y as i32,
                AreaType::Area,
                slot.xs as i32,
                slot.ys as i32,
            );
            for id in ids {
                if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_object_look2_item(pc_arc.fd, pc_arc.read().player.identity.id, &*fl_raw);
                }
            }
        }
    } else {
        drop(fl);
    }

    0
}

// ─── clif_throw_check ─────────────────────────────────────────────────────────

/// foreach_in_cell callback: check if a cell is blocked for throwing.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_throw_check_id(entity_id: u32, found: *mut i32) -> i32 {
    if !found.is_null() && *found != 0 {
        return 0;
    }

    // Check entity type and alive status via typed lookups
    if let Some(arc) = crate::game::map_server::map_id2npc_ref(entity_id) {
        let nd = arc.read();
        if nd.subtype != SCRIPT {
            return 0;
        }
    } else if let Some(arc) = crate::game::map_server::map_id2mob_ref(entity_id) {
        if arc.read().state == MOB_DEAD {
            return 0;
        }
    } else if let Some(arc) = crate::game::map_server::map_id2sd_pc(entity_id) {
        let sd = arc.read();
        if sd.player.combat.state == 1 || (sd.optFlags & OPT_FLAG_STEALTH) != 0 {
            return 0;
        }
    } else {
        return 0; // Entity not found
    }

    if !found.is_null() {
        *found += 1;
    }

    0
}

// ─── clif_throwconfirm ────────────────────────────────────────────────────────

/// Send a throw-confirm packet to the client.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_throwconfirm(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, 7u16.swap_bytes());
    wfifob(fd, 3, 0x4E);
    wfifob(fd, 5, rfifob(pe.fd, 6));
    wfifob(fd, 6, 0);
    wfifoset(fd, encrypt(fd) as usize);

    0
}

// ─── clif_parsethrow ──────────────────────────────────────────────────────────

/// Handle a throw-item packet from the client.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parsethrow(pe: &PlayerEntity) -> i32 {
    let reg_str = b"goldbardupe\0";
    let dupe_times = pc_readglobalreg(
        &mut *pe.write() as *mut MapSessionData,
        reg_str.as_ptr().cast(),
    );
    if dupe_times != 0 {
        return 0;
    }

    let (gm_level, combat_state, combat_side, sd_m, sd_x, sd_y) = {
        let g = pe.read();
        (
            g.player.identity.gm_level,
            g.player.combat.state,
            g.player.combat.side,
            g.m,
            g.x,
            g.y,
        )
    };

    if gm_level == 0 {
        if combat_state == 1 {
            clif_sendminitext(pe, c"Spirits can't do that.".as_ptr());
            return 0;
        }
        if combat_state == 3 {
            clif_sendminitext(pe, c"You cannot do that while riding a mount.".as_ptr());
            return 0;
        }
        if combat_state == 4 {
            clif_sendminitext(pe, c"You cannot do that while transformed.".as_ptr());
            return 0;
        }
    }

    let pos = rfifob(pe.fd, 6) as usize - 1;
    let inv_id = pe.read().player.inventory.inventory[pos].id;
    if item_db::search(inv_id).droppable != 0 {
        clif_sendminitext(pe, c"You can't throw this item.".as_ptr());
        return 0;
    }

    let max = 8i32;
    let mut newx: i32 = sd_x as i32;
    let mut newy: i32 = sd_y as i32;
    let mut xmod: i32 = 0;
    let mut ymod: i32 = 0;
    let mut found = [0i32; 1];

    match combat_side {
        0 => {
            ymod = -1;
        } // up
        1 => {
            xmod = 1;
        } // left
        2 => {
            ymod = 1;
        } // down
        3 => {
            xmod = -1;
        } // right
        _ => {}
    }

    let m = sd_m as i32;
    let map_data = &*raw_map_ptr().add(m as usize);

    'search: for i in 0..max {
        let mut x1: i32 = sd_x as i32 + (i * xmod) + xmod;
        let mut y1: i32 = sd_y as i32 + (i * ymod) + ymod;
        if x1 < 0 {
            x1 = 0;
        }
        if y1 < 0 {
            y1 = 0;
        }
        if x1 >= map_data.xs as i32 {
            x1 = map_data.xs as i32 - 1;
        }
        if y1 >= map_data.ys as i32 {
            y1 = map_data.ys as i32 - 1;
        }

        if let Some(grid) = block_grid::get_grid(m as usize) {
            let cell_ids = grid.ids_at_tile(x1 as u16, y1 as u16);
            for cid in cell_ids {
                clif_throw_check_id(cid, found.as_mut_ptr());
            }
        }
        // read_pass(m, x, y) — accesses map[m].pass[x + y*xs]
        let pass_val = if raw_map_ptr().is_null() {
            0
        } else {
            let md = &*raw_map_ptr().add(m as usize);
            if md.pass.is_null() {
                0
            } else {
                *md.pass.add(x1 as usize + y1 as usize * md.xs as usize) as i32
            }
        };
        found[0] += pass_val;
        found[0] += clif_object_canmove(m, x1, y1, combat_side as i32);
        found[0] += clif_object_canmove_from(m, x1, y1, combat_side as i32);

        // Check warp list at this block cell
        if !map_data.warp.is_null() {
            let bidx =
                x1 as usize / BLOCK_SIZE + (y1 as usize / BLOCK_SIZE) * map_data.bxs as usize;
            let mut warp: *mut WarpList = map_data.warp.add(bidx).read();
            while !warp.is_null() && found[0] == 0 {
                if (*warp).x == x1 && (*warp).y == y1 {
                    found[0] += 1;
                }
                warp = (*warp).next;
            }
        }

        if found[0] != 0 {
            break 'search;
        }
        newx = x1;
        newy = y1;
    }

    clif_throwitem_sub(pe, pos as i32, 0, newx, newy)
}
