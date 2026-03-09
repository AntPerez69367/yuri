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

use crate::database::map_db::{BlockList, WarpList, BLOCK_SIZE};
use crate::database::map_db::raw_map_ptr;
use crate::session::{rust_session_exists, rust_session_set_eof, rust_session_wdata_ptr};
use crate::game::pc::{
    MapSessionData,
    BL_PC, BL_MOB, BL_NPC,
    OPT_FLAG_STEALTH, OPT_FLAG_GHOSTS,
    FLAG_FASTMOVE, FLAG_HELM, FLAG_NECKLACE,
};
use crate::servers::char::charstatus::{MAX_SPELLS, MAX_MAGIC_TIMERS};

use super::packet::{
    encrypt, rfifob, rfifol, rfifow,
    wfifob, wfifohead, wfifol, wfifoset, wfifow,
    wfifoheader,
    AREA, AREA_WOS,
};

// ─── Constants ────────────────────────────────────────────────────────────────

// enum { LOOK_GET = 0, LOOK_SEND = 1 } from map_parse.h
const LOOK_GET:  i32 = 0;
const LOOK_SEND: i32 = 1;

// Equipment slot indices (from map_server.h EQ_* enum)
const EQ_ARMOR:      i32 = 0;
const EQ_COAT:       i32 = 1;
const EQ_WEAP:       i32 = 2;
const EQ_SHIELD:     i32 = 3;
const EQ_HELM:       i32 = 4;
const EQ_FACEACC:    i32 = 5;
const EQ_CROWN:      i32 = 6;
const EQ_FACEACCTWO: i32 = 7;
const EQ_MANTLE:     i32 = 8;
const EQ_NECKLACE:   i32 = 9;
const EQ_BOOTS:      i32 = 10;

// PC state values (from map_server.h enum)
const PC_DIE:      i8 = 1;
const PC_INVIS:    i8 = 2;
const PC_MOUNTED:  i8 = 3;
const PC_DISGUISE: i8 = 4;

// BL_ALL — all block-list types
const BL_ALL: i32 = 0x0F;

// CRC lookup table (from `static short crctable[256]` in map_parse.c)
// Values stored as i16 to match C `short` type exactly.
#[allow(clippy::unreadable_literal)]
static CRC_TABLE: [i16; 256] = [
    0x0000, 0x1021, 0x2042, 0x3063, 0x4084, 0x50A5u16 as i16,
    0x60C6u16 as i16, 0x70E7u16 as i16, 0x8108u16 as i16, 0x9129u16 as i16,
    0xA14Au16 as i16, 0xB16Bu16 as i16, 0xC18Cu16 as i16, 0xD1ADu16 as i16,
    0xE1CEu16 as i16, 0xF1EFu16 as i16, 0x1231, 0x0210, 0x3273, 0x2252,
    0x52B5u16 as i16, 0x4294u16 as i16, 0x72F7u16 as i16, 0x62D6u16 as i16,
    0x9339u16 as i16, 0x8318u16 as i16, 0xB37Bu16 as i16, 0xA35Au16 as i16,
    0xD3BDu16 as i16, 0xC39Cu16 as i16, 0xF3FFu16 as i16, 0xE3DEu16 as i16,
    0x2462, 0x3443, 0x0420, 0x1401, 0x64E6u16 as i16, 0x74C7u16 as i16,
    0x44A4u16 as i16, 0x5485u16 as i16, 0xA56Au16 as i16, 0xB54Bu16 as i16,
    0x8528u16 as i16, 0x9509u16 as i16, 0xE5EEu16 as i16, 0xF5CFu16 as i16,
    0xC5ACu16 as i16, 0xD58Du16 as i16, 0x3653, 0x2672, 0x1611, 0x0630,
    0x76D7u16 as i16, 0x66F6u16 as i16, 0x5695u16 as i16, 0x46B4u16 as i16,
    0xB75Bu16 as i16, 0xA77Au16 as i16, 0x9719u16 as i16, 0x8738u16 as i16,
    0xF7DFu16 as i16, 0xE7FEu16 as i16, 0xD79Du16 as i16, 0xC7BCu16 as i16,
    0x48C4u16 as i16, 0x58E5u16 as i16, 0x6886u16 as i16, 0x78A7u16 as i16,
    0x0840, 0x1861, 0x2802, 0x3823, 0xC9CCu16 as i16, 0xD9EDu16 as i16,
    0xE98Eu16 as i16, 0xF9AFu16 as i16, 0x8948u16 as i16, 0x9969u16 as i16,
    0xA90Au16 as i16, 0xB92Bu16 as i16, 0x5AF5u16 as i16, 0x4AD4u16 as i16,
    0x7AB7u16 as i16, 0x6A96u16 as i16, 0x1A71, 0x0A50, 0x3A33, 0x2A12,
    0xDBFDu16 as i16, 0xCBDCu16 as i16, 0xFBBFu16 as i16, 0xEB9Eu16 as i16,
    0x9B79u16 as i16, 0x8B58u16 as i16, 0xBB3Bu16 as i16, 0xAB1Au16 as i16,
    0x6CA6u16 as i16, 0x7C87u16 as i16, 0x4CE4u16 as i16, 0x5CC5u16 as i16,
    0x2C22, 0x3C03, 0x0C60, 0x1C41, 0xEDAEu16 as i16, 0xFD8Fu16 as i16,
    0xCDECu16 as i16, 0xDDCDu16 as i16, 0xAD2Au16 as i16, 0xBD0Bu16 as i16,
    0x8D68u16 as i16, 0x9D49u16 as i16, 0x7E97u16 as i16, 0x6EB6u16 as i16,
    0x5ED5u16 as i16, 0x4EF4u16 as i16, 0x3E13, 0x2E32, 0x1E51, 0x0E70,
    0xFF9Fu16 as i16, 0xEFBEu16 as i16, 0xDFDDu16 as i16, 0xCFFCu16 as i16,
    0xBF1Bu16 as i16, 0xAF3Au16 as i16, 0x9F59u16 as i16, 0x8F78u16 as i16,
    0x9188u16 as i16, 0x81A9u16 as i16, 0xB1CAu16 as i16, 0xA1EBu16 as i16,
    0xD10Cu16 as i16, 0xC12Du16 as i16, 0xF14Eu16 as i16, 0xE16Fu16 as i16,
    0x1080, 0x00A1, 0x30C2, 0x20E3, 0x5004u16 as i16, 0x4025,
    0x7046u16 as i16, 0x6067u16 as i16, 0x83B9u16 as i16, 0x9398u16 as i16,
    0xA3FBu16 as i16, 0xB3DAu16 as i16, 0xC33Du16 as i16, 0xD31Cu16 as i16,
    0xE37Fu16 as i16, 0xF35Eu16 as i16, 0x02B1, 0x1290, 0x22F3, 0x32D2,
    0x4235u16 as i16, 0x5214u16 as i16, 0x6277u16 as i16, 0x7256u16 as i16,
    0xB5EAu16 as i16, 0xA5CBu16 as i16, 0x95A8u16 as i16, 0x8589u16 as i16,
    0xF56Eu16 as i16, 0xE54Fu16 as i16, 0xD52Cu16 as i16, 0xC50Du16 as i16,
    0x34E2, 0x24C3, 0x14A0, 0x0481, 0x7466u16 as i16, 0x6447u16 as i16,
    0x5424u16 as i16, 0x4405u16 as i16, 0xA7DBu16 as i16, 0xB7FAu16 as i16,
    0x8799u16 as i16, 0x97B8u16 as i16, 0xE75Fu16 as i16, 0xF77Eu16 as i16,
    0xC71Du16 as i16, 0xD73Cu16 as i16, 0x26D3, 0x36F2, 0x0691, 0x16B0,
    0x6657u16 as i16, 0x7676u16 as i16, 0x4615u16 as i16, 0x5634u16 as i16,
    0xD94Cu16 as i16, 0xC96Du16 as i16, 0xF90Eu16 as i16, 0xE92Fu16 as i16,
    0x99C8u16 as i16, 0x89E9u16 as i16, 0xB98Au16 as i16, 0xA9ABu16 as i16,
    0x5844u16 as i16, 0x4865u16 as i16, 0x7806u16 as i16, 0x6827u16 as i16,
    0x18C0, 0x08E1, 0x3882, 0x28A3, 0xCB7Du16 as i16, 0xDB5Cu16 as i16,
    0xEB3Fu16 as i16, 0xFB1Eu16 as i16, 0x8BF9u16 as i16, 0x9BD8u16 as i16,
    0xABBBu16 as i16, 0xBB9Au16 as i16, 0x4A75u16 as i16, 0x5A54u16 as i16,
    0x6A37u16 as i16, 0x7A16u16 as i16, 0x0AF1, 0x1AD0, 0x2AB3, 0x3A92,
    0xFD2Eu16 as i16, 0xED0Fu16 as i16, 0xDD6Cu16 as i16, 0xCD4Du16 as i16,
    0xBDAAu16 as i16, 0xAD8Bu16 as i16, 0x9DE8u16 as i16, 0x8DC9u16 as i16,
    0x7C26u16 as i16, 0x6C07u16 as i16, 0x5C64u16 as i16, 0x4C45u16 as i16,
    0x3CA2, 0x2C83, 0x1CE0, 0x0CC1, 0xEF1Fu16 as i16, 0xFF3Eu16 as i16,
    0xCF5Du16 as i16, 0xDF7Cu16 as i16, 0xAF9Bu16 as i16, 0xBFBAu16 as i16,
    0x8FD9u16 as i16, 0x9FF8u16 as i16, 0x6E17u16 as i16, 0x7E36u16 as i16,
    0x4E55u16 as i16, 0x5E74u16 as i16, 0x2E93, 0x3EB2, 0x0ED1, 0x1EF0,
];


