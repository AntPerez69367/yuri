//!
//! Group state is stored in the global flat array
//! `groups[MAX_GROUPS][MAX_GROUP_MEMBERS]` (256×256 u32 values, 65536 total),
//! accessed as `groups[groupid * 256 + slot]`.

#![allow(non_snake_case, clippy::wildcard_imports)]


use crate::database::map_db::BlockList;
use crate::database::map_db::{get_map_ptr, map_is_loaded};
use crate::session::{session_exists, SessionId};
use crate::game::mob::MobSpawnData;
use crate::database::get_pool;

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
    rfifob, rfifop, wfifop,
    wfifob, wfifoset, wfifohead,
};
use crate::game::block::AreaType;
use crate::game::block_grid;

// ─── Constants ────────────────────────────────────────────────────────────────

const MAX_GROUPS: usize = 256;

// BL_ALL: all block-list types (from map_server.h enum)
const BL_ALL: i32 = 0x0F;


use crate::game::map_parse::chat::clif_sendminitext;
use crate::game::map_server::{map_name2sd, groups as groups_raw};
use crate::game::map_parse::movement::clif_object_canmove;
use crate::database::class_db::{path as classdb_path, level as classdb_level};
use crate::database::item_db;

// pc_isequip returns i32; usage here expects u32 — wrap with cast.
#[inline]
unsafe fn pc_isequip(sd: *mut MapSessionData, slot: i32) -> u32 {
    crate::game::pc::rust_pc_isequip(sd, slot) as u32
}

/// Dispatch a Lua event with two block_list arguments.
#[allow(dead_code)]
unsafe fn sl_doscript_2(root: *const i8, method: *const i8, bl1: *mut crate::database::map_db::BlockList, bl2: *mut crate::database::map_db::BlockList) -> i32 {
    crate::game::scripting::doscript_blargs(root, method, &[bl1 as *mut _, bl2 as *mut _])
}


// ─── inline helper: groups array access ──────────────────────────────────────

#[inline]
fn groups_get(groupid: usize, slot: usize) -> u32 {
    groups_raw()[groupid.min(MAX_GROUPS - 1) * MAX_GROUP_MEMBERS + slot.min(MAX_GROUP_MEMBERS - 1)]
}

#[allow(dead_code)]
#[inline]
fn groups_set(groupid: usize, slot: usize, val: u32) {
    groups_raw()[groupid.min(MAX_GROUPS - 1) * MAX_GROUP_MEMBERS + slot.min(MAX_GROUP_MEMBERS - 1)] = val;
}

// ─── wfifop_copy: write a counted string into the send buffer ─────────────────

/// Copy `len` bytes from `src` into the send-buffer at `pos`.
#[inline]
unsafe fn wfifop_copy(fd: SessionId, pos: usize, src: *const u8, len: usize) {
    let dst = wfifop(fd, pos);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(src, dst, len);
    }
}

/// Write a big-endian u16 into the send buffer at `pos`.
#[inline]
unsafe fn wfifow_be(fd: SessionId, pos: usize, val: u16) {
    let p = wfifop(fd, pos) as *mut u16;
    if !p.is_null() { p.write_unaligned(val.to_be()); }
}

/// Write a big-endian u32 into the send buffer at `pos`.
#[inline]
unsafe fn wfifol_be(fd: SessionId, pos: usize, val: u32) {
    let p = wfifop(fd, pos) as *mut u32;
    if !p.is_null() { p.write_unaligned(val.to_be()); }
}

// ─── clif_groupstatus ─────────────────────────────────────────────────────────

