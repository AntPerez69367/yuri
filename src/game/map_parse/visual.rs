//! Covers object spawn packets, look callbacks, and the area-broadcast
//! helpers that send appearance data to players.

#![allow(non_snake_case, clippy::wildcard_imports)]


use crate::database::map_db::BlockList;
use crate::game::mob::{MobSpawnData, MOB_DEAD};
use crate::game::npc::NpcData;
use crate::game::pc::{
    MapSessionData,
    BL_PC, BL_MOB, BL_NPC, BL_ITEM,
    EQ_ARMOR, EQ_COAT, EQ_WEAP, EQ_SHIELD, EQ_HELM,
    EQ_FACEACC, EQ_CROWN, EQ_FACEACCTWO, EQ_MANTLE, EQ_NECKLACE, EQ_BOOTS,
    FLAG_HELM, FLAG_NECKLACE,
    OPT_FLAG_STEALTH,
};
use crate::game::scripting::types::floor::FloorItemData;
use super::packet::{
    encrypt, wfifob, wfifohead, wfifol, wfifop, wfifoset, wfifow, wfifoheader,
    AREA_WOS,
};
use crate::session::session_exists;

// ─── Constants ────────────────────────────────────────────────────────────────

const LOOK_GET:  i32 = 0;
const LOOK_SEND: i32 = 1;

/// `ITM_TRAPS` item type constant (from item_db).
const ITM_TRAPS: i32 = 4;

/// `bl_type` field is `u8`; BL_* constants from pc.rs are `i32`.
/// These local aliases allow direct comparison without casts at every use site.
const BL_PC_U8:   u8 = BL_PC   as u8;
const BL_MOB_U8:  u8 = BL_MOB  as u8;
const BL_NPC_U8:  u8 = BL_NPC  as u8;
const BL_ITEM_U8: u8 = BL_ITEM as u8;


use crate::game::client::clif_send;
use crate::game::block::map_addblock;
use crate::game::map_parse::groups::clif_isingroup;
use crate::game::map_parse::movement::clif_sendchararea;
use crate::database::item_db;
use crate::game::pc::pc_isequip;

#[inline]
fn map_id2bl(id: u32) -> *mut BlockList {
    crate::game::map_server::map_id2bl_ref(id)
}

use crate::game::map_parse::combat::clif_sendanimations;

// ─── clif_lookgone ────────────────────────────────────────────────────────────

/// Send an object-despawn packet to all nearby clients.
///
pub unsafe fn clif_lookgone(bl: *mut BlockList) -> i32 {
    let mut buf = [0u8; 16];

    let bl_ref = &*bl;
    let is_char_type = bl_ref.bl_type == BL_PC_U8
        || (bl_ref.bl_type == BL_NPC_U8 && (*(bl as *const NpcData)).npctype == 1)
        || bl_ref.bl_type == BL_MOB_U8;

    if is_char_type {
        buf[0] = 0xAA;
        let size = 6u16.to_be_bytes();
        buf[1] = size[0];
        buf[2] = size[1];
        buf[3] = 0x0E;
        buf[4] = 0x03;
        let id_bytes = bl_ref.id.to_be_bytes();
        buf[5] = id_bytes[0];
        buf[6] = id_bytes[1];
        buf[7] = id_bytes[2];
        buf[8] = id_bytes[3];
    } else {
        buf[0] = 0xAA;
        let size = 6u16.to_be_bytes();
        buf[1] = size[0];
        buf[2] = size[1];
        buf[3] = 0x5F;
        buf[4] = 0x03;
        let id_bytes = bl_ref.id.to_be_bytes();
        buf[5] = id_bytes[0];
        buf[6] = id_bytes[1];
        buf[7] = id_bytes[2];
        buf[8] = id_bytes[3];
    }

    clif_send(buf.as_ptr(), 16, bl, AREA_WOS);
    0
}

// ─── clif_mob_look_start_func ─────────────────────────────────────────────────

/// Initialise the mob-look accumulation fields on a player session.
///
///
/// Called with `BL_PC` type so `bl` is a `MapSessionData`.
/// Mirrors `clif_mob_look_start_func` (~line 1426).
pub unsafe fn clif_mob_look_start_func_inner(bl: *mut BlockList) -> i32 {
    let sd = bl as *mut MapSessionData;
    if sd.is_null() { return 0; }

    (*sd).mob_len   = 0;
    (*sd).mob_count = 0;
    (*sd).mob_item  = 0;

    if !session_exists((*sd).fd) {
        return 0;
    }

    wfifohead((*sd).fd, 65535);
    0
}

// ─── clif_mob_look_close_func ─────────────────────────────────────────────────

///
/// Flush the accumulated mob-look packet buffer to the client.
/// Mirrors `clif_mob_look_close_func` (~line 1446).
pub unsafe fn clif_mob_look_close_func_inner(bl: *mut BlockList) -> i32 {
    let sd = bl as *mut MapSessionData;
    if sd.is_null() { return 0; }

    if (*sd).mob_count == 0 { return 0; }

    if (*sd).mob_item == 0 {
        wfifob((*sd).fd, ((*sd).mob_len + 7) as usize, 0);
        (*sd).mob_len += 1;
    }

    wfifoheader((*sd).fd, 0x07, ((*sd).mob_len + 4) as u16);
    wfifow((*sd).fd, 5, ((*sd).mob_count as u16).swap_bytes());
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);

    (*sd).mob_len   = 0;
    (*sd).mob_count = 0;
    0
}

// ─── clif_object_look_sub ────────────────────────────────────────────────────

///
/// Write one object entry into the batched mob-look packet buffer.
///
/// Args:
///   - `bl`:        the block-list entry being iterated
///   - `look_type`: `LOOK_GET` or `LOOK_SEND`
///   - `arg`:       if `LOOK_SEND`, `bl` is the receiving player and `arg` is the object;
///                  if `LOOK_GET`, `bl` is the object and `arg` is cast to `*mut MapSessionData`.
///
/// Mirrors `clif_object_look_sub` (~line 1472).
pub unsafe fn clif_object_look_sub_inner(bl: *mut BlockList, look_type: i32, arg: *mut BlockList) -> i32 {
    let (sd, b): (*mut MapSessionData, *mut BlockList) = if look_type == LOOK_SEND {
        // bl is the receiving player, arg is the object
        if bl.is_null() { return 0; }
        if arg.is_null() { return 0; }
        (bl as *mut MapSessionData, arg)
    } else {
        // bl is the object, arg is the receiving player
        if bl.is_null() { return 0; }
        if arg.is_null() { return 0; }
        (arg as *mut MapSessionData, bl)
    };

    if (*b).bl_type == BL_PC_U8 { return 0; }

    let len = (*sd).mob_len as usize;

    wfifow((*sd).fd, len + 7,  ((*b).x as u16).swap_bytes());
    wfifow((*sd).fd, len + 9,  ((*b).y as u16).swap_bytes());
    wfifol((*sd).fd, len + 12, (*b).id.swap_bytes());

    match (*b).bl_type {
        t if t == BL_MOB_U8 => {
            let mob = b as *mut MobSpawnData;
            if (*mob).state == MOB_DEAD || (*(*mob).data).mobtype == 1 { return 0; }

            let nlen;
            if (*(*mob).data).isnpc == 0 {
                wfifob((*sd).fd, len + 11, 0x05);
                wfifow((*sd).fd, len + 16, (32768u16.wrapping_add((*mob).look)).swap_bytes());
                wfifob((*sd).fd, len + 18, (*mob).look_color);
                wfifob((*sd).fd, len + 19, (*mob).side as u8);
                wfifob((*sd).fd, len + 20, 0);
                wfifob((*sd).fd, len + 21, 0); // # of animations active

                let mut animlen: i32 = 0;
                let mut n: usize = 0;
                for x in 0..50usize {
                    if (*mob).da[x].duration != 0 && (*mob).da[x].animation != 0 {
                        wfifow((*sd).fd, n + len + 22,     ((*mob).da[x].animation as u16).swap_bytes());
                        wfifow((*sd).fd, n + len + 22 + 2, (((*mob).da[x].duration / 1000) as u16).swap_bytes());
                        animlen += 1;
                        n += 4;
                    }
                }
                nlen = n;

                wfifob((*sd).fd, len + 21, animlen as u8);
                wfifob((*sd).fd, len + 22 + nlen, 0); // pass flag
                (*sd).mob_len += 15 + nlen as i32;
            } else if (*(*mob).data).isnpc == 1 {
                wfifob((*sd).fd, len + 11, 12);
                wfifow((*sd).fd, len + 16, (32768u16.wrapping_add((*mob).look)).swap_bytes());
                wfifob((*sd).fd, len + 18, (*mob).look_color);
                wfifob((*sd).fd, len + 19, (*mob).side as u8);
                wfifow((*sd).fd, len + 20, 0);
                wfifob((*sd).fd, len + 22, 0);
                (*sd).mob_len += 15;
            }
        }
        t if t == BL_NPC_U8 => {
            let nd = b as *mut NpcData;
            if (*b).subtype != 0 || (*nd).bl.subtype != 0 || (*nd).npctype == 1 { return 0; }

            wfifob((*sd).fd, len + 11, 12);
            wfifow((*sd).fd, len + 16, (32768u16.wrapping_add((*b).graphic_id as u16)).swap_bytes());
            wfifob((*sd).fd, len + 18, (*b).graphic_color as u8);
            wfifob((*sd).fd, len + 19, (*nd).side as u8);
            wfifow((*sd).fd, len + 20, 0);
            wfifob((*sd).fd, len + 22, 0);
            (*sd).mob_len += 15;
        }
        t if t == BL_ITEM_U8 => {
            let item = b as *mut FloorItemData;

            let mut in_table = false;
            for &spotter in (*item).data.traps_table.iter() {
                if spotter == (*sd).status.id { in_table = true; break; }
            }

            let item_entry = item_db::search((*item).data.id);

            if item_entry.typ as i32 == ITM_TRAPS && !in_table {
                return 0;
            }

            wfifob((*sd).fd, len + 11, 0x02);

            if (*item).data.custom_icon != 0 {
                wfifow((*sd).fd, len + 16, (((*item).data.custom_icon as u16).wrapping_add(49152)).swap_bytes());
                wfifob((*sd).fd, len + 18, (*item).data.custom_icon_color as u8);
            } else {
                wfifow((*sd).fd, len + 16, (item_entry.icon as u16).swap_bytes());
                wfifob((*sd).fd, len + 18, item_entry.icon_color as u8);
            }

            wfifob((*sd).fd, len + 19, 0);
            wfifow((*sd).fd, len + 20, 0);
            wfifob((*sd).fd, len + 22, 0);
            (*sd).mob_len += 15;
            (*sd).mob_item = 1;
        }
        _ => {}
    }

    (*sd).mob_count += 1;
    0
}

