//! Port of item/equipment handling from `c_src/map_parse.c`.
//!
//! Functions declared `#[no_mangle] pub unsafe extern "C"` so they remain
//! callable from any remaining C code that has not yet been ported.

#![allow(non_snake_case, clippy::wildcard_imports)]

use std::ffi::{c_char, c_int, c_uint, c_void};

use crate::database::map_db::{BlockList, WarpList, BLOCK_SIZE};
use crate::ffi::map_db::map;
use crate::ffi::session::{rust_session_exists, rust_session_set_eof, rust_session_wdata_ptr};
use crate::game::mob::MOB_DEAD;
use crate::game::pc::{
    MapSessionData,
    BL_PC, BL_MOB, BL_NPC, BL_ITEM,
    EQ_WEAP, EQ_ARMOR, EQ_SHIELD, EQ_HELM, EQ_LEFT, EQ_RIGHT,
    EQ_SUBLEFT, EQ_SUBRIGHT, EQ_FACEACC, EQ_CROWN, EQ_MANTLE, EQ_NECKLACE, EQ_BOOTS, EQ_COAT,
    SFLAG_FULLSTATS, SFLAG_HPMP, SFLAG_XPMONEY,
    map_msg,
    LOOK_SEND,
};

// MAP_EQ* message indices (from c_src/map_server.h enum, after MAP_ERRMOUNT=12)
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
use crate::servers::char::charstatus::{MAX_INVENTORY, MAX_MAGIC_TIMERS};

use super::packet::{
    encrypt,
    wfifob, wfifow, wfifol, wfifoset, wfifoheader,
    rfifob,
    clif_send, map_foreachinarea, map_foreachincell,
    AREA, SAMEAREA,
};

// optFlag_stealth = 32 (from map_server.h)
const OPT_FLAG_STEALTH: c_int = 32;

// SCRIPT subtype constant (enum { SCRIPT=0, FLOOR=1 } in map_server.h)
const SCRIPT: u8 = 0;

// ─── C FFI: functions remaining in C ─────────────────────────────────────────

extern "C" {
    fn clif_sendstatus(sd: *mut MapSessionData, flags: c_int) -> c_int;
    fn clif_sendmsg(sd: *mut MapSessionData, t: c_int, msg: *const c_char) -> c_int;
    fn clif_sendminitext(sd: *mut MapSessionData, msg: *const c_char) -> c_int;
    fn clif_getequiptype(val: c_int) -> c_int;
    fn broadcast_update_state(sd: *mut MapSessionData);
    fn clif_sendaction(bl: *mut BlockList, action: c_int, unused: c_int, extra: c_int) -> c_int;
    fn clif_object_look_sub2(bl: *mut BlockList, ...) -> c_int;
    fn clif_object_canmove(m: c_int, x: c_int, y: c_int, dir: c_int) -> c_int;
    fn clif_object_canmove_from(m: c_int, x: c_int, y: c_int, dir: c_int) -> c_int;
    fn map_id2name(id: c_uint) -> *mut c_char;
    fn map_additem(bl: *mut BlockList);
    // read_pass — inlined below; no longer an extern "C" call
    #[link_name = "rust_pc_readglobalreg"]
    fn pc_readglobalreg(sd: *mut MapSessionData, reg: *const c_char) -> c_int;

    // rust_ variants for static-inline wrappers
    fn rust_itemdb_name(id: c_uint) -> *mut c_char;
    fn rust_itemdb_yname(id: c_uint) -> *mut c_char;
    fn rust_itemdb_text(id: c_uint) -> *mut c_char;
    fn rust_itemdb_type(id: c_uint) -> c_int;
    fn rust_itemdb_icon(id: c_uint) -> c_int;
    fn rust_itemdb_iconcolor(id: c_uint) -> c_int;
    fn rust_itemdb_dura(id: c_uint) -> c_int;
    fn rust_itemdb_protected(id: c_uint) -> c_int;
    fn rust_itemdb_breakondeath(id: c_uint) -> c_int;
    fn rust_itemdb_stackamount(id: c_uint) -> c_int;
    fn rust_itemdb_unequip(id: c_uint) -> c_int;
    fn rust_itemdb_droppable(id: c_uint) -> c_int;
    fn rust_magicdb_yname(id: c_int) -> *mut c_char;
    fn rust_pc_useitem(sd: *mut MapSessionData, id: c_int) -> c_int;
    fn rust_pc_unequip(sd: *mut MapSessionData, t: c_int) -> c_int;
    fn rust_pc_delitem(sd: *mut MapSessionData, id: c_int, amount: c_int, t: c_int) -> c_int;
    fn rust_pc_loadmagic(sd: *mut MapSessionData) -> c_int;
    fn rust_pc_reload_aether(sd: *mut MapSessionData) -> c_int;
    fn rust_pc_addtocurrent(bl: *mut BlockList, ...) -> c_int;
}

// ─── Lua dispatch helpers ─────────────────────────────────────────────────────

/// Dispatch a Lua event with a single block_list argument.
#[cfg(not(test))]
#[allow(dead_code)]
unsafe fn sl_doscript_simple(root: *const std::ffi::c_char, method: *const std::ffi::c_char, bl: *mut BlockList) -> std::ffi::c_int {
    crate::game::scripting::doscript_blargs(root, method, &[bl as *mut _])
}

/// Dispatch a Lua event with two block_list arguments.
#[cfg(not(test))]
#[allow(dead_code)]
unsafe fn sl_doscript_2(root: *const std::ffi::c_char, method: *const std::ffi::c_char, bl1: *mut BlockList, bl2: *mut BlockList) -> std::ffi::c_int {
    crate::game::scripting::doscript_blargs(root, method, &[bl1 as *mut _, bl2 as *mut _])
}

