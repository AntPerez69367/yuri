//! Rust implementations of sl_g_* global helpers previously in c_src/sl_compat.c.
//!
//! These functions are exported as `#[no_mangle] extern "C"` so existing
//! `extern "C"` declarations in ffi.rs and scripting type modules resolve
//! against the Rust symbols at link time.

use std::ffi::{c_char, c_int, c_uchar, c_void};
use std::os::raw::c_uint;

use crate::database::map_db::BlockList;
use crate::ffi::block::map_delblock;
use crate::ffi::map_db::get_map_ptr;
use crate::ffi::session::{rust_session_exists, rust_session_get_data, rust_session_get_eof};
use crate::game::client::visual::clif_sendweather;
use crate::game::map_server::{map_deliddb, map_id2sd, map_readglobalreg, map_setglobalreg};
use crate::game::pc::MapSessionData;

extern "C" {
    // fd_max — defined in src/bin/map_server.rs as #[no_mangle] pub static mut fd_max.
    static fd_max: c_int;
    fn clif_sendmsg(sd: *mut MapSessionData, color: c_int, msg: *const c_char) -> c_int;
    fn clif_lookgone(bl: *mut BlockList);
    fn clif_object_canmove(m: c_int, x: c_int, y: c_int, dir: c_int) -> c_int;
    fn clif_object_canmove_from(m: c_int, x: c_int, y: c_int, dir: c_int) -> c_int;
    fn clif_sendside(bl: *mut BlockList);
    fn clif_playsound(bl: *mut BlockList, sound: c_int);
    fn clif_sendaction(bl: *mut BlockList, action: c_int, speed: c_int, sound: c_int) -> c_int;
    fn clif_send(buf: *const u8, len: c_int, bl: *mut BlockList, area_type: c_int) -> c_int;
}

/// Set weather on all maps matching `region`/`indoor`, broadcasting to sessions on each map.
///
/// Mirrors `sl_g_setweather` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_setweather(region: c_uchar, indoor: c_uchar, weather: c_uchar) {
    let t = libc::time(std::ptr::null_mut()) as u32;
    for x in 0..65535u16 {
        let ptr = get_map_ptr(x);
        if ptr.is_null() || (*ptr).xs == 0 { continue; }
        let mut timer = map_readglobalreg(x as c_int, c"artificial_weather_timer".as_ptr()) as u32;
        if timer > 0 && timer <= t {
            map_setglobalreg(x as c_int, c"artificial_weather_timer".as_ptr(), 0);
            timer = 0;
        }
        if (*ptr).region != region || (*ptr).indoor != indoor || timer != 0 { continue; }
        (*ptr).weather = weather;
        for i in 1..fd_max {
            if rust_session_exists(i) == 0 { continue; }
            let tsd = rust_session_get_data(i) as *mut MapSessionData;
            if tsd.is_null() || rust_session_get_eof(i) != 0 { continue; }
            if (*tsd).bl.m == x { clif_sendweather(tsd); }
        }
    }
}

/// Set weather on a single map, broadcasting to sessions on that map.
///
/// Mirrors `sl_g_setweatherm` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_setweatherm(m: c_int, weather: c_uchar) {
    let ptr = get_map_ptr(m as u16);
    if ptr.is_null() || (*ptr).xs == 0 { return; }
    let t = libc::time(std::ptr::null_mut()) as u32;
    let mut timer = map_readglobalreg(m, c"artificial_weather_timer".as_ptr()) as u32;
    if timer > 0 && timer <= t {
        map_setglobalreg(m, c"artificial_weather_timer".as_ptr(), 0);
        timer = 0;
    }
    if timer != 0 { return; }
    (*ptr).weather = weather;
    for i in 1..fd_max {
        if rust_session_exists(i) == 0 { continue; }
        let tsd = rust_session_get_data(i) as *mut MapSessionData;
        if tsd.is_null() || rust_session_get_eof(i) != 0 { continue; }
        if (*tsd).bl.m == m as u16 { clif_sendweather(tsd); }
    }
}

/// Collect pointers to all online player block-lists into `out_ptrs`.
///
/// Returns the count written. Mirrors `sl_g_getusers` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_getusers(out_ptrs: *mut *mut c_void, max_count: c_int) -> c_int {
    let mut count = 0i32;
    for i in 0..fd_max {
        if count >= max_count { break; }
        if rust_session_exists(i) == 0 { continue; }
        if rust_session_get_eof(i) != 0 { continue; }
        let tsd = rust_session_get_data(i) as *mut MapSessionData;
        if tsd.is_null() { continue; }
        *out_ptrs.add(count as usize) = &mut (*tsd).bl as *mut _ as *mut c_void;
        count += 1;
    }
    count
}

/// Return `map[m].pvp`, or 0 if the map slot is not loaded.
///
/// Mirrors `sl_g_getmappvp` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_getmappvp(m: c_int) -> c_int {
    let ptr = get_map_ptr(m as u16);
    if ptr.is_null() || (*ptr).xs == 0 { return 0; }
    (*ptr).pvp as c_int
}