// ─── clif_object_look_sub2 ───────────────────────────────────────────────────

///
/// Send a single-object look packet immediately (not batched).
/// Same argument layout as `clif_object_look_sub_inner`.
/// Mirrors `clif_object_look_sub2` (~line 1592).
pub unsafe fn clif_object_look_sub2_inner(bl: *mut BlockList, look_type: i32, arg: *mut BlockList) -> i32 {
    let (sd, b): (*mut MapSessionData, *mut BlockList) = if look_type == LOOK_SEND {
        if bl.is_null() { return 0; }
        if arg.is_null() { return 0; }
        (bl as *mut MapSessionData, arg)
    } else {
        if bl.is_null() { return 0; }
        if arg.is_null() { return 0; }
        (arg as *mut MapSessionData, bl)
    };

    if !session_exists((*sd).fd) {
        return 0;
    }

    wfifohead((*sd).fd, 6000);

    if (*b).bl_type == BL_PC_U8 { return 0; }

    wfifob((*sd).fd, 0, 0xAA);
    wfifow((*sd).fd, 1, 20u16.swap_bytes());
    wfifob((*sd).fd, 3, 0x07);
    wfifow((*sd).fd, 5, 1u16.swap_bytes());
    wfifow((*sd).fd, 7, ((*b).x as u16).swap_bytes());
    wfifow((*sd).fd, 9, ((*b).y as u16).swap_bytes());
    wfifol((*sd).fd, 12, (*b).id.swap_bytes());

    let mut nlen: usize = 0;

    match (*b).bl_type {
        t if t == BL_MOB_U8 => {
            let mob = b as *mut MobSpawnData;
            if (*mob).state == MOB_DEAD || (*(*mob).data).mobtype == 1 { return 0; }

            if (*(*mob).data).isnpc == 0 {
                wfifob((*sd).fd, 11, 0x05);
                wfifow((*sd).fd, 16, (32768u16.wrapping_add((*mob).look)).swap_bytes());
                wfifob((*sd).fd, 18, (*mob).look_color);
                wfifob((*sd).fd, 19, (*mob).side as u8);
                wfifob((*sd).fd, 20, 0);
                wfifob((*sd).fd, 21, 0);

                for x in 0..50usize {
                    if (*mob).da[x].duration != 0 && (*mob).da[x].animation != 0 {
                        wfifow((*sd).fd, nlen + 22,     ((*mob).da[x].animation as u16).swap_bytes());
                        wfifow((*sd).fd, nlen + 22 + 2, (((*mob).da[x].duration / 1000) as u16).swap_bytes());
                        nlen += 4;
                    }
                }

                wfifob((*sd).fd, 21, (nlen / 4) as u8);
                wfifob((*sd).fd, nlen + 22, 0); // passflag
            } else if (*(*mob).data).isnpc == 1 {
                // NOTE: C uses `len` (always 0 here) — kept for fidelity
                wfifob((*sd).fd, 11, 12);
                wfifow((*sd).fd, 16, (32768u16.wrapping_add((*mob).look)).swap_bytes());
                wfifob((*sd).fd, 18, (*mob).look_color);
                wfifob((*sd).fd, 19, (*mob).side as u8);
                wfifow((*sd).fd, 20, 0);
                wfifob((*sd).fd, 22, 0);
            }
        }
        t if t == BL_NPC_U8 => {
            let nd = b as *mut NpcData;
            if (*b).subtype != 0 || (*nd).bl.subtype != 0 || (*nd).npctype == 1 { return 0; }

            wfifob((*sd).fd, 11, 12);
            wfifow((*sd).fd, 16, (32768u16.wrapping_add((*b).graphic_id as u16)).swap_bytes());
            wfifob((*sd).fd, 18, (*b).graphic_color as u8);
            wfifob((*sd).fd, 19, (*nd).side as u8);
            wfifow((*sd).fd, 20, 0);
            wfifob((*sd).fd, 22, 0);
        }
        t if t == BL_ITEM_U8 => {
            let item = b as *mut FloorItemData;

            let mut in_table = false;
            for &spotter in (*item).data.traps_table.iter() {
                if spotter == (*sd).status.id { in_table = true; break; }
            }

            let item_entry = item_db::search((*item).data.id);

            if item_entry.typ as i32 == ITM_TRAPS && !in_table {
                return 0;
            }

            wfifob((*sd).fd, 11, 0x02);

            if (*item).data.custom_icon != 0 {
                wfifow((*sd).fd, 16, (((*item).data.custom_icon as u16).wrapping_add(49152)).swap_bytes());
                wfifob((*sd).fd, 18, (*item).data.custom_icon_color as u8);
            } else {
                wfifow((*sd).fd, 16, (item_entry.icon as u16).swap_bytes());
                wfifob((*sd).fd, 18, item_entry.icon_color as u8);
            }

            wfifob((*sd).fd, 19, 0);
            wfifow((*sd).fd, 20, 0);
            wfifob((*sd).fd, 22, 0);
        }
        _ => {}
    }

    wfifow((*sd).fd, 1, (20u16.wrapping_add(nlen as u16)).swap_bytes());
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_object_look_specific ───────────────────────────────────────────────

/// Send a single-object look packet for a specific block-list ID.
///
/// Mirrors `clif_object_look_specific` (~line 1716).
pub unsafe fn clif_object_look_specific(sd: *mut MapSessionData, id: u32) -> i32 {
    if sd.is_null() { return 0; }

    let b = map_id2bl(id);
    if b.is_null() { return 0; }

    if (*b).bl_type == BL_PC_U8 { return 0; }

    wfifoheader((*sd).fd, 0x07, 20);
    wfifow((*sd).fd, 5, 1u16.swap_bytes());
    wfifow((*sd).fd, 7, ((*b).x as u16).swap_bytes());
    wfifow((*sd).fd, 9, ((*b).y as u16).swap_bytes());
    wfifol((*sd).fd, 12, (*b).id.swap_bytes());

    match (*b).bl_type {
        t if t == BL_MOB_U8 => {
            let mob = b as *mut MobSpawnData;
            if (*mob).state == MOB_DEAD || (*(*mob).data).mobtype == 1 { return 0; }

            if (*(*mob).data).isnpc == 0 {
                wfifob((*sd).fd, 11, 0x05);
                wfifow((*sd).fd, 16, (32768u16.wrapping_add((*mob).look)).swap_bytes());
                wfifob((*sd).fd, 18, (*mob).look_color);
                wfifob((*sd).fd, 19, (*mob).side as u8);
                wfifow((*sd).fd, 20, 0);
                wfifob((*sd).fd, 22, 0);
            } else if (*(*mob).data).isnpc == 1 {
                wfifob((*sd).fd, 11, 12);
                wfifow((*sd).fd, 16, (32768u16.wrapping_add((*mob).look)).swap_bytes());
                wfifob((*sd).fd, 18, (*mob).look_color);
                wfifob((*sd).fd, 19, (*mob).side as u8);
                wfifow((*sd).fd, 20, 0);
                wfifob((*sd).fd, 22, 0);
                (*sd).mob_len += 15;
            }
        }
        t if t == BL_NPC_U8 => {
            let nd = b as *mut NpcData;
            if (*b).subtype != 0 || (*nd).bl.subtype != 0 || (*nd).npctype == 1 { return 0; }

            wfifob((*sd).fd, 11, 12);
            wfifow((*sd).fd, 16, (32768u16.wrapping_add((*b).graphic_id as u16)).swap_bytes());
            wfifob((*sd).fd, 18, (*b).graphic_color as u8);
            wfifob((*sd).fd, 19, 2); // looking down
            wfifow((*sd).fd, 20, 0);
            wfifob((*sd).fd, 22, 0);
        }
        t if t == BL_ITEM_U8 => {
            let item = b as *mut FloorItemData;

            let mut in_table = false;
            for &spotter in (*item).data.traps_table.iter() {
                if spotter == (*sd).status.id { in_table = true; break; }
            }

            let item_entry = item_db::search((*item).data.id);

            if item_entry.typ as i32 == ITM_TRAPS && !in_table {
                return 0;
            }

            wfifob((*sd).fd, 11, 0x02);

            if (*item).data.custom_icon != 0 {
                wfifow((*sd).fd, 16, (((*item).data.custom_icon as u16).wrapping_add(49152)).swap_bytes());
                wfifob((*sd).fd, 18, (*item).data.custom_icon_color as u8);
            } else {
                wfifow((*sd).fd, 16, (item_entry.icon as u16).swap_bytes());
                wfifob((*sd).fd, 18, item_entry.icon_color as u8);
            }

            wfifob((*sd).fd, 19, 0);
            wfifow((*sd).fd, 20, 0);
            wfifob((*sd).fd, 22, 0);
            wfifob((*sd).fd, 2, 0x13);
            wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
            return 0;
        }
        _ => {}
    }

    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_mob_look_start ─────────────────────────────────────────────────────

/// Initialise mob-look accumulation state and reserve send-buffer space.
///
/// Direct call (not callback). Mirrors `clif_mob_look_start` (~line 1813).
pub unsafe fn clif_mob_look_start(sd: *mut MapSessionData) -> i32 {
    (*sd).mob_count = 0;
    (*sd).mob_len   = 0;
    (*sd).mob_item  = 0;

    if !session_exists((*sd).fd) {
        return 0;
    }

    wfifohead((*sd).fd, 65535);
    0
}

// ─── clif_mob_look_close ─────────────────────────────────────────────────────

/// Flush the batched mob-look packet if any entries were accumulated.
///
/// Direct call (not callback). Mirrors `clif_mob_look_close` (~line 1832).
pub unsafe fn clif_mob_look_close(sd: *mut MapSessionData) -> i32 {
    if (*sd).mob_count == 0 { return 0; }

    if (*sd).mob_item == 0 {
        wfifob((*sd).fd, ((*sd).mob_len + 7) as usize, 0);
        (*sd).mob_len += 1;
    }

    wfifoheader((*sd).fd, 0x07, ((*sd).mob_len + 4) as u16);
    wfifow((*sd).fd, 5, ((*sd).mob_count as u16).swap_bytes());
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_cnpclook_sub ───────────────────────────────────────────────────────

///
/// Send full NPC (charstate NPC) appearance packet to a player.
///
/// Args:
///   - `bl`:        the block-list entry being iterated
///   - `look_type`: `LOOK_GET` or `LOOK_SEND`
///   - `arg`:       if `LOOK_GET`, `bl` is the NPC and `arg` is cast to `*mut MapSessionData`;
///                  if `LOOK_SEND`, `bl` is the player and `arg` is cast to `*mut NpcData`.
///
/// Mirrors `clif_cnpclook_sub` (~line 2773).
pub unsafe fn clif_cnpclook_inner(bl: *mut BlockList, look_type: i32, arg: *mut BlockList) -> i32 {
    let (nd, sd): (*mut NpcData, *mut MapSessionData) = if look_type == LOOK_GET {
        if bl.is_null() { return 0; }
        if arg.is_null() { return 0; }
        (bl as *mut NpcData, arg as *mut MapSessionData)
    } else {
        // LOOK_SEND
        if bl.is_null() { return 0; }
        if arg.is_null() { return 0; }
        (arg as *mut NpcData, bl as *mut MapSessionData)
    };

    if (*nd).bl.m != (*sd).bl.m || (*nd).npctype != 1 {
        return 0;
    }

    if !session_exists((*sd).fd) {
        return 0;
    }

    wfifohead((*sd).fd, 512);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 3, 0x33);
    wfifow((*sd).fd, 5, ((*nd).bl.x as u16).swap_bytes());
    wfifow((*sd).fd, 7, ((*nd).bl.y as u16).swap_bytes());
    wfifob((*sd).fd, 9, (*nd).side as u8);
    wfifol((*sd).fd, 10, (*nd).bl.id.swap_bytes());

    if ((*nd).state as u8) < 4 {
        wfifow((*sd).fd, 14, (*nd).sex.swap_bytes());
    } else {
        wfifob((*sd).fd, 14, 1);
        wfifob((*sd).fd, 15, 15);
    }

    if (*nd).state == 2 && (*sd).status.gm_level != 0 {
        wfifob((*sd).fd, 16, 5);
    } else {
        wfifob((*sd).fd, 16, (*nd).state as u8);
    }

    wfifob((*sd).fd, 19, 80);

    if (*nd).state == 3 {
        wfifow((*sd).fd, 17, ((*nd).bl.graphic_id as u16).swap_bytes());
    } else if (*nd).state == 4 {
        wfifow((*sd).fd, 17, ((*nd).bl.graphic_id as u16).wrapping_add(32768).swap_bytes());
        wfifob((*sd).fd, 19, (*nd).bl.graphic_color as u8);
    } else {
        wfifow((*sd).fd, 17, 0);
    }

    wfifob((*sd).fd, 20, 0);
    wfifob((*sd).fd, 21, (*nd).face as u8);
    wfifob((*sd).fd, 22, (*nd).hair as u8);
    wfifob((*sd).fd, 23, (*nd).hair_color as u8);
    wfifob((*sd).fd, 24, (*nd).face_color as u8);
    wfifob((*sd).fd, 25, (*nd).skin_color as u8);

    // armor
    if (*nd).equip[EQ_ARMOR as usize].amount == 0 {
        wfifow((*sd).fd, 26, (*nd).sex.swap_bytes());
    } else {
        wfifow((*sd).fd, 26, ((*nd).equip[EQ_ARMOR as usize].id as u16).swap_bytes());
        if (*nd).armor_color > 0 {
            wfifob((*sd).fd, 28, (*nd).armor_color as u8);
        } else {
            wfifob((*sd).fd, 28, (*nd).equip[EQ_ARMOR as usize].custom_look_color as u8);
        }
    }

    // coat
    if (*nd).equip[EQ_COAT as usize].amount > 0 {
        wfifow((*sd).fd, 26, ((*nd).equip[EQ_COAT as usize].id as u16).swap_bytes());
        wfifob((*sd).fd, 28, (*nd).equip[EQ_COAT as usize].custom_look_color as u8);
    }

    // weapon
    if (*nd).equip[EQ_WEAP as usize].amount == 0 {
        wfifow((*sd).fd, 29, 0xFFFF);
        wfifob((*sd).fd, 31, 0);
    } else {
        wfifow((*sd).fd, 29, ((*nd).equip[EQ_WEAP as usize].id as u16).swap_bytes());
        wfifob((*sd).fd, 31, (*nd).equip[EQ_WEAP as usize].custom_look_color as u8);
    }

    // shield
    if (*nd).equip[EQ_SHIELD as usize].amount == 0 {
        wfifow((*sd).fd, 32, 0xFFFF);
        wfifob((*sd).fd, 34, 0);
    } else {
        wfifow((*sd).fd, 32, ((*nd).equip[EQ_SHIELD as usize].id as u16).swap_bytes());
        wfifob((*sd).fd, 34, (*nd).equip[EQ_SHIELD as usize].custom_look_color as u8);
    }

    // helm
    if (*nd).equip[EQ_HELM as usize].amount == 0 {
        wfifob((*sd).fd, 35, 0);
        wfifow((*sd).fd, 36, 0xFF);
    } else {
        wfifob((*sd).fd, 35, 1);
        wfifob((*sd).fd, 36, (*nd).equip[EQ_HELM as usize].id as u8);
        wfifob((*sd).fd, 37, (*nd).equip[EQ_HELM as usize].custom_look_color as u8);
    }

    // beard (face acc)
    if (*nd).equip[EQ_FACEACC as usize].amount == 0 {
        wfifow((*sd).fd, 38, 0xFFFF);
        wfifob((*sd).fd, 40, 0);
    } else {
        wfifow((*sd).fd, 38, ((*nd).equip[EQ_FACEACC as usize].id as u16).swap_bytes());
        wfifob((*sd).fd, 40, (*nd).equip[EQ_FACEACC as usize].custom_look_color as u8);
    }

    // crown
    if (*nd).equip[EQ_CROWN as usize].amount == 0 {
        wfifow((*sd).fd, 41, 0xFFFF);
        wfifob((*sd).fd, 43, 0);
    } else {
        wfifob((*sd).fd, 35, 0);
        wfifow((*sd).fd, 41, ((*nd).equip[EQ_CROWN as usize].id as u16).swap_bytes());
        wfifob((*sd).fd, 43, (*nd).equip[EQ_CROWN as usize].custom_look_color as u8);
    }

    // second face acc
    if (*nd).equip[EQ_FACEACCTWO as usize].amount == 0 {
        wfifow((*sd).fd, 44, 0xFFFF);
        wfifob((*sd).fd, 46, 0);
    } else {
        wfifow((*sd).fd, 44, ((*nd).equip[EQ_FACEACCTWO as usize].id as u16).swap_bytes());
        wfifob((*sd).fd, 46, (*nd).equip[EQ_FACEACCTWO as usize].custom_look_color as u8);
    }

    // mantle
    if (*nd).equip[EQ_MANTLE as usize].amount == 0 {
        wfifow((*sd).fd, 47, 0xFFFF);
        wfifob((*sd).fd, 49, 0xFF);
    } else {
        wfifow((*sd).fd, 47, ((*nd).equip[EQ_MANTLE as usize].id as u16).swap_bytes());
        wfifob((*sd).fd, 49, (*nd).equip[EQ_MANTLE as usize].custom_look_color as u8);
    }

    // necklace
    if (*nd).equip[EQ_NECKLACE as usize].amount == 0 {
        wfifow((*sd).fd, 50, 0xFFFF);
        wfifob((*sd).fd, 52, 0);
    } else {
        wfifow((*sd).fd, 50, ((*nd).equip[EQ_NECKLACE as usize].id as u16).swap_bytes());
        wfifob((*sd).fd, 52, (*nd).equip[EQ_NECKLACE as usize].custom_look_color as u8);
    }

    // boots
    if (*nd).equip[EQ_BOOTS as usize].amount == 0 {
        wfifow((*sd).fd, 53, (*nd).sex.swap_bytes());
        wfifob((*sd).fd, 55, 0);
    } else {
        wfifow((*sd).fd, 53, ((*nd).equip[EQ_BOOTS as usize].id as u16).swap_bytes());
        wfifob((*sd).fd, 55, (*nd).equip[EQ_BOOTS as usize].custom_look_color as u8);
    }

    wfifob((*sd).fd, 56, 0);
    wfifob((*sd).fd, 57, 128);
    wfifob((*sd).fd, 58, 0);

    // name
    let name_ptr = (*nd).npc_name.as_ptr() as *const i8;
    let name_len = libc_strlen(name_ptr);

    if (*nd).state != 2 {
        wfifob((*sd).fd, 59, name_len as u8);
        let dst = wfifop((*sd).fd, 60);
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(name_ptr as *const u8, dst, name_len);
        }
    } else {
        wfifob((*sd).fd, 59, 0);
    }
    let len = if (*nd).state != 2 { name_len } else { 1 };

    // clone override
    if (*nd).clone != 0 {
        let gfx = &(*nd).gfx;
        wfifob((*sd).fd, 21, gfx.face);
        wfifob((*sd).fd, 22, gfx.hair);
        wfifob((*sd).fd, 23, gfx.chair);
        wfifob((*sd).fd, 24, gfx.cface);
        wfifob((*sd).fd, 25, gfx.cskin);
        wfifow((*sd).fd, 26, gfx.armor.swap_bytes());
        if gfx.dye > 0 {
            wfifob((*sd).fd, 28, gfx.dye);
        } else {
            wfifob((*sd).fd, 28, gfx.carmor);
        }
        wfifow((*sd).fd, 29, gfx.weapon.swap_bytes());
        wfifob((*sd).fd, 31, gfx.cweapon);
        wfifow((*sd).fd, 32, gfx.shield.swap_bytes());
        wfifob((*sd).fd, 34, gfx.cshield);

        if gfx.helm < 255 {
            wfifob((*sd).fd, 35, 1);
        } else if gfx.crown < 65535 {
            wfifob((*sd).fd, 35, 0xFF);
        } else {
            wfifob((*sd).fd, 35, 0);
        }

        wfifob((*sd).fd, 36, gfx.helm as u8);
        wfifob((*sd).fd, 37, gfx.chelm);
        wfifow((*sd).fd, 38, gfx.face_acc.swap_bytes());
        wfifob((*sd).fd, 40, gfx.cface_acc);
        wfifow((*sd).fd, 41, gfx.crown.swap_bytes());
        wfifob((*sd).fd, 43, gfx.ccrown);
        wfifow((*sd).fd, 44, gfx.face_acc_t.swap_bytes());
        wfifob((*sd).fd, 46, gfx.cface_acc_t);
        wfifow((*sd).fd, 47, gfx.mantle.swap_bytes());
        wfifob((*sd).fd, 49, gfx.cmantle);
        wfifow((*sd).fd, 50, gfx.necklace.swap_bytes());
        wfifob((*sd).fd, 52, gfx.cnecklace);
        wfifow((*sd).fd, 53, gfx.boots.swap_bytes());
        wfifob((*sd).fd, 55, gfx.cboots);

        wfifob((*sd).fd, 56, 0);
        wfifob((*sd).fd, 57, 128);
        wfifob((*sd).fd, 58, 0);

        let gfx_name_ptr = gfx.name.as_ptr() as *const i8;
        let gfx_name_len = libc_strlen(gfx_name_ptr);
        let gfx_name_empty = gfx_name_len == 0 || *gfx_name_ptr == 0;
        if !gfx_name_empty {
            wfifob((*sd).fd, 59, gfx_name_len as u8);
            let dst = wfifop((*sd).fd, 60);
            if !dst.is_null() {
                std::ptr::copy_nonoverlapping(gfx_name_ptr as *const u8, dst, gfx_name_len);
            }
        } else {
            wfifow((*sd).fd, 59, 0);
        }
        let _len = if !gfx_name_empty { gfx_name_len } else { 1 };
        // Use gfx name length for packet size — mirrors C behaviour
        let final_len = if !gfx_name_empty { gfx_name_len } else { 1 };
        wfifow((*sd).fd, 1, (final_len as u16 + 60).swap_bytes());
        wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
        return 0;
    }

    wfifow((*sd).fd, 1, (len as u16 + 60).swap_bytes());
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_cmoblook_sub ───────────────────────────────────────────────────────

///
/// Send full character-mob (charstate mob) appearance packet to a player.
///
/// Args:
///   - `bl`:        the block-list entry being iterated
///   - `look_type`: `LOOK_GET` or `LOOK_SEND`
///   - `arg`:       if `LOOK_GET`, `bl` is the mob and `arg` is cast to `*mut MapSessionData`;
///                  if `LOOK_SEND`, `bl` is the player and `arg` is cast to `*mut MobSpawnData`.
///
/// Mirrors `clif_cmoblook_sub` (~line 3016).
pub unsafe fn clif_cmoblook_inner(bl: *mut BlockList, look_type: i32, arg: *mut BlockList) -> i32 {
    let (mob, sd): (*mut MobSpawnData, *mut MapSessionData) = if look_type == LOOK_GET {
        if bl.is_null() { return 0; }
        if arg.is_null() { return 0; }
        (bl as *mut MobSpawnData, arg as *mut MapSessionData)
    } else {
        // LOOK_SEND
        if bl.is_null() { return 0; }
        if arg.is_null() { return 0; }
        (arg as *mut MobSpawnData, bl as *mut MapSessionData)
    };

    if (*mob).bl.m != (*sd).bl.m || (*(*mob).data).mobtype != 1 || (*mob).state == 1 {
        return 0;
    }

    if !session_exists((*sd).fd) {
        return 0;
    }

    wfifohead((*sd).fd, 512);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 3, 0x33);
    wfifow((*sd).fd, 5, ((*mob).bl.x as u16).swap_bytes());
    wfifow((*sd).fd, 7, ((*mob).bl.y as u16).swap_bytes());
    wfifob((*sd).fd, 9, (*mob).side as u8);
    wfifol((*sd).fd, 10, (*mob).bl.id.swap_bytes());

    if (*mob).charstate < 4 {
        wfifow((*sd).fd, 14, (*(*mob).data).sex.swap_bytes());
    } else {
        wfifob((*sd).fd, 14, 1);
        wfifob((*sd).fd, 15, 15);
    }

    if (*mob).charstate == 2 && (*sd).status.gm_level != 0 {
        wfifob((*sd).fd, 16, 5);
    } else {
        wfifob((*sd).fd, 16, (*mob).charstate as u8);
    }

    wfifob((*sd).fd, 19, 80);

    if (*mob).charstate == 3 {
        wfifow((*sd).fd, 17, (*mob).look.swap_bytes());
    } else if (*mob).charstate == 4 {
        wfifow((*sd).fd, 17, (*mob).look.wrapping_add(32768).swap_bytes());
        wfifob((*sd).fd, 19, (*mob).look_color);
    } else {
        wfifow((*sd).fd, 17, 0);
    }

    wfifob((*sd).fd, 20, 0);
    wfifob((*sd).fd, 21, (*(*mob).data).face as u8);
    wfifob((*sd).fd, 22, (*(*mob).data).hair as u8);
    wfifob((*sd).fd, 23, (*(*mob).data).hair_color as u8);
    wfifob((*sd).fd, 24, (*(*mob).data).face_color as u8);
    wfifob((*sd).fd, 25, (*(*mob).data).skin_color as u8);

    // armor
    if (*(*mob).data).equip[EQ_ARMOR as usize].amount == 0 {
        wfifow((*sd).fd, 26, (*(*mob).data).sex.swap_bytes());
    } else {
        wfifow((*sd).fd, 26, ((*(*mob).data).equip[EQ_ARMOR as usize].id as u16).swap_bytes());
        if (*(*mob).data).armor_color > 0 {
            wfifob((*sd).fd, 28, (*(*mob).data).armor_color as u8);
        } else {
            wfifob((*sd).fd, 28, (*(*mob).data).equip[EQ_ARMOR as usize].custom_look_color as u8);
        }
    }

    // coat
    if (*(*mob).data).equip[EQ_COAT as usize].amount > 0 {
        wfifow((*sd).fd, 26, ((*(*mob).data).equip[EQ_COAT as usize].id as u16).swap_bytes());
        wfifob((*sd).fd, 28, (*(*mob).data).equip[EQ_COAT as usize].custom_look_color as u8);
    }

    // weapon
    if (*(*mob).data).equip[EQ_WEAP as usize].amount == 0 {
        wfifow((*sd).fd, 29, 0xFFFF);
        wfifob((*sd).fd, 31, 0);
    } else {
        wfifow((*sd).fd, 29, ((*(*mob).data).equip[EQ_WEAP as usize].id as u16).swap_bytes());
        wfifob((*sd).fd, 31, (*(*mob).data).equip[EQ_WEAP as usize].custom_look_color as u8);
    }

    // shield
    if (*(*mob).data).equip[EQ_SHIELD as usize].amount == 0 {
        wfifow((*sd).fd, 32, 0xFFFF);
        wfifob((*sd).fd, 34, 0);
    } else {
        wfifow((*sd).fd, 32, ((*(*mob).data).equip[EQ_SHIELD as usize].id as u16).swap_bytes());
        wfifob((*sd).fd, 34, (*(*mob).data).equip[EQ_SHIELD as usize].custom_look_color as u8);
    }

    // helm
    if (*(*mob).data).equip[EQ_HELM as usize].amount == 0 {
        wfifob((*sd).fd, 35, 0);
        wfifow((*sd).fd, 36, 0xFF);
    } else {
        wfifob((*sd).fd, 35, 1);
        wfifob((*sd).fd, 36, (*(*mob).data).equip[EQ_HELM as usize].id as u8);
        wfifob((*sd).fd, 37, (*(*mob).data).equip[EQ_HELM as usize].custom_look_color as u8);
    }

    // beard (face acc)
    if (*(*mob).data).equip[EQ_FACEACC as usize].amount == 0 {
        wfifow((*sd).fd, 38, 0xFFFF);
        wfifob((*sd).fd, 40, 0);
    } else {
        wfifow((*sd).fd, 38, ((*(*mob).data).equip[EQ_FACEACC as usize].id as u16).swap_bytes());
        wfifob((*sd).fd, 40, (*(*mob).data).equip[EQ_FACEACC as usize].custom_look_color as u8);
    }

    // crown
    if (*(*mob).data).equip[EQ_CROWN as usize].amount == 0 {
        wfifow((*sd).fd, 41, 0xFFFF);
        wfifob((*sd).fd, 43, 0);
    } else {
        wfifob((*sd).fd, 35, 0);
        wfifow((*sd).fd, 41, ((*(*mob).data).equip[EQ_CROWN as usize].id as u16).swap_bytes());
        wfifob((*sd).fd, 43, (*(*mob).data).equip[EQ_CROWN as usize].custom_look_color as u8);
    }

    // second face acc
    if (*(*mob).data).equip[EQ_FACEACCTWO as usize].amount == 0 {
        wfifow((*sd).fd, 44, 0xFFFF);
        wfifob((*sd).fd, 46, 0);
    } else {
        wfifow((*sd).fd, 44, ((*(*mob).data).equip[EQ_FACEACCTWO as usize].id as u16).swap_bytes());
        wfifob((*sd).fd, 46, (*(*mob).data).equip[EQ_FACEACCTWO as usize].custom_look_color as u8);
    }

    // mantle
    if (*(*mob).data).equip[EQ_MANTLE as usize].amount == 0 {
        wfifow((*sd).fd, 47, 0xFFFF);
        wfifob((*sd).fd, 49, 0xFF);
    } else {
        wfifow((*sd).fd, 47, ((*(*mob).data).equip[EQ_MANTLE as usize].id as u16).swap_bytes());
        wfifob((*sd).fd, 49, (*(*mob).data).equip[EQ_MANTLE as usize].custom_look_color as u8);
    }

    // necklace
    if (*(*mob).data).equip[EQ_NECKLACE as usize].amount == 0 {
        wfifow((*sd).fd, 50, 0xFFFF);
        wfifob((*sd).fd, 52, 0);
    } else {
        wfifow((*sd).fd, 50, ((*(*mob).data).equip[EQ_NECKLACE as usize].id as u16).swap_bytes());
        wfifob((*sd).fd, 52, (*(*mob).data).equip[EQ_NECKLACE as usize].custom_look_color as u8);
    }

    // boots
    if (*(*mob).data).equip[EQ_BOOTS as usize].amount == 0 {
        wfifow((*sd).fd, 53, (*(*mob).data).sex.swap_bytes());
        wfifob((*sd).fd, 55, 0);
    } else {
        wfifow((*sd).fd, 53, ((*(*mob).data).equip[EQ_BOOTS as usize].id as u16).swap_bytes());
        wfifob((*sd).fd, 55, (*(*mob).data).equip[EQ_BOOTS as usize].custom_look_color as u8);
    }

    wfifob((*sd).fd, 56, 0);
    wfifob((*sd).fd, 57, 128);
    wfifob((*sd).fd, 58, 0);

    // name
    let name_ptr = (*(*mob).data).name.as_ptr() as *const i8;
    let name_len = libc_strlen(name_ptr);

    if (*mob).state != 2 {
        wfifob((*sd).fd, 59, name_len as u8);
        let dst = wfifop((*sd).fd, 60);
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(name_ptr as *const u8, dst, name_len);
        }
    } else {
        wfifob((*sd).fd, 59, 0);
    }
    let len = if (*mob).state != 2 { name_len } else { 1 };

    // clone override
    if (*mob).clone != 0 {
        let gfx = &(*mob).gfx;
        wfifob((*sd).fd, 21, gfx.face);
        wfifob((*sd).fd, 22, gfx.hair);
        wfifob((*sd).fd, 23, gfx.chair);
        wfifob((*sd).fd, 24, gfx.cface);
        wfifob((*sd).fd, 25, gfx.cskin);
        wfifow((*sd).fd, 26, gfx.armor.swap_bytes());
        if gfx.dye > 0 {
            wfifob((*sd).fd, 28, gfx.dye);
        } else {
            wfifob((*sd).fd, 28, gfx.carmor);
        }
        wfifow((*sd).fd, 29, gfx.weapon.swap_bytes());
        wfifob((*sd).fd, 31, gfx.cweapon);
        wfifow((*sd).fd, 32, gfx.shield.swap_bytes());
        wfifob((*sd).fd, 34, gfx.cshield);

        if gfx.helm < 255 {
            wfifob((*sd).fd, 35, 1);
        } else if gfx.crown < 65535 {
            wfifob((*sd).fd, 35, 0xFF);
        } else {
            wfifob((*sd).fd, 35, 0);
        }

        wfifob((*sd).fd, 36, gfx.helm as u8);
        wfifob((*sd).fd, 37, gfx.chelm);
        wfifow((*sd).fd, 38, gfx.face_acc.swap_bytes());
        wfifob((*sd).fd, 40, gfx.cface_acc);
        wfifow((*sd).fd, 41, gfx.crown.swap_bytes());
        wfifob((*sd).fd, 43, gfx.ccrown);
        wfifow((*sd).fd, 44, gfx.face_acc_t.swap_bytes());
        wfifob((*sd).fd, 46, gfx.cface_acc_t);
        wfifow((*sd).fd, 47, gfx.mantle.swap_bytes());
        wfifob((*sd).fd, 49, gfx.cmantle);
        wfifow((*sd).fd, 50, gfx.necklace.swap_bytes());
        wfifob((*sd).fd, 52, gfx.cnecklace);
        wfifow((*sd).fd, 53, gfx.boots.swap_bytes());
        wfifob((*sd).fd, 55, gfx.cboots);

        wfifob((*sd).fd, 56, 0);
        wfifob((*sd).fd, 57, 128);
        wfifob((*sd).fd, 58, 0);

        let gfx_name_ptr = gfx.name.as_ptr() as *const i8;
        let gfx_name_len = libc_strlen(gfx_name_ptr);
        let gfx_name_empty = gfx_name_len == 0 || *gfx_name_ptr == 0;
        if !gfx_name_empty {
            wfifob((*sd).fd, 59, gfx_name_len as u8);
            let dst = wfifop((*sd).fd, 60);
            if !dst.is_null() {
                std::ptr::copy_nonoverlapping(gfx_name_ptr as *const u8, dst, gfx_name_len);
            }
        } else {
            wfifow((*sd).fd, 59, 0);
        }
        let final_len = if !gfx_name_empty { gfx_name_len } else { 1 };
        wfifow((*sd).fd, 1, (final_len as u16 + 60).swap_bytes());
        wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
        return 0;
    }

    wfifow((*sd).fd, 1, (len as u16 + 60).swap_bytes());
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_charlook_sub ───────────────────────────────────────────────────────

///
/// Send full player appearance packet to another player.
///
/// Args:
///   - `bl`:        the block-list entry being iterated
///   - `look_type`: `LOOK_GET` or `LOOK_SEND`
///   - `arg`:       if `LOOK_GET`, `bl` is the player whose appearance we send and `arg` is the viewer;
///                  if `LOOK_SEND`, `bl` is the viewer and `arg` is the player whose appearance we send.
///
/// Mirrors `clif_charlook_sub` (~line 3285).
pub unsafe fn clif_charlook_inner(bl: *mut BlockList, look_type: i32, arg: *mut MapSessionData) -> i32 {
    // sd  = the player whose appearance we send
    // src_sd = the player receiving the packet
    let (sd, src_sd): (*mut MapSessionData, *mut MapSessionData) = if look_type == LOOK_GET {
        if bl.is_null() { return 0; }
        if arg.is_null() { return 0; }
        // C: sd=(USER*)bl, src_sd=va_arg — if src_sd==sd return 0
        if bl as *mut MapSessionData == arg { return 0; }
        (bl as *mut MapSessionData, arg)
    } else {
        if bl.is_null() { return 0; }
        if arg.is_null() { return 0; }
        (arg, bl as *mut MapSessionData)
    };

    if (*sd).bl.m != (*src_sd).bl.m { return 0; }

    if ((*sd).optFlags & OPT_FLAG_STEALTH) != 0
        && (*src_sd).status.gm_level == 0
        && (*sd).status.id != (*src_sd).status.id
    {
        return 0;
    }

    // Ghost visibility check (mirrors C: `if (map[sd->bl.m].show_ghosts && ...)`)
    {
        let slot = crate::database::map_db::get_map_ptr((*sd).bl.m);
        if !slot.is_null()
            && (*slot).show_ghosts != 0
            && (*sd).status.state == 1
            && (*sd).bl.id != (*src_sd).bl.id
        {
            if (*src_sd).status.state != 1
                && ((*src_sd).optFlags & crate::game::pc::OPT_FLAG_GHOSTS) == 0
            {
                return 0;
            }
        }
    }

    if !session_exists((*sd).fd) {
        return 0;
    }

    wfifohead((*src_sd).fd, 512);
    wfifob((*src_sd).fd, 0, 0xAA);
    wfifob((*src_sd).fd, 3, 0x33);
    wfifow((*src_sd).fd, 5, ((*sd).bl.x as u16).swap_bytes());
    wfifow((*src_sd).fd, 7, ((*sd).bl.y as u16).swap_bytes());
    wfifob((*src_sd).fd, 9, (*sd).status.side as u8);
    wfifol((*src_sd).fd, 10, (*sd).status.id.swap_bytes());

    if (*sd).status.state < 4 {
        wfifow((*src_sd).fd, 14, ((*sd).status.sex as u16).swap_bytes());
    } else {
        wfifob((*src_sd).fd, 14, 1);
        wfifob((*src_sd).fd, 15, 15);
    }

    // Invisibility / stealth state
    let invis_cond = ((*sd).status.state == 2 || ((*sd).optFlags & OPT_FLAG_STEALTH) != 0)
        && (*sd).bl.id != (*src_sd).bl.id
        && ((*src_sd).status.gm_level != 0
            || clif_isingroup(src_sd, sd) != 0
            || ((*sd).gfx.dye == (*src_sd).gfx.dye
                && (*sd).gfx.dye != 0
                && (*src_sd).gfx.dye != 0));

    if invis_cond {
        wfifob((*src_sd).fd, 16, 5);
    } else {
        wfifob((*src_sd).fd, 16, (*sd).status.state as u8);
    }

    if ((*sd).optFlags & OPT_FLAG_STEALTH) != 0
        && (*sd).status.state == 0
        && ((*src_sd).status.gm_level == 0 || (*sd).bl.id == (*src_sd).bl.id)
    {
        wfifob((*src_sd).fd, 16, 2);
    }

    wfifob((*src_sd).fd, 19, (*sd).speed as u8);

    if (*sd).status.state == 3 {
        wfifow((*src_sd).fd, 17, (*sd).disguise.swap_bytes());
    } else if (*sd).status.state == 4 {
        wfifow((*src_sd).fd, 17, (*sd).disguise.wrapping_add(32768).swap_bytes());
        wfifob((*src_sd).fd, 19, (*sd).disguise_color as u8);
    } else {
        wfifow((*src_sd).fd, 17, 0u16.swap_bytes());
    }

    wfifob((*src_sd).fd, 20, 0);

    wfifob((*src_sd).fd, 21, (*sd).status.face as u8);
    wfifob((*src_sd).fd, 22, (*sd).status.hair as u8);
    wfifob((*src_sd).fd, 23, (*sd).status.hair_color as u8);
    wfifob((*src_sd).fd, 24, (*sd).status.face_color as u8);
    wfifob((*src_sd).fd, 25, (*sd).status.skin_color as u8);

    // armor
    let armor_id = pc_isequip(sd, EQ_ARMOR) as u32;
    if armor_id == 0 {
        wfifow((*src_sd).fd, 26, ((*sd).status.sex as u16).swap_bytes());
    } else {
        let armor_item = item_db::search(armor_id);
        if (*sd).status.equip[EQ_ARMOR as usize].custom_look != 0 {
            wfifow((*src_sd).fd, 26, ((*sd).status.equip[EQ_ARMOR as usize].custom_look as u16).swap_bytes());
        } else {
            wfifow((*src_sd).fd, 26, (armor_item.look as u16).swap_bytes());
        }
        if (*sd).status.armor_color > 0 {
            wfifob((*src_sd).fd, 28, (*sd).status.armor_color as u8);
        } else if (*sd).status.equip[EQ_ARMOR as usize].custom_look != 0 {
            wfifob((*src_sd).fd, 28, (*sd).status.equip[EQ_ARMOR as usize].custom_look_color as u8);
        } else {
            wfifob((*src_sd).fd, 28, armor_item.look_color as u8);
        }
    }

    // coat
    let coat_id = pc_isequip(sd, EQ_COAT) as u32;
    if coat_id != 0 {
        let coat_item = item_db::search(coat_id);
        wfifow((*src_sd).fd, 26, (coat_item.look as u16).swap_bytes());
        if (*sd).status.armor_color > 0 {
            wfifob((*src_sd).fd, 28, (*sd).status.armor_color as u8);
        } else {
            wfifob((*src_sd).fd, 28, coat_item.look_color as u8);
        }
    }

    // weapon
    let weap_id = pc_isequip(sd, EQ_WEAP) as u32;
    if weap_id == 0 {
        wfifow((*src_sd).fd, 29, 0xFFFF);
        wfifob((*src_sd).fd, 31, 0x0);
    } else if (*sd).status.equip[EQ_WEAP as usize].custom_look != 0 {
        wfifow((*src_sd).fd, 29, ((*sd).status.equip[EQ_WEAP as usize].custom_look as u16).swap_bytes());
        wfifob((*src_sd).fd, 31, (*sd).status.equip[EQ_WEAP as usize].custom_look_color as u8);
    } else {
        let weap_item = item_db::search(weap_id);
        wfifow((*src_sd).fd, 29, (weap_item.look as u16).swap_bytes());
        wfifob((*src_sd).fd, 31, weap_item.look_color as u8);
    }

    // shield
    let shield_id = pc_isequip(sd, EQ_SHIELD) as u32;
    if shield_id == 0 {
        wfifow((*src_sd).fd, 32, 0xFFFF);
        wfifob((*src_sd).fd, 34, 0);
    } else if (*sd).status.equip[EQ_SHIELD as usize].custom_look != 0 {
        wfifow((*src_sd).fd, 32, ((*sd).status.equip[EQ_SHIELD as usize].custom_look as u16).swap_bytes());
        wfifob((*src_sd).fd, 34, (*sd).status.equip[EQ_SHIELD as usize].custom_look_color as u8);
    } else {
        let shield_item = item_db::search(shield_id);
        wfifow((*src_sd).fd, 32, (shield_item.look as u16).swap_bytes());
        wfifob((*src_sd).fd, 34, shield_item.look_color as u8);
    }

    // helm
    let helm_id = pc_isequip(sd, EQ_HELM) as u32;
    let helm_item = item_db::search(helm_id);
    if helm_id == 0
        || ((*sd).status.setting_flags & FLAG_HELM as u16) == 0
        || helm_item.look == -1
    {
        wfifob((*src_sd).fd, 35, 0);
        wfifow((*src_sd).fd, 36, 0xFFFF);
    } else {
        wfifob((*src_sd).fd, 35, 1);
        if (*sd).status.equip[EQ_HELM as usize].custom_look != 0 {
            wfifob((*src_sd).fd, 36, (*sd).status.equip[EQ_HELM as usize].custom_look as u8);
            wfifob((*src_sd).fd, 37, (*sd).status.equip[EQ_HELM as usize].custom_look_color as u8);
        } else {
            wfifob((*src_sd).fd, 36, helm_item.look as u8);
            wfifob((*src_sd).fd, 37, helm_item.look_color as u8);
        }
    }

    // beard (face acc)
    let faceacc_id = pc_isequip(sd, EQ_FACEACC) as u32;
    if faceacc_id == 0 {
        wfifow((*src_sd).fd, 38, 0xFFFF);
        wfifob((*src_sd).fd, 40, 0);
    } else {
        let faceacc_item = item_db::search(faceacc_id);
        wfifow((*src_sd).fd, 38, (faceacc_item.look as u16).swap_bytes());
        wfifob((*src_sd).fd, 40, faceacc_item.look_color as u8);
    }

    // crown
    let crown_id = pc_isequip(sd, EQ_CROWN) as u32;
    if crown_id == 0 {
        wfifow((*src_sd).fd, 41, 0xFFFF);
        wfifob((*src_sd).fd, 43, 0);
    } else {
        wfifob((*src_sd).fd, 35, 0xFF);
        if (*sd).status.equip[EQ_CROWN as usize].custom_look != 0 {
            wfifow((*src_sd).fd, 41, ((*sd).status.equip[EQ_CROWN as usize].custom_look as u16).swap_bytes());
            wfifob((*src_sd).fd, 43, (*sd).status.equip[EQ_CROWN as usize].custom_look_color as u8);
        } else {
            let crown_item = item_db::search(crown_id);
            wfifow((*src_sd).fd, 41, (crown_item.look as u16).swap_bytes());
            wfifob((*src_sd).fd, 43, crown_item.look_color as u8);
        }
    }

    // second face acc
    let faceacctwo_id = pc_isequip(sd, EQ_FACEACCTWO) as u32;
    if faceacctwo_id == 0 {
        wfifow((*src_sd).fd, 44, 0xFFFF);
        wfifob((*src_sd).fd, 46, 0);
    } else {
        let faceacctwo_item = item_db::search(faceacctwo_id);
        wfifow((*src_sd).fd, 44, (faceacctwo_item.look as u16).swap_bytes());
        wfifob((*src_sd).fd, 46, faceacctwo_item.look_color as u8);
    }

    // mantle
    let mantle_id = pc_isequip(sd, EQ_MANTLE) as u32;
    if mantle_id == 0 {
        wfifow((*src_sd).fd, 47, 0xFFFF);
        wfifob((*src_sd).fd, 49, 0xFF);
    } else {
        let mantle_item = item_db::search(mantle_id);
        wfifow((*src_sd).fd, 47, (mantle_item.look as u16).swap_bytes());
        wfifob((*src_sd).fd, 49, mantle_item.look_color as u8);
    }

    // necklace
    let necklace_id = pc_isequip(sd, EQ_NECKLACE) as u32;
    let necklace_item = item_db::search(necklace_id);
    if necklace_id == 0
        || ((*sd).status.setting_flags & FLAG_NECKLACE as u16) == 0
        || necklace_item.look == -1
    {
        wfifow((*src_sd).fd, 50, 0xFFFF);
        wfifob((*src_sd).fd, 52, 0);
    } else {
        wfifow((*src_sd).fd, 50, (necklace_item.look as u16).swap_bytes());
        wfifob((*src_sd).fd, 52, necklace_item.look_color as u8);
    }

    // boots
    let boots_id = pc_isequip(sd, EQ_BOOTS) as u32;
    if boots_id == 0 {
        wfifow((*src_sd).fd, 53, ((*sd).status.sex as u16).swap_bytes());
        wfifob((*src_sd).fd, 55, 0);
    } else if (*sd).status.equip[EQ_BOOTS as usize].custom_look != 0 {
        wfifow((*src_sd).fd, 53, ((*sd).status.equip[EQ_BOOTS as usize].custom_look as u16).swap_bytes());
        wfifob((*src_sd).fd, 55, (*sd).status.equip[EQ_BOOTS as usize].custom_look_color as u8);
    } else {
        let boots_item = item_db::search(boots_id);
        wfifow((*src_sd).fd, 53, (boots_item.look as u16).swap_bytes());
        wfifob((*src_sd).fd, 55, boots_item.look_color as u8);
    }

    // 56 = title colour, 57 = outline colour (128=black), 58 = normal colour
    wfifob((*src_sd).fd, 56, 0);
    wfifob((*src_sd).fd, 57, 128);
    wfifob((*src_sd).fd, 58, 0);

    // title colour: hidden for invisible/stealthy chars unless you're in their group
    if invis_cond {
        wfifob((*src_sd).fd, 56, 0);
    } else if (*sd).gfx.dye != 0 {
        wfifob((*src_sd).fd, 56, (*sd).gfx.title_color);
    } else {
        wfifob((*src_sd).fd, 56, 0);
    }

    // name
    let name_ptr = (*sd).status.name.as_ptr() as *const i8;
    let name_len = libc_strlen(name_ptr);

    // colour byte 58: clan=3, group=2, pk=1
    if (*src_sd).status.clan == (*sd).status.clan && (*src_sd).status.clan > 0
        && (*src_sd).status.id != (*sd).status.id
    {
        wfifob((*src_sd).fd, 58, 3);
    }

    if clif_isingroup(src_sd, sd) != 0 {
        wfifob((*src_sd).fd, 58, 2);
    }

    let mut exist: i32 = -1;
    for x in 0..20usize {
        if (*src_sd).pvp[x][0] == (*sd).bl.id {
            exist = x as i32;
            break;
        }
    }

    if (*sd).status.pk > 0 || exist != -1 {
        wfifob((*src_sd).fd, 58, 1);
    }

    // name field
    if (*sd).status.state != 2 && (*sd).status.state != 5 {
        wfifob((*src_sd).fd, 59, name_len as u8);
        let dst = wfifop((*src_sd).fd, 60);
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(name_ptr as *const u8, dst, name_len);
        }
    } else {
        wfifob((*src_sd).fd, 59, 0);
    }
    let len = if (*sd).status.state != 2 && (*sd).status.state != 5 { name_len } else { 0 };

    // gm/clone gfx override
    if ((*sd).status.gm_level != 0 && (*sd).gfx.toggle != 0) || (*sd).clone != 0 {
        let gfx = &(*sd).gfx;
        wfifob((*src_sd).fd, 21, gfx.face);
        wfifob((*src_sd).fd, 22, gfx.hair);
        wfifob((*src_sd).fd, 23, gfx.chair);
        wfifob((*src_sd).fd, 24, gfx.cface);
        wfifob((*src_sd).fd, 25, gfx.cskin);
        wfifow((*src_sd).fd, 26, gfx.armor.swap_bytes());
        if gfx.dye > 0 {
            wfifob((*src_sd).fd, 28, gfx.dye);
        } else {
            wfifob((*src_sd).fd, 28, gfx.carmor);
        }
        wfifow((*src_sd).fd, 29, gfx.weapon.swap_bytes());
        wfifob((*src_sd).fd, 31, gfx.cweapon);
        wfifow((*src_sd).fd, 32, gfx.shield.swap_bytes());
        wfifob((*src_sd).fd, 34, gfx.cshield);

        if gfx.helm < 255 {
            wfifob((*src_sd).fd, 35, 1);
        } else if gfx.crown < 65535 {
            wfifob((*src_sd).fd, 35, 0xFF);
        } else {
            wfifob((*src_sd).fd, 35, 0);
        }

        wfifob((*src_sd).fd, 36, gfx.helm as u8);
        wfifob((*src_sd).fd, 37, gfx.chelm);
        wfifow((*src_sd).fd, 38, gfx.face_acc.swap_bytes());
        wfifob((*src_sd).fd, 40, gfx.cface_acc);
        wfifow((*src_sd).fd, 41, gfx.crown.swap_bytes());
        wfifob((*src_sd).fd, 43, gfx.ccrown);
        wfifow((*src_sd).fd, 44, gfx.face_acc_t.swap_bytes());
        wfifob((*src_sd).fd, 46, gfx.cface_acc_t);
        wfifow((*src_sd).fd, 47, gfx.mantle.swap_bytes());
        wfifob((*src_sd).fd, 49, gfx.cmantle);
        wfifow((*src_sd).fd, 50, gfx.necklace.swap_bytes());
        wfifob((*src_sd).fd, 52, gfx.cnecklace);
        wfifow((*src_sd).fd, 53, gfx.boots.swap_bytes());
        wfifob((*src_sd).fd, 55, gfx.cboots);

        wfifob((*src_sd).fd, 56, 0);
        wfifob((*src_sd).fd, 57, 128);
        wfifob((*src_sd).fd, 58, 0);

        // gfx title colour
        if invis_cond {
            wfifob((*src_sd).fd, 56, 0);
        } else if gfx.dye != 0 {
            wfifob((*src_sd).fd, 56, gfx.title_color);
        } else {
            wfifob((*src_sd).fd, 56, 0);
        }

        let gfx_name_ptr = gfx.name.as_ptr() as *const i8;
        let gfx_name_len = libc_strlen(gfx_name_ptr);
        let gfx_name_empty = gfx_name_len == 0 || *gfx_name_ptr == 0;
        let visible = (*sd).status.state != 2 && (*sd).status.state != 5;
        if visible && !gfx_name_empty {
            wfifob((*src_sd).fd, 59, gfx_name_len as u8);
            let dst = wfifop((*src_sd).fd, 60);
            if !dst.is_null() {
                std::ptr::copy_nonoverlapping(gfx_name_ptr as *const u8, dst, gfx_name_len);
            }
        } else {
            wfifob((*src_sd).fd, 59, 0);
        }
        let final_len = if visible && !gfx_name_empty { gfx_name_len } else { 1 };
        wfifow((*src_sd).fd, 1, (final_len as u16 + 60 + 3).swap_bytes());
        wfifoset((*src_sd).fd, encrypt((*src_sd).fd) as usize);
        clif_sendanimations(&mut *src_sd, &mut *sd);
        return 0;
    }

    wfifow((*src_sd).fd, 1, (len as u16 + 60 + 3).swap_bytes());
    wfifoset((*src_sd).fd, encrypt((*src_sd).fd) as usize);
    clif_sendanimations(&mut *src_sd, &mut *sd);
    0
}

// ─── clif_spawn ──────────────────────────────────────────────────────────────

/// Add a player to the block grid and send their appearance to nearby clients.
///
/// Thin wrapper — mirrors `clif_spawn` (~line 4075).
pub unsafe fn clif_spawn(sd: *mut MapSessionData) -> i32 {
    if map_addblock(&mut (*sd).bl) != 0 {
        // printf("Error Spawn\n") — silently ignore in Rust
    }
    clif_sendchararea(sd);
    0
}

// ─── Inline strlen helper ─────────────────────────────────────────────────────

/// Compute the length of a nul-terminated C string without pulling in libc.
///
/// # Safety
/// `s` must point to a valid nul-terminated string.
unsafe fn libc_strlen(s: *const i8) -> usize {
    if s.is_null() { return 0; }
    let mut len = 0usize;
    while *s.add(len) != 0 { len += 1; }
    len
}