// ─── libc helpers ─────────────────────────────────────────────────────────────

unsafe fn strcasecmp_rs(a: *const c_char, b: *const u8) -> c_int {
    extern "C" {
        fn strcasecmp(a: *const c_char, b: *const c_char) -> c_int;
    }
    strcasecmp(a, b.cast())
}

unsafe fn strlen_cstr(p: *const c_char) -> usize {
    extern "C" { fn strlen(s: *const c_char) -> usize; }
    strlen(p)
}

unsafe fn strcpy_cstr(dst: *mut u8, src: *const c_char) {
    extern "C" { fn strcpy(d: *mut c_char, s: *const c_char) -> *mut c_char; }
    strcpy(dst.cast(), src);
}

unsafe fn sprintf_buf(dst: &mut [i8; 128], fmt: &[u8], arg: *const c_char) {
    // Used only for formatting name strings into fixed buffers.
    // We call libc snprintf for safety.
    extern "C" {
        fn snprintf(s: *mut c_char, n: usize, fmt: *const c_char, ...) -> c_int;
    }
    snprintf(dst.as_mut_ptr(), 128, fmt.as_ptr().cast(), arg);
}

// ─── clif_checkinvbod ─────────────────────────────────────────────────────────

/// Validate inventory on death: break or restore items as needed.
///
/// Mirrors `clif_checkinvbod` from `c_src/map_parse.c` ~line 5632.
#[no_mangle]
pub unsafe extern "C" fn clif_checkinvbod(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }

    for x in 0..MAX_INVENTORY {
        (*sd).invslot = x as u8;

        if (*sd).status.inventory[x].id == 0 { continue; }

        let id = (*sd).status.inventory[x].id;

        if (*sd).status.state == 1
            && rust_itemdb_breakondeath(id) == 1
        {
            if rust_itemdb_protected(id) != 0
                || (*sd).status.inventory[x].protected >= 1
            {
                (*sd).status.inventory[x].protected =
                    (*sd).status.inventory[x].protected.saturating_sub(1);
                (*sd).status.inventory[x].dura = rust_itemdb_dura(id);

                let mut buf = [0i8; 256];
                let name = rust_itemdb_name(id);
                extern "C" {
                    fn snprintf(s: *mut c_char, n: usize, fmt: *const c_char, ...) -> c_int;
                }
                snprintf(
                    buf.as_mut_ptr(),
                    256,
                    b"Your %s has been restored!\0".as_ptr().cast(),
                    name,
                );
                clif_sendstatus(sd, SFLAG_FULLSTATS | SFLAG_HPMP);
                clif_sendmsg(sd, 5, buf.as_ptr());
                sl_doscript_simple(b"characterLog\0".as_ptr().cast(), b"invRestore\0".as_ptr().cast(), &raw mut (*sd).bl);
                return 0;
            }

            // Copy item into boditems before clearing it
            let bod_idx = (*sd).boditems.bod_count as usize;
            if bod_idx < 52 {
                (*sd).boditems.item[bod_idx] = (*sd).status.inventory[x];
                (*sd).boditems.bod_count += 1;
            }

            let mut buf = [0i8; 256];
            extern "C" {
                fn snprintf(s: *mut c_char, n: usize, fmt: *const c_char, ...) -> c_int;
            }
            snprintf(
                buf.as_mut_ptr(),
                256,
                b"Your %s was destroyed!\0".as_ptr().cast(),
                rust_itemdb_name(id),
            );
            sl_doscript_simple(b"characterLog\0".as_ptr().cast(), b"invBreak\0".as_ptr().cast(), &raw mut (*sd).bl);

            (*sd).breakid = id;
            sl_doscript_simple(b"onBreak\0".as_ptr().cast(), std::ptr::null(), &raw mut (*sd).bl);
            sl_doscript_simple(rust_itemdb_yname(id), b"on_break\0".as_ptr().cast(), &raw mut (*sd).bl);

            rust_pc_delitem(sd, x as c_int, 1, 9);
            clif_sendmsg(sd, 5, buf.as_ptr());
        }

        broadcast_update_state(sd);
    }

    sl_doscript_simple(b"characterLog\0".as_ptr().cast(), b"bodLog\0".as_ptr().cast(), &raw mut (*sd).bl);
    (*sd).boditems.bod_count = 0;

    0
}

// ─── clif_senddelitem ─────────────────────────────────────────────────────────

