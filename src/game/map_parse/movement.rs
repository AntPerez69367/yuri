//! Movement and walk packet handlers.
//!
//! Covers:
//!   - `clif_blockmovement`   — send movement-block/unblock packet to one player
//!   - `clif_sendchararea`    — broadcast all nearby PCs to a player
//!   - `clif_charspecific`    — send a single PC appearance to another PC
//!   - `clif_parsewalk`       — handle a client walk request
//!   - `clif_noparsewalk`     — server-driven forced walk
//!   - `clif_parsewalkpong`   — walk ping/pong latency handler
//!   - `clif_parsemap`        — client map-data request
//!   - `clif_sendmapdata`     — send tile/collision data to a player
//!   - `clif_sendside`        — broadcast facing direction
//!   - `clif_parseside`       — handle client side-change packet

#![allow(non_snake_case, clippy::wildcard_imports)]

use std::ptr;

use crate::database::map_db::{WarpList, BLOCK_SIZE};
use crate::database::map_db::map_data;
use crate::session::session_exists;
use crate::config::Point;
use crate::game::player::prelude::*;
use crate::game::pc::{
    MapSessionData,
    BL_PC, BL_MOB, BL_NPC,
    OPT_FLAG_STEALTH, OPT_FLAG_GHOSTS,
    FLAG_FASTMOVE, FLAG_HELM, FLAG_NECKLACE,
};
use crate::common::player::spells::{MAX_SPELLS, MAX_MAGIC_TIMERS};

use super::packet::{
    encrypt, rfifob, rfifol, rfifow,
    wfifob, wfifohead, wfifol, wfifop, wfifoset, wfifow,
    wfifoheader,
    AREA, AREA_WOS,
};

// ─── Constants ────────────────────────────────────────────────────────────────


use crate::common::constants::entity::player::{
    PC_DIE_I8 as PC_DIE, PC_INVIS_I8 as PC_INVIS, PC_MOUNTED_I8 as PC_MOUNTED, PC_DISGUISE_I8 as PC_DISGUISE,
    EQ_WEAP, EQ_ARMOR, EQ_SHIELD, EQ_HELM, EQ_FACEACC, EQ_CROWN,
    EQ_FACEACCTWO, EQ_MANTLE, EQ_NECKLACE, EQ_BOOTS, EQ_COAT,
};


// CRC lookup table for the NexCRCC checksum algorithm.
#[allow(clippy::unreadable_literal)]
static CRC_TABLE: [u16; 256] = [
    0x0000, 0x1021, 0x2042, 0x3063, 0x4084, 0x50A5,
    0x60C6, 0x70E7, 0x8108, 0x9129,
    0xA14A, 0xB16B, 0xC18C, 0xD1AD,
    0xE1CE, 0xF1EF, 0x1231, 0x0210, 0x3273, 0x2252,
    0x52B5, 0x4294, 0x72F7, 0x62D6,
    0x9339, 0x8318, 0xB37B, 0xA35A,
    0xD3BD, 0xC39C, 0xF3FF, 0xE3DE,
    0x2462, 0x3443, 0x0420, 0x1401, 0x64E6, 0x74C7,
    0x44A4, 0x5485, 0xA56A, 0xB54B,
    0x8528, 0x9509, 0xE5EE, 0xF5CF,
    0xC5AC, 0xD58D, 0x3653, 0x2672, 0x1611, 0x0630,
    0x76D7, 0x66F6, 0x5695, 0x46B4,
    0xB75B, 0xA77A, 0x9719, 0x8738,
    0xF7DF, 0xE7FE, 0xD79D, 0xC7BC,
    0x48C4, 0x58E5, 0x6886, 0x78A7,
    0x0840, 0x1861, 0x2802, 0x3823, 0xC9CC, 0xD9ED,
    0xE98E, 0xF9AF, 0x8948, 0x9969,
    0xA90A, 0xB92B, 0x5AF5, 0x4AD4,
    0x7AB7, 0x6A96, 0x1A71, 0x0A50, 0x3A33, 0x2A12,
    0xDBFD, 0xCBDC, 0xFBBF, 0xEB9E,
    0x9B79, 0x8B58, 0xBB3B, 0xAB1A,
    0x6CA6, 0x7C87, 0x4CE4, 0x5CC5,
    0x2C22, 0x3C03, 0x0C60, 0x1C41, 0xEDAE, 0xFD8F,
    0xCDEC, 0xDDCD, 0xAD2A, 0xBD0B,
    0x8D68, 0x9D49, 0x7E97, 0x6EB6,
    0x5ED5, 0x4EF4, 0x3E13, 0x2E32, 0x1E51, 0x0E70,
    0xFF9F, 0xEFBE, 0xDFDD, 0xCFFC,
    0xBF1B, 0xAF3A, 0x9F59, 0x8F78,
    0x9188, 0x81A9, 0xB1CA, 0xA1EB,
    0xD10C, 0xC12D, 0xF14E, 0xE16F,
    0x1080, 0x00A1, 0x30C2, 0x20E3, 0x5004, 0x4025,
    0x7046, 0x6067, 0x83B9, 0x9398,
    0xA3FB, 0xB3DA, 0xC33D, 0xD31C,
    0xE37F, 0xF35E, 0x02B1, 0x1290, 0x22F3, 0x32D2,
    0x4235, 0x5214, 0x6277, 0x7256,
    0xB5EA, 0xA5CB, 0x95A8, 0x8589,
    0xF56E, 0xE54F, 0xD52C, 0xC50D,
    0x34E2, 0x24C3, 0x14A0, 0x0481, 0x7466, 0x6447,
    0x5424, 0x4405, 0xA7DB, 0xB7FA,
    0x8799, 0x97B8, 0xE75F, 0xF77E,
    0xC71D, 0xD73C, 0x26D3, 0x36F2, 0x0691, 0x16B0,
    0x6657, 0x7676, 0x4615, 0x5634,
    0xD94C, 0xC96D, 0xF90E, 0xE92F,
    0x99C8, 0x89E9, 0xB98A, 0xA9AB,
    0x5844, 0x4865, 0x7806, 0x6827,
    0x18C0, 0x08E1, 0x3882, 0x28A3, 0xCB7D, 0xDB5C,
    0xEB3F, 0xFB1E, 0x8BF9, 0x9BD8,
    0xABBB, 0xBB9A, 0x4A75, 0x5A54,
    0x6A37, 0x7A16, 0x0AF1, 0x1AD0, 0x2AB3, 0x3A92,
    0xFD2E, 0xED0F, 0xDD6C, 0xCD4D,
    0xBDAA, 0xAD8B, 0x9DE8, 0x8DC9,
    0x7C26, 0x6C07, 0x5C64, 0x4C45,
    0x3CA2, 0x2C83, 0x1CE0, 0x0CC1, 0xEF1F, 0xFF3E,
    0xCF5D, 0xDF7C, 0xAF9B, 0xBFBA,
    0x8FD9, 0x9FF8, 0x6E17, 0x7E36,
    0x4E55, 0x5E74, 0x2E93, 0x3EB2, 0x0ED1, 0x1EF0,
];


use crate::game::client::{clif_send, clif_sendtogm};
use crate::game::client::BroadcastSrc;
use crate::game::client::visual::clif_destroyold;
use crate::game::block::{map_moveblock_id, AreaType};
use crate::game::block_grid;
use crate::game::map_server::{
    map_readglobalreg, map_id2sd_pc, map_id2mob_ref, map_id2npc_ref, object_flags,
};
use crate::game::map_parse::visual::{
    clif_charlook, clif_cnpclook, clif_cmoblook,
    clif_object_look_by_id,
    clif_mob_look_start_func_inner, clif_mob_look_close_func_inner,
    load_visible_entities, announce_to_nearby,
};
use crate::game::map_parse::player_state::{
    clif_sendxy, clif_sendstatus, clif_sendxychange,
    clif_sendmapinfo, clif_sendxynoclick, clif_getchararea,
};
use crate::game::map_parse::chat::clif_sendminitext;
use crate::game::map_parse::groups::{clif_isingroup, clif_canmove_sub_inner, clif_leavegroup};
use crate::game::pc::{
    pc_warp, pc_isequip, pc_runfloor_sub,
    FLAG_GROUP,
};
use crate::game::mob::{MobSpawnData, MOB_DEAD};
use crate::game::npc::NpcData;
use crate::game::scripting::{carray_to_str, doscript_blargs_id};
use crate::game::time_util::gettick;
use crate::network::crypt::set_packet_indexes;
use crate::database::item_db;
use crate::database::magic_db;



/// Dispatch a Lua event with a single entity ID argument.
fn sl_doscript_simple(root: &str, method: Option<&str>, id: u32) -> i32 {
    doscript_blargs_id(root, method, &[id])
}

/// Dispatch a Lua event with two entity ID arguments.
fn sl_doscript_2(root: &str, method: Option<&str>, id1: u32, id2: u32) -> i32 {
    doscript_blargs_id(root, method, &[id1, id2])
}


