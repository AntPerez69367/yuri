//! Rust implementations of sl_g_* global helpers previously in c_src/sl_compat.c.
//!
//! These functions are exported as `#[no_mangle] extern "C"` so existing
//! `extern "C"` declarations in ffi.rs and scripting type modules resolve
//! against the Rust symbols at link time.

use std::ffi::{c_char, c_int, c_uchar, c_void};
use std::os::raw::c_uint;

use crate::database::map_db::{BlockList, WarpList, BLOCK_SIZE, MAX_MAPREG};
use crate::ffi::block::map_delblock;
use crate::ffi::map_db::get_map_ptr;
use crate::ffi::session::{rust_session_exists, rust_session_get_data, rust_session_get_eof};
use crate::game::block::{map_is_loaded, foreach_in_area, foreach_in_cell, AreaType};
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
    // Animation packet senders — still in C (map_parse.c / map_server_stubs.c).
    // They accept va_list; we call them via variadic FFI from within closures.
    fn clif_sendanimation(target_bl: *mut BlockList, ...) -> c_int;
    fn clif_sendanimation_xy(target_bl: *mut BlockList, ...) -> c_int;
    // Talk packet sender — still in C (map_parse.c).
    fn clif_speak(target_bl: *mut BlockList, ...) -> c_int;
    // Metadata sender — still in C (map_parse.c).
    fn send_metalist(sd: *mut MapSessionData) -> c_int;
    // NPC block-grid registration — Rust exports in ffi::block and game::map_server.
    fn map_addblock(bl: *mut BlockList) -> c_int;
}

// ---------------------------------------------------------------------------
// Ported from c_src/sl_compat.c — thin helpers that avoided Rust knowing about
// C struct layouts. Now Rust knows the layouts, so these are trivial wrappers.
// ---------------------------------------------------------------------------

/// Thin wrapper around `map_is_loaded` for code that still holds a `c_int` map index.
/// Replaces `int sl_map_isloaded(int m) { return map_isloaded(m); }` in sl_compat.c.
/// Called from `src/game/map_char.rs`.
#[no_mangle]
pub unsafe extern "C" fn sl_map_isloaded(m: c_int) -> c_int {
    map_is_loaded(m) as c_int
}

/// Extract `bl.m` from a `USER*` (= `MapSessionData*`) and call `map_readglobalreg`.
/// Replaces the C `map_readglobalreg_sd` bridge in sl_compat.c that was needed
/// before Rust knew the `MapSessionData` layout.
#[no_mangle]
pub unsafe extern "C" fn map_readglobalreg_sd(sd: *mut c_void, attrname: *const c_char) -> c_int {
    let sd = sd as *const MapSessionData;
    map_readglobalreg((*sd).bl.m as c_int, attrname)
}

/// Extract `bl.m` from a `USER*` (= `MapSessionData*`) and call `map_setglobalreg`.
/// Replaces the C `map_setglobalreg_sd` bridge in sl_compat.c.
#[no_mangle]
pub unsafe extern "C" fn map_setglobalreg_sd(sd: *mut c_void, attrname: *const c_char, val: c_int) -> c_int {
    let sd = sd as *const MapSessionData;
    map_setglobalreg((*sd).bl.m as c_int, attrname, val)
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

// ─── Task 1.4: NPC/Animation/Packet Broadcast Functions ──────────────────────

/// BL_PC type constant — matches C enum value.
const BL_PC_TYPE: c_int = 0x01;

/// Broadcast a spell/skill animation to all PCs in AREA around bl.
///
/// `clif_sendanimation(target_bl, anim, src_bl, times)` is still in C
/// (va_list-based); called via FFI from inside the closure.
///
/// Mirrors `sl_g_sendanimation` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_sendanimation(bl_ptr: *mut c_void, anim: c_int, times: c_int) {
    if bl_ptr.is_null() { return; }
    let bl = bl_ptr as *mut BlockList;
    let m  = (*bl).m as i32;
    let x  = (*bl).x as i32;
    let y  = (*bl).y as i32;
    foreach_in_area(m, x, y, AreaType::Area, BL_PC_TYPE, |target_bl| {
        clif_sendanimation(target_bl, anim, bl, times)
    });
}