/// Send full group status packet to `sd`.  C line 8343.
pub unsafe fn clif_groupstatus(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }

    let mut rogue:   [u32; 256] = [0; 256];
    let mut warrior: [u32; 256] = [0; 256];
    let mut mage:    [u32; 256] = [0; 256];
    let mut poet:    [u32; 256] = [0; 256];
    let mut peasant: [u32; 256] = [0; 256];
    let mut gm_arr:  [u32; 256] = [0; 256];

    let group_count = (*sd).group_count as usize;
    let groupid     = (*sd).groupid as usize;

    if !session_exists((*sd).fd) {
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
        let tsd_arc = match crate::game::map_server::map_id2sd_pc(member_id) {
            Some(a) => a, None => continue,
        };
        let mut _tsd_guard = tsd_arc.write();
        let tsd = &mut *_tsd_guard as *mut MapSessionData;

        // TNL calculation mirrors C exactly
        if (*tsd).status.level < 99 {
            (*tsd).status.maxtnl = classdb_level((*tsd).status.class as i32, (*tsd).status.level as i32);
            (*tsd).status.maxtnl = (*tsd).status.maxtnl.saturating_sub(
                classdb_level((*tsd).status.class as i32, (*tsd).status.level as i32 - 1)
            );
            let lvl_xp = classdb_level((*tsd).status.class as i32, (*tsd).status.level as i32);
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

        match classdb_path((*tsd).status.class as i32) {
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
        let member_id = if rogue[r] != 0 {
            let id = rogue[r]; r += 1; id
        } else if warrior[w] != 0 {
            let id = warrior[w]; w += 1; id
        } else if mage[m] != 0 {
            let id = mage[m]; m += 1; id
        } else if poet[p] != 0 {
            let id = poet[p]; p += 1; id
        } else if peasant[n] != 0 {
            let id = peasant[n]; n += 1; id
        } else if gm_arr[g] != 0 {
            let id = gm_arr[g]; g += 1; id
        } else {
            break;
        };
        let tsd_arc = match crate::game::map_server::map_id2sd_pc(member_id) {
            Some(a) => a, None => continue,
        };
        let _tsd_guard = tsd_arc.write();
        let tsd = &*_tsd_guard as *const MapSessionData as *mut MapSessionData;

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
        let helm_item = if helm_id != 0 { Some(item_db::search(helm_id)) } else { None };
        let helm_look = helm_item.as_ref().map_or(-1, |i| i.look);
        if helm_id == 0 || (*tsd).status.setting_flags as u32 & crate::game::pc::FLAG_HELM == 0
            || helm_look == -1
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
                wfifow_be((*sd).fd, len + 7, helm_look as u16);
                wfifob((*sd).fd, len + 9, helm_item.as_ref().unwrap().look_color as u8);
            }
        }

        // Face accessory slot
        let faceacc_id = pc_isequip(tsd, EQ_FACEACC);
        if faceacc_id == 0 {
            wfifow_be((*sd).fd, len + 10, 0xFFFF);
            wfifob((*sd).fd, len + 12, 0);
        } else {
            let faceacc_item = item_db::search(faceacc_id);
            wfifow_be((*sd).fd, len + 10, faceacc_item.look as u16);
            wfifob((*sd).fd, len + 12, faceacc_item.look_color as u8);
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
                let crown_item = item_db::search(crown_id);
                wfifow_be((*sd).fd, len + 13, crown_item.look as u16);
                wfifob((*sd).fd, len + 15, crown_item.look_color as u8);
            }
        }

        // Second face accessory
        let faceacc2_id = pc_isequip(tsd, EQ_FACEACCTWO);
        if faceacc2_id == 0 {
            wfifow_be((*sd).fd, len + 16, 0xFFFF);
            wfifob((*sd).fd, len + 18, 0);
        } else {
            let faceacc2_item = item_db::search(faceacc2_id);
            wfifow_be((*sd).fd, len + 16, faceacc2_item.look as u16);
            wfifob((*sd).fd, len + 18, faceacc2_item.look_color as u8);
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
pub unsafe fn clif_grouphealth_update(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }

    let group_count = (*sd).group_count as usize;
    let groupid     = (*sd).groupid as usize;

    for x in 0..group_count {
        let tsd_arc = match crate::game::map_server::map_id2sd_pc(groups_get(groupid, x)) {
            Some(a) => a, None => continue,
        };
        let _tsd_guard = tsd_arc.write();
        let tsd = &*_tsd_guard as *const MapSessionData as *mut MapSessionData;

        if !session_exists((*sd).fd) {
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
pub unsafe fn clif_addgroup(sd: *mut MapSessionData) -> i32 {
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
        clif_sendminitext(sd, b"You can't group yourself...\0".as_ptr() as *const i8);
        return 0;
    }

    if (*tsd).group_count != 0 {
        if (*tsd).group_leader == (*sd).group_leader && (*sd).group_leader == (*sd).bl.id {
            clif_leavegroup(tsd);
            return 0;
        }
    }

    if (*sd).group_count >= MAX_GROUP_MEMBERS as i32 {
        clif_sendminitext(sd, b"Your group is already full.\0".as_ptr() as *const i8);
        return 0;
    }

    if (*tsd).status.state == 1 {
        clif_sendminitext(sd, b"They are unable to join your party.\0".as_ptr() as *const i8);
        return 0;
    }

    // Map canGroup check
    let sd_map_ok = if map_is_loaded((*sd).bl.m) {
        (*get_map_ptr((*sd).bl.m)).can_group
    } else { 0 };
    if sd_map_ok == 0 {
        clif_sendminitext(sd,
            b"You are unable to join a party. (Grouping disabled on map)\0".as_ptr() as *const i8);
        return 0;
    }

    let tsd_map_ok = if map_is_loaded((*tsd).bl.m) {
        (*get_map_ptr((*tsd).bl.m)).can_group
    } else { 0 };
    if tsd_map_ok == 0 {
        clif_sendminitext(sd,
            b"They are unable to join your party. (Grouping disabled on map)\0".as_ptr() as *const i8);
        return 0;
    }

    if (*tsd).status.setting_flags as u32 & FLAG_GROUP == 0 {
        clif_sendminitext(sd, b"They have refused to join your party.\0".as_ptr() as *const i8);
        return 0;
    }
    if (*tsd).group_count != 0 {
        clif_sendminitext(sd, b"They have refused to join your party.\0".as_ptr() as *const i8);
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
                b"All groups are currently occupied, please try again later.\0".as_ptr() as *const i8);
            return 0;
        }
        groups_set(x, 0, (*sd).status.id);
        (*sd).group_leader = groups_get(x, 0);
        groups_set(x, 1, (*tsd).status.id);
        (*sd).group_count = 2;
        (*sd).groupid = x as u32;
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
        b"%s is joining the group.\0".as_ptr() as *const i8,
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
    message: *mut i8,
) -> i32 {
    if sd.is_null() { return 0; }

    let group_count = (*sd).group_count as usize;
    let groupid     = (*sd).groupid as usize;

    for x in 0..group_count {
        let tsd_arc = match crate::game::map_server::map_id2sd_pc(groups_get(groupid, x)) {
            Some(a) => a, None => continue,
        };
        let mut _tsd_guard = tsd_arc.write();
        let tsd = &mut *_tsd_guard as *mut MapSessionData;

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
pub unsafe fn clif_leavegroup(sd: *mut MapSessionData) -> i32 {
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
        b"%s is leaving the group.\0".as_ptr() as *const i8,
        (*sd).status.name.as_ptr(),
    );
    (*sd).group_count -= 1;
    clif_updategroup(sd, buff.as_mut_ptr());

    let msg_left = b"You have left the group.\0".as_ptr() as *const i8;
    clif_sendminitext(sd, msg_left);

    (*sd).group_count = 0;
    (*sd).groupid     = 0;
    clif_groupstatus(sd);
    0
}

// ─── clif_findmount ───────────────────────────────────────────────────────────

/// Find a mountable mob adjacent to `sd` and fire the onMount script.  C line 8794.
pub unsafe fn clif_findmount(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }

    let (mut x, mut y) = ((*sd).bl.x as i32, (*sd).bl.y as i32);
    match (*sd).status.side {
        0 => { y -= 1; }
        1 => { x += 1; }
        2 => { y += 1; }
        3 => { x -= 1; }
        _ => {}
    }

    let mob_id = match block_grid::first_in_cell((*sd).bl.m as usize, x as u16, y as u16, BL_MOB) {
        Some(id) => id,
        None => return 0,
    };
    let mob_arc = match crate::game::map_server::map_id2mob_ref(mob_id) {
        Some(a) => a,
        None => return 0,
    };
    let mut mob_guard = mob_arc.write();
    let mob = &mut *mob_guard as *mut MobSpawnData;

    if (*sd).status.state != 0 { return 0; }

    let can_mount = if map_is_loaded((*sd).bl.m) {
        (*get_map_ptr((*sd).bl.m)).can_mount
    } else { 0 };
    if can_mount == 0 && (*sd).status.gm_level == 0 {
        clif_sendminitext(sd, b"You cannot mount here.\0".as_ptr() as *const i8);
        return 0;
    }

    sl_doscript_2(b"onMount\0".as_ptr() as *const i8, std::ptr::null(), &mut (*sd).bl as *mut BlockList, &mut (*mob).bl as *mut BlockList);
    0
}

// ─── clif_isingroup ───────────────────────────────────────────────────────────

/// Return 1 if `tsd` is in `sd`'s group, 0 otherwise.  C line 9139.
pub unsafe fn clif_isingroup(
    sd:  *mut MapSessionData,
    tsd: *mut MapSessionData,
) -> i32 {
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

    if (*bl).bl_type as i32 == BL_PC {
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

    if (*bl).bl_type as i32 == BL_MOB {
        let mob = bl as *mut MobSpawnData;
        if (*mob).state == crate::game::mob::MOB_DEAD {
            return 0;
        }
    }

    if (*bl).bl_type as i32 == BL_NPC && (*bl).subtype == 2 {
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
    direct: i32,
) -> i32 {
    if sd.is_null() { return 0; }

    if (*sd).status.gm_level != 0 { return 0; }

    let (mut nx, mut ny) = (0i32, 0i32);
    match direct {
        0 => { ny = (*sd).bl.y as i32 - 1; }
        1 => { nx = (*sd).bl.x as i32 + 1; }
        2 => { ny = (*sd).bl.y as i32 + 1; }
        3 => { nx = (*sd).bl.x as i32 - 1; }
        _ => {}
    }

    if let Some(grid) = block_grid::get_grid((*sd).bl.m as usize) {
        let cell_ids = grid.ids_at_tile((*sd).bl.x, (*sd).bl.y);
        for id in cell_ids {
            if let Some(arc) = crate::game::map_server::map_id2mob_ref(id) {
                let mut guard = arc.write();
                clif_canmove_sub_inner(&mut guard.bl as *mut BlockList, sd);
            } else if let Some(arc) = crate::game::map_server::map_id2sd_pc(id) {
                let mut guard = arc.write();
                clif_canmove_sub_inner(&mut guard.bl as *mut BlockList, sd);
            }
        }
        let cell_ids2 = grid.ids_at_tile(nx as u16, ny as u16);
        for id in cell_ids2 {
            if let Some(arc) = crate::game::map_server::map_id2sd_pc(id) {
                let mut guard = arc.write();
                clif_canmove_sub_inner(&mut guard.bl as *mut BlockList, sd);
            }
        }
    }

    if clif_object_canmove((*sd).bl.m as i32, nx, ny, direct) != 0 {
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
    wm:    *const i8,
    x0:    *const i32,
    y0:    *const i32,
    mname: *const *const i8,
    id:    *const u32,
    x1:    *const i32,
    y1:    *const i32,
    i:     i32,
) -> i32 {
    if sd.is_null() { return 0; }

    if !session_exists((*sd).fd) {
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

///
/// Powerboard callback: writes one player entry.
/// `bl` is the player being rendered, `sd` is the player whose WFIFO buffer is being written,
/// `len_ptr` points to `int[2]`: len[0] = byte offset, len[1] = count (mutated in-place).
/// C line 9352.
pub unsafe fn clif_pb_sub_inner(
    bl: *mut BlockList,
    sd: *mut MapSessionData,
    len_ptr: *mut i32,
) -> i32 {
    if bl.is_null() { return 0; }

    let tsd = bl as *mut MapSessionData;
    if tsd.is_null() { return 0; }
    if sd.is_null() { return 0; }
    if len_ptr.is_null() { return 0; }

    let mut path = classdb_path((*tsd).status.class as i32);
    if path == 5 { path = 2; }
    if path == 50 || path == 0 { return 0; }

    let power_rating: u32 =
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

    *len_ptr += (name_len + 11) as i32;
    // len[1] is the count — stored at len_ptr + 1
    *len_ptr.add(1) += 1;
    0
}

// ─── clif_sendpowerboard ──────────────────────────────────────────────────────

/// Send the powerboard (class ranking) to `sd`.  C line 9389.
pub unsafe fn clif_sendpowerboard(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }

    if !session_exists((*sd).fd) {
        return 0;
    }

    let mut len: [i32; 2] = [0, 0];

    wfifohead((*sd).fd, 65535);
    wfifob((*sd).fd, 0, 0xAA);
    wfifob((*sd).fd, 3, 0x46);
    wfifob((*sd).fd, 4, 0x03);
    wfifob((*sd).fd, 5, 1);

    let len_ptr = len.as_mut_ptr();
    if let Some(grid) = block_grid::get_grid((*sd).bl.m as usize) {
        let slot = &*get_map_ptr((*sd).bl.m);
        let ids = block_grid::ids_in_area(grid, (*sd).bl.x as i32, (*sd).bl.y as i32, AreaType::SameMap, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if let Some(arc) = crate::game::map_server::map_id2sd_pc(id) {
                let mut guard = arc.write();
                clif_pb_sub_inner(&mut guard.bl as *mut BlockList, sd, len_ptr);
            }
        }
    }

    wfifow_be((*sd).fd, 6, len[1] as u16);
    wfifow_be((*sd).fd, 1, (len[0] + 5) as u16);
    wfifoset((*sd).fd, encrypt((*sd).fd) as usize);
    0
}

// ─── clif_parseparcel ─────────────────────────────────────────────────────────

/// Handle an incoming parcel packet — inform player to see the kingdom messenger.  C line 9412.
pub unsafe fn clif_parseparcel(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }
    clif_sendminitext(
        sd,
        b"You should go see your kingdom's messenger to collect this parcel\0".as_ptr()
            as *const i8,
    );
    0
}

// ─── clif_huntertoggle ────────────────────────────────────────────────────────

/// Toggle hunter mode on/off for `sd` and persist to database.  C line 9419.
pub async unsafe fn clif_huntertoggle(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }

    (*sd).hunter = rfifob((*sd).fd, 5) as i32;

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

    sqlx::query(
            "UPDATE `Character` SET `ChaHunter` = ?, `ChaHunterNote` = ? WHERE `ChaId` = ?"
        )
        .bind(hunter_val)
        .bind(hunter_tag_str)
        .bind(char_id)
        .execute(get_pool())
        .await
        .ok();

    if !session_exists((*sd).fd) {
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
pub async unsafe fn clif_sendhunternote(sd: *mut MapSessionData) -> i32 {
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

    let note_result = sqlx::query_scalar::<_, String>(
            "SELECT `ChaHunterNote` FROM `Character` WHERE `ChaName` = ?"
        )
        .bind(hunter_name_str)
        .fetch_optional(get_pool())
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

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