/// Length of a null-terminated `i8` buffer (stops at first 0 or end of slice).
#[inline]
fn cstr_len(buf: &[i8]) -> usize {
    buf.iter().position(|&b| b == 0).unwrap_or(buf.len())
}

// ─── Inline map-data helpers ────────────────────────────────────────────────

/// `read_tile(m, x, y)` — tile ID at cell (x, y) on map m.
#[inline]
unsafe fn read_tile(m: i32, x: i32, y: i32) -> u16 {
    let Some(md) = map_data(m as usize) else { return 0; };
    if md.tile.is_null() { return 0; }
    *md.tile.add(x as usize + y as usize * md.xs as usize)
}

/// `read_obj(m, x, y)` — object ID at cell (x, y) on map m.
#[inline]
unsafe fn read_obj(m: i32, x: i32, y: i32) -> u16 {
    let Some(md) = map_data(m as usize) else { return 0; };
    if md.obj.is_null() { return 0; }
    *md.obj.add(x as usize + y as usize * md.xs as usize)
}

/// `read_pass(m, x, y)` — passability value at cell (x, y) on map m.
/// Non-zero means blocked.
#[inline]
unsafe fn read_pass(m: i32, x: i32, y: i32) -> u16 {
    let Some(md) = map_data(m as usize) else { return 0; };
    if md.pass.is_null() { return 0; }
    *md.pass.add(x as usize + y as usize * md.xs as usize)
}

// ─── nexCRCC ──────────────────────────────────────────────────────────────────

/// Compute the NexCRCC checksum for a flat array of `i16` triples (tile, pass, obj).
///
/// `buf` contains N triples; C `len` was the byte count (`N * 3 * 2`).
#[inline]
fn nex_crcc(buf: &[u16]) -> u16 {
    let mut crc: u16 = 0;
    let mut i = 0usize;
    while i + 2 < buf.len() {
        crc = (CRC_TABLE[(crc >> 8) as usize] ^ (crc << 8)) ^ buf[i];
        let temp = CRC_TABLE[(crc >> 8) as usize] ^ buf[i + 1];
        crc = ((temp << 8) ^ CRC_TABLE[((crc & 0xFF) ^ (temp >> 8)) as usize])
            ^ buf[i + 2];
        i += 3;
    }
    crc
}

// ─── clif_blockmovement ──────────────────────────────────────────────────────

/// Send a movement-block (flag=0) or movement-unblock (flag=1) packet.
///
/// Packet: `WFIFOHEADER(fd, 0x51, 5)` + flag byte + two zero bytes = 8 bytes total.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_blockmovement(pe: &PlayerEntity, flag: i32) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }
    let fd = pe.fd;
    wfifohead(fd, 8);
    wfifoheader(fd, 0x51, 5);
    wfifob(fd, 5, flag as u8);
    wfifob(fd, 6, 0);
    wfifob(fd, 7, 0);
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_sendchararea ────────────────────────────────────────────────────────

/// Broadcast `pe`'s appearance to all nearby PCs.
///
/// Uses `AREA` (the full surrounding area, not just `SAMEAREA`).
pub fn clif_sendchararea(pe: &PlayerEntity) -> i32 {
    let pos = pe.position();
    let (m, x, y) = (pos.m as usize, pos.x as i32, pos.y as i32);
    if let (Some(grid), Some(slot)) = (block_grid::get_grid(m), map_data(m)) {
        let ids = block_grid::ids_in_area(grid, x, y, AreaType::Area, slot.xs as i32, slot.ys as i32);
        announce_to_nearby(pe, &ids);
    }
    0
}

// ─── clif_charspecific ────────────────────────────────────────────────────────

