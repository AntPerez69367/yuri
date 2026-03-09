//! Port of group/party and related UI functions from `c_src/map_parse.c`.
//!
//! Functions declared `pub unsafe extern "C"` so they remain
//! callable from any remaining C code that has not yet been ported.
//!
//! Group state is stored in the global flat array
//! `groups[MAX_GROUPS][MAX_GROUP_MEMBERS]` (256×256 u32 values, 65536 total),
//! accessed as `groups[groupid * 256 + slot]`.

#![allow(non_snake_case, clippy::wildcard_imports)]

use std::ffi::{c_char, c_int, c_uint};

use crate::database::map_db::BlockList;
use crate::database::map_db::{get_map_ptr, map_is_loaded};
use crate::session::{rust_session_exists, rust_session_set_eof, rust_session_wdata_ptr};
use crate::game::mob::MobSpawnData;
use crate::database::{blocking_run, get_pool};

use crate::game::pc::{
    MapSessionData,
    BL_MOB, BL_NPC, BL_PC,
    EQ_HELM, EQ_FACEACC, EQ_CROWN, EQ_FACEACCTWO,
    MAX_GROUP_MEMBERS,
    OPT_FLAG_STEALTH, OPT_FLAG_GHOSTS,
    FLAG_GROUP,
};

use super::packet::{
    encrypt,
    rfifob, rfifop,
    wfifob, wfifoset, wfifohead,
};
use crate::game::block::{foreach_in_area, foreach_in_cell, AreaType};

// ─── Constants ────────────────────────────────────────────────────────────────

const MAX_GROUPS: usize = 256;

// BL_ALL: all block-list types (from map_server.h enum)
const BL_ALL: c_int = 0x0F;

// ─── Direct Rust imports (replacing extern "C" declarations) ─────────────────

use crate::game::map_parse::chat::clif_sendminitext;
use crate::game::map_server::{map_name2sd, groups as groups_raw};
use crate::game::block::map_firstincell;
use crate::game::map_parse::movement::clif_object_canmove;
use crate::database::class_db::{rust_classdb_path as classdb_path, rust_classdb_level as classdb_level};
use crate::database::item_db::{
    rust_itemdb_look as itemdb_look, rust_itemdb_lookcolor as itemdb_lookcolor,
};

// map_id2sd in map_server returns *mut c_void — wrap with cast.
#[inline]
unsafe fn map_id2sd(id: c_uint) -> *mut MapSessionData {
    crate::game::map_server::map_id2sd(id) as *mut MapSessionData
}

// pc_isequip returns c_int; usage here expects c_uint — wrap with cast.
#[inline]
unsafe fn pc_isequip(sd: *mut MapSessionData, slot: c_int) -> c_uint {
    crate::game::pc::rust_pc_isequip(sd, slot) as c_uint
}

/// Dispatch a Lua event with two block_list arguments.
#[cfg(not(test))]
#[allow(dead_code)]
unsafe fn sl_doscript_2(root: *const std::ffi::c_char, method: *const std::ffi::c_char, bl1: *mut crate::database::map_db::BlockList, bl2: *mut crate::database::map_db::BlockList) -> std::ffi::c_int {
    crate::game::scripting::doscript_blargs(root, method, &[bl1 as *mut _, bl2 as *mut _])
}


// ─── inline helper: groups array access ──────────────────────────────────────

#[inline]
unsafe fn groups_get(groupid: usize, slot: usize) -> c_uint {
    groups_raw[groupid.min(MAX_GROUPS - 1) * MAX_GROUP_MEMBERS + slot.min(MAX_GROUP_MEMBERS - 1)]
}

#[allow(dead_code)]
#[inline]
unsafe fn groups_set(groupid: usize, slot: usize, val: c_uint) {
    groups_raw[groupid.min(MAX_GROUPS - 1) * MAX_GROUP_MEMBERS + slot.min(MAX_GROUP_MEMBERS - 1)] = val;
}

// ─── wfifop_copy: write a counted string into the send buffer ─────────────────

/// Copy `len` bytes from `src` into the send-buffer at `pos`.
#[inline]
unsafe fn wfifop_copy(fd: c_int, pos: usize, src: *const u8, len: usize) {
    let dst = rust_session_wdata_ptr(fd, pos);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(src, dst, len);
    }
}