use crate::game::client::{clif_send, clif_sendtogm};
use crate::game::block::{map_moveblock, foreach_in_area, foreach_in_cell, foreach_in_rect, AreaType};
use crate::game::map_server::map_readglobalreg;
use crate::game::map_parse::visual::{
    clif_mob_look_start, clif_mob_look_close,
    clif_charlook_inner, clif_cnpclook_inner, clif_cmoblook_inner,
    clif_object_look_sub_inner,
};
use crate::game::map_parse::player_state::{clif_sendxy, clif_sendstatus};
use crate::game::map_parse::chat::clif_sendminitext;
use crate::game::pc::{rust_pc_warp_sync as pc_warp, rust_pc_isequip as pc_isequip};
use crate::game::map_parse::groups::{clif_isingroup, clif_canmove_sub_inner};
use crate::game::time_util::gettick;
use crate::database::item_db::{
    rust_itemdb_look as itemdb_look, rust_itemdb_lookcolor as itemdb_lookcolor,
    rust_itemdb_yname as itemdb_yname,
};
use crate::database::magic_db::rust_magicdb_yname as magicdb_yname;

// map_id2sd returns *mut std::ffi::c_void but movement.rs uses it as *mut MapSessionData — wrap.
unsafe fn map_id2sd(id: u32) -> *mut MapSessionData {
    crate::game::map_server::map_id2sd(id) as *mut MapSessionData
}

/// Dispatch a Lua event with a single block_list argument.
#[cfg(not(test))]
#[allow(dead_code)]
unsafe fn sl_doscript_simple(root: *const i8, method: *const i8, bl: *mut crate::database::map_db::BlockList) -> i32 {
    crate::game::scripting::doscript_blargs(root, method, &[bl as *mut _])
}

/// Dispatch a Lua event with two block_list arguments.
#[cfg(not(test))]
#[allow(dead_code)]
unsafe fn sl_doscript_2(root: *const i8, method: *const i8, bl1: *mut crate::database::map_db::BlockList, bl2: *mut crate::database::map_db::BlockList) -> i32 {
    crate::game::scripting::doscript_blargs(root, method, &[bl1 as *mut _, bl2 as *mut _])
}


// ─── Inline map-data helpers ────────────────────────────────────────────────

/// `read_tile(m, x, y)` — tile ID at cell (x, y) on map m.
#[inline]
unsafe fn read_tile(m: i32, x: i32, y: i32) -> u16 {
    let md = &*raw_map_ptr().add(m as usize);
    if md.tile.is_null() { return 0; }
    *md.tile.add(x as usize + y as usize * md.xs as usize)
}

/// `read_obj(m, x, y)` — object ID at cell (x, y) on map m.
#[inline]
unsafe fn read_obj(m: i32, x: i32, y: i32) -> u16 {
    let md = &*raw_map_ptr().add(m as usize);
    if md.obj.is_null() { return 0; }
    *md.obj.add(x as usize + y as usize * md.xs as usize)
}

/// `read_pass(m, x, y)` — passability value at cell (x, y) on map m.
/// Non-zero means blocked.
#[inline]
unsafe fn read_pass(m: i32, x: i32, y: i32) -> u16 {
    let md = &*raw_map_ptr().add(m as usize);
    if md.pass.is_null() { return 0; }
    *md.pass.add(x as usize + y as usize * md.xs as usize)
}

// ─── nexCRCC ──────────────────────────────────────────────────────────────────

/// Compute the NexCRCC checksum for a flat array of `i16` triples (tile, pass, obj).
///
/// `buf` contains N triples; C `len` was the byte count (`N * 3 * 2`).
#[inline]
fn nex_crcc(buf: &[i16]) -> i16 {
    let mut crc: u16 = 0;
    let mut i = 0usize;
    while i + 2 < buf.len() {
        crc = (CRC_TABLE[(crc >> 8) as usize] as u16 ^ (crc << 8)) ^ buf[i] as u16;
        let temp = CRC_TABLE[(crc >> 8) as usize] as u16 ^ buf[i + 1] as u16;
        crc = ((temp << 8) ^ CRC_TABLE[((crc & 0xFF) ^ (temp >> 8)) as usize] as u16)
            ^ buf[i + 2] as u16;
        i += 3;
    }
    crc as i16
}

// ─── clif_blockmovement ──────────────────────────────────────────────────────

/// Send a movement-block (flag=0) or movement-unblock (flag=1) packet.
///
/// Packet: `WFIFOHEADER(fd, 0x51, 5)` + flag byte + two zero bytes = 8 bytes total.
///
pub unsafe fn clif_blockmovement(sd: *mut MapSessionData, flag: i32) -> i32 {
    if sd.is_null() { return 0; }
    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }
    let fd = (*sd).fd;
    wfifohead(fd, 8);
    wfifoheader(fd, 0x51, 5);
    wfifob(fd, 5, flag as u8);
    wfifob(fd, 6, 0);
    wfifob(fd, 7, 0);
    wfifoset(fd, encrypt(fd) as usize);
    0
}

// ─── clif_sendchararea ────────────────────────────────────────────────────────

/// Broadcast all nearby PCs to `sd` (LOOK_SEND direction).
///
/// Uses `AREA` (the full surrounding area, not just `SAMEAREA`).
///
pub unsafe fn clif_sendchararea(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }
    foreach_in_area(
        (*sd).bl.m as i32, (*sd).bl.x as i32, (*sd).bl.y as i32,
        AreaType::Area, BL_PC,
        |bl| clif_charlook_inner(bl, LOOK_SEND, sd),
    );
    0
}

// ─── clif_charspecific ────────────────────────────────────────────────────────