/// Broadcast an animation at position (x, y) to all PCs in AREA around bl.
///
/// `clif_sendanimation_xy(target_bl, anim, times, x, y)` is still in C
/// (va_list-based); called via FFI from inside the closure.
///
/// Mirrors `sl_g_sendanimxy` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_sendanimxy(
    bl_ptr: *mut c_void,
    anim: c_int,
    x: c_int,
    y: c_int,
    times: c_int,
) {
    if bl_ptr.is_null() { return; }
    let bl = bl_ptr as *mut BlockList;
    let m  = (*bl).m as i32;
    let bx = (*bl).x as i32;
    let by = (*bl).y as i32;
    foreach_in_area(m, bx, by, AreaType::Area, BL_PC_TYPE, |target_bl| {
        clif_sendanimation_xy(target_bl, anim, times, x, y)
    });
}

/// Broadcast a repeating animation to all PCs in AREA around bl.
///
/// `duration` is in milliseconds; divided by 1000 before sending on the wire.
/// Mirrors `sl_g_repeatanimation` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_repeatanimation(bl_ptr: *mut c_void, anim: c_int, duration: c_int) {
    if bl_ptr.is_null() { return; }
    let bl = bl_ptr as *mut BlockList;
    let m  = (*bl).m as i32;
    let x  = (*bl).x as i32;
    let y  = (*bl).y as i32;
    // Integer division: sub-second durations (1-999 ms) truncate to wire value 0,
    // same as the C original. Callers should pass multiples of 1000.
    let wire_dur = if duration > 0 { duration / 1000 } else { duration };
    foreach_in_area(m, x, y, AreaType::Area, BL_PC_TYPE, |target_bl| {
        clif_sendanimation(target_bl, anim, bl, wire_dur)
    });
}

/// Send a self-targeted animation from `bl` to the single player at `target_id`.
///
/// Resolves the target's map/cell via `map_id2sd`, then broadcasts to that
/// exact cell only.  Mirrors `sl_g_selfanimation` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_selfanimation(
    bl_ptr: *mut c_void,
    target_id: c_int,
    anim: c_int,
    times: c_int,
) {
    if bl_ptr.is_null() { return; }
    let bl = bl_ptr as *mut BlockList;
    let sd = map_id2sd(target_id as c_uint) as *mut MapSessionData;
    if sd.is_null() { return; }
    let m  = (*sd).bl.m as i32;
    let x  = (*sd).bl.x as i32;
    let y  = (*sd).bl.y as i32;
    foreach_in_cell(m, x, y, BL_PC_TYPE, |target_bl| {
        clif_sendanimation(target_bl, anim, bl, times)
    });
}

/// Send a self-targeted XY animation to the single player at `target_id`.
///
/// Resolves the target's map/cell, then broadcasts the XY animation to that
/// exact cell only.  Mirrors `sl_g_selfanimationxy` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_selfanimationxy(
    _bl_ptr: *mut c_void,
    target_id: c_int,
    anim: c_int,
    x: c_int,
    y: c_int,
    times: c_int,
) {
    let sd = map_id2sd(target_id as c_uint) as *mut MapSessionData;
    if sd.is_null() { return; }
    let m  = (*sd).bl.m as i32;
    let sx = (*sd).bl.x as i32;
    let sy = (*sd).bl.y as i32;
    foreach_in_cell(m, sx, sy, BL_PC_TYPE, |target_bl| {
        clif_sendanimation_xy(target_bl, anim, times, x, y)
    });
}