/// Write a big-endian u16 into the send buffer at `pos`.
#[inline]
unsafe fn wfifow_be(fd: c_int, pos: usize, val: u16) {
    let p = rust_session_wdata_ptr(fd, pos) as *mut u16;
    if !p.is_null() { p.write_unaligned(val.to_be()); }
}

/// Write a big-endian u32 into the send buffer at `pos`.
#[inline]
unsafe fn wfifol_be(fd: c_int, pos: usize, val: u32) {
    let p = rust_session_wdata_ptr(fd, pos) as *mut u32;
    if !p.is_null() { p.write_unaligned(val.to_be()); }
}

// ─── clif_groupstatus ─────────────────────────────────────────────────────────

/// Send full group status packet to `sd`.  C line 8343.
pub unsafe fn clif_groupstatus(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }

    let mut rogue:   [c_uint; 256] = [0; 256];
    let mut warrior: [c_uint; 256] = [0; 256];
    let mut mage:    [c_uint; 256] = [0; 256];
    let mut poet:    [c_uint; 256] = [0; 256];
    let mut peasant: [c_uint; 256] = [0; 256];
    let mut gm_arr:  [c_uint; 256] = [0; 256];

    let group_count = (*sd).group_count as usize;
    let groupid     = (*sd).groupid as usize;

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    wfifohead((*sd).fd, 65535);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 1, 0x00);
    wfifob((*sd).fd, 3, 0x63);
    wfifob((*sd).fd, 5, 2);
    wfifob((*sd).fd, 6, group_count as u8);

    // First pass: sort members by class path
    let (mut n, mut w, mut r, mut m, mut p, mut g) = (0usize, 0, 0, 0, 0, 0);
    let mut x = 0usize;
    while (n + w + r + m + p + g) < group_count {
        let member_id = groups_get(groupid, x);
        x += 1;
        let tsd = map_id2sd(member_id);
        if tsd.is_null() { continue; }

        // TNL calculation mirrors C exactly
        if (*tsd).status.level < 99 {
            (*tsd).status.maxtnl = classdb_level((*tsd).status.class as c_int, (*tsd).status.level as c_int);
            (*tsd).status.maxtnl = (*tsd).status.maxtnl.saturating_sub(
                classdb_level((*tsd).status.class as c_int, (*tsd).status.level as c_int - 1)
            );
            let lvl_xp = classdb_level((*tsd).status.class as c_int, (*tsd).status.level as c_int);
            (*tsd).status.tnl = lvl_xp.saturating_sub((*tsd).status.exp);
            let maxtnl = (*tsd).status.maxtnl as f32;
            let tnl    = (*tsd).status.tnl as f32;
            if maxtnl > 0.0 {
                (*tsd).status.percentage = ((maxtnl - tnl) / maxtnl * 100.0 + 0.5) + 0.5;
            } else {
                (*tsd).status.percentage = 0.5 + 0.5;
            }
        } else {
            (*tsd).status.percentage =
                ((*tsd).status.exp as f32 / 4_294_967_295.0 * 100.0) + 0.5;
        }
        (*tsd).status.int_percentage = (*tsd).status.percentage as i32;

        match classdb_path((*tsd).status.class as c_int) {
            0 => { peasant[n] = member_id; n += 1; }
            1 => { warrior[w] = member_id; w += 1; }
            2 => { rogue[r]   = member_id; r += 1; }
            3 => { mage[m]    = member_id; m += 1; }
            4 => { poet[p]    = member_id; p += 1; }
            _ => { gm_arr[g]  = member_id; g += 1; }
        }
    }

    // Second pass: emit per-member packet data in path order
    let (mut n, mut w, mut r, mut m, mut p, mut g) = (0usize, 0, 0, 0, 0, 0);
    let mut len = 0usize;
    while (n + w + r + m + p + g) < group_count {
        let tsd = if rogue[r] != 0 {
            let t = map_id2sd(rogue[r]); r += 1; t
        } else if warrior[w] != 0 {
            let t = map_id2sd(warrior[w]); w += 1; t
        } else if mage[m] != 0 {
            let t = map_id2sd(mage[m]); m += 1; t
        } else if poet[p] != 0 {
            let t = map_id2sd(poet[p]); p += 1; t
        } else if peasant[n] != 0 {
            let t = map_id2sd(peasant[n]); n += 1; t
        } else if gm_arr[g] != 0 {
            let t = map_id2sd(gm_arr[g]); g += 1; t
        } else {
            break;
        };
        if tsd.is_null() { continue; }

        // Name (null-terminated string from status.name)
        let name_ptr = (*tsd).status.name.as_ptr();
        let name_len = libc::strlen(name_ptr);

        wfifol_be((*sd).fd, len + 7, (*tsd).bl.id);
        wfifob((*sd).fd, len + 11, name_len as u8);
        wfifop_copy((*sd).fd, len + 12, name_ptr as *const u8, name_len);

        len += 11;
        len += name_len + 1;

        // Leader flag
        if (*sd).group_leader == (*tsd).status.id {
            wfifob((*sd).fd, len, 1);
        } else {
            wfifob((*sd).fd, len, 0);
        }

        wfifob((*sd).fd, len + 1, (*tsd).status.state as u8);
        wfifob((*sd).fd, len + 2, (*tsd).status.face as u8);
        wfifob((*sd).fd, len + 3, (*tsd).status.hair as u8);
        wfifob((*sd).fd, len + 4, (*tsd).status.hair_color as u8);
        wfifob((*sd).fd, len + 5, 0);

        // Helm slot
        let helm_id = pc_isequip(tsd, EQ_HELM);
        if helm_id == 0 || (*tsd).status.setting_flags as c_uint & crate::game::pc::FLAG_HELM == 0
            || itemdb_look(helm_id) == -1
        {
            wfifob((*sd).fd, len + 6, 0);
            wfifow_be((*sd).fd, len + 7, 0xFFFF);
            wfifob((*sd).fd, len + 9, 0);
        } else {
            wfifob((*sd).fd, len + 6, 1);
            if (*tsd).status.equip[EQ_HELM as usize].custom_look != 0 {
                wfifow_be((*sd).fd, len + 7,
                    (*tsd).status.equip[EQ_HELM as usize].custom_look as u16);
                wfifob((*sd).fd, len + 9,
                    (*tsd).status.equip[EQ_HELM as usize].custom_look_color as u8);
            } else {
                wfifow_be((*sd).fd, len + 7, itemdb_look(helm_id) as u16);
                wfifob((*sd).fd, len + 9, itemdb_lookcolor(helm_id) as u8);
            }
        }

        // Face accessory slot
        let faceacc_id = pc_isequip(tsd, EQ_FACEACC);
        if faceacc_id == 0 {
            wfifow_be((*sd).fd, len + 10, 0xFFFF);
            wfifob((*sd).fd, len + 12, 0);
        } else {
            wfifow_be((*sd).fd, len + 10, itemdb_look(faceacc_id) as u16);
            wfifob((*sd).fd, len + 12, itemdb_lookcolor(faceacc_id) as u8);
        }

        // Crown slot
        let crown_id = pc_isequip(tsd, EQ_CROWN);
        if crown_id == 0 {
            wfifow_be((*sd).fd, len + 13, 0xFFFF);
            wfifob((*sd).fd, len + 15, 0);
        } else {
            wfifob((*sd).fd, len + 6, 0); // clears helm flag when crown is present
            if (*tsd).status.equip[EQ_CROWN as usize].custom_look != 0 {
                wfifow_be((*sd).fd, len + 13,
                    (*tsd).status.equip[EQ_CROWN as usize].custom_look as u16);
                wfifob((*sd).fd, len + 15,
                    (*tsd).status.equip[EQ_CROWN as usize].custom_look_color as u8);
            } else {
                wfifow_be((*sd).fd, len + 13, itemdb_look(crown_id) as u16);
                wfifob((*sd).fd, len + 15, itemdb_lookcolor(crown_id) as u8);
            }
        }

        // Second face accessory
        let faceacc2_id = pc_isequip(tsd, EQ_FACEACCTWO);
        if faceacc2_id == 0 {
            wfifow_be((*sd).fd, len + 16, 0xFFFF);
            wfifob((*sd).fd, len + 18, 0);
        } else {
            wfifow_be((*sd).fd, len + 16, itemdb_look(faceacc2_id) as u16);
            wfifob((*sd).fd, len + 18, itemdb_lookcolor(faceacc2_id) as u8);
        }

        len += 12; // move past the equipment bytes

        wfifol_be((*sd).fd, len + 7, (*tsd).max_hp);
        len += 4;
        wfifol_be((*sd).fd, len + 7, (*tsd).status.hp);
        len += 4;
        wfifol_be((*sd).fd, len + 7, (*tsd).max_mp);
        len += 4;
        wfifol_be((*sd).fd, len + 7, (*tsd).status.mp);
        len += 4;
    }

    wfifob((*sd).fd, 6, group_count as u8);

    len += 6;
    wfifow_be((*sd).fd, 1, (len + 3) as u16);
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_grouphealth_update ──────────────────────────────────────────────────