/// Send the appearance of player `sender` to player `id`.
///
/// Builds a 0x33 packet containing position, state, equipment look, and name.
/// Applies visibility rules (stealth, ghost, GFX override).
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_charspecific(sender: i32, id: i32) -> i32 {
    // Read locks: this function is verified read-only (builds 0x33 appearance packet).
    // .read() fixes the sender==id deadlock (re-entrant write) and enables concurrent readers.
    let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(sender as u32) else { return 0; };
    let sd: *const MapSessionData = &*sd_arc.read() as *const MapSessionData;
    let Some(src_arc) = crate::game::map_server::map_id2sd_pc(id as u32) else { return 0; };
    let src_sd: *const MapSessionData = &*src_arc.read() as *const MapSessionData;

    // Stealth: hide from non-GM viewers (except from self)
    if ((*sd).optFlags & OPT_FLAG_STEALTH) != 0
        && (*sd).id != (*src_sd).id
        && (*src_sd).player.identity.gm_level == 0
    {
        return 0;
    }

    // Ghost visibility: dead players hidden from non-ghost viewers
    if let Some(md) = map_data((*sd).m as usize) {
        if md.show_ghosts != 0
            && (*sd).player.combat.state == PC_DIE
            && (*sd).id != (*src_sd).id
            && (*src_sd).player.combat.state != PC_DIE
                && ((*src_sd).optFlags & OPT_FLAG_GHOSTS) == 0
            {
                return 0;
            }
    }

    if !session_exists((*sd).fd) {
        return 0;
    }

    let src_fd = (*src_sd).fd;
    wfifohead(src_fd, 512);
    wfifob(src_fd, 0, 0xAA);
    wfifob(src_fd, 3, 0x33);
    wfifob(src_fd, 4, 0x03);
    wfifow(src_fd, 5,  ((*sd).x).swap_bytes());
    wfifow(src_fd, 7,  ((*sd).y).swap_bytes());
    wfifob(src_fd, 9,  (*sd).player.combat.side as u8);
    wfifol(src_fd, 10, ((*sd).player.identity.id).swap_bytes());

    // Sex / disguise look at [14..15]
    if ((*sd).player.combat.state as i32) < PC_DISGUISE as i32 {
        wfifow(src_fd, 14, ((*sd).player.identity.sex as u16).swap_bytes());
    } else {
        wfifob(src_fd, 14, 1);
        wfifob(src_fd, 15, 15);
    }

    // State / invis at [16]
    let can_see_invis = (*sd).id != (*src_sd).id
        && ((*src_sd).player.identity.gm_level != 0
            || clif_isingroup(src_arc.as_ref(), sd as *mut MapSessionData) != 0
            || ((*sd).gfx.dye == (*src_sd).gfx.dye
                && (*sd).gfx.dye != 0
                && (*src_sd).gfx.dye != 0));
    let state_byte: u8 = if ((*sd).player.combat.state == PC_INVIS
        || ((*sd).optFlags & OPT_FLAG_STEALTH) != 0)
        && can_see_invis
    {
        5
    } else {
        (*sd).player.combat.state as u8
    };
    wfifob(src_fd, 16, state_byte);

    // Stealth-only override for non-GM viewers
    if ((*sd).optFlags & OPT_FLAG_STEALTH) != 0
        && (*sd).player.combat.state == 0
        && (*src_sd).player.identity.gm_level == 0
    {
        wfifob(src_fd, 16, 2);
    }

    wfifob(src_fd, 19, (*sd).speed as u8);

    // Disguise at [17..18]
    if (*sd).player.combat.state == PC_MOUNTED {
        wfifow(src_fd, 17, (*sd).disguise.swap_bytes());
    } else if (*sd).player.combat.state == PC_DISGUISE {
        wfifow(src_fd, 17, (*sd).disguise.wrapping_add(32768).swap_bytes());
        wfifob(src_fd, 19, (*sd).disguise_color as u8);
    } else {
        wfifow(src_fd, 17, 0);
    }

    wfifob(src_fd, 20, 0);
    wfifob(src_fd, 21, (*sd).player.appearance.face as u8);
    wfifob(src_fd, 22, (*sd).player.appearance.hair as u8);
    wfifob(src_fd, 23, (*sd).player.appearance.hair_color as u8);
    wfifob(src_fd, 24, (*sd).player.appearance.face_color as u8);
    wfifob(src_fd, 25, (*sd).player.appearance.skin_color as u8);

    // Armor at [26..27], color at [28]
    let armor_id = pc_isequip(sd as *mut MapSessionData,EQ_ARMOR);
    if armor_id == 0 {
        wfifow(src_fd, 26, ((*sd).player.identity.sex as u16).swap_bytes());
    } else {
        let armor_item = item_db::search(armor_id as u32);
        let armor_look = if (&(*sd).player.inventory.equip)[EQ_ARMOR as usize].custom_look != 0 {
            (&(*sd).player.inventory.equip)[EQ_ARMOR as usize].custom_look as u16
        } else {
            armor_item.look as u16
        };
        wfifow(src_fd, 26, armor_look.swap_bytes());
        let armor_color: u8 = if (*sd).player.appearance.armor_color > 0 {
            (*sd).player.appearance.armor_color as u8
        } else if (&(*sd).player.inventory.equip)[EQ_ARMOR as usize].custom_look != 0 {
            (&(*sd).player.inventory.equip)[EQ_ARMOR as usize].custom_look_color as u8
        } else {
            armor_item.look_color as u8
        };
        wfifob(src_fd, 28, armor_color);
    }
    // Coat overrides armor
    let coat_id = pc_isequip(sd as *mut MapSessionData,EQ_COAT);
    if coat_id != 0 {
        let coat_item = item_db::search(coat_id as u32);
        wfifow(src_fd, 26, (coat_item.look as u16).swap_bytes());
        wfifob(src_fd, 28, coat_item.look_color as u8);
    }

    // Weapon at [29..30], color at [31]
    let weap_id = pc_isequip(sd as *mut MapSessionData,EQ_WEAP);
    if weap_id == 0 {
        wfifow(src_fd, 29, 0xFFFFu16.swap_bytes());
        wfifob(src_fd, 31, 0);
    } else {
        let weap_item = item_db::search(weap_id as u32);
        let (wlook, wcolor) = if (&(*sd).player.inventory.equip)[EQ_WEAP as usize].custom_look != 0 {
            ((&(*sd).player.inventory.equip)[EQ_WEAP as usize].custom_look as u16,
             (&(*sd).player.inventory.equip)[EQ_WEAP as usize].custom_look_color as u8)
        } else {
            (weap_item.look as u16, weap_item.look_color as u8)
        };
        wfifow(src_fd, 29, wlook.swap_bytes());
        wfifob(src_fd, 31, wcolor);
    }

    // Shield at [32..33], color at [34]
    let shield_id = pc_isequip(sd as *mut MapSessionData,EQ_SHIELD);
    if shield_id == 0 {
        wfifow(src_fd, 32, 0xFFFFu16.swap_bytes());
        wfifob(src_fd, 34, 0);
    } else {
        let shield_item = item_db::search(shield_id as u32);
        let (slook, scolor) = if (&(*sd).player.inventory.equip)[EQ_SHIELD as usize].custom_look != 0 {
            ((&(*sd).player.inventory.equip)[EQ_SHIELD as usize].custom_look as u16,
             (&(*sd).player.inventory.equip)[EQ_SHIELD as usize].custom_look_color as u8)
        } else {
            (shield_item.look as u16, shield_item.look_color as u8)
        };
        wfifow(src_fd, 32, slook.swap_bytes());
        wfifob(src_fd, 34, scolor);
    }

    // Helm at [35] flag, [36..37] look+color
    let helm_id    = pc_isequip(sd as *mut MapSessionData,EQ_HELM);
    let helm_item  = if helm_id != 0 { Some(item_db::search(helm_id as u32)) } else { None };
    let helm_look  = helm_item.as_ref().map_or(-1, |i| i.look);
    if helm_id == 0
        || ((*sd).player.appearance.setting_flags & FLAG_HELM) == 0
        || helm_look == -1
    {
        wfifob(src_fd, 35, 0);
        wfifow(src_fd, 36, 0xFFFFu16.swap_bytes());
    } else {
        wfifob(src_fd, 35, 1);
        if (&(*sd).player.inventory.equip)[EQ_HELM as usize].custom_look != 0 {
            wfifob(src_fd, 36, (&(*sd).player.inventory.equip)[EQ_HELM as usize].custom_look as u8);
            wfifob(src_fd, 37, (&(*sd).player.inventory.equip)[EQ_HELM as usize].custom_look_color as u8);
        } else {
            wfifob(src_fd, 36, helm_look as u8);
            wfifob(src_fd, 37, helm_item.as_ref().unwrap().look_color as u8);
        }
    }

    // Face accessory at [38..39], color at [40]
    let faceacc_id = pc_isequip(sd as *mut MapSessionData,EQ_FACEACC);
    if faceacc_id == 0 {
        wfifow(src_fd, 38, 0xFFFFu16.swap_bytes());
        wfifob(src_fd, 40, 0);
    } else {
        let faceacc_item = item_db::search(faceacc_id as u32);
        wfifow(src_fd, 38, (faceacc_item.look as u16).swap_bytes());
        wfifob(src_fd, 40, faceacc_item.look_color as u8);
    }

    // Crown at [41..42], color at [43]; also clears helm flag at [35]
    let crown_id = pc_isequip(sd as *mut MapSessionData,EQ_CROWN);
    if crown_id == 0 {
        wfifow(src_fd, 41, 0xFFFFu16.swap_bytes());
        wfifob(src_fd, 43, 0);
    } else {
        wfifob(src_fd, 35, 0); // crown present → clear helm flag
        let crown_item = item_db::search(crown_id as u32);
        let (clook, ccolor) = if (&(*sd).player.inventory.equip)[EQ_CROWN as usize].custom_look != 0 {
            ((&(*sd).player.inventory.equip)[EQ_CROWN as usize].custom_look as u16,
             (&(*sd).player.inventory.equip)[EQ_CROWN as usize].custom_look_color as u8)
        } else {
            (crown_item.look as u16, crown_item.look_color as u8)
        };
        wfifow(src_fd, 41, clook.swap_bytes());
        wfifob(src_fd, 43, ccolor);
    }

    // Face accessory 2 at [44..45], color at [46]
    let faceacc2_id = pc_isequip(sd as *mut MapSessionData,EQ_FACEACCTWO);
    if faceacc2_id == 0 {
        wfifow(src_fd, 44, 0xFFFFu16.swap_bytes());
        wfifob(src_fd, 46, 0);
    } else {
        let faceacc2_item = item_db::search(faceacc2_id as u32);
        wfifow(src_fd, 44, (faceacc2_item.look as u16).swap_bytes());
        wfifob(src_fd, 46, faceacc2_item.look_color as u8);
    }

    // Mantle at [47..48], color at [49]
    let mantle_id = pc_isequip(sd as *mut MapSessionData,EQ_MANTLE);
    if mantle_id == 0 {
        wfifow(src_fd, 47, 0xFFFFu16.swap_bytes());
        wfifob(src_fd, 49, 0xFF);
    } else {
        let mantle_item = item_db::search(mantle_id as u32);
        wfifow(src_fd, 47, (mantle_item.look as u16).swap_bytes());
        wfifob(src_fd, 49, mantle_item.look_color as u8);
    }

    // Necklace at [50..51], color at [52]
    let neck_id   = pc_isequip(sd as *mut MapSessionData,EQ_NECKLACE);
    let neck_item = if neck_id != 0 { Some(item_db::search(neck_id as u32)) } else { None };
    let neck_look = neck_item.as_ref().map_or(-1, |i| i.look);
    if neck_id == 0
        || ((*sd).player.appearance.setting_flags & FLAG_NECKLACE) == 0
        || neck_look == -1
    {
        wfifow(src_fd, 50, 0xFFFFu16.swap_bytes());
        wfifob(src_fd, 52, 0);
    } else {
        wfifow(src_fd, 50, (neck_look as u16).swap_bytes());
        wfifob(src_fd, 52, neck_item.as_ref().unwrap().look_color as u8);
    }

    // Boots at [53..54], color at [55]
    let boots_id = pc_isequip(sd as *mut MapSessionData,EQ_BOOTS);
    if boots_id == 0 {
        wfifow(src_fd, 53, ((*sd).player.identity.sex as u16).swap_bytes());
        wfifob(src_fd, 55, 0);
    } else {
        let boots_item = item_db::search(boots_id as u32);
        let (blook, bcolor) = if (&(*sd).player.inventory.equip)[EQ_BOOTS as usize].custom_look != 0 {
            ((&(*sd).player.inventory.equip)[EQ_BOOTS as usize].custom_look as u16,
             (&(*sd).player.inventory.equip)[EQ_BOOTS as usize].custom_look_color as u8)
        } else {
            (boots_item.look as u16, boots_item.look_color as u8)
        };
        wfifow(src_fd, 53, blook.swap_bytes());
        wfifob(src_fd, 55, bcolor);
    }

    // Title color / name at [56..57+len]
    wfifob(src_fd, 56, 0);
    wfifob(src_fd, 57, 128);
    wfifob(src_fd, 58, 0);

    // Title color: invis/stealth GM/group/clan viewers see a special color
    let invis_or_stealth = (*sd).player.combat.state == PC_INVIS
        || ((*sd).optFlags & OPT_FLAG_STEALTH) != 0;
    if invis_or_stealth
        && (*sd).id != (*src_sd).id
        && ((*src_sd).player.identity.gm_level != 0
            || clif_isingroup(src_arc.as_ref(), sd as *mut MapSessionData) != 0
            || ((*sd).gfx.dye == (*src_sd).gfx.dye
                && (*sd).gfx.dye != 0
                && (*src_sd).gfx.dye != 0))
    {
        wfifob(src_fd, 56, 0);
    } else if (*sd).gfx.dye != 0 {
        wfifob(src_fd, 56, (*sd).gfx.title_color);
    }

    let name_ref: &str = &(*sd).player.identity.name;
    let name_src = name_ref.as_ptr();
    let name_len = name_ref.len();
    let mut len = name_len;

    // Same-clan → title color 3
    if (*src_sd).player.social.clan == (*sd).player.social.clan
        && (*src_sd).player.social.clan > 0
        && (*src_sd).player.identity.id != (*sd).player.identity.id
    {
        wfifob(src_fd, 56, 3);
    }
    // Same group → title color 2
    if clif_isingroup(src_arc.as_ref(), sd as *mut MapSessionData) != 0 {
        wfifob(src_fd, 56, 2);
    }

    // Name (only for visible states)
    if (*sd).player.combat.state != PC_INVIS && (*sd).player.combat.state != 5 {
        wfifob(src_fd, 57, len as u8);
        let dst = wfifop(src_fd, 58);
        if !dst.is_null() {
            ptr::copy_nonoverlapping(name_src, dst, len);
        }
    } else {
        wfifow(src_fd, 57, 0);
        len = 1;
    }

    // GFX override: GM gfx toggle or clone active — overwrite appearance fields
    if ((*sd).player.identity.gm_level != 0 && (*sd).gfx.toggle != 0) || (*sd).clone != 0 {
        wfifob(src_fd, 21, (*sd).gfx.face);
        wfifob(src_fd, 22, (*sd).gfx.hair);
        wfifob(src_fd, 23, (*sd).gfx.chair);
        wfifob(src_fd, 24, (*sd).gfx.cface);
        wfifob(src_fd, 25, (*sd).gfx.cskin);
        wfifow(src_fd, 26, (*sd).gfx.armor.swap_bytes());
        if (*sd).gfx.dye > 0 {
            wfifob(src_fd, 28, (*sd).gfx.dye);
        } else {
            wfifob(src_fd, 28, (*sd).gfx.carmor);
        }
        wfifow(src_fd, 29, (*sd).gfx.weapon.swap_bytes());
        wfifob(src_fd, 31, (*sd).gfx.cweapon);
        wfifow(src_fd, 32, (*sd).gfx.shield.swap_bytes());
        wfifob(src_fd, 34, (*sd).gfx.cshield);

        if (*sd).gfx.helm < 65535 {
            wfifob(src_fd, 35, 1);
        } else if (*sd).gfx.crown < 65535 {
            wfifob(src_fd, 35, 0xFF);
        } else {
            wfifob(src_fd, 35, 0);
        }

        wfifob(src_fd, 36, (*sd).gfx.helm as u8);
        wfifob(src_fd, 37, (*sd).gfx.chelm);
        wfifow(src_fd, 38, (*sd).gfx.face_acc.swap_bytes());
        wfifob(src_fd, 40, (*sd).gfx.cface_acc);
        wfifow(src_fd, 41, (*sd).gfx.crown.swap_bytes());
        wfifob(src_fd, 43, (*sd).gfx.ccrown);
        wfifow(src_fd, 44, (*sd).gfx.face_acc_t.swap_bytes());
        wfifob(src_fd, 46, (*sd).gfx.cface_acc_t);
        wfifow(src_fd, 47, (*sd).gfx.mantle.swap_bytes());
        wfifob(src_fd, 49, (*sd).gfx.cmantle);
        wfifow(src_fd, 50, (*sd).gfx.necklace.swap_bytes());
        wfifob(src_fd, 52, (*sd).gfx.cnecklace);
        wfifow(src_fd, 53, (*sd).gfx.boots.swap_bytes());
        wfifob(src_fd, 55, (*sd).gfx.cboots);

        // Override name with gfx.name (if non-empty and not invis)
        let gfx_name = (*sd).gfx.name.as_ptr();
        let gfx_name_len = cstr_len(&(*sd).gfx.name);
        if (*sd).player.combat.state != PC_INVIS && (*sd).player.combat.state != 5 && gfx_name_len > 0 {
            len = gfx_name_len;
            wfifob(src_fd, 57, len as u8);
            let dst = wfifop(src_fd, 58);
            if !dst.is_null() {
                ptr::copy_nonoverlapping(gfx_name as *const u8, dst, len);
            }
        } else {
            wfifob(src_fd, 57, 0);
            len = 1;
        }
    }

    // Packet size at [1..2] BE: len + 55
    {
        let p = wfifop(src_fd, 1) as *mut u16;
        if !p.is_null() { p.write_unaligned(((len + 55) as u16).to_be()); }
    }
    wfifoset(src_fd, encrypt(src_fd) as usize);
    0
}