/// Copy `map[m].title` into `buf` (null-terminated, at most `buflen` bytes including NUL).
///
/// Returns 1 on success, 0 if the map is not loaded or args are invalid.
/// Mirrors `sl_g_getmaptitle` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_getmaptitle(m: c_int, buf: *mut c_char, buflen: c_int) -> c_int {
    if buf.is_null() || buflen <= 0 { return 0; }
    let ptr = get_map_ptr(m as u16);
    if ptr.is_null() || (*ptr).xs == 0 { return 0; }
    let src = (*ptr).title.as_ptr();
    let cap = (buflen - 1) as usize;
    let mut i = 0;
    while i < cap {
        let c = *src.add(i);
        *buf.add(i) = c;
        if c == 0 { return 1; }
        i += 1;
    }
    *buf.add(i) = 0;
    1
}

/// Send a colored message to a specific player by ID.
///
/// `target == 0` is a no-op (area broadcast not implemented here).
/// Mirrors `sl_g_msg` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_msg(bl: *mut c_void, color: c_int, msg: *const c_char, target: c_int) {
    if bl.is_null() || msg.is_null() || target == 0 { return; }
    let tsd = map_id2sd(target as c_uint) as *mut MapSessionData;
    if !tsd.is_null() { clif_sendmsg(tsd, color, msg); }
}

/// Return 1 if cell (x, y) on bl's map is passable from `side`, else 0.
///
/// Mirrors `sl_g_objectcanmove` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_objectcanmove(bl: *mut c_void, x: c_int, y: c_int, side: c_int) -> c_int {
    if bl.is_null() { return 0; }
    let m = (*(bl as *mut BlockList)).m as c_int;
    if clif_object_canmove(m, x, y, side) != 0 { 0 } else { 1 }
}

/// Return 1 if the block at (x, y) can move from that cell toward `side`, else 0.
///
/// Mirrors `sl_g_objectcanmovefrom` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_objectcanmovefrom(bl: *mut c_void, x: c_int, y: c_int, side: c_int) -> c_int {
    if bl.is_null() { return 0; }
    let m = (*(bl as *mut BlockList)).m as c_int;
    if clif_object_canmove_from(m, x, y, side) != 0 { 0 } else { 1 }
}

/// Remove a floor item from the spatial grid and ID DB, broadcasting disappearance.
///
/// Does NOT free memory — the Lua object may still hold references.
/// Mirrors `sl_fl_delete` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_fl_delete(bl_ptr: *mut c_void) {
    use crate::game::pc::BL_PC;
    if bl_ptr.is_null() { return; }
    let bl = bl_ptr as *mut BlockList;
    if (*bl).bl_type as c_int == BL_PC { return; }
    map_delblock(bl);
    map_deliddb(bl);
    if (*bl).id > 0 { clif_lookgone(bl); }
}

/// Remove block from the map ID database only (no grid, no broadcast, no free).
///
/// Mirrors `sl_g_deliddb` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_deliddb(bl_ptr: *mut c_void) {
    if bl_ptr.is_null() { return; }
    map_deliddb(bl_ptr as *mut BlockList);
}

/// No-op — permanent spawn tracking is handled in Lua.
///
/// Mirrors `sl_g_addpermanentspawn` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_addpermanentspawn(_bl_ptr: *mut c_void) {}

/// Broadcast block's look packet to surrounding players.
///
/// Mirrors `sl_g_sendside` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_sendside(bl: *mut c_void) {
    if bl.is_null() { return; }
    clif_sendside(bl as *mut BlockList);
}

/// Play a sound effect at bl's position.
///
/// Mirrors `sl_g_playsound` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_playsound(bl: *mut c_void, sound: c_int) {
    if bl.is_null() { return; }
    clif_playsound(bl as *mut BlockList, sound);
}

/// Delete a non-PC block from the world and free its memory.
///
/// Unlike `sl_fl_delete`, this frees the block — callers guarantee no Lua reference remains.
/// Mirrors `sl_g_delete_bl` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_delete_bl(bl_ptr: *mut c_void) {
    use crate::game::pc::BL_PC;
    if bl_ptr.is_null() { return; }
    let bl = bl_ptr as *mut BlockList;
    if (*bl).bl_type as c_int == BL_PC { return; }
    map_delblock(bl);
    map_deliddb(bl);
    if (*bl).id > 0 {
        clif_lookgone(bl);
        libc::free(bl_ptr);
    }
}

/// Broadcast an action animation at bl's position.
///
/// Mirrors `sl_g_sendaction` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_sendaction(bl_ptr: *mut c_void, action: c_int, speed: c_int) {
    if bl_ptr.is_null() { return; }
    clif_sendaction(bl_ptr as *mut BlockList, action, speed, 0);
}