/// Send per-member HP/MP update and re-send full group status.  C line 8565.
pub unsafe fn clif_grouphealth_update(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }

    let group_count = (*sd).group_count as usize;
    let groupid     = (*sd).groupid as usize;

    for x in 0..group_count {
        let tsd = map_id2sd(groups_get(groupid, x));
        if tsd.is_null() { continue; }

        if rust_session_exists((*sd).fd) == 0 {
            rust_session_set_eof((*sd).fd, 8);
            return 0;
        }

        wfifohead((*sd).fd, 512);
        wfifob((*sd).fd, 0, 0xAA);
        wfifob((*sd).fd, 3, 0x63);
        wfifob((*sd).fd, 4, 0x03);
        wfifob((*sd).fd, 5, 0x03);

        wfifol_be((*sd).fd, 6, (*tsd).bl.id);

        let name_ptr = (*tsd).status.name.as_ptr();
        let name_len = libc::strlen(name_ptr);
        wfifob((*sd).fd, 10, name_len as u8);
        wfifop_copy((*sd).fd, 11, name_ptr as *const u8, name_len);

        let mut len = 10usize + name_len + 1;

        wfifol_be((*sd).fd, len, (*tsd).status.hp);
        len += 4;
        wfifol_be((*sd).fd, len, (*tsd).status.mp);
        len += 4;

        wfifow_be((*sd).fd, 1, (len + 3) as u16);
        wfifoset((*sd).fd, encrypt((*sd).fd) as usize);

        clif_groupstatus(sd);
    }
    0
}