/// Remove an item from the client inventory view.
///
/// Mirrors `clif_senddelitem` from `c_src/map_parse.c` ~line 5695.
#[no_mangle]
pub unsafe extern "C" fn clif_senddelitem(sd: *mut MapSessionData, num: c_int, r#type: c_int) -> c_int {
    let n = num as usize;
    (*sd).status.inventory[n].id = 0;
    (*sd).status.inventory[n].dura = 0;
    (*sd).status.inventory[n].protected = 0;
    (*sd).status.inventory[n].amount = 0;
    (*sd).status.inventory[n].owner = 0;
    (*sd).status.inventory[n].custom = 0;
    (*sd).status.inventory[n].custom_look = 0;
    (*sd).status.inventory[n].custom_look_color = 0;
    (*sd).status.inventory[n].custom_icon = 0;
    (*sd).status.inventory[n].custom_icon_color = 0;
    (*sd).status.inventory[n].traps_table = [0u32; 100];
    (*sd).status.inventory[n].time = 0;
    (*sd).status.inventory[n].real_name[0] = 0;

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
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
/// Mirrors `clif_sendadditem` from `c_src/map_parse.c` ~line 5738.
#[no_mangle]
pub unsafe extern "C" fn clif_sendadditem(sd: *mut MapSessionData, num: c_int) -> c_int {
    let n = num as usize;
    let id = (*sd).status.inventory[n].id;

    if id < 4 {
        (*sd).status.inventory[n] = crate::servers::char::charstatus::Item {
            id: 0, owner: 0, custom: 0, time: 0, dura: 0, amount: 0,
            pos: 0, _pad0: [0; 3], custom_look: 0, custom_icon: 0,
            custom_look_color: 0, custom_icon_color: 0, protected: 0,
            traps_table: [0; 100], buytext: [0; 64], note: [0; 300],
            repair: 0, real_name: [0; 64], _pad1: [0; 3],
        };
        return 0;
    }

    let item_name = rust_itemdb_name(id);
    if id > 0 && strcasecmp_rs(item_name, b"??\0".as_ptr()) == 0 {
        (*sd).status.inventory[n] = crate::servers::char::charstatus::Item {
            id: 0, owner: 0, custom: 0, time: 0, dura: 0, amount: 0,
            pos: 0, _pad0: [0; 3], custom_look: 0, custom_icon: 0,
            custom_look_color: 0, custom_icon_color: 0, protected: 0,
            traps_table: [0; 100], buytext: [0; 64], note: [0; 300],
            repair: 0, real_name: [0; 64], _pad1: [0; 3],
        };
        return 0;
    }

    // Choose display name
    let name_ptr: *const c_char = if (*sd).status.inventory[n].real_name[0] != 0 {
        (*sd).status.inventory[n].real_name.as_ptr()
    } else {
        item_name
    };

    // Build display name string into a fixed buffer
    let mut buf = [0i8; 128];
    {
        extern "C" {
            fn snprintf(s: *mut c_char, n: usize, fmt: *const c_char, ...) -> c_int;
        }
        let item_type = rust_itemdb_type(id);
        let dura = (*sd).status.inventory[n].dura;
        let amount = (*sd).status.inventory[n].amount;
        // ITM_SMOKE=2, ITM_BAG=21, ITM_MAP=22, ITM_QUIVER=23 (from c_src/item_db.h enum)
        // These are handled via format string exactly as in C.
        if amount > 1 {
            snprintf(
                buf.as_mut_ptr(), 128,
                b"%s (%d)\0".as_ptr().cast(),
                name_ptr, amount,
            );
        } else if item_type == 2 {
            // ITM_SMOKE
            snprintf(
                buf.as_mut_ptr(), 128,
                b"%s [%d %s]\0".as_ptr().cast(),
                name_ptr, dura, rust_itemdb_text(id),
            );
        } else if item_type == 21 {
            // ITM_BAG
            snprintf(
                buf.as_mut_ptr(), 128,
                b"%s [%d]\0".as_ptr().cast(),
                name_ptr, dura,
            );
        } else if item_type == 22 {
            // ITM_MAP
            snprintf(
                buf.as_mut_ptr(), 128,
                b"[T%d] %s\0".as_ptr().cast(),
                dura, name_ptr,
            );
        } else if item_type == 23 {
            // ITM_QUIVER
            snprintf(
                buf.as_mut_ptr(), 128,
                b"%s [%d]\0".as_ptr().cast(),
                name_ptr, dura,
            );
        } else {
            snprintf(
                buf.as_mut_ptr(), 128,
                b"%s\0".as_ptr().cast(),
                name_ptr,
            );
        }
    }

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    let fd = (*sd).fd;
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x0F);
    wfifob(fd, 5, (num + 1) as u8);

    // icon
    if (*sd).status.inventory[n].custom_icon != 0 {
        wfifow(fd, 6, (((*sd).status.inventory[n].custom_icon + 49152) as u16).swap_bytes());
        wfifob(fd, 8, (*sd).status.inventory[n].custom_icon_color as u8);
    } else {
        wfifow(fd, 6, (rust_itemdb_icon(id) as u16).swap_bytes());
        wfifob(fd, 8, rust_itemdb_iconcolor(id) as u8);
    }

    // display name
    let buf_len = strlen_cstr(buf.as_ptr()) as usize;
    wfifob(fd, 9, buf_len as u8);
    {
        let dst = rust_session_wdata_ptr(fd, 10);
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(buf.as_ptr().cast::<u8>(), dst, buf_len);
        }
    }
    let mut len = buf_len + 10;

    // base item name
    let base_name = rust_itemdb_name(id);
    let base_len = strlen_cstr(base_name);
    wfifob(fd, len, base_len as u8);
    {
        let dst = rust_session_wdata_ptr(fd, len + 1);
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(base_name.cast::<u8>(), dst, base_len);
        }
    }
    len += base_len + 1;

    // amount (big-endian u32)
    wfifol(fd, len, ((*sd).status.inventory[n].amount as u32).swap_bytes());
    len += 4;

    // dura/protected block
    let item_type = rust_itemdb_type(id);
    if item_type >= 3 && item_type <= 17 {
        wfifob(fd, len, 0);
        wfifol(fd, len + 1, ((*sd).status.inventory[n].dura as u32).swap_bytes());

        let inv_prot = (*sd).status.inventory[n].protected;
        let db_prot = rust_itemdb_protected(id) as u32;
        let final_prot = if inv_prot >= db_prot { inv_prot } else { db_prot };
        wfifob(fd, len + 5, final_prot as u8);

        len += 6;
    } else {
        if rust_itemdb_stackamount(id) > 1 {
            wfifob(fd, len, 1);
        } else {
            wfifob(fd, len, 0);
        }
        wfifol(fd, len + 1, 0);

        let inv_prot = (*sd).status.inventory[n].protected;
        let db_prot = rust_itemdb_protected(id) as u32;
        let final_prot = if inv_prot >= db_prot { inv_prot } else { db_prot };
        wfifob(fd, len + 5, final_prot as u8);

        len += 6;
    }

    // owner name
    if (*sd).status.inventory[n].owner != 0 {
        let owner_ptr = map_id2name((*sd).status.inventory[n].owner);
        if !owner_ptr.is_null() {
            let owner_len = strlen_cstr(owner_ptr);
            wfifob(fd, len, owner_len as u8);
            {
                let dst = rust_session_wdata_ptr(fd, len + 1);
                if !dst.is_null() {
                    std::ptr::copy_nonoverlapping(owner_ptr.cast::<u8>(), dst, owner_len);
                }
            }
            len += owner_len + 1;
            libc_free(owner_ptr.cast());
        } else {
            wfifob(fd, len, 0);
            len += 1;
        }
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

unsafe fn libc_free(p: *mut c_void) {
    extern "C" { fn free(p: *mut c_void); }
    free(p);
}

// ─── clif_equipit ─────────────────────────────────────────────────────────────

/// Send an equip slot item to the client.
///
/// Mirrors `clif_equipit` from `c_src/map_parse.c` ~line 5870.
#[no_mangle]
pub unsafe extern "C" fn clif_equipit(sd: *mut MapSessionData, id: c_int) -> c_int {
    let slot = id as usize;

    let nameof: *const c_char = if (*sd).status.equip[slot].real_name[0] != 0 {
        (*sd).status.equip[slot].real_name.as_ptr()
    } else {
        rust_itemdb_name((*sd).status.equip[slot].id)
    };

    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    let fd = (*sd).fd;
    wfifob(fd, 5, clif_getequiptype(id) as u8);

    if (*sd).status.equip[slot].custom_icon != 0 {
        wfifow(fd, 6, (((*sd).status.equip[slot].custom_icon + 49152) as u16).swap_bytes());
        wfifob(fd, 8, (*sd).status.equip[slot].custom_icon_color as u8);
    } else {
        wfifow(fd, 6, (rust_itemdb_icon((*sd).status.equip[slot].id) as u16).swap_bytes());
        wfifob(fd, 8, rust_itemdb_iconcolor((*sd).status.equip[slot].id) as u8);
    }

    let nameof_len = strlen_cstr(nameof);
    wfifob(fd, 9, nameof_len as u8);
    {
        let dst = rust_session_wdata_ptr(fd, 10);
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(nameof.cast::<u8>(), dst, nameof_len);
        }
    }
    let mut len = nameof_len + 1;

    let base_name = rust_itemdb_name((*sd).status.equip[slot].id);
    let base_len = strlen_cstr(base_name);
    wfifob(fd, len + 9, base_len as u8);
    {
        let dst = rust_session_wdata_ptr(fd, len + 10);
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(base_name.cast::<u8>(), dst, base_len);
        }
    }
    len += base_len + 1;

    wfifol(fd, len + 9, ((*sd).status.equip[slot].dura as u32).swap_bytes());
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
/// Mirrors `clif_sendequip` from `c_src/map_parse.c` ~line 5912.
#[no_mangle]
pub unsafe extern "C" fn clif_sendequip(sd: *mut MapSessionData, id: c_int) -> c_int {
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

    if (*sd).status.equip[slot].id > 0
        && strcasecmp_rs(rust_itemdb_name((*sd).status.equip[slot].id), b"??\0".as_ptr()) == 0
    {
        (*sd).status.equip[slot] = crate::servers::char::charstatus::Item {
            id: 0, owner: 0, custom: 0, time: 0, dura: 0, amount: 0,
            pos: 0, _pad0: [0; 3], custom_look: 0, custom_icon: 0,
            custom_look_color: 0, custom_icon_color: 0, protected: 0,
            traps_table: [0; 100], buytext: [0; 64], note: [0; 300],
            repair: 0, real_name: [0; 64], _pad1: [0; 3],
        };
        return 0;
    }

    let name: *const c_char = if (*sd).status.equip[slot].real_name[0] != 0 {
        (*sd).status.equip[slot].real_name.as_ptr()
    } else {
        rust_itemdb_name((*sd).status.equip[slot].id)
    };

    let mut buff = [0i8; 256];
    extern "C" {
        fn snprintf(s: *mut c_char, n: usize, fmt: *const c_char, ...) -> c_int;
    }
    snprintf(
        buff.as_mut_ptr(), 256,
        map_msg[msgnum].message.as_ptr(),
        name,
    );
    clif_equipit(sd, id);
    clif_sendminitext(sd, buff.as_ptr());

    0
}