// ─── clif_parsewalk ───────────────────────────────────────────────────────────

/// Handle a client walk-request packet.
///
/// Validates position match, collision, status effects, updates viewport,
/// sends walk packets, moves block, triggers area scans and scripted events,
/// and checks warp tiles.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parsewalk(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;
    let (m, sd_m, sd_x, sd_y, state, gm_level, player_id) = {
        let sd = pe.read();
        (sd.m as i32, sd.m, sd.x, sd.y, sd.player.combat.state, sd.player.identity.gm_level, sd.id)
    };
    let Some(md) = map_data(m as usize) else { return 0; };

    // Dismount on non-mount maps
    if md.can_mount == 0 && state == PC_MOUNTED && gm_level == 0 {
        sl_doscript_simple("onDismount", None, player_id);
    }

    let direction = rfifob(fd, 5);
    let xold = rfifow(fd, 8).swap_bytes() as i32;
    let yold = rfifow(fd, 10).swap_bytes() as i32;
    let mut dx = xold;
    let mut dy = yold;

    // Map-data sub-packet (packet type 6 carries viewport coords)
    let mut x0: i32 = 0;
    let mut y0: i32 = 0;
    let mut x1: i32 = 0;
    let mut y1: i32 = 0;
    let mut checksum: u16 = 0;
    if rfifob(fd, 3) == 6 {
        x0 = rfifow(fd, 12).swap_bytes() as i32;
        y0 = rfifow(fd, 14).swap_bytes() as i32;
        x1 = rfifob(fd, 16) as i32;
        y1 = rfifob(fd, 17) as i32;
        checksum = rfifow(fd, 18).swap_bytes();
    }

    // Position mismatch: snap back
    if dx != sd_x as i32 {
        clif_blockmovement(pe, 0);
        map_moveblock_id(player_id, sd_m, sd_x, sd_y, sd_x, sd_y);
        clif_sendxy(pe);
        clif_blockmovement(pe, 1);
        return 0;
    }
    if dy != sd_y as i32 {
        clif_blockmovement(pe, 0);
        map_moveblock_id(player_id, sd_m, sd_x, sd_y, sd_x, sd_y);
        clif_sendxy(pe);
        clif_blockmovement(pe, 1);
        return 0;
    }

    pe.write().canmove = 0;

    // Apply direction
    match direction {
        0 => dy -= 1,
        1 => dx += 1,
        2 => dy += 1,
        3 => dx -= 1,
        _ => {}
    }

    // Clamp to map bounds
    if dx < 0 { dx = 0; }
    if dx >= md.xs as i32 { dx = md.xs as i32 - 1; }
    if dy < 0 { dy = 0; }
    if dy >= md.ys as i32 { dy = md.ys as i32 - 1; }

    // Collision checks (GM bypasses)
    if gm_level == 0 {
        if let Some(grid) = block_grid::get_grid(m as usize) {
            let cell_ids = grid.ids_at_tile(dx as u16, dy as u16);
            for id in cell_ids {
                clif_canmove_sub_inner(id, pe);
            }
        }
        if read_pass(m, dx, dy) != 0 { pe.write().canmove = 1; }
    }

    // Status blocks movement
    let (canmove, paralyzed, sleep, snare) = {
        let sd = pe.read();
        (sd.canmove, sd.paralyzed, sd.sleep, sd.snare)
    };
    if (canmove != 0 || paralyzed != 0 || sleep != 1.0f32 || snare != 0) && gm_level == 0 {
        clif_blockmovement(pe, 0);
        clif_sendxy(pe);
        clif_blockmovement(pe, 1);
        return 0;
    }

    // Update viewport offsets
    let (vx, vy, setting_flags, opt_flags) = {
        let sd = pe.read();
        (sd.viewx as i32, sd.viewy as i32, sd.player.appearance.setting_flags, sd.optFlags)
    };
    {
        let mut sd = pe.write();
        if direction == 0 && (dy <= vy || ((md.ys as i32 - 1 - dy) < 7 && vy > 7)) {
            sd.viewy = sd.viewy.saturating_sub(1);
        }
        if direction == 1 && ((dx < 8 && vx < 8) || 16 - (md.xs as i32 - 1 - dx) <= vx) {
            sd.viewx = sd.viewx.wrapping_add(1);
        }
        if direction == 2 && ((dy < 7 && vy < 7) || 14 - (md.ys as i32 - 1 - dy) <= vy) {
            sd.viewy = sd.viewy.wrapping_add(1);
        }
        if direction == 3 && (dx <= vx || ((md.xs as i32 - 1 - dx) < 8 && vx > 8)) {
            sd.viewx = sd.viewx.saturating_sub(1);
        }
        if sd.viewx > 16 { sd.viewx = 16; }
        if sd.viewy > 14 { sd.viewy = 14; }
    }

    // Send walk-ack to self (skipped in FASTMOVE mode)
    if (setting_flags & FLAG_FASTMOVE) == 0 {
        if !session_exists(fd) {
            return 0;
        }
        let (viewx, viewy) = {
            let sd = pe.read();
            (sd.viewx, sd.viewy)
        };
        wfifohead(fd, 15);
        wfifob(fd, 0, 0xAA);
        wfifob(fd, 1, 0x00);
        wfifob(fd, 2, 0x0C);
        wfifob(fd, 3, 0x26);
        // [4] intentionally not written (C comments it out too)
        wfifob(fd, 5, direction);
        wfifow(fd, 6, (xold as u16).swap_bytes());
        wfifow(fd, 8, (yold as u16).swap_bytes());
        wfifow(fd, 10, viewx.swap_bytes());
        wfifow(fd, 12, viewy.swap_bytes());
        wfifob(fd, 14, 0x00);
        wfifoset(fd, encrypt(fd) as usize);
    }

    // No actual position change
    if dx == sd_x as i32 && dy == sd_y as i32 { return 0; }

    // Broadcast movement to area.
    let mut buf = [0u8; 32];
    buf[0] = 0xAA;
    buf[1] = 0x00;
    buf[2] = 0x0C;
    buf[3] = 0x0C;
    // [4] = 0 (C comments out this byte)
    buf[5..9].copy_from_slice(&player_id.swap_bytes().to_ne_bytes());
    buf[9..11].copy_from_slice(&(xold as u16).swap_bytes().to_ne_bytes());
    buf[11..13].copy_from_slice(&(yold as u16).swap_bytes().to_ne_bytes());
    buf[13] = direction;
    buf[14] = 0x00;

    if (opt_flags & OPT_FLAG_STEALTH) != 0 {
        clif_sendtogm(buf.as_mut_ptr(), 32, BroadcastSrc { id: player_id, m: sd_m, x: sd_x, y: sd_y, bl_type: BL_PC as u8 }, AREA_WOS);
    } else {
        clif_send(buf.as_ptr(), 32, BroadcastSrc { id: player_id, m: sd_m, x: sd_x, y: sd_y, bl_type: BL_PC as u8 }, AREA_WOS);
    }

    {
        let mut sd = pe.write();
        map_moveblock_id(player_id, sd.m, sd.x, sd.y, dx as u16, dy as u16);
        sd.x = dx as u16;
        sd.y = dy as u16;
    }
    pe.set_position(Point { m: sd_m, x: dx as u16, y: dy as u16 });

    // If client sent viewport sub-packet, scan and send new tile strip
    if rfifob(fd, 3) == 6 {
        clif_sendmapdata(pe, m, x0, y0, x1, y1, checksum);
        if let Some(grid) = block_grid::get_grid(m as usize) {
            let rect_ids = grid.ids_in_rect(x0, y0, x0 + (x1 - 1), y0 + (y1 - 1));
            {
                let mut net = pe.net.write();
                clif_mob_look_start_func_inner(fd, &mut net.look);
                for &id in &rect_ids {
                    clif_object_look_by_id(fd, &mut net.look, player_id, id);
                }
                clif_mob_look_close_func_inner(fd, &mut net.look);
            }
            load_visible_entities(pe, &rect_ids);
            announce_to_nearby(pe, &rect_ids);
        }
    }

    // Equipment walk scripts
    for i in 0..14usize {
        let eq_id = pe.read().player.inventory.equip[i].id;
        if eq_id > 0 {
            let equip_item = item_db::search(eq_id);
            let yn = carray_to_str(&equip_item.yname);
            if !yn.is_empty() {
                sl_doscript_simple(yn, Some("on_walk"), player_id);
            }
        }
    }

    // Skill passive walk scripts
    for i in 0..MAX_SPELLS {
        let skill_id = pe.read().player.spells.skills[i];
        if skill_id > 0 {
            let magic = magic_db::search(skill_id as i32);
            let yn = carray_to_str(&magic.yname);
            if !yn.is_empty() {
                sl_doscript_simple(yn, Some("on_walk_passive"), player_id);
            }
        }
    }

    // Aether walk scripts
    for i in 0..MAX_MAGIC_TIMERS {
        let (aether_id, aether_duration) = {
            let sd = pe.read();
            (sd.player.spells.dura_aether[i].id, sd.player.spells.dura_aether[i].duration)
        };
        if aether_id > 0 && aether_duration > 0 {
            let magic = magic_db::search(aether_id as i32);
            let yn = carray_to_str(&magic.yname);
            if !yn.is_empty() {
                sl_doscript_simple(yn, Some("on_walk_while_cast"), player_id);
            }
        }
    }

    sl_doscript_simple("onScriptedTile", None, player_id);
    pc_runfloor_sub(&mut *pe.write() as *mut MapSessionData);

    // Warp check
    do_warp_check(pe);
    0
}