// ─── clif_addgroup ────────────────────────────────────────────────────────────

/// Add a player by name to the caller's group.  C line 8638.
pub unsafe fn clif_addgroup(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }

    let name_len = rfifob((*sd).fd, 5) as usize;
    let mut nameof = [0i8; 256];
    let src = rfifop((*sd).fd, 6);
    if !src.is_null() {
        std::ptr::copy_nonoverlapping(src, nameof.as_mut_ptr() as *mut u8, name_len.min(255));
    }

    let tsd = map_name2sd(nameof.as_ptr());
    if tsd.is_null() { return 0; }

    if (*sd).status.gm_level == 0 && ((*tsd).optFlags & OPT_FLAG_STEALTH) != 0 {
        return 0;
    }

    if (*tsd).status.id == (*sd).status.id {
        clif_sendminitext(sd, b"You can't group yourself...\0".as_ptr() as *const c_char);
        return 0;
    }

    if (*tsd).group_count != 0 {
        if (*tsd).group_leader == (*sd).group_leader && (*sd).group_leader == (*sd).bl.id {
            clif_leavegroup(tsd);
            return 0;
        }
    }

    if (*sd).group_count >= MAX_GROUP_MEMBERS as c_int {
        clif_sendminitext(sd, b"Your group is already full.\0".as_ptr() as *const c_char);
        return 0;
    }

    if (*tsd).status.state == 1 {
        clif_sendminitext(sd, b"They are unable to join your party.\0".as_ptr() as *const c_char);
        return 0;
    }

    // Map canGroup check
    let sd_map_ok = if map_is_loaded((*sd).bl.m) {
        (*get_map_ptr((*sd).bl.m)).can_group
    } else { 0 };
    if sd_map_ok == 0 {
        clif_sendminitext(sd,
            b"You are unable to join a party. (Grouping disabled on map)\0".as_ptr() as *const c_char);
        return 0;
    }

    let tsd_map_ok = if map_is_loaded((*tsd).bl.m) {
        (*get_map_ptr((*tsd).bl.m)).can_group
    } else { 0 };
    if tsd_map_ok == 0 {
        clif_sendminitext(sd,
            b"They are unable to join your party. (Grouping disabled on map)\0".as_ptr() as *const c_char);
        return 0;
    }

    if (*tsd).status.setting_flags as c_uint & FLAG_GROUP == 0 {
        clif_sendminitext(sd, b"They have refused to join your party.\0".as_ptr() as *const c_char);
        return 0;
    }
    if (*tsd).group_count != 0 {
        clif_sendminitext(sd, b"They have refused to join your party.\0".as_ptr() as *const c_char);
        return 0;
    }

    let groupid = (*sd).groupid as usize;

    if (*sd).group_count == 0 {
        // Find first empty group slot
        let mut x = 1usize;
        while x < MAX_GROUPS {
            if groups_get(x, 0) == 0 { break; }
            x += 1;
        }
        if x == MAX_GROUPS {
            clif_sendminitext(sd,
                b"All groups are currently occupied, please try again later.\0".as_ptr() as *const c_char);
            return 0;
        }
        groups_set(x, 0, (*sd).status.id);
        (*sd).group_leader = groups_get(x, 0);
        groups_set(x, 1, (*tsd).status.id);
        (*sd).group_count = 2;
        (*sd).groupid = x as c_uint;
        (*tsd).groupid = (*sd).groupid;
    } else {
        let gc = (*sd).group_count as usize;
        groups_set(groupid, gc, (*tsd).status.id);
        (*sd).group_count += 1;
        (*tsd).groupid = (*sd).groupid;
    }

    let mut buff = [0i8; 256];
    libc::snprintf(
        buff.as_mut_ptr(), buff.len(),
        b"%s is joining the group.\0".as_ptr() as *const c_char,
        (*tsd).status.name.as_ptr(),
    );

    clif_updategroup(sd, buff.as_mut_ptr());
    clif_groupstatus(sd);
    0
}

