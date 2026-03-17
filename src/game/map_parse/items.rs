//! Item and equipment packet handlers.

#![allow(non_snake_case, clippy::wildcard_imports)]


use crate::database::map_db::{BlockList, WarpList, BLOCK_SIZE};
use crate::database::map_db::raw_map_ptr;
use crate::session::session_exists;
use crate::game::mob::MOB_DEAD;
use crate::game::pc::{
    MapSessionData,
    BL_PC, BL_MOB, BL_NPC,
    EQ_WEAP, EQ_ARMOR, EQ_SHIELD, EQ_HELM, EQ_LEFT, EQ_RIGHT,
    EQ_SUBLEFT, EQ_SUBRIGHT, EQ_FACEACC, EQ_CROWN, EQ_MANTLE, EQ_NECKLACE, EQ_BOOTS, EQ_COAT,
    SFLAG_FULLSTATS, SFLAG_HPMP, SFLAG_XPMONEY,
    map_msg,
    LOOK_SEND,
};

// MAP_EQ* message indices
const MAP_EQHELM:     usize = 13;
const MAP_EQWEAP:     usize = 14;
const MAP_EQARMOR:    usize = 15;
const MAP_EQSHIELD:   usize = 16;
const MAP_EQLEFT:     usize = 17;
const MAP_EQRIGHT:    usize = 18;
const MAP_EQSUBLEFT:  usize = 19;
const MAP_EQSUBRIGHT: usize = 20;
const MAP_EQFACEACC:  usize = 21;
const MAP_EQCROWN:    usize = 22;
const MAP_EQMANTLE:   usize = 23;
const MAP_EQNECKLACE: usize = 24;
const MAP_EQBOOTS:    usize = 25;
const MAP_EQCOAT:     usize = 26;

use crate::game::mob::MobSpawnData;
use crate::game::scripting::types::floor::FloorItemData;
use crate::common::player::inventory::MAX_INVENTORY;
use crate::common::player::spells::MAX_MAGIC_TIMERS;

use super::packet::{
    encrypt,
    wfifob, wfifow, wfifol, wfifop, wfifoset, wfifoheader,
    rfifob,
    clif_send,
    SAMEAREA,
};

// optFlag_stealth = 32 (from map_server.h)
const OPT_FLAG_STEALTH: i32 = 32;

// SCRIPT subtype constant (enum { SCRIPT=0, FLOOR=1 } in map_server.h)
const SCRIPT: u8 = 0;


use crate::game::map_parse::player_state::clif_sendstatus;
use crate::game::map_parse::chat::{clif_sendmsg, clif_sendminitext};
use crate::game::client::visual::{clif_getequiptype, broadcast_update_state};
use crate::game::map_parse::combat::clif_sendaction;
use crate::game::map_parse::movement::{clif_object_canmove, clif_object_canmove_from};
use crate::game::map_server::{map_id2name, map_additem};
use crate::game::pc::{
    pc_readglobalreg,
    pc_useitem, pc_unequip, pc_delitem, pc_loadmagic, pc_reload_aether,
};
use crate::database::item_db;
use crate::database::magic_db;



use crate::game::pc::pc_addtocurrent_inner;
use crate::game::block::AreaType;
use crate::game::block_grid;
use crate::game::map_parse::visual::clif_object_look_sub2_inner;

// ─── Lua dispatch helpers ─────────────────────────────────────────────────────

fn sl_doscript_simple(root: &str, method: Option<&str>, id: u32) -> i32 {
    crate::game::scripting::doscript_blargs_id(root, method, &[id])
}

fn sl_doscript_2(root: &str, method: Option<&str>, id1: u32, id2: u32) -> i32 {
    crate::game::scripting::doscript_blargs_id(root, method, &[id1, id2])
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
pub unsafe fn clif_checkinvbod(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }

    for x in 0..MAX_INVENTORY {
        (*sd).invslot = x as u8;

        if (&(*sd).player.inventory.inventory)[x].id == 0 { continue; }

        let id = (&(*sd).player.inventory.inventory)[x].id;
        let item = item_db::search(id);

        if (*sd).player.combat.state == 1
            && item.bod == 1
        {
            if item.protected != 0
                || (&(*sd).player.inventory.inventory)[x].protected >= 1
            {
                (&mut (*sd).player.inventory.inventory)[x].protected =
                    (&(*sd).player.inventory.inventory)[x].protected.saturating_sub(1);
                (&mut (*sd).player.inventory.inventory)[x].dura = item.dura;

                let mut buf = [0i8; 256];
                libc::snprintf(
                    buf.as_mut_ptr(),
                    256,
                    b"Your %s has been restored!\0".as_ptr().cast(),
                    item.name.as_ptr(),
                );
                clif_sendstatus(sd, SFLAG_FULLSTATS | SFLAG_HPMP);
                clif_sendmsg(sd, 5, buf.as_ptr());
                sl_doscript_simple("characterLog", Some("invRestore"), (*sd).id);
                return 0;
            }

            // Copy item into boditems before clearing it
            let bod_idx = (*sd).boditems.bod_count as usize;
            if bod_idx < 52 {
                (*sd).boditems.item[bod_idx] = (&(*sd).player.inventory.inventory)[x];
                (*sd).boditems.bod_count += 1;
            }

            let mut buf = [0i8; 256];
            libc::snprintf(
                buf.as_mut_ptr(),
                256,
                b"Your %s was destroyed!\0".as_ptr().cast(),
                item.name.as_ptr(),
            );
            sl_doscript_simple("characterLog", Some("invBreak"), (*sd).id);

            (*sd).breakid = id;
            sl_doscript_simple("onBreak", None, (*sd).id);
            sl_doscript_simple(crate::game::scripting::carray_to_str(&item.yname), Some("on_break"), (*sd).id);

            pc_delitem(sd, x as i32, 1, 9);
            clif_sendmsg(sd, 5, buf.as_ptr());
        }

        broadcast_update_state(sd);
    }

    sl_doscript_simple("characterLog", Some("bodLog"), (*sd).id);
    (*sd).boditems.bod_count = 0;

    0
}