// ─── clif_noparsewalk ─────────────────────────────────────────────────────────

/// Server-driven forced walk (no RFIFO packet from client).
///
/// Reads direction from `sd->status.side`, applies the same collision and
/// viewport logic as `clif_parsewalk`, but sends `[4]=0x03` and computes
/// the new-strip viewport coords internally.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_noparsewalk(pe: &PlayerEntity, _speed: i8) -> i32 {
    let fd = pe.fd;
    let (m_val, sd_m, sd_x, sd_y, state, gm_level, player_id, side, viewx, viewy) = {
        let sd = pe.read();
        (sd.m as i32, sd.m, sd.x, sd.y, sd.player.combat.state,
         sd.player.identity.gm_level, sd.id, sd.player.combat.side as i32,
         sd.viewx as i32, sd.viewy as i32)
    };
    let Some(md) = map_data(m_val as usize) else { return 0; };

    let xold = sd_x as i32;
    let yold = sd_y as i32;
    let mut dx = xold;
    let mut dy = yold;

    // Dismount on non-mount maps
    if md.can_mount == 0 && state == PC_MOUNTED && gm_level == 0 {
        sl_doscript_simple("onDismount", None, player_id);
    }

    let direction = side;

    // Compute destination and new viewport strip
    let (x0, y0, x1, y1): (i32, i32, i32, i32);
    match direction {
        0 => {
            dy -= 1;
            x0 = sd_x as i32 - (viewx + 1);
            y0 = dy - (viewy + 1);
            x1 = 19;
            y1 = 1;
        }
        1 => {
            dx += 1;
            x0 = dx + (18 - (viewx + 1));
            y0 = sd_y as i32 - (viewy + 1);
            x1 = 1;
            y1 = 17;
        }
        2 => {
            dy += 1;
            x0 = sd_x as i32 - (viewx + 1);
            y0 = dy + (16 - (viewy + 1));
            x1 = 19;
            y1 = 1;
        }
        3 => {
            dx -= 1;
            x0 = dx - (viewx + 1);
            y0 = sd_y as i32 - (viewy + 1);
            x1 = 1;
            y1 = 17;
        }
        _ => {
            x0 = 0; y0 = 0; x1 = 0; y1 = 0;
        }
    }

    // Clamp
    if dx < 0 { dx = 0; }
    if dx >= md.xs as i32 { dx = md.xs as i32 - 1; }
    if dy < 0 { dy = 0; }
    if dy >= md.ys as i32 { dy = md.ys as i32 - 1; }

    pe.write().canmove = 0;
    if gm_level == 0 {
        if let Some(grid) = block_grid::get_grid(m_val as usize) {
            let cell_ids = grid.ids_at_tile(dx as u16, dy as u16);
            for id in cell_ids {
                clif_canmove_sub_inner(id, pe);
            }
        }
        if read_pass(m_val, dx, dy) != 0 { pe.write().canmove = 1; }
    }

    let (canmove, paralyzed, sleep, snare) = {
        let sd = pe.read();
        (sd.canmove, sd.paralyzed, sd.sleep, sd.snare)
    };
    if (canmove != 0 || paralyzed != 0 || sleep != 1.0f32 || snare != 0) && gm_level == 0 {
        clif_blockmovement(pe, 0);
        clif_sendxy(pe);
        clif_blockmovement(pe, 1);
        return 0;
    }

    if dx == sd_x as i32 && dy == sd_y as i32 { return 0; }

    // Viewport update
    {
        let mut sd = pe.write();
        if direction == 0 && (dy <= viewy || ((md.ys as i32 - 1 - dy) < 7 && viewy > 7)) {
            sd.viewy = sd.viewy.saturating_sub(1);
        }
        if direction == 1 && ((dx < 8 && viewx < 8) || 16 - (md.xs as i32 - 1 - dx) <= viewx) {
            sd.viewx = sd.viewx.wrapping_add(1);
        }
        if direction == 2 && ((dy < 7 && viewy < 7) || 14 - (md.ys as i32 - 1 - dy) <= viewy) {
            sd.viewy = sd.viewy.wrapping_add(1);
        }
        if direction == 3 && (dx <= viewx || ((md.xs as i32 - 1 - dx) < 8 && viewx > 8)) {
            sd.viewx = sd.viewx.saturating_sub(1);
        }
        if sd.viewx > 16 { sd.viewx = 16; }
        if sd.viewy > 14 { sd.viewy = 14; }
    }

    // Temporarily toggle off FASTMOVE (noparsewalk always sends the walk packet)
    let had_fastmove = (pe.read().player.appearance.setting_flags & FLAG_FASTMOVE) != 0;
    if had_fastmove {
        pe.write().player.appearance.setting_flags ^= FLAG_FASTMOVE;
        clif_sendstatus(pe, 0);
    }

    if !session_exists(fd) {
        return 0;
    }

    let (cur_viewx, cur_viewy) = {
        let sd = pe.read();
        (sd.viewx, sd.viewy)
    };

    wfifohead(fd, 15);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 1, 0x00);
    wfifob(fd, 2, 0x0C);
    wfifob(fd, 3, 0x26);
    wfifob(fd, 4, 0x03); // noparsewalk always writes [4]=0x03
    wfifob(fd, 5, direction as u8);
    wfifow(fd, 6, (xold as u16).swap_bytes());
    wfifow(fd, 8, (yold as u16).swap_bytes());
    wfifow(fd, 10, cur_viewx.swap_bytes());
    wfifow(fd, 12, cur_viewy.swap_bytes());
    wfifob(fd, 14, 0x00);
    wfifoset(fd, encrypt(fd) as usize);

    // Restore FASTMOVE
    if had_fastmove {
        pe.write().player.appearance.setting_flags ^= FLAG_FASTMOVE;
        clif_sendstatus(pe, 0);
    }

    // Broadcast movement to area
    let opt_flags = pe.read().optFlags;
    let mut buf = [0u8; 32];
    buf[0] = 0xAA;
    buf[1] = 0x00;
    buf[2] = 0x0C;
    buf[3] = 0x0C;
    buf[5..9].copy_from_slice(&player_id.swap_bytes().to_ne_bytes());
    buf[9..11].copy_from_slice(&(xold as u16).swap_bytes().to_ne_bytes());
    buf[11..13].copy_from_slice(&(yold as u16).swap_bytes().to_ne_bytes());
    buf[13] = direction as u8;
    buf[14] = 0x00;

    if (opt_flags & OPT_FLAG_STEALTH) != 0 {
        clif_sendtogm(buf.as_mut_ptr(), 32, BroadcastSrc { id: player_id, m: sd_m, x: sd_x, y: sd_y, bl_type: BL_PC as u8 }, AREA_WOS);
    } else {
        clif_send(buf.as_ptr(), 32, BroadcastSrc { id: player_id, m: sd_m, x: sd_x, y: sd_y, bl_type: BL_PC as u8 }, AREA_WOS);
    }

    {
        let mut sd = pe.write();
        map_moveblock_id(player_id, sd.m, sd.x, sd.y, dx as u16, dy as u16);
        sd.x = dx as u16;
        sd.y = dy as u16;
    }
    pe.set_position(Point { m: sd_m, x: dx as u16, y: dy as u16 });

    // Send new viewport strip if in bounds
    if x0 >= 0 && y0 >= 0
        && x0 + (x1 - 1) < md.xs as i32
        && y0 + (y1 - 1) < md.ys as i32
    {
        clif_sendmapdata(pe, m_val, x0, y0, x1, y1, 0);
        if let Some(grid) = block_grid::get_grid(m_val as usize) {
            let rect_ids = grid.ids_in_rect(x0, y0, x0 + (x1 - 1), y0 + (y1 - 1));
            {
                let mut net = pe.net.write();
                clif_mob_look_start_func_inner(fd, &mut net.look);
                for &id in &rect_ids {
                    clif_object_look_by_id(fd, &mut net.look, player_id, id);
                }
                clif_mob_look_close_func_inner(fd, &mut net.look);
            }
            load_visible_entities(pe, &rect_ids);
            announce_to_nearby(pe, &rect_ids);
        }
    }

    sl_doscript_simple("onScriptedTile", None, player_id);
    pc_runfloor_sub(&mut *pe.write() as *mut MapSessionData);

    do_warp_check(pe);
    1
}