// ─── clif_updategroup ─────────────────────────────────────────────────────────

/// Broadcast a group message to all members and refresh their status.  C line 8727.
pub unsafe fn clif_updategroup(
    sd:      *mut MapSessionData,
    message: *mut c_char,
) -> c_int {
    if sd.is_null() { return 0; }

    let group_count = (*sd).group_count as usize;
    let groupid     = (*sd).groupid as usize;

    for x in 0..group_count {
        let tsd = map_id2sd(groups_get(groupid, x));
        if tsd.is_null() { continue; }

        (*tsd).group_count  = (*sd).group_count;
        (*tsd).group_leader = (*sd).group_leader;

        if (*tsd).group_count == 1 {
            groups_set(groupid, 0, 0);
            (*tsd).group_count = 0;
            (*tsd).groupid     = 0;
        }

        clif_sendminitext(tsd, message);
        clif_grouphealth_update(tsd);
        clif_groupstatus(tsd);
    }
    0
}

// ─── clif_leavegroup ──────────────────────────────────────────────────────────

/// Remove the caller from their current group.  C line 8756.
pub unsafe fn clif_leavegroup(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }

    let group_count = (*sd).group_count as usize;
    let groupid     = (*sd).groupid as usize;

    let mut taken = 0i32;
    for x in 0..group_count {
        if taken == 1 {
            let val = groups_get(groupid, x);
            groups_set(groupid, x - 1, val);
        } else if groups_get(groupid, x) == (*sd).status.id {
            groups_set(groupid, x, 0);
            taken = 1;
            if (*sd).group_leader == (*sd).status.id {
                (*sd).group_leader = groups_get(groupid, 0);
            }
        }
    }

    if (*sd).group_leader == 0 {
        (*sd).group_leader = groups_get(groupid, 0);
    }

    let mut buff = [0i8; 256];
    libc::snprintf(
        buff.as_mut_ptr(), buff.len(),
        b"%s is leaving the group.\0".as_ptr() as *const c_char,
        (*sd).status.name.as_ptr(),
    );
    (*sd).group_count -= 1;
    clif_updategroup(sd, buff.as_mut_ptr());

    let msg_left = b"You have left the group.\0".as_ptr() as *const c_char;
    clif_sendminitext(sd, msg_left);

    (*sd).group_count = 0;
    (*sd).groupid     = 0;
    clif_groupstatus(sd);
    0
}

// ─── clif_findmount ───────────────────────────────────────────────────────────

/// Find a mountable mob adjacent to `sd` and fire the onMount script.  C line 8794.
pub unsafe fn clif_findmount(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }

    let (mut x, mut y) = ((*sd).bl.x as c_int, (*sd).bl.y as c_int);
    match (*sd).status.side {
        0 => { y -= 1; }
        1 => { x += 1; }
        2 => { y += 1; }
        3 => { x -= 1; }
        _ => {}
    }

    let bl = map_firstincell((*sd).bl.m as c_int, x, y, BL_MOB);
    if bl.is_null() { return 0; }

    let mob = bl as *mut MobSpawnData;

    if (*sd).status.state != 0 { return 0; }

    let can_mount = if map_is_loaded((*sd).bl.m) {
        (*get_map_ptr((*sd).bl.m)).can_mount
    } else { 0 };
    if can_mount == 0 && (*sd).status.gm_level == 0 {
        clif_sendminitext(sd, b"You cannot mount here.\0".as_ptr() as *const c_char);
        return 0;
    }

    sl_doscript_2(b"onMount\0".as_ptr() as *const c_char, std::ptr::null(), &mut (*sd).bl as *mut BlockList, &mut (*mob).bl as *mut BlockList);
    0
}