/// Send a talk/speech packet from `bl` to all PCs in AREA.
///
/// `clif_speak(target_bl, msg, src_bl, type)` is still in C (va_list-based);
/// called via FFI from inside the closure.
///
/// Mirrors `sl_g_talk` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_talk(bl_ptr: *mut c_void, talk_type: c_int, msg: *const c_char) {
    if bl_ptr.is_null() || msg.is_null() { return; }
    let bl = bl_ptr as *mut BlockList;
    let m  = (*bl).m as i32;
    let x  = (*bl).x as i32;
    let y  = (*bl).y as i32;
    foreach_in_area(m, x, y, AreaType::Area, BL_PC_TYPE, |target_bl| {
        clif_speak(target_bl, msg, bl, talk_type)
    });
}

/// Send metadata to all online players.
///
/// Iterates every fd slot, finds live sessions, and calls `send_metalist`
/// for each.  `send_metalist` is still in C (map_parse.c); called via FFI.
///
/// Mirrors `sl_g_sendmeta` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_sendmeta() {
    for i in 0..fd_max {
        if rust_session_exists(i) == 0 { continue; }
        if rust_session_get_eof(i) != 0 { continue; }
        let tsd = rust_session_get_data(i) as *mut MapSessionData;
        if tsd.is_null() { continue; }
        send_metalist(tsd);
    }
}

/// Broadcast a throw-item packet to all PCs in SAMEAREA around (m, x, y).
///
/// Builds a 30-byte packet once and calls `clif_send` once with SAMEAREA — it
/// internally iterates all nearby PCs.  The previous implementation wrapped
/// `clif_send(..., SAMEAREA)` inside `foreach_in_area(..., SameArea, ...)`,
/// causing N² delivers (each of the N nearby players received the packet N
/// times instead of once).
///
/// Packet layout (big-endian wire format):
///   [0]    0xAA          opcode
///   [1..2] 0x001B        length
///   [3]    0x16          type
///   [4]    0x03          subtype
///   [5..8] id            entity id
///   [9..10] icon+49152   icon index
///   [11]   color
///   [12..15] 0           padding
///   [16..17] x           source x (appears twice)
///   [18..19] x           source x (appears twice in C original — preserved)
///   [20..21] x2          dest x
///   [22..23] y2          dest y
///   [24..27] 0           padding
///   [28]   action
///   [29]   0             padding
///
/// Mirrors `sl_g_throw` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_g_throw(
    id: c_int,
    m: c_int,
    x: c_int,
    y: c_int,
    x2: c_int,
    y2: c_int,
    icon: c_int,
    color: c_int,
    action: c_int,
) {
    let mut buf = [0u8; 30];
    buf[0]      = 0xAA;
    buf[1..3].copy_from_slice(&0x001Bu16.to_be_bytes());
    buf[3]      = 0x16;
    buf[4]      = 0x03;
    buf[5..9].copy_from_slice(&(id as u32).to_be_bytes());
    buf[9..11].copy_from_slice(&((icon + 49152) as u16).to_be_bytes());
    buf[11]     = color as u8;
    // buf[12..16] = 0 (zero-initialized)
    buf[16..18].copy_from_slice(&(x as u16).to_be_bytes());
    buf[18..20].copy_from_slice(&(x as u16).to_be_bytes()); // C wrote x twice
    buf[20..22].copy_from_slice(&(x2 as u16).to_be_bytes());
    buf[22..24].copy_from_slice(&(y2 as u16).to_be_bytes());
    // buf[24..28] = 0, buf[29] = 0
    buf[28]     = action as u8;

    // Anchor BlockList at (m, x, y) so clif_send can locate the area.
    // clif_send with type SAMEAREA (6) handles broadcasting to all nearby PCs
    // internally — no outer foreach_in_area loop is needed or correct here.
    let mut anchor: BlockList = std::mem::zeroed();
    anchor.m = m as u16;
    anchor.x = x as u16;
    anchor.y = y as u16;
    clif_send(buf.as_ptr(), 30, &mut anchor as *mut BlockList, 6 /* SAMEAREA */);
}