// ─── clif_parsewalkpong ───────────────────────────────────────────────────────

/// Handle a walk-ping pong response from the client.
///
/// Reads the timestamp at [9..12] (u32 BE → host), updates `msPing` and
/// `LastPongStamp`.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parsewalkpong(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;

    // [5..8] = HASH (unused); [9..12] = TS (u32 big-endian)
    let ts = rfifol(fd, 9).swap_bytes() as u64;

    let (last_ping_tick, last_pong_stamp) = {
        let sd = pe.read();
        (sd.LastPingTick, sd.LastPongStamp)
    };

    if last_ping_tick != 0 {
        pe.write().msPing = (gettick() as u64).wrapping_sub(last_ping_tick) as i32;
    }

    if last_pong_stamp != 0 {
        let difference = ts.wrapping_sub(last_pong_stamp) as i32;
        if difference > 43000 {
            // Speedhack detection — C commented the enforcement out; replicate no-op
        }
    }

    pe.write().LastPongStamp = ts;
    0
}

// ─── clif_parsemap ────────────────────────────────────────────────────────────

/// Handle a client map-data request.
///
/// Sets `sd->loaded = 1`, reads viewport parameters, then delegates to
/// `clif_sendmapdata`.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parsemap(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;

    pe.write().loaded = 1;

    let x0 = rfifow(fd, 5).swap_bytes() as i32;
    let y0 = rfifow(fd, 7).swap_bytes() as i32;
    let x1 = rfifob(fd, 9) as i32;
    let y1 = rfifob(fd, 10) as i32;
    let mut checksum = rfifow(fd, 11).swap_bytes();

    // Packet type 5 → force full resend (checksum=0 means always send)
    if rfifob(fd, 3) == 5 {
        checksum = 0;
    }

    let m = pe.read().m as i32;
    tracing::debug!("[map] [parsemap] fd={} m={} x0={} y0={} x1={} y1={} check={}", fd, m, x0, y0, x1, y1, checksum);
    clif_sendmapdata(pe, m, x0, y0, x1, y1, checksum);
    0
}

// ─── clif_sendmapdata ─────────────────────────────────────────────────────────

/// Send tile, passability, and object data for a viewport rectangle.
///
/// Builds the tile packet locally, computes NexCRCC checksum, and skips the
/// send if the client's cached checksum already matches.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendmapdata(
    pe: &PlayerEntity,
    m: i32,
    mut x0: i32,
    mut y0: i32,
    mut x1: i32,
    mut y1: i32,
    check: u16,
) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }

    let fd = pe.fd;
    let player_id = pe.id;

    // Blackout map: delegate to Lua
    if map_readglobalreg(m, c"blackout".as_ptr()) != 0 {
        sl_doscript_simple("sendMapData", None, player_id);
        return 0;
    }

    // Sanity: C limit is x1*y1 > 323
    if x1 * y1 > 323 {
        tracing::warn!("[map] [sendmapdata] fd={} viewport too large x1={} y1={} product={}", fd, x1, y1, x1 * y1);
        return 0;
    }

    let Some(md) = map_data(m as usize) else { return 0; };
    if x0 < 0 { x0 = 0; }
    if y0 < 0 { y0 = 0; }
    if x1 > md.xs as i32 { x1 = md.xs as i32; }
    if y1 > md.ys as i32 { y1 = md.ys as i32; }

    // CRC buffer: flat array of i16 triples (tile, pass, obj)
    // Maximum tiles = 323, so max triples = 323 × 3 = 969 i16s.
    let mut crc_buf = [0u16; 1024];
    // Packet buffer: 12 header + 323 * 6 data bytes = 1950 max; use 4096 for safety.
    let mut buf2 = [0u8; 65536];

    buf2[0] = 0xAA;
    buf2[3] = 0x06;
    buf2[4] = 0x03;
    buf2[5] = 0;
    buf2[6..8].copy_from_slice(&(x0 as u16).swap_bytes().to_ne_bytes());
    buf2[8..10].copy_from_slice(&(y0 as u16).swap_bytes().to_ne_bytes());
    buf2[10] = x1 as u8;
    buf2[11] = y1 as u8;

    let mut pos: usize = 12;
    let mut a:   usize = 0;

    for y in 0..y1 {
        if y + y0 >= md.ys as i32 { break; }
        for x in 0..x1 {
            if x + x0 >= md.xs as i32 { break; }
            let t = read_tile(m, x0 + x, y0 + y);
            let p = read_pass(m, x0 + x, y0 + y);
            let o = read_obj(m,  x0 + x, y0 + y);

            if a + 2 < crc_buf.len() {
                crc_buf[a]     = t;
                crc_buf[a + 1] = p;
                crc_buf[a + 2] = o;
            }

            buf2[pos..pos+2].copy_from_slice(&t.swap_bytes().to_ne_bytes()); pos += 2;
            buf2[pos..pos+2].copy_from_slice(&p.swap_bytes().to_ne_bytes()); pos += 2;
            buf2[pos..pos+2].copy_from_slice(&o.swap_bytes().to_ne_bytes()); pos += 2;

            a += 3;
        }
    }

    let checksum = nex_crcc(&crc_buf[..a]);

    if pos <= 12 {
        tracing::warn!("[map] [sendmapdata] fd={} no tiles written pos={}", fd, pos);
        return 0;
    }
    if checksum == check {
        tracing::debug!("[map] [sendmapdata] fd={} checksum match={} skip send", fd, checksum);
        return 0;
    }
    tracing::debug!("[map] [sendmapdata] fd={} sending {} bytes computed_check={} client_check={}", fd, pos, checksum, check);

    // Write big-endian packet size at [1..2]
    buf2[1..3].copy_from_slice(&((pos - 3) as u16).swap_bytes().to_ne_bytes());

    wfifohead(fd, 65535);
    {
        let dst = wfifop(fd, 0);
        if !dst.is_null() {
            ptr::copy_nonoverlapping(buf2.as_ptr(), dst, pos);
        }
    }
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_sendside ────────────────────────────────────────────────────────────