// ─── clif_isingroup ───────────────────────────────────────────────────────────

/// Return 1 if `tsd` is in `sd`'s group, 0 otherwise.  C line 9139.
pub unsafe fn clif_isingroup(
    sd:  *mut MapSessionData,
    tsd: *mut MapSessionData,
) -> c_int {
    if sd.is_null() || tsd.is_null() { return 0; }

    let group_count = (*sd).group_count as usize;
    let groupid     = (*sd).groupid as usize;

    for x in 0..group_count {
        if groups_get(groupid, x) == (*tsd).bl.id {
            return 1;
        }
    }
    0
}

// ─── clif_canmove_sub_inner ───────────────────────────────────────────────────

/// Typed inner function replacing the old variadic `clif_canmove_sub` callback.
///
/// Sets `sd->canmove = 1` if `bl` blocks movement.
/// C line 9148.
pub unsafe fn clif_canmove_sub_inner(
    bl: *mut BlockList,
    sd: *mut MapSessionData,
) -> i32 {
    if bl.is_null() { return 0; }
    if sd.is_null() { return 0; }

    if (*sd).canmove == 1 { return 0; }

    if (*bl).bl_type as c_int == BL_PC {
        let tsd = bl as *mut MapSessionData;
        if !tsd.is_null() {
            let show_ghosts = if map_is_loaded((*tsd).bl.m) {
                (*get_map_ptr((*tsd).bl.m)).show_ghosts
            } else { 0 };

            if (show_ghosts != 0
                && (*tsd).status.state == 1       // tsd is dead (ghost)
                && (*tsd).bl.id != (*sd).bl.id    // not self
                && (*sd).status.state != 1        // sd is alive
                && ((*sd).optFlags & OPT_FLAG_GHOSTS) == 0)
                || ((*tsd).status.state == -1)
                || ((*tsd).status.gm_level != 0 && ((*tsd).optFlags & OPT_FLAG_STEALTH) != 0)
            {
                return 0;
            }
        }
    }

    if (*bl).bl_type as c_int == BL_MOB {
        let mob = bl as *mut MobSpawnData;
        if (*mob).state == crate::game::mob::MOB_DEAD {
            return 0;
        }
    }

    if (*bl).bl_type as c_int == BL_NPC && (*bl).subtype == 2 {
        return 0;
    }

    if (*bl).id != (*sd).bl.id {
        (*sd).canmove = 1;
    }
    0
}

// ─── clif_canmove ─────────────────────────────────────────────────────────────

/// Check whether `sd` can move in direction `direct`.  C line 9189.
///
/// Returns `sd->canmove` (0 = blocked by nothing, 1 = something is blocking).
pub unsafe fn clif_canmove(
    sd:     *mut MapSessionData,
    direct: c_int,
) -> c_int {
    if sd.is_null() { return 0; }

    if (*sd).status.gm_level != 0 { return 0; }

    let (mut nx, mut ny) = (0i32, 0i32);
    match direct {
        0 => { ny = (*sd).bl.y as c_int - 1; }
        1 => { nx = (*sd).bl.x as c_int + 1; }
        2 => { ny = (*sd).bl.y as c_int + 1; }
        3 => { nx = (*sd).bl.x as c_int - 1; }
        _ => {}
    }

    foreach_in_cell((*sd).bl.m as i32, (*sd).bl.x as i32, (*sd).bl.y as i32, BL_MOB, |bl| clif_canmove_sub_inner(bl, sd));
    foreach_in_cell((*sd).bl.m as i32, (*sd).bl.x as i32, (*sd).bl.y as i32, BL_PC,  |bl| clif_canmove_sub_inner(bl, sd));
    foreach_in_cell((*sd).bl.m as i32, nx, ny, BL_PC, |bl| clif_canmove_sub_inner(bl, sd));

    if clif_object_canmove((*sd).bl.m as c_int, nx, ny, direct) != 0 {
        (*sd).canmove = 1;
    }
    (*sd).canmove
}