/// Send a throw animation packet from bl's position toward (x, y).
///
/// Packet layout: opcode 0xAA, length 0x001B, type 0x16 subtype 0x03.
/// Mirrors `sl_g_throwblock` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_throwblock(
    bl_ptr: *mut c_void,
    x: c_int, y: c_int,
    icon: c_int, color: c_int, action: c_int,
) {
    if bl_ptr.is_null() { return; }
    let bl = bl_ptr as *mut BlockList;
    let mut buf = [0u8; 30];
    buf[0]       = 0xAA;
    buf[1..3].copy_from_slice(&0x001Bu16.to_be_bytes());
    buf[3]       = 0x16;
    buf[4]       = 0x03;
    buf[5..9].copy_from_slice(&((*bl).id as u32).to_be_bytes());
    buf[9..11].copy_from_slice(&((icon + 49152) as u16).to_be_bytes());
    buf[11]      = color as u8;
    // buf[12..16] = 0 (already zero-initialized)
    buf[16..18].copy_from_slice(&((*bl).x as u16).to_be_bytes());
    buf[18..20].copy_from_slice(&((*bl).y as u16).to_be_bytes());
    buf[20..22].copy_from_slice(&(x as u16).to_be_bytes());
    buf[22..24].copy_from_slice(&(y as u16).to_be_bytes());
    // buf[24..28] = 0, buf[29] = 0
    buf[28]      = action as u8;
    clif_send(buf.as_ptr(), 30, bl, 6 /* SAMEAREA */);
}

/// Drop an item at bl's position.
///
/// Mirrors `sl_g_dropitem` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_dropitem(bl_ptr: *mut c_void, item_id: c_int, amount: c_int, owner: c_int) {
    if bl_ptr.is_null() { return; }
    let bl = bl_ptr as *mut BlockList;
    let id = item_id as c_uint;
    let sd = if owner != 0 { map_id2sd(owner as c_uint) as *mut MapSessionData } else { std::ptr::null_mut() };
    let dura = crate::ffi::item_db::rust_itemdb_dura(id);
    let prot = crate::ffi::item_db::rust_itemdb_protected(id);
    crate::game::mob::rust_mob_dropitem(
        (*bl).id as c_uint, id, amount, dura, prot, 0,
        (*bl).m as c_int, (*bl).x as c_int, (*bl).y as c_int, sd,
    );
}

/// Drop an item at a specific map coordinate, ignoring bl's position.
///
/// Mirrors `sl_g_dropitemxy` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_dropitemxy(
    _bl_ptr: *mut c_void,
    item_id: c_int, amount: c_int,
    m: c_int, x: c_int, y: c_int,
    owner: c_int,
) {
    let id = item_id as c_uint;
    let sd = if owner != 0 { map_id2sd(owner as c_uint) as *mut MapSessionData } else { std::ptr::null_mut() };
    let dura = crate::ffi::item_db::rust_itemdb_dura(id);
    let prot = crate::ffi::item_db::rust_itemdb_protected(id);
    crate::game::mob::rust_mob_dropitem(0, id, amount, dura, prot, 0, m, x, y, sd);
}

/// Insert a parcel into the Parcels table, assigning the next available slot.
///
/// Mirrors `sl_g_sendparcel` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_sendparcel(
    _bl_ptr: *mut c_void,
    receiver: c_int, sender: c_int,
    item: c_int, amount: c_int, owner: c_int,
    engrave: *const c_char, npcflag: c_int,
) {
    let engrave_str: String = if engrave.is_null() {
        String::new()
    } else {
        std::ffi::CStr::from_ptr(engrave).to_string_lossy().into_owned()
    };
    let receiver_u = receiver as u32;
    let item_u = item as u32;
    let dura = crate::ffi::item_db::rust_itemdb_dura(item_u) as i32;
    let prot = crate::ffi::item_db::rust_itemdb_protected(item_u) as i32;
    let _ = crate::database::blocking_run(async move {
        let newest: i32 = sqlx::query_scalar::<_, i32>(
            "SELECT COALESCE(MAX(`ParPosition`), -1) FROM `Parcels` WHERE `ParChaIdDestination`=?"
        )
        .bind(receiver_u)
        .fetch_one(crate::database::get_pool()).await
        .unwrap_or(-1);
        sqlx::query(
            "INSERT INTO `Parcels` \
             (`ParChaIdDestination`,`ParSender`,`ParItmId`,`ParAmount`,`ParChaIdOwner`,\
              `ParEngrave`,`ParPosition`,`ParNpc`,\
              `ParCustomLook`,`ParCustomLookColor`,`ParCustomIcon`,`ParCustomIconColor`,\
              `ParProtected`,`ParItmDura`) \
             VALUES (?,?,?,?,?,?,?,?,0,0,0,0,?,?)"
        )
        .bind(receiver_u)
        .bind(sender as u32)
        .bind(item_u)
        .bind(amount as u32)
        .bind(owner as u32)
        .bind(&engrave_str)
        .bind(newest + 1)
        .bind(npcflag)
        .bind(prot)
        .bind(dura)
        .execute(crate::database::get_pool()).await
    });
}