// ─── clif_parseuseitem ────────────────────────────────────────────────────────

/// Handle a use-item packet from the client.
///
/// Mirrors `clif_parseuseitem` from `c_src/map_parse.c` ~line 6452.
#[no_mangle]
pub unsafe extern "C" fn clif_parseuseitem(sd: *mut MapSessionData) -> c_int {
    rust_pc_useitem(sd, rfifob((*sd).fd, 5) as c_int - 1);
    0
}

// ─── clif_parseeatitem ────────────────────────────────────────────────────────

/// Handle an eat-item packet; only processes items of type ITM_EAT.
///
/// Mirrors `clif_parseeatitem` from `c_src/map_parse.c` ~line 6457.
#[no_mangle]
pub unsafe extern "C" fn clif_parseeatitem(sd: *mut MapSessionData) -> c_int {
    let slot = rfifob((*sd).fd, 5) as usize - 1;
    let id = (*sd).status.inventory[slot].id;
    // ITM_EAT = 0 (first entry in item_db.h enum)
    if rust_itemdb_type(id) == 0 {
        rust_pc_useitem(sd, slot as c_int);
    } else {
        clif_sendminitext(sd, b"That item is not edible.\0".as_ptr().cast());
    }
    0
}

// ─── clif_parsegetitem ────────────────────────────────────────────────────────