// ─── clif_mapselect ───────────────────────────────────────────────────────────

/// Send the map-selection UI to `sd`.  C line 9306.
///
/// # Safety
/// `x0`, `y0`, `mname`, `id`, `x1`, `y1` must each point to at least `i` valid elements.
pub unsafe fn clif_mapselect(
    sd:    *mut MapSessionData,
    wm:    *const c_char,
    x0:    *const c_int,
    y0:    *const c_int,
    mname: *const *const c_char,
    id:    *const c_uint,
    x1:    *const c_int,
    y1:    *const c_int,
    i:     c_int,
) -> c_int {
    if sd.is_null() { return 0; }

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    wfifohead((*sd).fd, 65535);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 3, 0x2E);
    wfifob((*sd).fd, 4, 0x03);

    let wm_len = libc::strlen(wm);
    wfifob((*sd).fd, 5, wm_len as u8);
    wfifop_copy((*sd).fd, 6, wm as *const u8, wm_len);
    let mut len = wm_len + 1;

    wfifob((*sd).fd, len + 5, i as u8);
    wfifob((*sd).fd, len + 6, 0); // maybe look?
    len += 2;

    for x in 0..(i as usize) {
        wfifow_be((*sd).fd, len + 5, *x0.add(x) as u16);
        wfifow_be((*sd).fd, len + 7, *y0.add(x) as u16);
        len += 4;

        let mn = *mname.add(x);
        let mn_len = libc::strlen(mn);
        wfifob((*sd).fd, len + 5, mn_len as u8);
        wfifop_copy((*sd).fd, len + 6, mn as *const u8, mn_len);
        len += mn_len + 1;

        wfifol_be((*sd).fd, len + 5, *id.add(x));
        wfifow_be((*sd).fd, len + 9,  *x1.add(x) as u16);
        wfifow_be((*sd).fd, len + 11, *y1.add(x) as u16);
        len += 8;

        // Count of entries (i) as u16, then indices 0..i
        wfifow_be((*sd).fd, len + 5, i as u16);
        len += 2;
        for y in 0..(i as usize) {
            wfifow_be((*sd).fd, len + 5, y as u16);
            len += 2;
        }
    }

    wfifow_be((*sd).fd, 1, (len + 3) as u16);
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_pb_sub ──────────────────────────────────────────────────────────────

/// Typed inner function replacing the old variadic `clif_pb_sub` callback.
///
/// Powerboard callback: writes one player entry.
/// `bl` is the player being rendered, `sd` is the player whose WFIFO buffer is being written,
/// `len_ptr` points to `int[2]`: len[0] = byte offset, len[1] = count (mutated in-place).
/// C line 9352.
pub unsafe fn clif_pb_sub_inner(
    bl: *mut BlockList,
    sd: *mut MapSessionData,
    len_ptr: *mut c_int,
) -> i32 {
    if bl.is_null() { return 0; }

    let tsd = bl as *mut MapSessionData;
    if tsd.is_null() { return 0; }
    if sd.is_null() { return 0; }
    if len_ptr.is_null() { return 0; }

    let mut path = classdb_path((*tsd).status.class as c_int);
    if path == 5 { path = 2; }
    if path == 50 || path == 0 { return 0; }

    let power_rating: c_uint =
        (*tsd).status.basehp.saturating_add((*tsd).status.basemp);

    let offset = *len_ptr as usize;

    wfifol_be((*sd).fd, offset + 8, (*tsd).bl.id);
    wfifob((*sd).fd, offset + 12, path as u8);
    wfifol_be((*sd).fd, offset + 13, power_rating);
    wfifob((*sd).fd, offset + 17, (*tsd).status.armor_color as u8);

    let name_ptr = (*tsd).status.name.as_ptr();
    let name_len = libc::strlen(name_ptr);
    wfifob((*sd).fd, offset + 18, name_len as u8);
    wfifop_copy((*sd).fd, offset + 19, name_ptr as *const u8, name_len);

    *len_ptr += (name_len + 11) as c_int;
    // len[1] is the count — stored at len_ptr + 1
    *len_ptr.add(1) += 1;
    0
}

// ─── clif_sendpowerboard ──────────────────────────────────────────────────────