// ─── clif_senddelitem ─────────────────────────────────────────────────────────

/// Remove an item from the client inventory view.
///
pub unsafe fn clif_senddelitem(sd: *mut MapSessionData, num: i32, r#type: i32) -> i32 {
    let n = num as usize;
    (&mut (*sd).player.inventory.inventory)[n].id = 0;
    (&mut (*sd).player.inventory.inventory)[n].dura = 0;
    (&mut (*sd).player.inventory.inventory)[n].protected = 0;
    (&mut (*sd).player.inventory.inventory)[n].amount = 0;
    (&mut (*sd).player.inventory.inventory)[n].owner = 0;
    (&mut (*sd).player.inventory.inventory)[n].custom = 0;
    (&mut (*sd).player.inventory.inventory)[n].custom_look = 0;
    (&mut (*sd).player.inventory.inventory)[n].custom_look_color = 0;
    (&mut (*sd).player.inventory.inventory)[n].custom_icon = 0;
    (&mut (*sd).player.inventory.inventory)[n].custom_icon_color = 0;
    (&mut (*sd).player.inventory.inventory)[n].traps_table = [0u32; 100];
    (&mut (*sd).player.inventory.inventory)[n].time = 0;
    (&mut (*sd).player.inventory.inventory)[n].real_name[0] = 0;

    if !session_exists((*sd).fd) {
        return 0;
    }

    let fd = (*sd).fd;
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
pub unsafe fn clif_sendadditem(sd: *mut MapSessionData, num: i32) -> i32 {
    let n = num as usize;
    let id = (&(*sd).player.inventory.inventory)[n].id;

    if id < 4 {
        (&mut (*sd).player.inventory.inventory)[n] = crate::common::types::Item {
            id: 0, owner: 0, custom: 0, time: 0, dura: 0, amount: 0,
            pos: 0, _pad0: [0; 3], custom_look: 0, custom_icon: 0,
            custom_look_color: 0, custom_icon_color: 0, protected: 0,
            traps_table: [0; 100], buytext: [0; 64], note: [0; 300],
            repair: 0, real_name: [0; 64], _pad1: [0; 3],
        };
        return 0;
    }

    let item = item_db::search(id);
    let item_name = item.name.as_ptr();
    if id > 0 && strcasecmp_rs(item_name, b"??\0".as_ptr()) == 0 {
        (&mut (*sd).player.inventory.inventory)[n] = crate::common::types::Item {
            id: 0, owner: 0, custom: 0, time: 0, dura: 0, amount: 0,
            pos: 0, _pad0: [0; 3], custom_look: 0, custom_icon: 0,
            custom_look_color: 0, custom_icon_color: 0, protected: 0,
            traps_table: [0; 100], buytext: [0; 64], note: [0; 300],
            repair: 0, real_name: [0; 64], _pad1: [0; 3],
        };
        return 0;
    }

    // Choose display name
    let name_ptr: *const i8 = if (&(*sd).player.inventory.inventory)[n].real_name[0] != 0 {
        (&(*sd).player.inventory.inventory)[n].real_name.as_ptr()
    } else {
        item_name
    };

    // Build display name string into a fixed buffer
    let mut buf = [0i8; 128];
    {
        let item_type = item.typ as i32;
        let dura = (&(*sd).player.inventory.inventory)[n].dura;
        let amount = (&(*sd).player.inventory.inventory)[n].amount;
        // ITM_SMOKE=2, ITM_BAG=21, ITM_MAP=22, ITM_QUIVER=23
        // These are handled via format string exactly as in C.
        if amount > 1 {
            libc::snprintf(
                buf.as_mut_ptr(), 128,
                b"%s (%d)\0".as_ptr().cast(),
                name_ptr, amount,
            );
        } else if item_type == 2 {
            // ITM_SMOKE
            libc::snprintf(
                buf.as_mut_ptr(), 128,
                b"%s [%d %s]\0".as_ptr().cast(),
                name_ptr, dura, item.text.as_ptr(),
            );
        } else if item_type == 21 {
            // ITM_BAG
            libc::snprintf(
                buf.as_mut_ptr(), 128,
                b"%s [%d]\0".as_ptr().cast(),
                name_ptr, dura,
            );
        } else if item_type == 22 {
            // ITM_MAP
            libc::snprintf(
                buf.as_mut_ptr(), 128,
                b"[T%d] %s\0".as_ptr().cast(),
                dura, name_ptr,
            );
        } else if item_type == 23 {
            // ITM_QUIVER
            libc::snprintf(
                buf.as_mut_ptr(), 128,
                b"%s [%d]\0".as_ptr().cast(),
                name_ptr, dura,
            );
        } else {
            libc::snprintf(
                buf.as_mut_ptr(), 128,
                b"%s\0".as_ptr().cast(),
                name_ptr,
            );
        }
    }

    if !session_exists((*sd).fd) {
        return 0;
    }

    let fd = (*sd).fd;
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x0F);
    wfifob(fd, 5, (num + 1) as u8);

    // icon
    if (&(*sd).player.inventory.inventory)[n].custom_icon != 0 {
        wfifow(fd, 6, (((&(*sd).player.inventory.inventory)[n].custom_icon + 49152) as u16).swap_bytes());
        wfifob(fd, 8, (&(*sd).player.inventory.inventory)[n].custom_icon_color as u8);
    } else {
        wfifow(fd, 6, (item.icon as u16).swap_bytes());
        wfifob(fd, 8, item.icon_color);
    }

    // display name
    let buf_len = strlen_cstr(buf.as_ptr()) as usize;
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
    wfifol(fd, len, ((&(*sd).player.inventory.inventory)[n].amount as u32).swap_bytes());
    len += 4;

    // dura/protected block
    let item_type = item.typ as i32;
    if item_type >= 3 && item_type <= 17 {
        wfifob(fd, len, 0);
        wfifol(fd, len + 1, ((&(*sd).player.inventory.inventory)[n].dura as u32).swap_bytes());

        let inv_prot = (&(*sd).player.inventory.inventory)[n].protected;
        let db_prot = item.protected as u32;
        let final_prot = if inv_prot >= db_prot { inv_prot } else { db_prot };
        wfifob(fd, len + 5, final_prot as u8);

        len += 6;
    } else {
        if item.stack_amount > 1 {
            wfifob(fd, len, 1);
        } else {
            wfifob(fd, len, 0);
        }
        wfifol(fd, len + 1, 0);

        let inv_prot = (&(*sd).player.inventory.inventory)[n].protected;
        let db_prot = item.protected as u32;
        let final_prot = if inv_prot >= db_prot { inv_prot } else { db_prot };
        wfifob(fd, len + 5, final_prot as u8);

        len += 6;
    }

    // owner name
    if (&(*sd).player.inventory.inventory)[n].owner != 0 {
        let owner_id = (&(*sd).player.inventory.inventory)[n].owner;
        let owner_name: String = crate::database::blocking_run_async(async move {
            map_id2name(owner_id).await
        });
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
pub unsafe fn clif_equipit(sd: *mut MapSessionData, id: i32) -> i32 {
    let slot = id as usize;

    let eq_item = item_db::search((&(*sd).player.inventory.equip)[slot].id);

    let nameof: *const i8 = if (&(*sd).player.inventory.equip)[slot].real_name[0] != 0 {
        (&(*sd).player.inventory.equip)[slot].real_name.as_ptr()
    } else {
        eq_item.name.as_ptr()
    };

    if !session_exists((*sd).fd) {
        return 0;
    }

    let fd = (*sd).fd;
    wfifob(fd, 5, clif_getequiptype(id) as u8);

    if (&(*sd).player.inventory.equip)[slot].custom_icon != 0 {
        wfifow(fd, 6, (((&(*sd).player.inventory.equip)[slot].custom_icon + 49152) as u16).swap_bytes());
        wfifob(fd, 8, (&(*sd).player.inventory.equip)[slot].custom_icon_color as u8);
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

    wfifol(fd, len + 9, ((&(*sd).player.inventory.equip)[slot].dura as u32).swap_bytes());
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
pub unsafe fn clif_sendequip(sd: *mut MapSessionData, id: i32) -> i32 {
    let slot = id as usize;

    let msgnum: usize = match id {
        EQ_HELM     => MAP_EQHELM,
        EQ_WEAP     => MAP_EQWEAP,
        EQ_ARMOR    => MAP_EQARMOR,
        EQ_SHIELD   => MAP_EQSHIELD,
        EQ_RIGHT    => MAP_EQRIGHT,
        EQ_LEFT     => MAP_EQLEFT,
        EQ_SUBLEFT  => MAP_EQSUBLEFT,
        EQ_SUBRIGHT => MAP_EQSUBRIGHT,
        EQ_FACEACC  => MAP_EQFACEACC,
        EQ_CROWN    => MAP_EQCROWN,
        EQ_BOOTS    => MAP_EQBOOTS,
        EQ_MANTLE   => MAP_EQMANTLE,
        EQ_COAT     => MAP_EQCOAT,
        EQ_NECKLACE => MAP_EQNECKLACE,
        _           => return -1,
    };

    let eq_item = item_db::search((&(*sd).player.inventory.equip)[slot].id);

    if (&(*sd).player.inventory.equip)[slot].id > 0
        && strcasecmp_rs(eq_item.name.as_ptr(), b"??\0".as_ptr()) == 0
    {
        (&mut (*sd).player.inventory.equip)[slot] = crate::common::types::Item {
            id: 0, owner: 0, custom: 0, time: 0, dura: 0, amount: 0,
            pos: 0, _pad0: [0; 3], custom_look: 0, custom_icon: 0,
            custom_look_color: 0, custom_icon_color: 0, protected: 0,
            traps_table: [0; 100], buytext: [0; 64], note: [0; 300],
            repair: 0, real_name: [0; 64], _pad1: [0; 3],
        };
        return 0;
    }

    let name: *const i8 = if (&(*sd).player.inventory.equip)[slot].real_name[0] != 0 {
        (&(*sd).player.inventory.equip)[slot].real_name.as_ptr()
    } else {
        eq_item.name.as_ptr()
    };

    let mut buff = [0i8; 256];
    libc::snprintf(
        buff.as_mut_ptr(), 256,
        map_msg()[msgnum].message.as_ptr(),
        name,
    );
    clif_equipit(sd, id);
    clif_sendminitext(sd, buff.as_ptr());

    0
}

// ─── clif_parseuseitem ────────────────────────────────────────────────────────

/// Handle a use-item packet from the client.
///
pub unsafe fn clif_parseuseitem(sd: *mut MapSessionData) -> i32 {
    pc_useitem(sd, rfifob((*sd).fd, 5) as i32 - 1);
    0
}

// ─── clif_parseeatitem ────────────────────────────────────────────────────────

/// Handle an eat-item packet; only processes items of type ITM_EAT.
///
pub unsafe fn clif_parseeatitem(sd: *mut MapSessionData) -> i32 {
    let slot = rfifob((*sd).fd, 5) as usize - 1;
    let id = (&(*sd).player.inventory.inventory)[slot].id;
    // ITM_EAT = 0 (first entry in item_db.h enum)
    if item_db::search(id).typ as i32 == 0 {
        pc_useitem(sd, slot as i32);
    } else {
        clif_sendminitext(sd, b"That item is not edible.\0".as_ptr().cast());
    }
    0
}

// ─── clif_parsegetitem ────────────────────────────────────────────────────────

/// Handle a pick-up-item packet from the client.
///
pub unsafe fn clif_parsegetitem(sd: *mut MapSessionData) -> i32 {
    if (*sd).player.combat.state == 1 || (*sd).player.combat.state == 3 {
        return 0; // dead can't pick up
    }

    if (*sd).player.combat.state == 2 {
        (*sd).player.combat.state = 0;
        sl_doscript_simple("invis_rogue", Some("uncast"), (*sd).id);
        broadcast_update_state(sd);
    }

    clif_sendaction((*sd).as_bl_mut(), 4, 40, 0);

    (*sd).pickuptype = rfifob((*sd).fd, 5);

    for x in 0..MAX_MAGIC_TIMERS {
        if (&(*sd).player.spells.dura_aether)[x].id > 0
            && (&(*sd).player.spells.dura_aether)[x].duration > 0
        {
            sl_doscript_simple(crate::game::scripting::carray_to_str(&magic_db::search((&(*sd).player.spells.dura_aether)[x].id as i32).yname), Some("on_pickup_while_cast"), (*sd).id);
        }
    }

    sl_doscript_simple("onPickUp", None, (*sd).id);

    0
}

// ─── clif_unequipit ───────────────────────────────────────────────────────────

/// Send an unequip confirmation to the client.
///
pub unsafe fn clif_unequipit(sd: *mut MapSessionData, spot: i32) -> i32 {
    if !session_exists((*sd).fd) {
        return 0;
    }

    let fd = (*sd).fd;
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
pub unsafe fn clif_parseunequip(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }

    let slot_byte = rfifob((*sd).fd, 5) as i32;
    let eq_type: i32 = match slot_byte {
        0x01 => EQ_WEAP,
        0x02 => EQ_ARMOR,
        0x03 => EQ_SHIELD,
        0x04 => EQ_HELM,
        0x06 => EQ_NECKLACE,
        0x07 => EQ_LEFT,
        0x08 => EQ_RIGHT,
        13   => EQ_BOOTS,
        14   => EQ_MANTLE,
        16   => EQ_COAT,
        20   => EQ_SUBLEFT,
        21   => EQ_SUBRIGHT,
        22   => EQ_FACEACC,
        23   => EQ_CROWN,
        _    => return 0,
    };

    if item_db::search((&(*sd).player.inventory.equip)[eq_type as usize].id).unequip as i32 == 1
        && (*sd).player.identity.gm_level == 0
    {
        clif_sendminitext(sd, b"You are unable to unequip that.\0".as_ptr().cast());
        return 0;
    }

    let maxinv = (*sd).player.inventory.max_inv as usize;
    for x in 0..maxinv {
        if (&(*sd).player.inventory.inventory)[x].id == 0 {
            pc_unequip(sd, eq_type);
            clif_unequipit(sd, slot_byte);
            return 0;
        }
    }

    clif_sendminitext(sd, b"Your inventory is full.\0".as_ptr().cast());

    0
}

// ─── clif_parsewield ──────────────────────────────────────────────────────────

/// Handle a wield (equip) packet from the client.
///
pub unsafe fn clif_parsewield(sd: *mut MapSessionData) -> i32 {
    let pos = rfifob((*sd).fd, 5) as usize - 1;
    let id = (&(*sd).player.inventory.inventory)[pos].id;
    let item_type = item_db::search(id).typ as i32;

    if item_type >= 3 && item_type <= 16 {
        pc_useitem(sd, pos as i32);
    } else {
        clif_sendminitext(sd, b"You cannot wield that!\0".as_ptr().cast());
    }

    0
}

// ─── clif_addtocurrent ────────────────────────────────────────────────────────

/// foreach_in_cell callback: add gold to an existing floor item.
///
pub unsafe fn clif_addtocurrent_inner(bl: *mut BlockList, def: *mut i32, amount: u32, _sd: *mut MapSessionData) -> i32 {
    if bl.is_null() { return 0; }
    let fl = bl as *mut FloorItemData;

    if !def.is_null() && *def != 0 { return 0; }

    if (*fl).data.id <= 3 {
        (*fl).data.amount = ((*fl).data.amount as i64 + amount as i64) as i32;
        if !def.is_null() { *def = 1; }
    }

    0
}

// ─── clif_dropgold ────────────────────────────────────────────────────────────

/// Drop gold coins onto the current cell.
///
pub unsafe fn clif_dropgold(sd: *mut MapSessionData, amounts: u32) -> i32 {
    let reg_str = b"goldbardupe\0";
    let dupe_times = pc_readglobalreg(sd, reg_str.as_ptr().cast());
    if dupe_times != 0 {
        return 0;
    }

    if (*sd).player.identity.gm_level == 0 {
        if (*sd).player.combat.state == 1 {
            clif_sendminitext(sd, b"Spirits can't do that.\0".as_ptr().cast());
            return 0;
        }
        if (*sd).player.combat.state == 3 {
            clif_sendminitext(sd, b"You cannot do that while riding a mount.\0".as_ptr().cast());
            return 0;
        }
        if (*sd).player.combat.state == 4 {
            clif_sendminitext(sd, b"You cannot do that while transformed.\0".as_ptr().cast());
            return 0;
        }
    }

    if (*sd).player.inventory.money == 0 { return 0; }
    if amounts == 0 { return 0; }

    let mut amount = amounts;

    clif_sendaction((*sd).as_bl_mut(), 5, 20, 0);

    let mut fl = Box::new(unsafe { std::mem::zeroed::<FloorItemData>() });
    (*fl).m = (*sd).m;
    (*fl).x = (*sd).x;
    (*fl).y = (*sd).y;

    if (*sd).player.inventory.money < amount {
        amount = (*sd).player.inventory.money;
        (*sd).player.inventory.money = 0;
    } else {
        (*sd).player.inventory.money -= amount;
    }

    (*fl).data.id = match amount {
        1          => 0u32,
        2..=99     => 1u32,
        100..=999  => 2u32,
        _          => 3u32,
    };
    (*fl).data.amount = amount as i32;

    (*sd).fakeDrop = 0;

    sl_doscript_2("on_drop_gold", None, (*sd).id, (*fl).id);

    for x in 0..MAX_MAGIC_TIMERS {
        if (&(*sd).player.spells.dura_aether)[x].id > 0
            && (&(*sd).player.spells.dura_aether)[x].duration > 0
        {
            sl_doscript_2(crate::game::scripting::carray_to_str(&magic_db::search((&(*sd).player.spells.dura_aether)[x].id as i32).yname), Some("on_drop_gold_while_cast"), (*sd).id, (*fl).id);
        }
    }

    for x in 0..MAX_MAGIC_TIMERS {
        if (&(*sd).player.spells.dura_aether)[x].id > 0
            && (&(*sd).player.spells.dura_aether)[x].aether > 0
        {
            sl_doscript_2(crate::game::scripting::carray_to_str(&magic_db::search((&(*sd).player.spells.dura_aether)[x].id as i32).yname), Some("on_drop_gold_while_aether"), (*sd).id, (*fl).id);
        }
    }

    if (*sd).fakeDrop != 0 { return 0; }

    let mut mini = [0i8; 64];
    libc::snprintf(
        mini.as_mut_ptr(), 64,
        b"You dropped %d coins\0".as_ptr().cast(),
        (*fl).data.amount,
    );
    clif_sendminitext(sd, mini.as_ptr());

    let mut def = [0i32; 1];
    if let Some(grid) = block_grid::get_grid((*sd).m as usize) {
        let cell_ids = grid.ids_at_tile((*sd).x, (*sd).y);
        for id in cell_ids {
            if let Some(fl_arc) = crate::game::map_server::map_id2fl_ref(id) { let fl = &mut *fl_arc.write();
                clif_addtocurrent_inner(fl.bl_ptr_mut(), def.as_mut_ptr(), amount, std::ptr::null_mut());
            }
        }
    }

    if def[0] == 0 {
        let fl_raw = Box::into_raw(fl);
        map_additem((*fl_raw).bl_ptr_mut());

        sl_doscript_2("after_drop_gold", None, (*sd).id, (*fl_raw).id);

        for x in 0..MAX_MAGIC_TIMERS {
            if (&(*sd).player.spells.dura_aether)[x].id > 0
                && (&(*sd).player.spells.dura_aether)[x].duration > 0
            {
                sl_doscript_2(crate::game::scripting::carray_to_str(&magic_db::search((&(*sd).player.spells.dura_aether)[x].id as i32).yname), Some("after_drop_gold_while_cast"), (*sd).id, (*fl_raw).id);
            }
        }

        for x in 0..MAX_MAGIC_TIMERS {
            if (&(*sd).player.spells.dura_aether)[x].id > 0
                && (&(*sd).player.spells.dura_aether)[x].aether > 0
            {
                sl_doscript_2(crate::game::scripting::carray_to_str(&magic_db::search((&(*sd).player.spells.dura_aether)[x].id as i32).yname), Some("after_drop_gold_while_aether"), (*sd).id, (*fl_raw).id);
            }
        }

        sl_doscript_2("characterLog", Some("dropWrite"), (*sd).id, (*fl_raw).id);

        let fl_bl = (*fl_raw).bl_ptr();
        if let Some(grid) = block_grid::get_grid((*sd).m as usize) {
            let slot = &*raw_map_ptr().add((*sd).m as usize);
            let ids = block_grid::ids_in_area(grid, (*sd).x as i32, (*sd).y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
            for id in ids {
                if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_object_look_sub2_inner(pc_arc.read().bl_ptr(), LOOK_SEND, fl_bl);
                }
            }
        }
    } else {
        drop(fl);
    }

    clif_sendstatus(sd, SFLAG_XPMONEY);

    0
}

// ─── clif_open_sub ────────────────────────────────────────────────────────────

/// Trigger the onOpen script hook.
///
pub unsafe fn clif_open_sub(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }
    sl_doscript_simple("onOpen", None, (*sd).id);
    0
}

// ─── clif_removespell ─────────────────────────────────────────────────────────

/// Send a remove-spell packet to the client.
///
pub unsafe fn clif_removespell(sd: *mut MapSessionData, pos: i32) -> i32 {
    if !session_exists((*sd).fd) {
        return 0;
    }

    let fd = (*sd).fd;
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
pub unsafe fn clif_parsechangespell(sd: *mut MapSessionData) -> i32 {
    let start_pos = rfifob((*sd).fd, 6) as usize - 1;
    let stop_pos  = rfifob((*sd).fd, 7) as usize - 1;

    let start_id = (&(*sd).player.spells.skills)[start_pos];
    let stop_id  = (&(*sd).player.spells.skills)[stop_pos];

    clif_removespell(sd, start_pos as i32);
    clif_removespell(sd, stop_pos as i32);

    (&mut (*sd).player.spells.skills)[start_pos] = stop_id;
    (&mut (*sd).player.spells.skills)[stop_pos]  = start_id;

    pc_loadmagic(sd);
    pc_reload_aether(sd);

    0
}

// ─── clif_throwitem_sub ───────────────────────────────────────────────────────

/// Execute a throw: fire the onThrow script with source/destination floor items.
///
/// Note: this is NOT a foreachinarea callback; it is called directly.
pub unsafe fn clif_throwitem_sub(
    sd: *mut MapSessionData,
    id: i32,
    _type: i32,
    x: i32,
    y: i32,
) -> i32 {
    if (&(*sd).player.inventory.inventory)[id as usize].id == 0 { return 0; }

    if (&(*sd).player.inventory.inventory)[id as usize].amount <= 0 {
        clif_senddelitem(sd, id, 4);
        return 0;
    }

    let mut fl = Box::new(unsafe { std::mem::zeroed::<FloorItemData>() });
    (*fl).m = (*sd).m;
    (*fl).x = x as u16;
    (*fl).y = y as u16;

    // memcpy(&fl->data, &sd->status.inventory[id], sizeof(struct item))
    std::ptr::copy_nonoverlapping(
        &(&(*sd).player.inventory.inventory)[id as usize] as *const _ as *const u8,
        &raw mut (*fl).data as *mut u8,
        std::mem::size_of::<crate::common::types::Item>(),
    );

    (*sd).invslot = id as u8;
    (*sd).throwx = x as u16;
    (*sd).throwy = y as u16;

    sl_doscript_2("onThrow", None, (*sd).id, (*fl).id);

    // fl is dropped here — it was a temporary used only to pass data to the script.
    drop(fl);
    0
}

// ─── clif_throwitem_script ────────────────────────────────────────────────────

/// Complete a throw action after script approval.
///
pub unsafe fn clif_throwitem_script(sd: *mut MapSessionData) -> i32 {
    let id   = (*sd).invslot as usize;
    let x    = (*sd).throwx as i32;
    let y    = (*sd).throwy as i32;
    let item_type = 0i32;

    let mut fl = Box::new(unsafe { std::mem::zeroed::<FloorItemData>() });
    (*fl).m = (*sd).m;
    (*fl).x = x as u16;
    (*fl).y = y as u16;

    std::ptr::copy_nonoverlapping(
        &(&(*sd).player.inventory.inventory)[id] as *const _ as *const u8,
        &raw mut (*fl).data as *mut u8,
        std::mem::size_of::<crate::common::types::Item>(),
    );

    let mut def = [0i32; 1];

    if (*fl).data.dura == item_db::search((*fl).data.id).dura {
        if let Some(grid) = block_grid::get_grid((*sd).m as usize) {
            let cell_ids = grid.ids_at_tile(x as u16, y as u16);
            for cid in cell_ids {
                if let Some(fl_ref_arc) = crate::game::map_server::map_id2fl_ref(cid) { let fl_ref = &mut *fl_ref_arc.write();
                    pc_addtocurrent_inner(fl_ref.bl_ptr_mut(), def.as_mut_ptr(), id as i32, item_type, sd);
                }
            }
        }
    }

    (&mut (*sd).player.inventory.inventory)[id].amount -= 1;

    if item_type != 0 || (&(*sd).player.inventory.inventory)[id].amount == 0 {
        (&mut (*sd).player.inventory.inventory)[id] = crate::common::types::Item {
            id: 0, owner: 0, custom: 0, time: 0, dura: 0, amount: 0,
            pos: 0, _pad0: [0; 3], custom_look: 0, custom_icon: 0,
            custom_look_color: 0, custom_icon_color: 0, protected: 0,
            traps_table: [0; 100], buytext: [0; 64], note: [0; 300],
            repair: 0, real_name: [0; 64], _pad1: [0; 3],
        };
        clif_senddelitem(sd, id as i32, 4);
    } else {
        (*fl).data.amount = 1;
        clif_sendadditem(sd, id as i32);
    }

    if (*sd).x as i32 != x {
        let mut sndbuf = [0u8; 48];
        sndbuf[0] = 0xAA;
        let len_be = 0x1Bu16.to_be_bytes();
        sndbuf[1] = len_be[0];
        sndbuf[2] = len_be[1];
        sndbuf[3] = 0x16;
        sndbuf[4] = 0x03;
        let id_be = ((*sd).id).to_be_bytes();
        sndbuf[5] = id_be[0]; sndbuf[6] = id_be[1];
        sndbuf[7] = id_be[2]; sndbuf[8] = id_be[3];

        if (*fl).data.custom_icon != 0 {
            let icon_be = (((*fl).data.custom_icon + 49152) as u16).to_be_bytes();
            sndbuf[9]  = icon_be[0];
            sndbuf[10] = icon_be[1];
            sndbuf[11] = (*fl).data.custom_icon_color as u8;
        } else {
            let fl_item = item_db::search((*fl).data.id);
            let icon_be = (fl_item.icon as u16).to_be_bytes();
            sndbuf[9]  = icon_be[0];
            sndbuf[10] = icon_be[1];
            sndbuf[11] = fl_item.icon_color;
        }

        let fl_id_be = if def[0] != 0 {
            (def[0] as u32).to_be_bytes()
        } else {
            ((*fl).id).to_be_bytes()
        };
        sndbuf[12] = fl_id_be[0]; sndbuf[13] = fl_id_be[1];
        sndbuf[14] = fl_id_be[2]; sndbuf[15] = fl_id_be[3];

        let sx_be = (*sd).x.to_be_bytes();
        sndbuf[16] = sx_be[0]; sndbuf[17] = sx_be[1];
        let sy_be = (*sd).y.to_be_bytes();
        sndbuf[18] = sy_be[0]; sndbuf[19] = sy_be[1];
        let dx_be = (x as u16).to_be_bytes();
        sndbuf[20] = dx_be[0]; sndbuf[21] = dx_be[1];
        let dy_be = (y as u16).to_be_bytes();
        sndbuf[22] = dy_be[0]; sndbuf[23] = dy_be[1];
        // bytes 24..27 already 0
        sndbuf[28] = 0x02;
        sndbuf[29] = 0x00;

        clif_send(sndbuf.as_ptr(), 48, (*sd).bl_ptr_mut(), SAMEAREA);
    } else {
        clif_sendaction((*sd).as_bl_mut(), 2, 30, 0);
    }

    if def[0] == 0 {
        let fl_raw = Box::into_raw(fl);
        map_additem((*fl_raw).bl_ptr_mut());
        let fl_bl = (*fl_raw).bl_ptr();
        if let Some(grid) = block_grid::get_grid((*sd).m as usize) {
            let slot = &*raw_map_ptr().add((*sd).m as usize);
            let ids = block_grid::ids_in_area(grid, (*sd).x as i32, (*sd).y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
            for id in ids {
                if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_object_look_sub2_inner(pc_arc.read().bl_ptr(), LOOK_SEND, fl_bl);
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
pub unsafe fn clif_throw_check_inner(bl: *mut BlockList, found: *mut i32) -> i32 {
    if bl.is_null() { return 0; }

    if !found.is_null() && *found != 0 { return 0; }

    if (*bl).bl_type == BL_NPC as u8 {
        if (*bl).subtype != SCRIPT as u8 { return 0; }
    }
    if (*bl).bl_type == BL_MOB as u8 {
        let mob = bl as *mut MobSpawnData;
        if (*mob).state == MOB_DEAD { return 0; }
    }
    if (*bl).bl_type == BL_PC as u8 {
        let tsd = bl as *mut MapSessionData;
        if (*tsd).player.combat.state == 1 || ((*tsd).optFlags & OPT_FLAG_STEALTH as u64) != 0 {
            return 0;
        }
    }

    if !found.is_null() { *found += 1; }

    0
}

// ─── clif_throwconfirm ────────────────────────────────────────────────────────

/// Send a throw-confirm packet to the client.
///
pub unsafe fn clif_throwconfirm(sd: *mut MapSessionData) -> i32 {
    let fd = (*sd).fd;
    wfifob(fd, 0, 0xAA);
    wfifow(fd, 1, 7u16.swap_bytes());
    wfifob(fd, 3, 0x4E);
    wfifob(fd, 5, rfifob((*sd).fd, 6));
    wfifob(fd, 6, 0);
    wfifoset(fd, encrypt(fd) as usize);

    0
}

// ─── clif_parsethrow ──────────────────────────────────────────────────────────

/// Handle a throw-item packet from the client.
///
pub unsafe fn clif_parsethrow(sd: *mut MapSessionData) -> i32 {
    let reg_str = b"goldbardupe\0";
    let dupe_times = pc_readglobalreg(sd, reg_str.as_ptr().cast());
    if dupe_times != 0 {
        return 0;
    }

    if (*sd).player.identity.gm_level == 0 {
        if (*sd).player.combat.state == 1 {
            clif_sendminitext(sd, b"Spirits can't do that.\0".as_ptr().cast());
            return 0;
        }
        if (*sd).player.combat.state == 3 {
            clif_sendminitext(sd, b"You cannot do that while riding a mount.\0".as_ptr().cast());
            return 0;
        }
        if (*sd).player.combat.state == 4 {
            clif_sendminitext(sd, b"You cannot do that while transformed.\0".as_ptr().cast());
            return 0;
        }
    }

    let pos = rfifob((*sd).fd, 6) as usize - 1;
    if item_db::search((&(*sd).player.inventory.inventory)[pos].id).droppable != 0 {
        clif_sendminitext(sd, b"You can't throw this item.\0".as_ptr().cast());
        return 0;
    }

    let max = 8i32;
    let mut newx: i32 = (*sd).x as i32;
    let mut newy: i32 = (*sd).y as i32;
    let mut xmod: i32 = 0;
    let mut ymod: i32 = 0;
    let mut found = [0i32; 1];

    match (*sd).player.combat.side {
        0 => { ymod = -1; } // up
        1 => { xmod = 1; }  // left
        2 => { ymod = 1; }  // down
        3 => { xmod = -1; } // right
        _ => {}
    }

    let m = (*sd).m as i32;
    let map_data = &*raw_map_ptr().add(m as usize);

    'search: for i in 0..max {
        let mut x1: i32 = (*sd).x as i32 + (i * xmod) + xmod;
        let mut y1: i32 = (*sd).y as i32 + (i * ymod) + ymod;
        if x1 < 0 { x1 = 0; }
        if y1 < 0 { y1 = 0; }
        if x1 >= map_data.xs as i32 { x1 = map_data.xs as i32 - 1; }
        if y1 >= map_data.ys as i32 { y1 = map_data.ys as i32 - 1; }

        if let Some(grid) = block_grid::get_grid(m as usize) {
            let cell_ids = grid.ids_at_tile(x1 as u16, y1 as u16);
            for cid in cell_ids {
                let bl = crate::game::map_server::map_id2bl_ref(cid);
                if !bl.is_null() {
                    let ty = (*bl).bl_type as i32;
                    if ty == BL_NPC || ty == BL_PC || ty == BL_MOB {
                        clif_throw_check_inner(bl, found.as_mut_ptr());
                    }
                }
            }
        }
        // read_pass(m, x, y) — accesses map[m].pass[x + y*xs]
        let pass_val = if raw_map_ptr().is_null() { 0 } else {
            let md = &*raw_map_ptr().add(m as usize);
            if md.pass.is_null() { 0 } else { *md.pass.add(x1 as usize + y1 as usize * md.xs as usize) as i32 }
        };
        found[0] += pass_val;
        found[0] += clif_object_canmove(m, x1, y1, (*sd).player.combat.side as i32);
        found[0] += clif_object_canmove_from(m, x1, y1, (*sd).player.combat.side as i32);

        // Check warp list at this block cell
        if !map_data.warp.is_null() {
            let bidx = x1 as usize / BLOCK_SIZE + (y1 as usize / BLOCK_SIZE) * map_data.bxs as usize;
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

    clif_throwitem_sub(sd, pos as i32, 0, newx, newy)
}