/// Allocate and register a scripted temporary NPC.
///
/// Allocates a zeroed `NpcData`, fills all fields from the arguments,
/// registers it in the block grid and ID database, then fires the `on_spawn`
/// Lua event.  Mirrors `sl_g_addnpc` in `c_src/sl_compat.c`.
///
/// `npc_yname` may be null; defaults to `"nothing"` in that case.
#[no_mangle]
pub unsafe extern "C" fn sl_g_addnpc(
    name:     *const c_char,
    m:        c_int,
    x:        c_int,
    y:        c_int,
    subtype:  c_int,
    timer:    c_int,
    duration: c_int,
    owner:    c_int,
    movetime: c_int,
    npc_yname: *const c_char,
) {
    use crate::game::npc::{NpcData, BL_NPC, npc_get_new_npctempid};
    use crate::game::map_server::map_addiddb;

    // CALLOC — allocate zeroed NpcData on the heap.
    let layout = std::alloc::Layout::new::<NpcData>();
    let raw = std::alloc::alloc_zeroed(layout) as *mut NpcData;
    if raw.is_null() { return; }

    // Fill name fields (bounded copy, no overflow).
    // If name is null, (*raw).name remains zeroed ("\0"), which doscript_blargs
    // treats as an empty event name — Lua will receive an empty string root.
    if !name.is_null() {
        let src = std::ffi::CStr::from_ptr(name).to_bytes();
        let dst = &mut (*raw).name;
        let n = src.len().min(dst.len() - 1);
        for i in 0..n { dst[i] = src[i] as c_char; }
        dst[n] = 0;
    }
    let yname: &[u8] = if npc_yname.is_null() {
        b"nothing"
    } else {
        std::ffi::CStr::from_ptr(npc_yname).to_bytes()
    };
    {
        let dst = &mut (*raw).npc_name;
        let n = yname.len().min(dst.len() - 1);
        for i in 0..n { dst[i] = yname[i] as c_char; }
        dst[n] = 0;
    }

    // Fill BlockList header.
    (*raw).bl.bl_type     = BL_NPC as u8;
    (*raw).bl.subtype     = subtype as u8;
    (*raw).bl.m           = m as u16;
    (*raw).bl.x           = x as u16;
    (*raw).bl.y           = y as u16;
    (*raw).bl.graphic_id  = 0;
    (*raw).bl.graphic_color = 0;
    (*raw).bl.id          = npc_get_new_npctempid();
    (*raw).bl.next        = std::ptr::null_mut();
    (*raw).bl.prev        = std::ptr::null_mut();

    // NpcData-specific fields.
    (*raw).actiontime = timer as c_uint;
    (*raw).duration   = duration as c_uint;
    (*raw).owner      = owner as c_uint;
    (*raw).movetime   = movetime as c_uint;

    // Register in spatial grid and ID database.
    map_addblock(&mut (*raw).bl);
    map_addiddb(&mut (*raw).bl);

    // Fire on_spawn Lua event: npc.on_spawn(nd).
    crate::game::scripting::doscript_blargs(
        (*raw).name.as_ptr(),
        c"on_spawn".as_ptr(),
        &[&mut (*raw).bl as *mut BlockList],
    );
}

// ─── sl_g_setmap ─────────────────────────────────────────────────────────────