/// Broadcast a facing-direction change for any block_list entity.
///
/// Packet layout (11 bytes in 32-byte buf):
///   [0..2]  = 0xAA 0x00 0x08
///   [3]     = 0x11
///   [5..8]  = BE u32 bl->id
///   [9]     = side byte
///   [10]    = 0
///
/// PC: sent to AREA (including self). MOB/NPC: AREA_WOS.
///
/// Send a facing-direction packet for a PC.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendside_pc(sd: &MapSessionData) -> i32 {
    let mut buf = [0u8; 32];
    buf[0] = 0xAA;
    buf[1] = 0x00;
    buf[2] = 0x08;
    buf[3] = 0x11;
    buf[5..9].copy_from_slice(&sd.id.to_be_bytes());
    buf[9]  = sd.player.combat.side as u8;
    buf[10] = 0;

    clif_send(buf.as_ptr(), 32, BroadcastSrc { id: sd.id, m: sd.m, x: sd.x, y: sd.y, bl_type: BL_PC as u8 }, AREA);
    0
}

/// Send a facing-direction packet for a mob.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendside_mob(mob: &MobSpawnData) -> i32 {
    let mut buf = [0u8; 32];
    buf[0] = 0xAA;
    buf[1] = 0x00;
    buf[2] = 0x08;
    buf[3] = 0x11;
    buf[5..9].copy_from_slice(&mob.id.to_be_bytes());
    buf[9]  = mob.side as u8;
    buf[10] = 0;

    clif_send(buf.as_ptr(), 32, BroadcastSrc { id: mob.id, m: mob.m, x: mob.x, y: mob.y, bl_type: BL_MOB as u8 }, AREA_WOS);
    0
}

/// Send a facing-direction packet for an NPC.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendside_npc(npc: &NpcData) -> i32 {
    let mut buf = [0u8; 32];
    buf[0] = 0xAA;
    buf[1] = 0x00;
    buf[2] = 0x08;
    buf[3] = 0x11;
    buf[5..9].copy_from_slice(&npc.id.to_be_bytes());
    buf[9]  = npc.side as u8;
    buf[10] = 0;

    clif_send(buf.as_ptr(), 32, BroadcastSrc { id: npc.id, m: npc.m, x: npc.x, y: npc.y, bl_type: BL_NPC as u8 }, AREA_WOS);
    0
}

// ─── clif_parseside ───────────────────────────────────────────────────────────

/// Handle a client facing-direction change.
///
/// Reads new side from RFIFO[5], broadcasts via `clif_sendside`, fires
/// `onTurn` Lua event.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parseside(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;
    pe.write().player.combat.side = rfifob(fd, 5) as i8;
    {
        let sd = pe.read();
        clif_sendside_pc(&sd);
    }
    sl_doscript_simple("onTurn", None, pe.id);
    0
}

// ─── Private: warp check ─────────────────────────────────────────────────────

/// Check whether the player's current position has a warp tile, and if so
/// validate entry requirements and call `pc_warp`.
///
/// Shared by both `clif_parsewalk` and `clif_noparsewalk`.
#[inline]
unsafe fn do_warp_check(pe: &crate::game::player::entity::PlayerEntity) {
    let sd_ptr = &mut *pe.write() as *mut MapSessionData;
    let sd = &mut *sd_ptr;
    let fm = sd.m as i32;
    let Some(fmd) = map_data(fm as usize) else { return; };

    let mut fx = sd.x as i32;
    let mut fy = sd.y as i32;
    if fx >= fmd.xs as i32 { fx = fmd.xs as i32 - 1; }
    if fy >= fmd.ys as i32 { fy = fmd.ys as i32 - 1; }

    if fmd.warp.is_null() { return; }

    let bidx = fx as usize / BLOCK_SIZE + (fy as usize / BLOCK_SIZE) * fmd.bxs as usize;
    let mut wp: *mut WarpList = fmd.warp.add(bidx).read();
    let mut zm: i32 = 0;
    let mut zx: i32 = 0;
    let mut zy: i32 = 0;
    while !wp.is_null() {
        if (*wp).x == fx && (*wp).y == fy {
            zm = (*wp).tm;
            zx = (*wp).tx;
            zy = (*wp).ty;
            break;
        }
        wp = (*wp).next;
    }

    if zx == 0 && zy == 0 && zm == 0 { return; }

    let Some(zmd) = map_data(zm as usize) else { return; };

    // Level / vita / mana / mark / path minimum requirements
    let below_min = (sd.player.progression.level as u32) < zmd.reqlvl
        || (sd.player.combat.max_hp < zmd.reqvita && sd.player.combat.max_mp < zmd.reqmana)
        || sd.player.progression.mark < zmd.reqmark
        || (zmd.reqpath > 0 && sd.player.progression.class != zmd.reqpath);

    if below_min && sd.player.identity.gm_level == 0 {
        clif_pushback(sd);
        let maprejectmsg = zmd.maprejectmsg.as_ptr();
        if *maprejectmsg == 0 {
            let lvl_diff = (zmd.reqlvl as i32 - sd.player.progression.level as i32).unsigned_abs();
            let msg: &std::ffi::CStr = if lvl_diff >= 10 {
                c"Nightmarish visions of your own death repel you."
            } else if lvl_diff >= 5 {
                c"You're not quite ready to enter yet."
            } else if sd.player.progression.mark < zmd.reqmark {
                c"You do not understand the secrets to enter."
            } else if zmd.reqpath > 0 && sd.player.progression.class != zmd.reqpath {
                c"Your path forbids it."
            } else {
                c"A powerful force repels you."
            };
            clif_sendminitext(pe, msg.as_ptr());
        } else {
            clif_sendminitext(pe, maprejectmsg);
        }
        return;
    }

    // Level / vita / mana maximum requirements
    let above_max = (sd.player.progression.level as u32) > zmd.lvlmax
        || (sd.player.combat.max_hp > zmd.vitamax && sd.player.combat.max_mp > zmd.manamax);

    if above_max && sd.player.identity.gm_level == 0 {
        clif_pushback(sd);
        clif_sendminitext(pe, c"A magical barrier prevents you from entering.".as_ptr());
        return;
    }

    pc_warp(sd_ptr, zm, zx, zy);
}

// ─── Object collision flag queries ───────────────────────────────────────────
//
// OBJ_* bits:
const OBJ_UP:    u8 = 1;
const OBJ_DOWN:  u8 = 2;
const OBJ_RIGHT: u8 = 4;
const OBJ_LEFT:  u8 = 8;

/// Return non-zero if the object at `(m, x, y)` blocks movement in `side` direction.
/// `side`: 0=up, 1=right, 2=down, 3=left.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_object_canmove(m: i32, x: i32, y: i32, side: i32) -> i32 {
    let object = read_obj(m, x, y) as usize;
    let Some(flags) = object_flags() else { return 0; };
    let flag = flags.get(object).copied().unwrap_or(0);
    match side {
        0 => if flag & OBJ_UP    != 0 { 1 } else { 0 },
        1 => if flag & OBJ_RIGHT != 0 { 1 } else { 0 },
        2 => if flag & OBJ_DOWN  != 0 { 1 } else { 0 },
        3 => if flag & OBJ_LEFT  != 0 { 1 } else { 0 },
        _ => 0,
    }
}

/// Return non-zero if movement is blocked when *leaving* `(m, x, y)` in `side` direction.
/// Uses the reverse-direction flag (leaving down = OBJ_UP on the destination side).
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_object_canmove_from(m: i32, x: i32, y: i32, side: i32) -> i32 {
    let object = read_obj(m, x, y) as usize;
    let Some(flags) = object_flags() else { return 0; };
    let flag = flags.get(object).copied().unwrap_or(0);
    match side {
        0 => if flag & OBJ_DOWN  != 0 { 1 } else { 0 },
        1 => if flag & OBJ_LEFT  != 0 { 1 } else { 0 },
        2 => if flag & OBJ_UP    != 0 { 1 } else { 0 },
        3 => if flag & OBJ_RIGHT != 0 { 1 } else { 0 },
        _ => 0,
    }
}

/// Push player back 2 tiles opposite their facing direction.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_pushback(sd: &mut MapSessionData) -> i32 {
    let sd_ptr = sd as *mut MapSessionData;
    let m = sd.m as i32;
    let x = sd.x as i32;
    let y = sd.y as i32;
    match sd.player.combat.side {
        0 => { pc_warp(sd_ptr, m, x,     y + 2); }
        1 => { pc_warp(sd_ptr, m, x - 2, y    ); }
        2 => { pc_warp(sd_ptr, m, x,     y - 2); }
        3 => { pc_warp(sd_ptr, m, x + 2, y    ); }
        _ => {}
    }
    0
}