/// Send the appearance of player `sender` to player `id`.
///
/// Builds a 0x33 packet containing position, state, equipment look, and name.
/// Applies visibility rules (stealth, ghost, GFX override).
///
pub unsafe fn clif_charspecific(sender: i32, id: i32) -> i32 {
    let sd = map_id2sd(sender as u32);
    if sd.is_null() { return 0; }
    let src_sd = map_id2sd(id as u32);
    if src_sd.is_null() { return 0; }

    // Stealth: hide from non-GM viewers (except from self)
    if ((*sd).optFlags & OPT_FLAG_STEALTH) != 0
        && (*sd).bl.id != (*src_sd).bl.id
        && (*src_sd).status.gm_level == 0
    {
        return 0;
    }

    // Ghost visibility: dead players hidden from non-ghost viewers
    if !raw_map_ptr().is_null() {
        let md = &*raw_map_ptr().add((*sd).bl.m as usize);
        if md.show_ghosts != 0
            && (*sd).status.state == PC_DIE
            && (*sd).bl.id != (*src_sd).bl.id
        {
            if (*src_sd).status.state != PC_DIE
                && ((*src_sd).optFlags & OPT_FLAG_GHOSTS) == 0
            {
                return 0;
            }
        }
    }

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    let src_fd = (*src_sd).fd;
    wfifohead(src_fd, 512);
    wfifob(src_fd, 0, 0xAA);
    wfifob(src_fd, 3, 0x33);
    wfifob(src_fd, 4, 0x03);
    wfifow(src_fd, 5,  ((*sd).bl.x).swap_bytes());
    wfifow(src_fd, 7,  ((*sd).bl.y).swap_bytes());
    wfifob(src_fd, 9,  (*sd).status.side as u8);
    wfifol(src_fd, 10, ((*sd).status.id).swap_bytes());

    // Sex / disguise look at [14..15]
    if ((*sd).status.state as i32) < PC_DISGUISE as i32 {
        wfifow(src_fd, 14, ((*sd).status.sex as u16).swap_bytes());
    } else {
        wfifob(src_fd, 14, 1);
        wfifob(src_fd, 15, 15);
    }

    // State / invis at [16]
    let can_see_invis = (*sd).bl.id != (*src_sd).bl.id
        && ((*src_sd).status.gm_level != 0
            || clif_isingroup(src_sd, sd) != 0
            || ((*sd).gfx.dye == (*src_sd).gfx.dye
                && (*sd).gfx.dye != 0
                && (*src_sd).gfx.dye != 0));
    let state_byte: u8 = if ((*sd).status.state == PC_INVIS
        || ((*sd).optFlags & OPT_FLAG_STEALTH) != 0)
        && can_see_invis
    {
        5
    } else {
        (*sd).status.state as u8
    };
    wfifob(src_fd, 16, state_byte);

    // Stealth-only override for non-GM viewers
    if ((*sd).optFlags & OPT_FLAG_STEALTH) != 0
        && (*sd).status.state == 0
        && (*src_sd).status.gm_level == 0
    {
        wfifob(src_fd, 16, 2);
    }

    wfifob(src_fd, 19, (*sd).speed as u8);

    // Disguise at [17..18]
    if (*sd).status.state == PC_MOUNTED {
        wfifow(src_fd, 17, (*sd).disguise.swap_bytes());
    } else if (*sd).status.state == PC_DISGUISE {
        wfifow(src_fd, 17, (*sd).disguise.wrapping_add(32768).swap_bytes());
        wfifob(src_fd, 19, (*sd).disguise_color as u8);
    } else {
        wfifow(src_fd, 17, 0);
    }

    wfifob(src_fd, 20, 0);
    wfifob(src_fd, 21, (*sd).status.face as u8);
    wfifob(src_fd, 22, (*sd).status.hair as u8);
    wfifob(src_fd, 23, (*sd).status.hair_color as u8);
    wfifob(src_fd, 24, (*sd).status.face_color as u8);
    wfifob(src_fd, 25, (*sd).status.skin_color as u8);

    // Armor at [26..27], color at [28]
    let armor_id = pc_isequip(sd, EQ_ARMOR);
    if armor_id == 0 {
        wfifow(src_fd, 26, ((*sd).status.sex as u16).swap_bytes());
    } else {
        let armor_look = if (*sd).status.equip[EQ_ARMOR as usize].custom_look != 0 {
            (*sd).status.equip[EQ_ARMOR as usize].custom_look as u16
        } else {
            itemdb_look(armor_id as u32) as u16
        };
        wfifow(src_fd, 26, armor_look.swap_bytes());
        let armor_color: u8 = if (*sd).status.armor_color > 0 {
            (*sd).status.armor_color as u8
        } else if (*sd).status.equip[EQ_ARMOR as usize].custom_look != 0 {
            (*sd).status.equip[EQ_ARMOR as usize].custom_look_color as u8
        } else {
            itemdb_lookcolor(armor_id as u32) as u8
        };
        wfifob(src_fd, 28, armor_color);
    }
    // Coat overrides armor
    let coat_id = pc_isequip(sd, EQ_COAT);
    if coat_id != 0 {
        wfifow(src_fd, 26, (itemdb_look(coat_id as u32) as u16).swap_bytes());
        wfifob(src_fd, 28, itemdb_lookcolor(coat_id as u32) as u8);
    }

    // Weapon at [29..30], color at [31]
    let weap_id = pc_isequip(sd, EQ_WEAP);
    if weap_id == 0 {
        wfifow(src_fd, 29, 0xFFFFu16.swap_bytes());
        wfifob(src_fd, 31, 0);
    } else {
        let (wlook, wcolor) = if (*sd).status.equip[EQ_WEAP as usize].custom_look != 0 {
            ((*sd).status.equip[EQ_WEAP as usize].custom_look as u16,
             (*sd).status.equip[EQ_WEAP as usize].custom_look_color as u8)
        } else {
            (itemdb_look(weap_id as u32) as u16, itemdb_lookcolor(weap_id as u32) as u8)
        };
        wfifow(src_fd, 29, wlook.swap_bytes());
        wfifob(src_fd, 31, wcolor);
    }

    // Shield at [32..33], color at [34]
    let shield_id = pc_isequip(sd, EQ_SHIELD);
    if shield_id == 0 {
        wfifow(src_fd, 32, 0xFFFFu16.swap_bytes());
        wfifob(src_fd, 34, 0);
    } else {
        let (slook, scolor) = if (*sd).status.equip[EQ_SHIELD as usize].custom_look != 0 {
            ((*sd).status.equip[EQ_SHIELD as usize].custom_look as u16,
             (*sd).status.equip[EQ_SHIELD as usize].custom_look_color as u8)
        } else {
            (itemdb_look(shield_id as u32) as u16, itemdb_lookcolor(shield_id as u32) as u8)
        };
        wfifow(src_fd, 32, slook.swap_bytes());
        wfifob(src_fd, 34, scolor);
    }

    // Helm at [35] flag, [36..37] look+color
    let helm_id    = pc_isequip(sd, EQ_HELM);
    let helm_look  = if helm_id != 0 { itemdb_look(helm_id as u32) } else { -1 };
    if helm_id == 0
        || ((*sd).status.setting_flags as u32 & FLAG_HELM) == 0
        || helm_look == -1
    {
        wfifob(src_fd, 35, 0);
        wfifow(src_fd, 36, 0xFFFFu16.swap_bytes());
    } else {
        wfifob(src_fd, 35, 1);
        if (*sd).status.equip[EQ_HELM as usize].custom_look != 0 {
            wfifob(src_fd, 36, (*sd).status.equip[EQ_HELM as usize].custom_look as u8);
            wfifob(src_fd, 37, (*sd).status.equip[EQ_HELM as usize].custom_look_color as u8);
        } else {
            wfifob(src_fd, 36, helm_look as u8);
            wfifob(src_fd, 37, itemdb_lookcolor(helm_id as u32) as u8);
        }
    }

    // Face accessory at [38..39], color at [40]
    let faceacc_id = pc_isequip(sd, EQ_FACEACC);
    if faceacc_id == 0 {
        wfifow(src_fd, 38, 0xFFFFu16.swap_bytes());
        wfifob(src_fd, 40, 0);
    } else {
        wfifow(src_fd, 38, (itemdb_look(faceacc_id as u32) as u16).swap_bytes());
        wfifob(src_fd, 40, itemdb_lookcolor(faceacc_id as u32) as u8);
    }

    // Crown at [41..42], color at [43]; also clears helm flag at [35]
    let crown_id = pc_isequip(sd, EQ_CROWN);
    if crown_id == 0 {
        wfifow(src_fd, 41, 0xFFFFu16.swap_bytes());
        wfifob(src_fd, 43, 0);
    } else {
        wfifob(src_fd, 35, 0); // crown present → clear helm flag
        let (clook, ccolor) = if (*sd).status.equip[EQ_CROWN as usize].custom_look != 0 {
            ((*sd).status.equip[EQ_CROWN as usize].custom_look as u16,
             (*sd).status.equip[EQ_CROWN as usize].custom_look_color as u8)
        } else {
            (itemdb_look(crown_id as u32) as u16, itemdb_lookcolor(crown_id as u32) as u8)
        };
        wfifow(src_fd, 41, clook.swap_bytes());
        wfifob(src_fd, 43, ccolor);
    }

    // Face accessory 2 at [44..45], color at [46]
    let faceacc2_id = pc_isequip(sd, EQ_FACEACCTWO);
    if faceacc2_id == 0 {
        wfifow(src_fd, 44, 0xFFFFu16.swap_bytes());
        wfifob(src_fd, 46, 0);
    } else {
        wfifow(src_fd, 44, (itemdb_look(faceacc2_id as u32) as u16).swap_bytes());
        wfifob(src_fd, 46, itemdb_lookcolor(faceacc2_id as u32) as u8);
    }

    // Mantle at [47..48], color at [49]
    let mantle_id = pc_isequip(sd, EQ_MANTLE);
    if mantle_id == 0 {
        wfifow(src_fd, 47, 0xFFFFu16.swap_bytes());
        wfifob(src_fd, 49, 0xFF);
    } else {
        wfifow(src_fd, 47, (itemdb_look(mantle_id as u32) as u16).swap_bytes());
        wfifob(src_fd, 49, itemdb_lookcolor(mantle_id as u32) as u8);
    }

    // Necklace at [50..51], color at [52]
    let neck_id   = pc_isequip(sd, EQ_NECKLACE);
    let neck_look = if neck_id != 0 { itemdb_look(neck_id as u32) } else { -1 };
    if neck_id == 0
        || ((*sd).status.setting_flags as u32 & FLAG_NECKLACE) == 0
        || neck_look == -1
    {
        wfifow(src_fd, 50, 0xFFFFu16.swap_bytes());
        wfifob(src_fd, 52, 0);
    } else {
        wfifow(src_fd, 50, (neck_look as u16).swap_bytes());
        wfifob(src_fd, 52, itemdb_lookcolor(neck_id as u32) as u8);
    }

    // Boots at [53..54], color at [55]
    let boots_id = pc_isequip(sd, EQ_BOOTS);
    if boots_id == 0 {
        wfifow(src_fd, 53, ((*sd).status.sex as u16).swap_bytes());
        wfifob(src_fd, 55, 0);
    } else {
        let (blook, bcolor) = if (*sd).status.equip[EQ_BOOTS as usize].custom_look != 0 {
            ((*sd).status.equip[EQ_BOOTS as usize].custom_look as u16,
             (*sd).status.equip[EQ_BOOTS as usize].custom_look_color as u8)
        } else {
            (itemdb_look(boots_id as u32) as u16, itemdb_lookcolor(boots_id as u32) as u8)
        };
        wfifow(src_fd, 53, blook.swap_bytes());
        wfifob(src_fd, 55, bcolor);
    }

    // Title color / name at [56..57+len]
    wfifob(src_fd, 56, 0);
    wfifob(src_fd, 57, 128);
    wfifob(src_fd, 58, 0);

    // Title color: invis/stealth GM/group/clan viewers see a special color
    let invis_or_stealth = (*sd).status.state == PC_INVIS
        || ((*sd).optFlags & OPT_FLAG_STEALTH) != 0;
    if invis_or_stealth
        && (*sd).bl.id != (*src_sd).bl.id
        && ((*src_sd).status.gm_level != 0
            || clif_isingroup(src_sd, sd) != 0
            || ((*sd).gfx.dye == (*src_sd).gfx.dye
                && (*sd).gfx.dye != 0
                && (*src_sd).gfx.dye != 0))
    {
        wfifob(src_fd, 56, 0);
    } else if (*sd).gfx.dye != 0 {
        wfifob(src_fd, 56, (*sd).gfx.title_color);
    }

    // Build name string (sd->status.name is [i8; 16])
    let name_src = (*sd).status.name.as_ptr();
    let name_len = {
        let mut l = 0usize;
        while l < (*sd).status.name.len() && *name_src.add(l) != 0 { l += 1; }
        l
    };
    let mut len = name_len;

    // Same-clan → title color 3
    if (*src_sd).status.clan == (*sd).status.clan
        && (*src_sd).status.clan > 0
        && (*src_sd).status.id != (*sd).status.id
    {
        wfifob(src_fd, 56, 3);
    }
    // Same group → title color 2
    if clif_isingroup(src_sd, sd) != 0 {
        wfifob(src_fd, 56, 2);
    }

    // Name (only for visible states)
    if (*sd).status.state != PC_INVIS && (*sd).status.state != 5 {
        wfifob(src_fd, 57, len as u8);
        let dst = rust_session_wdata_ptr(src_fd, 58);
        if !dst.is_null() {
            ptr::copy_nonoverlapping(name_src as *const u8, dst, len);
        }
    } else {
        wfifow(src_fd, 57, 0);
        len = 1;
    }

    // GFX override: GM gfx toggle or clone active — overwrite appearance fields
    if ((*sd).status.gm_level != 0 && (*sd).gfx.toggle != 0) || (*sd).clone != 0 {
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
        let gfx_name_len = {
            let mut l = 0usize;
            while l < (*sd).gfx.name.len() && *gfx_name.add(l) != 0 { l += 1; }
            l
        };
        // C: `if (!strcasecmp(sd->gfx.name, ""))` — empty means skip
        if (*sd).status.state != PC_INVIS && (*sd).status.state != 5 && gfx_name_len > 0 {
            len = gfx_name_len;
            wfifob(src_fd, 57, len as u8);
            let dst = rust_session_wdata_ptr(src_fd, 58);
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
        let p = rust_session_wdata_ptr(src_fd, 1) as *mut u16;
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
pub unsafe fn clif_parsewalk(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }

    let fd = (*sd).fd;
    let m  = (*sd).bl.m as i32;
    let md = &*raw_map_ptr().add(m as usize);

    // Dismount on non-mount maps
    if md.can_mount == 0 && (*sd).status.state == PC_MOUNTED && (*sd).status.gm_level == 0 {
        sl_doscript_simple(c"onDismount".as_ptr(), ptr::null(), &mut (*sd).bl as *mut BlockList);
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
    if dx != (*sd).bl.x as i32 {
        clif_blockmovement(sd, 0);
        map_moveblock(&mut (*sd).bl, (*sd).bl.x as i32, (*sd).bl.y as i32);
        clif_sendxy(sd);
        clif_blockmovement(sd, 1);
        return 0;
    }
    if dy != (*sd).bl.y as i32 {
        clif_blockmovement(sd, 0);
        map_moveblock(&mut (*sd).bl, (*sd).bl.x as i32, (*sd).bl.y as i32);
        clif_sendxy(sd);
        clif_blockmovement(sd, 1);
        return 0;
    }

    (*sd).canmove = 0;

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
    if (*sd).status.gm_level == 0 {
        foreach_in_cell(m, dx, dy, BL_PC,  |bl| clif_canmove_sub_inner(bl, sd));
        foreach_in_cell(m, dx, dy, BL_MOB, |bl| clif_canmove_sub_inner(bl, sd));
        foreach_in_cell(m, dx, dy, BL_NPC, |bl| clif_canmove_sub_inner(bl, sd));
        if read_pass(m, dx, dy) != 0 { (*sd).canmove = 1; }
    }

    // Status blocks movement
    if ((*sd).canmove != 0 || (*sd).paralyzed != 0
        || (*sd).sleep != 1.0f32 || (*sd).snare != 0)
        && (*sd).status.gm_level == 0
    {
        clif_blockmovement(sd, 0);
        clif_sendxy(sd);
        clif_blockmovement(sd, 1);
        return 0;
    }

    // Update viewport offsets
    let vx = (*sd).viewx as i32;
    let vy = (*sd).viewy as i32;
    if direction == 0 && (dy <= vy || ((md.ys as i32 - 1 - dy) < 7 && vy > 7)) {
        (*sd).viewy = (*sd).viewy.saturating_sub(1);
    }
    if direction == 1 && ((dx < 8 && vx < 8) || 16 - (md.xs as i32 - 1 - dx) <= vx) {
        (*sd).viewx = (*sd).viewx.wrapping_add(1);
    }
    if direction == 2 && ((dy < 7 && vy < 7) || 14 - (md.ys as i32 - 1 - dy) <= vy) {
        (*sd).viewy = (*sd).viewy.wrapping_add(1);
    }
    if direction == 3 && (dx <= vx || ((md.xs as i32 - 1 - dx) < 8 && vx > 8)) {
        (*sd).viewx = (*sd).viewx.saturating_sub(1);
    }
    if (*sd).viewx > 16 { (*sd).viewx = 16; }
    if (*sd).viewy > 14 { (*sd).viewy = 14; }

    // Send walk-ack to self (skipped in FASTMOVE mode)
    if ((*sd).status.setting_flags as u32 & FLAG_FASTMOVE) == 0 {
        if rust_session_exists(fd) == 0 {
            rust_session_set_eof(fd, 8);
            return 0;
        }
        wfifohead(fd, 15);
        wfifob(fd, 0, 0xAA);
        wfifob(fd, 1, 0x00);
        wfifob(fd, 2, 0x0C);
        wfifob(fd, 3, 0x26);
        // [4] intentionally not written (C comments it out too)
        wfifob(fd, 5, direction);
        wfifow(fd, 6, (xold as u16).swap_bytes());
        wfifow(fd, 8, (yold as u16).swap_bytes());
        wfifow(fd, 10, ((*sd).viewx as u16).swap_bytes());
        wfifow(fd, 12, ((*sd).viewy as u16).swap_bytes());
        wfifob(fd, 14, 0x00);
        wfifoset(fd, encrypt(fd) as usize);
    }

    // No actual position change
    if dx == (*sd).bl.x as i32 && dy == (*sd).bl.y as i32 { return 0; }

    // Broadcast movement to area.
    let mut buf = [0u8; 32];
    buf[0] = 0xAA;
    buf[1] = 0x00;
    buf[2] = 0x0C;
    buf[3] = 0x0C;
    // [4] = 0 (C comments out this byte)
    buf[5..9].copy_from_slice(&((*sd).status.id as u32).swap_bytes().to_ne_bytes());
    buf[9..11].copy_from_slice(&(xold as u16).swap_bytes().to_ne_bytes());
    buf[11..13].copy_from_slice(&(yold as u16).swap_bytes().to_ne_bytes());
    buf[13] = direction;
    buf[14] = 0x00;

    if ((*sd).optFlags & OPT_FLAG_STEALTH) != 0 {
        clif_sendtogm(buf.as_mut_ptr(), 32, &mut (*sd).bl, AREA_WOS);
    } else {
        clif_send(buf.as_ptr(), 32, &mut (*sd).bl, AREA_WOS);
    }

    map_moveblock(&mut (*sd).bl, dx, dy);

    // If client sent viewport sub-packet, scan and send new tile strip
    if rfifob(fd, 3) == 6 {
        clif_sendmapdata(sd, m, x0, y0, x1, y1, checksum);
        clif_mob_look_start(sd);
        foreach_in_rect(
            m, x0, y0, x0 + (x1 - 1), y0 + (y1 - 1), BL_ALL,
            |bl| clif_object_look_sub_inner(bl, LOOK_GET, sd as *mut BlockList),
        );
        clif_mob_look_close(sd);
        foreach_in_rect(m, x0, y0, x0+(x1-1), y0+(y1-1), BL_PC,  |bl| clif_charlook_inner(bl, LOOK_GET,  sd));
        foreach_in_rect(m, x0, y0, x0+(x1-1), y0+(y1-1), BL_NPC, |bl| clif_cnpclook_inner(bl, LOOK_GET,  sd as *mut BlockList));
        foreach_in_rect(m, x0, y0, x0+(x1-1), y0+(y1-1), BL_MOB, |bl| clif_cmoblook_inner(bl, LOOK_GET,  sd as *mut BlockList));
        foreach_in_rect(m, x0, y0, x0+(x1-1), y0+(y1-1), BL_PC,  |bl| clif_charlook_inner(bl, LOOK_SEND, sd));
    }

    // Equipment walk scripts
    for i in 0..14usize {
        if (*sd).status.equip[i].id > 0 {
            let yn = itemdb_yname((*sd).status.equip[i].id);
            if !yn.is_null() {
                sl_doscript_simple(yn, c"on_walk".as_ptr(), &mut (*sd).bl as *mut BlockList);
            }
        }
    }

    // Skill passive walk scripts
    for i in 0..MAX_SPELLS {
        if (*sd).status.skill[i] > 0 {
            let yn = magicdb_yname((*sd).status.skill[i] as i32);
            if !yn.is_null() {
                sl_doscript_simple(yn, c"on_walk_passive".as_ptr(), &mut (*sd).bl as *mut BlockList);
            }
        }
    }

    // Aether walk scripts
    for i in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[i].id > 0 && (*sd).status.dura_aether[i].duration > 0 {
            let yn = magicdb_yname((*sd).status.dura_aether[i].id as i32);
            if !yn.is_null() {
                sl_doscript_simple(yn, c"on_walk_while_cast".as_ptr(), &mut (*sd).bl as *mut BlockList);
            }
        }
    }

    sl_doscript_simple(c"onScriptedTile".as_ptr(), ptr::null(), &mut (*sd).bl as *mut BlockList);
    crate::game::pc::rust_pc_runfloor_sub(sd);

    // Warp check
    do_warp_check(sd);
    0
}

// ─── clif_noparsewalk ─────────────────────────────────────────────────────────

/// Server-driven forced walk (no RFIFO packet from client).
///
/// Reads direction from `sd->status.side`, applies the same collision and
/// viewport logic as `clif_parsewalk`, but sends `[4]=0x03` and computes
/// the new-strip viewport coords internally.
///
pub unsafe fn clif_noparsewalk(sd: *mut MapSessionData, _speed: i8) -> i32 {
    if sd.is_null() { return 0; }

    let m  = (*sd).bl.m as i32;
    let md = &*raw_map_ptr().add(m as usize);

    let xold = (*sd).bl.x as i32;
    let yold = (*sd).bl.y as i32;
    let mut dx = xold;
    let mut dy = yold;

    // Position guards (always false since dx/dy == bl.x/y)
    if dx != (*sd).bl.x as i32 {
        clif_blockmovement(sd, 0);
        map_moveblock(&mut (*sd).bl, (*sd).bl.x as i32, (*sd).bl.y as i32);
        clif_sendxy(sd);
        clif_blockmovement(sd, 1);
        return 0;
    }
    if dy != (*sd).bl.y as i32 {
        clif_blockmovement(sd, 0);
        map_moveblock(&mut (*sd).bl, (*sd).bl.x as i32, (*sd).bl.y as i32);
        clif_sendxy(sd);
        clif_blockmovement(sd, 1);
        return 0;
    }

    // Dismount on non-mount maps
    if md.can_mount == 0 && (*sd).status.state == PC_MOUNTED && (*sd).status.gm_level == 0 {
        sl_doscript_simple(c"onDismount".as_ptr(), ptr::null(), &mut (*sd).bl as *mut BlockList);
    }

    let direction = (*sd).status.side as i32;

    // Compute destination and new viewport strip
    let (x0, y0, x1, y1): (i32, i32, i32, i32);
    match direction {
        0 => {
            dy -= 1;
            x0 = (*sd).bl.x as i32 - ((*sd).viewx as i32 + 1);
            y0 = dy - ((*sd).viewy as i32 + 1);
            x1 = 19;
            y1 = 1;
        }
        1 => {
            dx += 1;
            x0 = dx + (18 - ((*sd).viewx as i32 + 1));
            y0 = (*sd).bl.y as i32 - ((*sd).viewy as i32 + 1);
            x1 = 1;
            y1 = 17;
        }
        2 => {
            dy += 1;
            x0 = (*sd).bl.x as i32 - ((*sd).viewx as i32 + 1);
            y0 = dy + (16 - ((*sd).viewy as i32 + 1));
            x1 = 19;
            y1 = 1;
        }
        3 => {
            dx -= 1;
            x0 = dx - ((*sd).viewx as i32 + 1);
            y0 = (*sd).bl.y as i32 - ((*sd).viewy as i32 + 1);
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

    (*sd).canmove = 0;
    if (*sd).status.gm_level == 0 {
        foreach_in_cell(m, dx, dy, BL_PC,  |bl| clif_canmove_sub_inner(bl, sd));
        foreach_in_cell(m, dx, dy, BL_MOB, |bl| clif_canmove_sub_inner(bl, sd));
        foreach_in_cell(m, dx, dy, BL_NPC, |bl| clif_canmove_sub_inner(bl, sd));
        if read_pass(m, dx, dy) != 0 { (*sd).canmove = 1; }
    }

    if (*sd).canmove != 0 || (*sd).paralyzed != 0 || (*sd).sleep != 1.0f32 || (*sd).snare != 0 {
        clif_blockmovement(sd, 0);
        clif_sendxy(sd);
        clif_blockmovement(sd, 1);
        return 0;
    }

    if dx == (*sd).bl.x as i32 && dy == (*sd).bl.y as i32 { return 0; }

    // Viewport update
    let vx = (*sd).viewx as i32;
    let vy = (*sd).viewy as i32;
    if direction == 0 && (dy <= vy || ((md.ys as i32 - 1 - dy) < 7 && vy > 7)) {
        (*sd).viewy = (*sd).viewy.saturating_sub(1);
    }
    if direction == 1 && ((dx < 8 && vx < 8) || 16 - (md.xs as i32 - 1 - dx) <= vx) {
        (*sd).viewx = (*sd).viewx.wrapping_add(1);
    }
    if direction == 2 && ((dy < 7 && vy < 7) || 14 - (md.ys as i32 - 1 - dy) <= vy) {
        (*sd).viewy = (*sd).viewy.wrapping_add(1);
    }
    if direction == 3 && (dx <= vx || ((md.xs as i32 - 1 - dx) < 8 && vx > 8)) {
        (*sd).viewx = (*sd).viewx.saturating_sub(1);
    }
    if (*sd).viewx > 16 { (*sd).viewx = 16; }
    if (*sd).viewy > 14 { (*sd).viewy = 14; }

    // Temporarily toggle off FASTMOVE (noparsewalk always sends the walk packet)
    let had_fastmove = ((*sd).status.setting_flags as u32 & FLAG_FASTMOVE) != 0;
    if had_fastmove {
        (*sd).status.setting_flags ^= FLAG_FASTMOVE as u16;
        clif_sendstatus(sd, 0);
    }

    let fd = (*sd).fd;
    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    wfifohead(fd, 15);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 1, 0x00);
    wfifob(fd, 2, 0x0C);
    wfifob(fd, 3, 0x26);
    wfifob(fd, 4, 0x03); // noparsewalk always writes [4]=0x03
    wfifob(fd, 5, direction as u8);
    wfifow(fd, 6, (xold as u16).swap_bytes());
    wfifow(fd, 8, (yold as u16).swap_bytes());
    wfifow(fd, 10, ((*sd).viewx as u16).swap_bytes());
    wfifow(fd, 12, ((*sd).viewy as u16).swap_bytes());
    wfifob(fd, 14, 0x00);
    wfifoset(fd, encrypt(fd) as usize);

    // Restore FASTMOVE
    if had_fastmove {
        (*sd).status.setting_flags ^= FLAG_FASTMOVE as u16;
        clif_sendstatus(sd, 0);
    }

    // Broadcast movement to area
    let mut buf = [0u8; 32];
    buf[0] = 0xAA;
    buf[1] = 0x00;
    buf[2] = 0x0C;
    buf[3] = 0x0C;
    buf[5..9].copy_from_slice(&((*sd).status.id as u32).swap_bytes().to_ne_bytes());
    buf[9..11].copy_from_slice(&(xold as u16).swap_bytes().to_ne_bytes());
    buf[11..13].copy_from_slice(&(yold as u16).swap_bytes().to_ne_bytes());
    buf[13] = direction as u8;
    buf[14] = 0x00;

    if ((*sd).optFlags & OPT_FLAG_STEALTH) != 0 {
        clif_sendtogm(buf.as_mut_ptr(), 32, &mut (*sd).bl, AREA_WOS);
    } else {
        clif_send(buf.as_ptr(), 32, &mut (*sd).bl, AREA_WOS);
    }

    map_moveblock(&mut (*sd).bl, dx, dy);

    // Send new viewport strip if in bounds
    if x0 >= 0 && y0 >= 0
        && x0 + (x1 - 1) < md.xs as i32
        && y0 + (y1 - 1) < md.ys as i32
    {
        clif_sendmapdata(sd, m, x0, y0, x1, y1, 0);
        clif_mob_look_start(sd);
        foreach_in_rect(
            m, x0, y0, x0+(x1-1), y0+(y1-1), BL_ALL,
            |bl| clif_object_look_sub_inner(bl, LOOK_GET, sd as *mut BlockList),
        );
        clif_mob_look_close(sd);
        foreach_in_rect(m, x0, y0, x0+(x1-1), y0+(y1-1), BL_PC,  |bl| clif_charlook_inner(bl, LOOK_GET,  sd));
        foreach_in_rect(m, x0, y0, x0+(x1-1), y0+(y1-1), BL_NPC, |bl| clif_cnpclook_inner(bl, LOOK_GET,  sd as *mut BlockList));
        foreach_in_rect(m, x0, y0, x0+(x1-1), y0+(y1-1), BL_MOB, |bl| clif_cmoblook_inner(bl, LOOK_GET,  sd as *mut BlockList));
        foreach_in_rect(m, x0, y0, x0+(x1-1), y0+(y1-1), BL_PC,  |bl| clif_charlook_inner(bl, LOOK_SEND, sd));
    }

    sl_doscript_simple(c"onScriptedTile".as_ptr(), ptr::null(), &mut (*sd).bl as *mut BlockList);
    crate::game::pc::rust_pc_runfloor_sub(sd);

    do_warp_check(sd);
    1
}

// ─── clif_parsewalkpong ───────────────────────────────────────────────────────

/// Handle a walk-ping pong response from the client.
///
/// Reads the timestamp at [9..12] (u32 BE → host), updates `msPing` and
/// `LastPongStamp`.
///
pub unsafe fn clif_parsewalkpong(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }
    let fd = (*sd).fd;

    // [5..8] = HASH (unused); [9..12] = TS (u32 big-endian)
    let ts = rfifol(fd, 9).swap_bytes() as u64;

    if (*sd).LastPingTick != 0 {
        (*sd).msPing = (gettick() as u64).wrapping_sub((*sd).LastPingTick) as i32;
    }

    if (*sd).LastPongStamp != 0 {
        let difference = ts.wrapping_sub((*sd).LastPongStamp) as i32;
        if difference > 43000 {
            // Speedhack detection — C commented the enforcement out; replicate no-op
        }
    }

    (*sd).LastPongStamp = ts;
    0
}

// ─── clif_parsemap ────────────────────────────────────────────────────────────

/// Handle a client map-data request.
///
/// Sets `sd->loaded = 1`, reads viewport parameters, then delegates to
/// `clif_sendmapdata`.
///
pub unsafe fn clif_parsemap(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }
    let fd = (*sd).fd;

    (*sd).loaded = 1;

    let x0 = rfifow(fd, 5).swap_bytes() as i32;
    let y0 = rfifow(fd, 7).swap_bytes() as i32;
    let x1 = rfifob(fd, 9) as i32;
    let y1 = rfifob(fd, 10) as i32;
    let mut checksum = rfifow(fd, 11).swap_bytes();

    // Packet type 5 → force full resend (checksum=0 means always send)
    if rfifob(fd, 3) == 5 {
        checksum = 0;
    }

    tracing::debug!("[map] [parsemap] fd={} m={} x0={} y0={} x1={} y1={} check={}", fd, (*sd).bl.m, x0, y0, x1, y1, checksum);
    clif_sendmapdata(sd, (*sd).bl.m as i32, x0, y0, x1, y1, checksum);
    0
}

// ─── clif_sendmapdata ─────────────────────────────────────────────────────────

/// Send tile, passability, and object data for a viewport rectangle.
///
/// Builds the tile packet locally, computes NexCRCC checksum, and skips the
/// send if the client's cached checksum already matches.
///
pub unsafe fn clif_sendmapdata(
    sd: *mut MapSessionData,
    m: i32,
    mut x0: i32,
    mut y0: i32,
    mut x1: i32,
    mut y1: i32,
    check: u16,
) -> i32 {
    if sd.is_null() { return 0; }

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    let fd = (*sd).fd;

    // Blackout map: delegate to Lua
    if map_readglobalreg(m, c"blackout".as_ptr()) != 0 {
        sl_doscript_simple(c"sendMapData".as_ptr(), ptr::null(), &mut (*sd).bl as *mut BlockList);
        return 0;
    }

    // Sanity: C limit is x1*y1 > 323
    if x1 * y1 > 323 {
        tracing::warn!("[map] [sendmapdata] fd={} viewport too large x1={} y1={} product={}", fd, x1, y1, x1 * y1);
        return 0;
    }

    let md = &*raw_map_ptr().add(m as usize);
    if x0 < 0 { x0 = 0; }
    if y0 < 0 { y0 = 0; }
    if x1 > md.xs as i32 { x1 = md.xs as i32; }
    if y1 > md.ys as i32 { y1 = md.ys as i32; }

    // CRC buffer: flat array of i16 triples (tile, pass, obj)
    // Maximum tiles = 323, so max triples = 323 × 3 = 969 i16s.
    let mut crc_buf = [0i16; 1024];
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
            let t = read_tile(m, x0 + x, y0 + y) as i16;
            let p = read_pass(m, x0 + x, y0 + y) as i16;
            let o = read_obj(m,  x0 + x, y0 + y) as i16;

            if a + 2 < crc_buf.len() {
                crc_buf[a]     = t;
                crc_buf[a + 1] = p;
                crc_buf[a + 2] = o;
            }

            buf2[pos..pos+2].copy_from_slice(&(t as u16).swap_bytes().to_ne_bytes()); pos += 2;
            buf2[pos..pos+2].copy_from_slice(&(p as u16).swap_bytes().to_ne_bytes()); pos += 2;
            buf2[pos..pos+2].copy_from_slice(&(o as u16).swap_bytes().to_ne_bytes()); pos += 2;

            a += 3;
        }
    }

    let checksum = nex_crcc(&crc_buf[..a]);

    if pos <= 12 {
        tracing::warn!("[map] [sendmapdata] fd={} no tiles written pos={}", fd, pos);
        return 0;
    }
    if checksum == check as i16 {
        tracing::debug!("[map] [sendmapdata] fd={} checksum match={} skip send", fd, checksum);
        return 0;
    }
    tracing::debug!("[map] [sendmapdata] fd={} sending {} bytes computed_check={} client_check={}", fd, pos, checksum, check);

    // Write big-endian packet size at [1..2]
    buf2[1..3].copy_from_slice(&((pos - 3) as u16).swap_bytes().to_ne_bytes());

    wfifohead(fd, 65535);
    {
        let dst = rust_session_wdata_ptr(fd, 0);
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
pub unsafe fn clif_sendside(bl: *mut BlockList) -> i32 {
    if bl.is_null() { return 0; }

    // Read the side byte from the typed struct.
    // PC: MapSessionData.status.side (i8)
    // MOB/NPC: the `side` field lives at the same offset in both structs:
    //   immediately after the block_list header (48 bytes).
    let (side_byte, target): (u8, i32) = if (*bl).bl_type == BL_PC as u8 {
        let sd = bl as *mut MapSessionData;
        ((*sd).status.side as u8, AREA)
    } else if (*bl).bl_type == BL_MOB as u8 || (*bl).bl_type == BL_NPC as u8 {
        // Both MOB and NPC have their C `side` field (char/u8) at sizeof(block_list) = 48.
        let side_ptr = (bl as *const u8).add(std::mem::size_of::<BlockList>());
        (*side_ptr, AREA_WOS)
    } else {
        return 0;
    };

    let mut buf = [0u8; 32];
    buf[0] = 0xAA;
    buf[1] = 0x00;
    buf[2] = 0x08;
    buf[3] = 0x11;
    buf[5..9].copy_from_slice(&(*bl).id.swap_bytes().to_ne_bytes());
    buf[9]  = side_byte;
    buf[10] = 0;

    clif_send(buf.as_ptr(), 32, bl, target);
    0
}

// ─── clif_parseside ───────────────────────────────────────────────────────────

/// Handle a client facing-direction change.
///
/// Reads new side from RFIFO[5], broadcasts via `clif_sendside`, fires
/// `onTurn` Lua event.
///
pub unsafe fn clif_parseside(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }
    let fd = (*sd).fd;
    (*sd).status.side = rfifob(fd, 5) as i8;
    clif_sendside(&mut (*sd).bl);
    sl_doscript_simple(c"onTurn".as_ptr(), ptr::null(), &mut (*sd).bl as *mut BlockList);
    0
}