/// Handle a pick-up-item packet from the client.
///
/// Mirrors `clif_parsegetitem` from `c_src/map_parse.c` ~line 6467.
#[no_mangle]
pub unsafe extern "C" fn clif_parsegetitem(sd: *mut MapSessionData) -> c_int {
    if (*sd).status.state == 1 || (*sd).status.state == 3 {
        return 0; // dead can't pick up
    }

    if (*sd).status.state == 2 {
        (*sd).status.state = 0;
        sl_doscript_simple(b"invis_rogue\0".as_ptr().cast(), b"uncast\0".as_ptr().cast(), &raw mut (*sd).bl);
        broadcast_update_state(sd);
    }

    clif_sendaction(&raw mut (*sd).bl, 4, 40, 0);

    (*sd).pickuptype = rfifob((*sd).fd, 5);

    for x in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[x].id > 0
            && (*sd).status.dura_aether[x].duration > 0
        {
            sl_doscript_simple(rust_magicdb_yname((*sd).status.dura_aether[x].id as c_int), b"on_pickup_while_cast\0".as_ptr().cast(), &raw mut (*sd).bl);
        }
    }

    sl_doscript_simple(b"onPickUp\0".as_ptr().cast(), std::ptr::null(), &raw mut (*sd).bl);

    0
}

// ─── clif_unequipit ───────────────────────────────────────────────────────────