/// Reconfigure a map slot at runtime: reload its tile data from a binary `.map`
/// file and update all scalar fields (BGM, PvP flags, light, etc.).
///
/// Faithfully ported from `int sl_g_setmap(...)` in `c_src/sl_compat.c`.
///
/// Memory model
/// ------------
/// * `tile`/`pass`/`obj`/`map` arrays are Rust-Vec-allocated; freed via
///   `Vec::from_raw_parts` and replaced with fresh ones from `parse_map_file`.
/// * `registry` is Rust-alloc-allocated (Layout::array); freed with
///   `std::alloc::dealloc` and replaced with a zeroed allocation.
/// * `block`, `block_mob`, `warp` are Rust-Vec-allocated (null-pointer arrays);
///   freed via `Vec::from_raw_parts` and replaced when block dimensions change.
///
/// After loading, calls `rust_map_loadregistry` and broadcasts `sl_updatepeople`
/// to all PCs on the map so their client receives updated map metadata.
///
/// # Safety
/// The `map` global must have been initialised via `rust_map_init` +
/// `map_initblock`. `m` must be a valid index in `0..MAP_SLOTS`.  `mapfile`
/// must be a valid null-terminated C string pointing to a readable file.
#[no_mangle]
pub unsafe extern "C" fn sl_g_setmap(
    m: c_int,
    mapfile: *const c_char,
    title: *const c_char,
    bgm: c_int,
    bgmtype: c_int,
    pvp: c_int,
    spell: c_int,
    light: c_uchar,
    weather: c_int,
    sweeptime: c_int,
    cantalk: c_int,
    show_ghosts: c_int,
    region: c_int,
    indoor: c_int,
    warpout: c_int,
    bind: c_int,
    reqlvl: c_int,
    reqvita: c_int,
    reqmana: c_int,
) -> c_int {
    use std::ffi::c_ushort;
    use crate::database::map_db::{GlobalReg, parse_map_file};
    use crate::ffi::map_db::rust_map_loadregistry;

    if mapfile.is_null() { return -1; }
    let path = match std::ffi::CStr::from_ptr(mapfile).to_str() {
        Ok(s) => s.to_owned(),
        Err(_) => return -1,
    };

    // Validate slot index.
    let slot_ptr = get_map_ptr(m as u16);
    if slot_ptr.is_null() { return -1; }
    let slot = &mut *slot_ptr;

    // Parse new .map file (tile/pass/obj arrays).
    let mut tiles = match parse_map_file(&path) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("[map] sl_g_setmap: cannot read map file '{path}': {e:#}");
            println!("MAP_ERR: Map file not found ({path}).");
            return -1;
        }
    };

    let was_loaded = slot.xs > 0;
    let old_bxs = slot.bxs as usize;
    let old_bys = slot.bys as usize;
    let old_block_count = old_bxs * old_bys;

    // ── Scalar fields ──────────────────────────────────────────────────────
    if !title.is_null() {
        let src = std::ffi::CStr::from_ptr(title).to_bytes();
        let dst = &mut slot.title;
        let n = src.len().min(dst.len() - 1);
        for i in 0..n { dst[i] = src[i] as std::ffi::c_char; }
        dst[n] = 0;
    }
    slot.bgm       = bgm as c_ushort;
    slot.bgmtype   = bgmtype as c_ushort;
    slot.pvp       = pvp as u8;
    slot.spell     = spell as u8;
    slot.light     = light;
    slot.weather   = weather as u8;
    slot.sweeptime = sweeptime as u32;
    slot.cantalk   = cantalk as u8;
    slot.show_ghosts = show_ghosts as u8;
    slot.region    = region as u8;
    slot.indoor    = indoor as u8;
    slot.warpout   = warpout as u8;
    slot.bind      = bind as u8;
    slot.reqlvl    = reqlvl as u32;
    slot.reqvita   = reqvita as u32;
    slot.reqmana   = reqmana as u32;

    // ── Tile arrays ────────────────────────────────────────────────────────
    if was_loaded {
        // Free old Rust-Vec-allocated tile arrays.
        let old_cells = slot.xs as usize * slot.ys as usize;
        if old_cells > 0 {
            drop(Vec::from_raw_parts(slot.tile, old_cells, old_cells));
            drop(Vec::from_raw_parts(slot.pass, old_cells, old_cells));
            drop(Vec::from_raw_parts(slot.obj,  old_cells, old_cells));
            drop(Vec::from_raw_parts(slot.map,  old_cells, old_cells));
        }
        // Free old registry, then allocate a fresh zeroed one so rust_map_loadregistry
        // does not bail on a null pointer.
        if !slot.registry.is_null() {
            let reg_layout = std::alloc::Layout::array::<GlobalReg>(MAX_MAPREG).unwrap();
            std::alloc::dealloc(slot.registry as *mut u8, reg_layout);
        }
        let reg_layout = std::alloc::Layout::array::<GlobalReg>(MAX_MAPREG).unwrap();
        slot.registry = std::alloc::alloc_zeroed(reg_layout) as *mut GlobalReg;
    }

    slot.xs = tiles.xs;
    slot.ys = tiles.ys;
    // Transfer ownership — null out tiles fields so ParsedTiles::drop skips them.
    slot.tile = std::mem::replace(&mut tiles.tile, std::ptr::null_mut());
    slot.pass = std::mem::replace(&mut tiles.pass, std::ptr::null_mut());
    slot.obj  = std::mem::replace(&mut tiles.obj,  std::ptr::null_mut());
    slot.map  = std::mem::replace(&mut tiles.map,  std::ptr::null_mut());

    // ── Block grid dimensions ───────────────────────────────────────────────
    let new_bxs = ((tiles.xs as usize + BLOCK_SIZE - 1) / BLOCK_SIZE) as c_ushort;
    let new_bys = ((tiles.ys as usize + BLOCK_SIZE - 1) / BLOCK_SIZE) as c_ushort;
    slot.bxs = new_bxs;
    slot.bys = new_bys;
    let new_block_count = new_bxs as usize * new_bys as usize;

    if was_loaded {
        // Free old warp array; allocate fresh zeroed one.
        if !slot.warp.is_null() && old_block_count > 0 {
            drop(Vec::<*mut WarpList>::from_raw_parts(
                slot.warp, old_block_count, old_block_count,
            ));
        }
        let mut new_warp: Vec<*mut WarpList> = vec![std::ptr::null_mut(); new_block_count];
        slot.warp = new_warp.as_mut_ptr();
        std::mem::forget(new_warp);

        // Reallocate block/block_mob: zero out extra slots if shrinking,
        // then resize (realloc semantics: preserve existing pointers, zero new).
        // Since map_foreachinarea walks the chains, any freed pointers must
        // already have been removed by map_delblock callers — we only resize
        // the pointer array itself.
        if !slot.block.is_null() && old_block_count > 0 {
            let mut v = Vec::<*mut BlockList>::from_raw_parts(
                slot.block, old_block_count, old_block_count,
            );
            v.resize(new_block_count, std::ptr::null_mut());
            slot.block = v.as_mut_ptr();
            std::mem::forget(v);
        }
        if !slot.block_mob.is_null() && old_block_count > 0 {
            let mut v = Vec::<*mut BlockList>::from_raw_parts(
                slot.block_mob, old_block_count, old_block_count,
            );
            v.resize(new_block_count, std::ptr::null_mut());
            slot.block_mob = v.as_mut_ptr();
            std::mem::forget(v);
        }
    } else {
        // Not previously loaded — allocate fresh zeroed block/block_mob/warp/registry.
        let mut warp_v: Vec<*mut WarpList>   = vec![std::ptr::null_mut(); new_block_count];
        let mut bl_v:   Vec<*mut BlockList>  = vec![std::ptr::null_mut(); new_block_count];
        let mut blm_v:  Vec<*mut BlockList>  = vec![std::ptr::null_mut(); new_block_count];
        slot.warp      = warp_v.as_mut_ptr();
        slot.block     = bl_v.as_mut_ptr();
        slot.block_mob = blm_v.as_mut_ptr();
        std::mem::forget(warp_v);
        std::mem::forget(bl_v);
        std::mem::forget(blm_v);

        let reg_layout = std::alloc::Layout::array::<GlobalReg>(MAX_MAPREG).unwrap();
        slot.registry = std::alloc::alloc_zeroed(reg_layout) as *mut GlobalReg;
    }

    // ── Registry + client update ────────────────────────────────────────────
    rust_map_loadregistry(m);
    foreach_in_area(m, 0, 0, AreaType::SameMap, BL_PC_TYPE, |bl| {
        crate::ffi::scripting::sl_updatepeople(bl as *mut c_void, std::ptr::null_mut())
    });

    0
}