// ─── Private: warp check ─────────────────────────────────────────────────────

/// Check whether the player's current position has a warp tile, and if so
/// validate entry requirements and call `pc_warp`.
///
/// Shared by both `clif_parsewalk` and `clif_noparsewalk`.
#[inline]
unsafe fn do_warp_check(sd: *mut MapSessionData) {
    let fm = (*sd).bl.m as i32;
    let fmd = &*raw_map_ptr().add(fm as usize);

    let mut fx = (*sd).bl.x as i32;
    let mut fy = (*sd).bl.y as i32;
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

    let zmd = &*raw_map_ptr().add(zm as usize);

    // Level / vita / mana / mark / path minimum requirements
    let below_min = ((*sd).status.level as u32) < zmd.reqlvl
        || ((*sd).status.basehp < zmd.reqvita && (*sd).status.basemp < zmd.reqmana)
        || ((*sd).status.mark as u8) < zmd.reqmark
        || (zmd.reqpath > 0 && (*sd).status.class != zmd.reqpath);

    if below_min && (*sd).status.gm_level == 0 {
        clif_pushback(sd);
        let maprejectmsg = zmd.maprejectmsg.as_ptr();
        if *maprejectmsg == 0 {
            let lvl_diff = (zmd.reqlvl as i32 - (*sd).status.level as i32).unsigned_abs();
            let msg: &[u8] = if lvl_diff >= 10 {
                b"Nightmarish visions of your own death repel you.\0"
            } else if lvl_diff >= 5 {
                b"You're not quite ready to enter yet.\0"
            } else if ((*sd).status.mark as u8) < zmd.reqmark {
                b"You do not understand the secrets to enter.\0"
            } else if zmd.reqpath > 0 && (*sd).status.class != zmd.reqpath {
                b"Your path forbids it.\0"
            } else {
                b"A powerful force repels you.\0"
            };
            clif_sendminitext(sd, msg.as_ptr() as *const i8);
        } else {
            clif_sendminitext(sd, maprejectmsg);
        }
        return;
    }

    // Level / vita / mana maximum requirements
    let above_max = ((*sd).status.level as u32) > zmd.lvlmax
        || ((*sd).status.basehp > zmd.vitamax && (*sd).status.basemp > zmd.manamax);

    if above_max && (*sd).status.gm_level == 0 {
        clif_pushback(sd);
        clif_sendminitext(
            sd,
            b"A magical barrier prevents you from entering.\0".as_ptr() as *const i8,
        );
        return;
    }

    pc_warp(sd, zm, zx, zy);
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
pub unsafe fn clif_object_canmove(m: i32, x: i32, y: i32, side: i32) -> i32 {
    use crate::game::map_server::objectFlags;
    let object = read_obj(m, x, y) as usize;
    if objectFlags.is_null() { return 0; }
    let flag = *objectFlags.add(object);
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
pub unsafe fn clif_object_canmove_from(m: i32, x: i32, y: i32, side: i32) -> i32 {
    use crate::game::map_server::objectFlags;
    let object = read_obj(m, x, y) as usize;
    if objectFlags.is_null() { return 0; }
    let flag = *objectFlags.add(object);
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
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
pub unsafe fn clif_pushback(sd: *mut MapSessionData) -> i32 {
    let m = (*sd).bl.m as i32;
    let x = (*sd).bl.x as i32;
    let y = (*sd).bl.y as i32;
    match (*sd).status.side {
        0 => { pc_warp(sd, m, x,     y + 2); }
        1 => { pc_warp(sd, m, x - 2, y    ); }
        2 => { pc_warp(sd, m, x,     y - 2); }
        3 => { pc_warp(sd, m, x + 2, y    ); }
        _ => {}
    }
    0
}

/// Respond to a client viewport scroll: update position delta and refresh visible objects.
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
pub unsafe fn clif_parseviewchange(sd: *mut MapSessionData) -> i32 {
    use crate::game::map_parse::chat::clif_sendminitext;
    use crate::game::map_parse::player_state::clif_sendxychange;

    let fd = (*sd).fd;
    let direction = *crate::session::rust_session_rdata_ptr(fd, 5) as i32;
    let mut dx = *crate::session::rust_session_rdata_ptr(fd, 6) as i32;
    let mut dy = *crate::session::rust_session_rdata_ptr(fd, 7) as i32;
    let x0 = u16::from_be_bytes([
        *crate::session::rust_session_rdata_ptr(fd, 8),
        *crate::session::rust_session_rdata_ptr(fd, 9),
    ]) as i32;
    let y0 = u16::from_be_bytes([
        *crate::session::rust_session_rdata_ptr(fd, 10),
        *crate::session::rust_session_rdata_ptr(fd, 11),
    ]) as i32;
    let x1 = *crate::session::rust_session_rdata_ptr(fd, 12) as i32;
    let y1 = *crate::session::rust_session_rdata_ptr(fd, 13) as i32;

    if (*sd).status.state == 3 {
        clif_sendminitext(sd, c"You cannot do that while riding a mount.".as_ptr());
        return 0;
    }

    match direction {
        0 => dy += 1,
        1 => dx -= 1,
        2 => dy -= 1,
        3 => dx += 1,
        _ => {}
    }

    clif_sendxychange(sd, dx, dy);
    let m2 = (*sd).bl.m as i32;
    clif_mob_look_start(sd);
    foreach_in_rect(m2, x0, y0, x0+(x1-1), y0+(y1-1), BL_ALL, |bl| clif_object_look_sub_inner(bl, LOOK_GET, sd as *mut BlockList));
    clif_mob_look_close(sd);
    foreach_in_rect(m2, x0, y0, x0+(x1-1), y0+(y1-1), BL_PC,  |bl| clif_charlook_inner(bl, LOOK_GET,  sd));
    foreach_in_rect(m2, x0, y0, x0+(x1-1), y0+(y1-1), BL_NPC, |bl| clif_cnpclook_inner(bl, LOOK_GET,  sd as *mut BlockList));
    foreach_in_rect(m2, x0, y0, x0+(x1-1), y0+(y1-1), BL_MOB, |bl| clif_cmoblook_inner(bl, LOOK_GET,  sd as *mut BlockList));
    foreach_in_rect(m2, x0, y0, x0+(x1-1), y0+(y1-1), BL_PC,  |bl| clif_charlook_inner(bl, LOOK_SEND, sd));
    0
}

// ─── Look-at handlers ────────────────────────────────────────────────────────
//


///
/// Fires the "onLook" Lua event when player looks at a cell.
/// Args: `bl` = the object being looked at, `sd` = the looking player.
#[cfg(not(test))]
pub unsafe fn clif_parselookat_sub_inner(bl: *mut BlockList, sd: *mut MapSessionData) -> i32 {
    if bl.is_null() || sd.is_null() { return 0; }
    sl_doscript_2(c"onLook".as_ptr(), std::ptr::null(), &raw mut (*sd).bl, bl);
    0
}

/// Dead code stub — body was removed in original C.
pub unsafe fn clif_parselookat_scriptsub(
    _sd: *mut MapSessionData,
    _bl: *mut BlockList,
) -> i32 {
    0
}

/// Look at the cell directly ahead of the player (based on `side`).
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
#[cfg(not(test))]
pub unsafe fn clif_parselookat_2(sd: *mut MapSessionData) -> i32 {
    use crate::game::mob::BL_ITEM;
    let mut dx = (*sd).bl.x as i32;
    let mut dy = (*sd).bl.y as i32;
    match (*sd).status.side {
        0 => dy -= 1,
        1 => dx += 1,
        2 => dy += 1,
        3 => dx -= 1,
        _ => {}
    }
    let m = (*sd).bl.m as i32;
    foreach_in_cell(m, dx, dy, BL_PC,   |bl| clif_parselookat_sub_inner(bl, sd));
    foreach_in_cell(m, dx, dy, BL_MOB,  |bl| clif_parselookat_sub_inner(bl, sd));
    foreach_in_cell(m, dx, dy, BL_ITEM, |bl| clif_parselookat_sub_inner(bl, sd));
    foreach_in_cell(m, dx, dy, BL_NPC,  |bl| clif_parselookat_sub_inner(bl, sd));
    0
}

/// Look at a specific map cell (coordinates from packet bytes 5–8).
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
#[cfg(not(test))]
pub unsafe fn clif_parselookat(sd: *mut MapSessionData) -> i32 {
    use crate::game::mob::BL_ITEM;
    let fd = (*sd).fd;
    let x = u16::from_be_bytes([
        *crate::session::rust_session_rdata_ptr(fd, 5),
        *crate::session::rust_session_rdata_ptr(fd, 6),
    ]) as i32;
    let y = u16::from_be_bytes([
        *crate::session::rust_session_rdata_ptr(fd, 7),
        *crate::session::rust_session_rdata_ptr(fd, 8),
    ]) as i32;
    let m = (*sd).bl.m as i32;
    foreach_in_cell(m, x, y, BL_PC,   |bl| clif_parselookat_sub_inner(bl, sd));
    foreach_in_cell(m, x, y, BL_MOB,  |bl| clif_parselookat_sub_inner(bl, sd));
    foreach_in_cell(m, x, y, BL_ITEM, |bl| clif_parselookat_sub_inner(bl, sd));
    foreach_in_cell(m, x, y, BL_NPC,  |bl| clif_parselookat_sub_inner(bl, sd));
    0
}

// ─── clif_refreshnoclick ─────────────────────────────────────────────────────
//

/// Resync the client's view (areas, chars, objects) after a non-click teleport.
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
#[cfg(not(test))]
pub unsafe fn clif_refreshnoclick(sd: *mut MapSessionData) -> i32 {
    use crate::database::map_db::raw_map_ptr;
    use crate::session::{rust_session_exists, rust_session_set_eof, rust_session_wdata_ptr, rust_session_commit, rust_session_wfifohead};
    use crate::game::map_parse::player_state::{clif_sendmapinfo, clif_sendxynoclick};
    use crate::game::client::visual::clif_destroyold;
    use crate::game::pc::FLAG_GROUP;
    use crate::network::crypt::set_packet_indexes;

    clif_sendmapinfo(sd);
    clif_sendxynoclick(sd);
    clif_mob_look_start(sd);
    foreach_in_area(
        (*sd).bl.m as i32, (*sd).bl.x as i32, (*sd).bl.y as i32,
        AreaType::SameArea, BL_ALL,
        |bl| clif_object_look_sub_inner(bl, LOOK_GET, sd as *mut BlockList),
    );
    clif_mob_look_close(sd);
    clif_destroyold(sd);
    clif_sendchararea(sd);
    crate::game::map_parse::player_state::clif_getchararea(sd);

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    // Send 0x22/0x03 packet: 5-byte payload + 3 index bytes = 8 committed
    rust_session_wfifohead((*sd).fd, 8);
    let w = |off: usize| rust_session_wdata_ptr((*sd).fd, off);
    *w(0) = 0xAA;
    *w(1) = 0x00;
    *w(2) = 0x02;  // payload length = 2
    *w(3) = 0x22;
    *w(4) = 0x03;
    let mut buf = std::slice::from_raw_parts_mut(rust_session_wdata_ptr((*sd).fd, 0), 8);
    let n = set_packet_indexes(&mut buf);  // appends 3 index bytes, updates [1-2]
    rust_session_commit((*sd).fd, n);

    let md = &*raw_map_ptr().add((*sd).bl.m as usize);
    if md.can_group == 0 {
        use crate::game::map_parse::groups::clif_leavegroup;
        (*sd).status.setting_flags ^= FLAG_GROUP as u16;
        if (*sd).status.setting_flags & FLAG_GROUP as u16 == 0 && (*sd).group_count > 0 {
            clif_leavegroup(sd);
            clif_sendstatus(sd, 0);
            clif_sendminitext(sd, c"Join a group     :OFF".as_ptr());
        }
    }
    0
}

// ─── clif_npc_move_inner ─────────────────────────────────────────────────────


///
/// Broadcast an NPC position packet to a nearby player.
/// `bl` is cast to `*mut MapSessionData` (the receiving player).
/// Builds a 32-byte buffer and calls `clif_send(buf, 32, &nd->bl, AREA_WOS)`.
#[cfg(not(test))]
pub unsafe fn clif_npc_move_inner(bl: *mut BlockList, nd: *mut crate::game::npc::NpcData) -> i32 {
    let sd = bl as *mut MapSessionData;
    if sd.is_null() || nd.is_null() { return 0; }

    let mut buf = [0u8; 32];
    buf[0] = 0xAA;
    buf[1] = 0x00;
    buf[2] = 0x0C;
    buf[3] = 0x0C;
    buf[5..9].copy_from_slice(&(*nd).bl.id.to_be_bytes());
    buf[9..11].copy_from_slice(&((*nd).bl.bx as u16).to_be_bytes());
    buf[11..13].copy_from_slice(&((*nd).bl.by as u16).to_be_bytes());
    buf[13] = (*nd).side as u8;
    // buf[14] = 0x00 (already zeroed)
    clif_send(buf.as_ptr(), 32, &raw mut (*nd).bl, AREA_WOS);
    0
}

// ─── clif_mob_move_inner ──────────────────────────────────────────────────────


///
/// Send a mob-position packet to a player.
/// `bl` is the viewing player, `mob` is the mob to render.
#[cfg(not(test))]
pub unsafe fn clif_mob_move_inner(bl: *mut BlockList, mob: *mut crate::game::mob::MobSpawnData) -> i32 {
    use crate::game::mob::MOB_DEAD;
    let sd = bl as *mut MapSessionData;
    if sd.is_null() || mob.is_null() { return 0; }
    if (*mob).state == MOB_DEAD { return 0; }
    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }
    let fd = (*sd).fd;
    wfifoheader(fd, 0x0C, 11);
    // WFIFOL(fd, 5) = SWAP32(mob->bl.id)
    let pw = |off: usize| crate::session::rust_session_wdata_ptr(fd, off);
    (pw(5) as *mut u32).write_unaligned((*mob).bl.id.to_be());
    (pw(9) as *mut u16).write_unaligned(((*mob).bx as u16).to_be());
    (pw(11) as *mut u16).write_unaligned(((*mob).by_ as u16).to_be());
    *pw(13) = (*mob).side as u8;
    wfifoset(fd, encrypt(fd) as usize);
    0
}