/// Send an unequip confirmation to the client.
///
/// Mirrors `clif_unequipit` from `c_src/map_parse.c` ~line 6495.
#[no_mangle]
pub unsafe extern "C" fn clif_unequipit(sd: *mut MapSessionData, spot: c_int) -> c_int {
    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
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
/// Mirrors `clif_parseunequip` from `c_src/map_parse.c` ~line 6511.
#[no_mangle]
pub unsafe extern "C" fn clif_parseunequip(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }

    let slot_byte = rfifob((*sd).fd, 5) as c_int;
    let eq_type: c_int = match slot_byte {
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

    if rust_itemdb_unequip((*sd).status.equip[eq_type as usize].id) == 1
        && (*sd).status.gm_level == 0
    {
        clif_sendminitext(sd, b"You are unable to unequip that.\0".as_ptr().cast());
        return 0;
    }

    let maxinv = (*sd).status.maxinv as usize;
    for x in 0..maxinv {
        if (*sd).status.inventory[x].id == 0 {
            rust_pc_unequip(sd, eq_type);
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
/// Mirrors `clif_parsewield` from `c_src/map_parse.c` ~line 6796.
#[no_mangle]
pub unsafe extern "C" fn clif_parsewield(sd: *mut MapSessionData) -> c_int {
    let pos = rfifob((*sd).fd, 5) as usize - 1;
    let id = (*sd).status.inventory[pos].id;
    let item_type = rust_itemdb_type(id);

    if item_type >= 3 && item_type <= 16 {
        rust_pc_useitem(sd, pos as c_int);
    } else {
        clif_sendminitext(sd, b"You cannot wield that!\0".as_ptr().cast());
    }

    0
}

// ─── clif_addtocurrent ────────────────────────────────────────────────────────

/// foreachinarea callback: add gold to an existing floor item.
///
/// Mirrors `clif_addtocurrent` from `c_src/map_parse.c` ~line 6808.
#[no_mangle]
pub unsafe extern "C" fn clif_addtocurrent(bl: *mut BlockList, mut ap: ...) -> c_int {
    if bl.is_null() { return 0; }
    let fl = bl as *mut FloorItemData;

    let def = ap.arg::<*mut c_int>();
    let amount = ap.arg::<c_uint>();
    let _sd = ap.arg::<*mut MapSessionData>();

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
/// Mirrors `clif_dropgold` from `c_src/map_parse.c` ~line 6828.
#[no_mangle]
pub unsafe extern "C" fn clif_dropgold(sd: *mut MapSessionData, amounts: c_uint) -> c_int {
    let reg_str = b"goldbardupe\0";
    let dupe_times = pc_readglobalreg(sd, reg_str.as_ptr().cast());
    if dupe_times != 0 {
        return 0;
    }

    if (*sd).status.gm_level == 0 {
        if (*sd).status.state == 1 {
            clif_sendminitext(sd, b"Spirits can't do that.\0".as_ptr().cast());
            return 0;
        }
        if (*sd).status.state == 3 {
            clif_sendminitext(sd, b"You cannot do that while riding a mount.\0".as_ptr().cast());
            return 0;
        }
        if (*sd).status.state == 4 {
            clif_sendminitext(sd, b"You cannot do that while transformed.\0".as_ptr().cast());
            return 0;
        }
    }

    if (*sd).status.money == 0 { return 0; }
    if amounts == 0 { return 0; }

    let mut amount = amounts;

    clif_sendaction(&raw mut (*sd).bl, 5, 20, 0);

    let fl = libc::calloc(1, std::mem::size_of::<FloorItemData>()) as *mut FloorItemData;
    (*fl).bl.m = (*sd).bl.m;
    (*fl).bl.x = (*sd).bl.x;
    (*fl).bl.y = (*sd).bl.y;

    if (*sd).status.money < amount {
        amount = (*sd).status.money;
        (*sd).status.money = 0;
    } else {
        (*sd).status.money -= amount;
    }

    (*fl).data.id = match amount {
        1          => 0u32,
        2..=99     => 1u32,
        100..=999  => 2u32,
        _          => 3u32,
    };
    (*fl).data.amount = amount as i32;

    (*sd).fakeDrop = 0;

    sl_doscript_2(b"on_drop_gold\0".as_ptr().cast(), std::ptr::null(), &raw mut (*sd).bl, &raw mut (*fl).bl);

    for x in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[x].id > 0
            && (*sd).status.dura_aether[x].duration > 0
        {
            sl_doscript_2(rust_magicdb_yname((*sd).status.dura_aether[x].id as c_int), b"on_drop_gold_while_cast\0".as_ptr().cast(), &raw mut (*sd).bl, &raw mut (*fl).bl);
        }
    }

    for x in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[x].id > 0
            && (*sd).status.dura_aether[x].aether > 0
        {
            sl_doscript_2(rust_magicdb_yname((*sd).status.dura_aether[x].id as c_int), b"on_drop_gold_while_aether\0".as_ptr().cast(), &raw mut (*sd).bl, &raw mut (*fl).bl);
        }
    }

    if (*sd).fakeDrop != 0 { return 0; }

    let mut mini = [0i8; 64];
    extern "C" {
        fn snprintf(s: *mut c_char, n: usize, fmt: *const c_char, ...) -> c_int;
    }

    snprintf(
        mini.as_mut_ptr(), 64,
        b"You dropped %d coins\0".as_ptr().cast(),
        (*fl).data.amount,
    );
    clif_sendminitext(sd, mini.as_ptr());

    let mut def = [0i32; 1];
    map_foreachincell(
        clif_addtocurrent,
        (*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int,
        BL_ITEM,
        def.as_mut_ptr(), amount,
    );

    if def[0] == 0 {
        map_additem(&raw mut (*fl).bl);

        sl_doscript_2(b"after_drop_gold\0".as_ptr().cast(), std::ptr::null(), &raw mut (*sd).bl, &raw mut (*fl).bl);

        for x in 0..MAX_MAGIC_TIMERS {
            if (*sd).status.dura_aether[x].id > 0
                && (*sd).status.dura_aether[x].duration > 0
            {
                sl_doscript_2(rust_magicdb_yname((*sd).status.dura_aether[x].id as c_int), b"after_drop_gold_while_cast\0".as_ptr().cast(), &raw mut (*sd).bl, &raw mut (*fl).bl);
            }
        }

        for x in 0..MAX_MAGIC_TIMERS {
            if (*sd).status.dura_aether[x].id > 0
                && (*sd).status.dura_aether[x].aether > 0
            {
                sl_doscript_2(rust_magicdb_yname((*sd).status.dura_aether[x].id as c_int), b"after_drop_gold_while_aether\0".as_ptr().cast(), &raw mut (*sd).bl, &raw mut (*fl).bl);
            }
        }

        sl_doscript_2(b"characterLog\0".as_ptr().cast(), b"dropWrite\0".as_ptr().cast(), &raw mut (*sd).bl, &raw mut (*fl).bl);

        map_foreachinarea(
            clif_object_look_sub2,
            (*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int,
            AREA,
            BL_PC,
            LOOK_SEND,
            &raw mut (*fl).bl,
        );
    } else {
        libc::free(fl.cast());
    }

    clif_sendstatus(sd, SFLAG_XPMONEY);

    0
}

// ─── clif_open_sub ────────────────────────────────────────────────────────────

/// Trigger the onOpen script hook.
///
/// Mirrors `clif_open_sub` from `c_src/map_parse.c` ~line 6960.
#[no_mangle]
pub unsafe extern "C" fn clif_open_sub(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }
    sl_doscript_simple(b"onOpen\0".as_ptr().cast(), std::ptr::null(), &raw mut (*sd).bl);
    0
}

// ─── clif_removespell ─────────────────────────────────────────────────────────

/// Send a remove-spell packet to the client.
///
/// Mirrors `clif_removespell` from `c_src/map_parse.c` ~line 6985.
#[no_mangle]
pub unsafe extern "C" fn clif_removespell(sd: *mut MapSessionData, pos: c_int) -> c_int {
    if rust_session_exists((*sd).fd) == 0 {
        rust_session_set_eof((*sd).fd, 8);
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
/// Mirrors `clif_parsechangespell` from `c_src/map_parse.c` ~line 6967.
#[no_mangle]
pub unsafe extern "C" fn clif_parsechangespell(sd: *mut MapSessionData) -> c_int {
    let start_pos = rfifob((*sd).fd, 6) as usize - 1;
    let stop_pos  = rfifob((*sd).fd, 7) as usize - 1;

    let start_id = (*sd).status.skill[start_pos];
    let stop_id  = (*sd).status.skill[stop_pos];

    clif_removespell(sd, start_pos as c_int);
    clif_removespell(sd, stop_pos as c_int);

    (*sd).status.skill[start_pos] = stop_id;
    (*sd).status.skill[stop_pos]  = start_id;

    rust_pc_loadmagic(sd);
    rust_pc_reload_aether(sd);

    0
}

// ─── clif_throwitem_sub ───────────────────────────────────────────────────────

/// Execute a throw: fire the onThrow script with source/destination floor items.
///
/// Mirrors `clif_throwitem_sub` from `c_src/map_parse.c` ~line 7000.
/// Note: this is NOT a foreachinarea callback; it is called directly.
#[no_mangle]
pub unsafe extern "C" fn clif_throwitem_sub(
    sd: *mut MapSessionData,
    id: c_int,
    _type: c_int,
    x: c_int,
    y: c_int,
) -> c_int {
    if (*sd).status.inventory[id as usize].id == 0 { return 0; }

    if (*sd).status.inventory[id as usize].amount <= 0 {
        clif_senddelitem(sd, id, 4);
        return 0;
    }

    let fl = libc::calloc(1, std::mem::size_of::<FloorItemData>()) as *mut FloorItemData;
    (*fl).bl.m = (*sd).bl.m;
    (*fl).bl.x = x as u16;
    (*fl).bl.y = y as u16;

    // memcpy(&fl->data, &sd->status.inventory[id], sizeof(struct item))
    std::ptr::copy_nonoverlapping(
        &(*sd).status.inventory[id as usize] as *const _ as *const u8,
        &raw mut (*fl).data as *mut u8,
        std::mem::size_of::<crate::servers::char::charstatus::Item>(),
    );

    (*sd).invslot = id as u8;
    (*sd).throwx = x as u16;
    (*sd).throwy = y as u16;

    sl_doscript_2(b"onThrow\0".as_ptr().cast(), std::ptr::null(), &raw mut (*sd).bl, &raw mut (*fl).bl);

    0
}

// ─── clif_throwitem_script ────────────────────────────────────────────────────

/// Complete a throw action after script approval.
///
/// Mirrors `clif_throwitem_script` from `c_src/map_parse.c` ~line 7023.
#[no_mangle]
pub unsafe extern "C" fn clif_throwitem_script(sd: *mut MapSessionData) -> c_int {
    let id   = (*sd).invslot as usize;
    let x    = (*sd).throwx as c_int;
    let y    = (*sd).throwy as c_int;
    let item_type = 0i32;

    let fl = libc::calloc(1, std::mem::size_of::<FloorItemData>()) as *mut FloorItemData;
    (*fl).bl.m = (*sd).bl.m;
    (*fl).bl.x = x as u16;
    (*fl).bl.y = y as u16;

    std::ptr::copy_nonoverlapping(
        &(*sd).status.inventory[id] as *const _ as *const u8,
        &raw mut (*fl).data as *mut u8,
        std::mem::size_of::<crate::servers::char::charstatus::Item>(),
    );

    let mut def = [0i32; 1];

    if (*fl).data.dura == rust_itemdb_dura((*fl).data.id) {
        map_foreachincell(
            rust_pc_addtocurrent,
            (*sd).bl.m as c_int, x, y,
            BL_ITEM,
            def.as_mut_ptr(), id as c_int, item_type, sd,
        );
    }

    (*sd).status.inventory[id].amount -= 1;

    if item_type != 0 || (*sd).status.inventory[id].amount == 0 {
        (*sd).status.inventory[id] = crate::servers::char::charstatus::Item {
            id: 0, owner: 0, custom: 0, time: 0, dura: 0, amount: 0,
            pos: 0, _pad0: [0; 3], custom_look: 0, custom_icon: 0,
            custom_look_color: 0, custom_icon_color: 0, protected: 0,
            traps_table: [0; 100], buytext: [0; 64], note: [0; 300],
            repair: 0, real_name: [0; 64], _pad1: [0; 3],
        };
        clif_senddelitem(sd, id as c_int, 4);
    } else {
        (*fl).data.amount = 1;
        clif_sendadditem(sd, id as c_int);
    }

    if (*sd).bl.x as c_int != x {
        let mut sndbuf = [0u8; 48];
        sndbuf[0] = 0xAA;
        let len_be = 0x1Bu16.to_be_bytes();
        sndbuf[1] = len_be[0];
        sndbuf[2] = len_be[1];
        sndbuf[3] = 0x16;
        sndbuf[4] = 0x03;
        let id_be = ((*sd).bl.id).to_be_bytes();
        sndbuf[5] = id_be[0]; sndbuf[6] = id_be[1];
        sndbuf[7] = id_be[2]; sndbuf[8] = id_be[3];

        if (*fl).data.custom_icon != 0 {
            let icon_be = (((*fl).data.custom_icon + 49152) as u16).to_be_bytes();
            sndbuf[9]  = icon_be[0];
            sndbuf[10] = icon_be[1];
            sndbuf[11] = (*fl).data.custom_icon_color as u8;
        } else {
            let icon_be = (rust_itemdb_icon((*fl).data.id as c_uint) as u16).to_be_bytes();
            sndbuf[9]  = icon_be[0];
            sndbuf[10] = icon_be[1];
            sndbuf[11] = rust_itemdb_iconcolor((*fl).data.id as c_uint) as u8;
        }

        let fl_id_be = if def[0] != 0 {
            (def[0] as u32).to_be_bytes()
        } else {
            ((*fl).bl.id).to_be_bytes()
        };
        sndbuf[12] = fl_id_be[0]; sndbuf[13] = fl_id_be[1];
        sndbuf[14] = fl_id_be[2]; sndbuf[15] = fl_id_be[3];

        let sx_be = (*sd).bl.x.to_be_bytes();
        sndbuf[16] = sx_be[0]; sndbuf[17] = sx_be[1];
        let sy_be = (*sd).bl.y.to_be_bytes();
        sndbuf[18] = sy_be[0]; sndbuf[19] = sy_be[1];
        let dx_be = (x as u16).to_be_bytes();
        sndbuf[20] = dx_be[0]; sndbuf[21] = dx_be[1];
        let dy_be = (y as u16).to_be_bytes();
        sndbuf[22] = dy_be[0]; sndbuf[23] = dy_be[1];
        // bytes 24..27 already 0
        sndbuf[28] = 0x02;
        sndbuf[29] = 0x00;

        clif_send(sndbuf.as_ptr(), 48, &raw mut (*sd).bl, SAMEAREA);
    } else {
        clif_sendaction(&raw mut (*sd).bl, 2, 30, 0);
    }

    if def[0] == 0 {
        map_additem(&raw mut (*fl).bl);
        map_foreachinarea(
            clif_object_look_sub2,
            (*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int,
            AREA,
            BL_PC,
            LOOK_SEND,
            &raw mut (*fl).bl,
        );
    } else {
        libc::free(fl.cast());
    }

    0
}

// ─── clif_throw_check ─────────────────────────────────────────────────────────

/// foreachinarea callback: check if a cell is blocked for throwing.
///
/// Mirrors `clif_throw_check` from `c_src/map_parse.c` ~line 7106.
#[no_mangle]
pub unsafe extern "C" fn clif_throw_check(bl: *mut BlockList, mut ap: ...) -> c_int {
    if bl.is_null() { return 0; }

    let found = ap.arg::<*mut c_int>();
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
        if (*tsd).status.state == 1 || ((*tsd).optFlags & OPT_FLAG_STEALTH as u64) != 0 {
            return 0;
        }
    }

    if !found.is_null() { *found += 1; }

    0
}

// ─── clif_throwconfirm ────────────────────────────────────────────────────────

/// Send a throw-confirm packet to the client.
///
/// Mirrors `clif_throwconfirm` from `c_src/map_parse.c` ~line 7129.
#[no_mangle]
pub unsafe extern "C" fn clif_throwconfirm(sd: *mut MapSessionData) -> c_int {
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
/// Mirrors `clif_parsethrow` from `c_src/map_parse.c` ~line 7141.
#[no_mangle]
pub unsafe extern "C" fn clif_parsethrow(sd: *mut MapSessionData) -> c_int {
    let reg_str = b"goldbardupe\0";
    let dupe_times = pc_readglobalreg(sd, reg_str.as_ptr().cast());
    if dupe_times != 0 {
        return 0;
    }

    if (*sd).status.gm_level == 0 {
        if (*sd).status.state == 1 {
            clif_sendminitext(sd, b"Spirits can't do that.\0".as_ptr().cast());
            return 0;
        }
        if (*sd).status.state == 3 {
            clif_sendminitext(sd, b"You cannot do that while riding a mount.\0".as_ptr().cast());
            return 0;
        }
        if (*sd).status.state == 4 {
            clif_sendminitext(sd, b"You cannot do that while transformed.\0".as_ptr().cast());
            return 0;
        }
    }

    let pos = rfifob((*sd).fd, 6) as usize - 1;
    if rust_itemdb_droppable((*sd).status.inventory[pos].id) != 0 {
        clif_sendminitext(sd, b"You can't throw this item.\0".as_ptr().cast());
        return 0;
    }

    let max = 8i32;
    let mut newx: c_int = (*sd).bl.x as c_int;
    let mut newy: c_int = (*sd).bl.y as c_int;
    let mut xmod: c_int = 0;
    let mut ymod: c_int = 0;
    let mut found = [0i32; 1];

    match (*sd).status.side {
        0 => { ymod = -1; } // up
        1 => { xmod = 1; }  // left
        2 => { ymod = 1; }  // down
        3 => { xmod = -1; } // right
        _ => {}
    }

    let m = (*sd).bl.m as c_int;
    let map_data = &*map.add(m as usize);

    'search: for i in 0..max {
        let mut x1: c_int = (*sd).bl.x as c_int + (i * xmod) + xmod;
        let mut y1: c_int = (*sd).bl.y as c_int + (i * ymod) + ymod;
        if x1 < 0 { x1 = 0; }
        if y1 < 0 { y1 = 0; }
        if x1 >= map_data.xs as c_int { x1 = map_data.xs as c_int - 1; }
        if y1 >= map_data.ys as c_int { y1 = map_data.ys as c_int - 1; }

        map_foreachincell(clif_throw_check, m, x1, y1, BL_NPC, found.as_mut_ptr());
        map_foreachincell(clif_throw_check, m, x1, y1, BL_PC,  found.as_mut_ptr());
        map_foreachincell(clif_throw_check, m, x1, y1, BL_MOB, found.as_mut_ptr());
        // read_pass(m, x, y) — mirrors map[m].pass[x + y*xs]
        let pass_val = if map.is_null() { 0 } else {
            let md = &*map.add(m as usize);
            if md.pass.is_null() { 0 } else { *md.pass.add(x1 as usize + y1 as usize * md.xs as usize) as c_int }
        };
        found[0] += pass_val;
        found[0] += clif_object_canmove(m, x1, y1, (*sd).status.side as c_int);
        found[0] += clif_object_canmove_from(m, x1, y1, (*sd).status.side as c_int);

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

    clif_throwitem_sub(sd, pos as c_int, 0, newx, newy)
}