/// Respond to a client viewport scroll: update position delta and refresh visible objects.
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
pub unsafe fn clif_parseviewchange(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;
    let player_id = pe.id;
    let direction = rfifob(fd, 5) as i32;
    let mut dx = rfifob(fd, 6) as i32;
    let mut dy = rfifob(fd, 7) as i32;
    let x0 = u16::from_be_bytes([
        rfifob(fd, 8),
        rfifob(fd, 9),
    ]) as i32;
    let y0 = u16::from_be_bytes([
        rfifob(fd, 10),
        rfifob(fd, 11),
    ]) as i32;
    let x1 = rfifob(fd, 12) as i32;
    let y1 = rfifob(fd, 13) as i32;

    if pe.read().player.combat.state == 3 {
        clif_sendminitext(pe, c"You cannot do that while riding a mount.".as_ptr());
        return 0;
    }

    match direction {
        0 => dy += 1,
        1 => dx -= 1,
        2 => dy -= 1,
        3 => dx += 1,
        _ => {}
    }

    clif_sendxychange(pe, dx, dy);
    let m2 = pe.read().m as i32;
    if let Some(grid) = block_grid::get_grid(m2 as usize) {
        let rect_ids = grid.ids_in_rect(x0, y0, x0 + (x1 - 1), y0 + (y1 - 1));
        {
            let mut net = pe.net.write();
            clif_mob_look_start_func_inner(fd, &mut net.look);
            for &id in &rect_ids {
                clif_object_look_by_id(fd, &mut net.look, player_id, id);
            }
            clif_mob_look_close_func_inner(fd, &mut net.look);
        }
        let sd_guard = pe.read();
        let sd_ref = &*sd_guard;
        for &id in &rect_ids {
            if let Some(other_player) = map_id2sd_pc(id) {
                clif_charlook(&other_player, pe);   // pe sees other_player
                clif_charlook(pe, &other_player);   // other_player sees pe
            } else if let Some(npc) = map_id2npc_ref(id) {
                clif_cnpclook(&npc.read(), pe);
            } else if let Some(mob) = map_id2mob_ref(id) {
                clif_cmoblook(&mob.read(), pe);
            }
        }
    }
    0
}

// ─── Look-at handlers ────────────────────────────────────────────────────────
//


///
/// Fires the "onLook" Lua event when player looks at a cell.
/// Args: `entity_id` = the object being looked at, `pe` = the looking player.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parselookat_sub_inner(entity_id: u32, pe: &PlayerEntity) -> i32 {
    sl_doscript_2("onLook", None, pe.id, entity_id);
    0
}

/// Dead code stub — body was removed in original C.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parselookat_scriptsub(
    _pe: &PlayerEntity,
) -> i32 {
    0
}

/// Look at the cell directly ahead of the player (based on `side`).
///
/// # Safety
/// `pe` must reference a valid, initialized [`PlayerEntity`].
pub unsafe fn clif_parselookat_2(pe: &PlayerEntity) -> i32 {
    let (x, y, side, m) = {
        let sd = pe.read();
        (sd.x as i32, sd.y as i32, sd.player.combat.side, sd.m as i32)
    };
    let player_id = pe.id;
    let mut dx = x;
    let mut dy = y;
    match side {
        0 => dy -= 1,
        1 => dx += 1,
        2 => dy += 1,
        3 => dx -= 1,
        _ => {}
    }
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let cell_ids = grid.ids_at_tile(dx as u16, dy as u16);
        for id in cell_ids {
            sl_doscript_2("onLook", None, player_id, id);
        }
    }
    0
}

/// Look at a specific map cell (coordinates from packet bytes 5–8).
///
/// # Safety
/// `pe` must reference a valid, initialized [`PlayerEntity`].
pub unsafe fn clif_parselookat(pe: &PlayerEntity) -> i32 {
    let fd = pe.fd;
    let player_id = pe.id;
    let x = u16::from_be_bytes([
        rfifob(fd, 5),
        rfifob(fd, 6),
    ]) as i32;
    let y = u16::from_be_bytes([
        rfifob(fd, 7),
        rfifob(fd, 8),
    ]) as i32;
    let m = pe.read().m as i32;
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let cell_ids = grid.ids_at_tile(x as u16, y as u16);
        for id in cell_ids {
            sl_doscript_2("onLook", None, player_id, id);
        }
    }
    0
}

// ─── clif_refreshnoclick ─────────────────────────────────────────────────────
//

/// Resync the client's view (areas, chars, objects) after a non-click teleport.
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
pub unsafe fn clif_refreshnoclick(pe: &PlayerEntity) -> i32 {
    clif_sendmapinfo(pe);
    clif_sendxynoclick(pe);
    {
        let (m, x, y, player_id) = {
            let sd = pe.read();
            (sd.m as usize, sd.x as i32, sd.y as i32, sd.player.identity.id)
        };
        let fd = pe.fd;
        let mut net = pe.net.write();
        clif_mob_look_start_func_inner(fd, &mut net.look);
        if let (Some(grid), Some(slot)) = (block_grid::get_grid(m), map_data(m)) {
            let ids = block_grid::ids_in_area(grid, x, y, AreaType::SameArea, slot.xs as i32, slot.ys as i32);
            for id in ids {
                clif_object_look_by_id(fd, &mut net.look, player_id, id);
            }
        }
        clif_mob_look_close_func_inner(fd, &mut net.look);
    }
    clif_destroyold(pe);
    clif_sendchararea(pe);
    clif_getchararea(pe);

    if !session_exists(pe.fd) {
        return 0;
    }

    let fd = pe.fd;
    // Send 0x22/0x03 packet: 5-byte payload + 3 index bytes = 8 committed
    wfifohead(fd, 8);
    let w = |off: usize| wfifop(fd, off);
    *w(0) = 0xAA;
    *w(1) = 0x00;
    *w(2) = 0x02;  // payload length = 2
    *w(3) = 0x22;
    *w(4) = 0x03;
    let buf = std::slice::from_raw_parts_mut(wfifop(fd, 0), 8);
    let n = set_packet_indexes(buf);  // appends 3 index bytes, updates [1-2]
    wfifoset(fd, n);

    let m = pe.read().m as usize;
    let Some(md) = map_data(m) else { return 0; };
    if md.can_group == 0 {
        pe.write().player.appearance.setting_flags ^= FLAG_GROUP;
        let (sf, group_count) = {
            let sd = pe.read();
            (sd.player.appearance.setting_flags, sd.group_count)
        };
        if sf & FLAG_GROUP == 0 && group_count > 0 {
            clif_leavegroup(pe);
            clif_sendstatus(pe, 0);
            clif_sendminitext(pe, c"Join a group     :OFF".as_ptr());
        }
    }
    0
}

// ─── clif_npc_move_inner ─────────────────────────────────────────────────────


///
/// Broadcast an NPC position packet to nearby players.
/// Builds a 32-byte buffer and calls `clif_send(buf, 32, BroadcastSrc { id: ..., m: AREA_WOS)`.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_npc_move_inner(nd: *const NpcData) -> i32 {
    if nd.is_null() { return 0; }

    let mut buf = [0u8; 32];
    buf[0] = 0xAA;
    buf[1] = 0x00;
    buf[2] = 0x0C;
    buf[3] = 0x0C;
    buf[5..9].copy_from_slice(&(*nd).id.to_be_bytes());
    buf[9..11].copy_from_slice(&(*nd).prev_x.to_be_bytes());
    buf[11..13].copy_from_slice(&(*nd).prev_y.to_be_bytes());
    buf[13] = (*nd).side as u8;
    // buf[14] = 0x00 (already zeroed)
    // SAFETY: clif_send only reads bl for area broadcast
    clif_send(buf.as_ptr(), 32, BroadcastSrc { id: (*nd).id, m: (*nd).m, x: (*nd).x, y: (*nd).y, bl_type: BL_NPC as u8 }, AREA_WOS);
    0
}

// ─── clif_mob_move_inner ──────────────────────────────────────────────────────


///
/// Send a mob-position packet to a player.
/// `pe` is the viewing player, `mob` is the mob to render.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_mob_move_inner(pe: &PlayerEntity, mob: *const MobSpawnData) -> i32 {
    if mob.is_null() { return 0; }
    if (*mob).state == MOB_DEAD { return 0; }
    if !session_exists(pe.fd) {
        return 0;
    }
    let fd = pe.fd;
    wfifoheader(fd, 0x0C, 11);
    // WFIFOL(fd, 5) = SWAP32(mob->bl.id)
    let pw = |off: usize| wfifop(fd, off);
    (pw(5) as *mut u32).write_unaligned((*mob).id.to_be());
    (pw(9) as *mut u16).write_unaligned((*mob).prev_x.to_be());
    (pw(11) as *mut u16).write_unaligned((*mob).prev_y.to_be());
    *pw(13) = (*mob).side as u8;
    wfifoset(fd, encrypt(fd) as usize);
    0
}