/// Send the powerboard (class ranking) to `sd`.  C line 9389.
pub unsafe fn clif_sendpowerboard(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    let mut len: [c_int; 2] = [0, 0];

    wfifohead((*sd).fd, 65535);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 3, 0x46);
    wfifob((*sd).fd, 4, 0x03);
    wfifob((*sd).fd, 5, 1);

    let len_ptr = len.as_mut_ptr();
    foreach_in_area(
        (*sd).bl.m as i32,
        (*sd).bl.x as i32,
        (*sd).bl.y as i32,
        AreaType::SameMap,
        BL_PC,
        |bl| clif_pb_sub_inner(bl, sd, len_ptr),
    );

    wfifow_be((*sd).fd, 6, len[1] as u16);
    wfifow_be((*sd).fd, 1, (len[0] + 5) as u16);
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_parseparcel ─────────────────────────────────────────────────────────

/// Handle an incoming parcel packet — inform player to see the kingdom messenger.  C line 9412.
pub unsafe fn clif_parseparcel(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }
    clif_sendminitext(
        sd,
        b"You should go see your kingdom's messenger to collect this parcel\0".as_ptr()
            as *const c_char,
    );
    0
}

// ─── clif_huntertoggle ────────────────────────────────────────────────────────

/// Toggle hunter mode on/off for `sd` and persist to database.  C line 9419.
pub unsafe fn clif_huntertoggle(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }

    (*sd).hunter = rfifob((*sd).fd, 5) as c_int;

    let tag_len = rfifob((*sd).fd, 6) as usize;
    let mut hunter_tag = [0i8; 40];
    let src = rfifop((*sd).fd, 7);
    if !src.is_null() {
        std::ptr::copy_nonoverlapping(src, hunter_tag.as_mut_ptr() as *mut u8, tag_len.min(39));
    }

    let hunter_val = (*sd).hunter;
    let char_id = (*sd).status.id as i32;
    let hunter_tag_str = std::ffi::CStr::from_ptr(hunter_tag.as_ptr())
        .to_str()
        .unwrap_or("")
        .to_owned();

    blocking_run(async move {
        sqlx::query(
            "UPDATE `Character` SET `ChaHunter` = ?, `ChaHunterNote` = ? WHERE `ChaId` = ?"
        )
        .bind(hunter_val)
        .bind(hunter_tag_str)
        .bind(char_id)
        .execute(get_pool())
        .await
        .ok();
    });

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }
    wfifohead((*sd).fd, 5);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 3, 0x83);
    wfifob((*sd).fd, 5, (*sd).hunter as u8);
    wfifow_be((*sd).fd, 1, 5);
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_sendhunternote ──────────────────────────────────────────────────────

/// Fetch and send the hunter note for a named player.  C line 9468.
pub unsafe fn clif_sendhunternote(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }

    let hname_len = rfifob((*sd).fd, 5) as usize;
    let mut huntername = [0i8; 16];
    let src = rfifop((*sd).fd, 6);
    if !src.is_null() {
        std::ptr::copy_nonoverlapping(src, huntername.as_mut_ptr() as *mut u8, hname_len.min(15));
    }

    // Don't send your own hunter note to yourself
    if libc::strcasecmp((*sd).status.name.as_ptr(), huntername.as_ptr()) == 0 {
        return 1;
    }

    let hunter_name_str = std::ffi::CStr::from_ptr(huntername.as_ptr())
        .to_str()
        .unwrap_or("")
        .to_owned();

    let note_result = blocking_run(async move {
        sqlx::query_scalar::<_, String>(
            "SELECT `ChaHunterNote` FROM `Character` WHERE `ChaName` = ?"
        )
        .bind(hunter_name_str)
        .fetch_optional(get_pool())
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
    });

    // Copy note into fixed-size buffer for packet building
    let mut hunternote = [0i8; 41];
    for (i, b) in note_result.bytes().take(40).enumerate() {
        hunternote[i] = b as i8;
    }

    // If empty note, skip sending
    if hunternote[0] == 0 {
        return 1;
    }

    wfifohead((*sd).fd, 65535);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 3, 0x84);

    let hname_out = libc::strlen(huntername.as_ptr());
    wfifob((*sd).fd, 5, hname_out as u8);

    let mut len = 6usize;
    wfifop_copy((*sd).fd, len, huntername.as_ptr() as *const u8, hname_out);
    len += hname_out;

    let note_len = libc::strlen(hunternote.as_ptr());
    wfifob((*sd).fd, len, note_len as u8);
    len += 1;
    wfifop_copy((*sd).fd, len, hunternote.as_ptr() as *const u8, note_len);
    len += note_len;

    wfifow_be((*sd).fd, 1, len as u16);
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}
