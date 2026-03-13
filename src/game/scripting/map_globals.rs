//! Global scripting helpers.


use crate::database::map_db::{BlockList, WarpList, BLOCK_SIZE, MAX_MAPREG};
use crate::game::block::map_delblock;
use crate::database::map_db::get_map_ptr;
use crate::session::{session_exists, session_get_data, session_get_eof, SessionId};
use crate::game::block::{map_is_loaded, AreaType};
use crate::game::block_grid;
use crate::game::client::visual::clif_sendweather;
use crate::game::map_server::{map_deliddb, map_id2sd_pc, map_readglobalreg, map_setglobalreg};
use crate::game::pc::MapSessionData;

use crate::game::map_parse::chat::{clif_sendmsg, clif_playsound, clif_speak_inner};
use crate::game::map_parse::visual::clif_lookgone;
use crate::game::map_parse::movement::{clif_object_canmove, clif_object_canmove_from, clif_sendside};
use crate::game::map_parse::combat::{clif_sendaction, clif_sendanimation_inner, clif_sendanimation_xy_inner};
use crate::game::client::clif_send;
use crate::network::crypt::send_metalist;
use crate::game::block::map_addblock;

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------

/// Thin wrapper around `map_is_loaded` for code that still holds a `i32` map index.
/// Called from `src/game/map_char.rs`.
pub unsafe fn sl_map_isloaded(m: i32) -> i32 {
    map_is_loaded(m) as i32
}

/// Extract `bl.m` from a `USER*` (= `MapSessionData*`) and call `map_readglobalreg`.
/// before Rust knew the `MapSessionData` layout.
pub unsafe fn map_readglobalreg_sd(sd: *mut MapSessionData, attrname: *const i8) -> i32 {
    map_readglobalreg((*sd).bl.m as i32, attrname)
}

/// Extract `bl.m` from a `USER*` (= `MapSessionData*`) and call `map_setglobalreg`.
///
/// Note: callers from Lua boundaries should use `map_setglobalreg_str` directly with
/// the extracted `m` index to avoid non-Send futures. This function is kept for callers
/// that already have an async context and a raw `attrname` pointer.
pub async unsafe fn map_setglobalreg_sd(sd: *mut MapSessionData, attrname: *const i8, val: i32) -> i32 {
    let m = (*sd).bl.m as i32;
    map_setglobalreg(m, attrname, val).await
}

/// Set weather on all maps matching `region`/`indoor`, broadcasting to sessions on each map.
///
pub async unsafe fn sl_g_setweather(region: u8, indoor: u8, weather: u8) {
    let t = libc::time(std::ptr::null_mut()) as u32;
    for x in 0..65535u16 {
        // Check map validity and read timer in a sync block — no raw ptr refs cross the await.
        let (map_region, map_indoor, timer_before) = {
            let ptr = get_map_ptr(x);
            if ptr.is_null() || (*ptr).xs == 0 { continue; }
            let timer = map_readglobalreg(x as i32, c"artificial_weather_timer".as_ptr()) as u32;
            ((*ptr).region, (*ptr).indoor, timer)
        };

        let mut timer = timer_before;
        if timer > 0 && timer <= t {
            crate::game::map_server::map_setglobalreg_str(
                x as i32, "artificial_weather_timer".to_string(), 0,
            ).await;
            timer = 0;
        }

        if map_region != region || map_indoor != indoor || timer != 0 { continue; }

        // Apply weather update and broadcast in a sync block.
        {
            let ptr = get_map_ptr(x);
            if ptr.is_null() || (*ptr).xs == 0 { continue; }
            (*ptr).weather = weather;
            for i in 1..crate::session::get_fd_max() {
                let sid = SessionId::from_raw(i);
                if !session_exists(sid) { continue; }
                let tsd = session_get_data(sid);
                if tsd.is_null() || session_get_eof(sid) != 0 { continue; }
                if (*tsd).bl.m == x { clif_sendweather(tsd); }
            }
        }
    }
}

/// Set weather on a single map, broadcasting to sessions on that map.
///
pub async unsafe fn sl_g_setweatherm(m: i32, weather: u8) {
    // Read initial state synchronously — no raw ptr refs cross the await.
    let timer_before = {
        let ptr = get_map_ptr(m as u16);
        if ptr.is_null() || (*ptr).xs == 0 { return; }
        map_readglobalreg(m, c"artificial_weather_timer".as_ptr()) as u32
    };

    let t = libc::time(std::ptr::null_mut()) as u32;
    let mut timer = timer_before;
    if timer > 0 && timer <= t {
        crate::game::map_server::map_setglobalreg_str(
            m, "artificial_weather_timer".to_string(), 0,
        ).await;
        timer = 0;
    }
    if timer != 0 { return; }

    // Apply weather update and broadcast.
    let ptr = get_map_ptr(m as u16);
    if ptr.is_null() || (*ptr).xs == 0 { return; }
    (*ptr).weather = weather;
    for i in 1..crate::session::get_fd_max() {
        let sid = SessionId::from_raw(i);
        if !session_exists(sid) { continue; }
        let tsd = session_get_data(sid);
        if tsd.is_null() || session_get_eof(sid) != 0 { continue; }
        if (*tsd).bl.m == m as u16 { clif_sendweather(tsd); }
    }
}

/// Collect pointers to all online player block-lists into `out_ptrs`.
///
/// Returns the count written.
pub unsafe fn sl_g_getusers(out_ptrs: *mut *mut std::ffi::c_void, max_count: i32) -> i32 {
    let mut count = 0i32;
    for i in 0..crate::session::get_fd_max() {
        if count >= max_count { break; }
        let sid = SessionId::from_raw(i);
        if !session_exists(sid) { continue; }
        if session_get_eof(sid) != 0 { continue; }
        let tsd = session_get_data(sid);
        if tsd.is_null() { continue; }
        *out_ptrs.add(count as usize) = &mut (*tsd).bl as *mut _ as *mut std::ffi::c_void;
        count += 1;
    }
    count
}

/// Return `map[m].pvp`, or 0 if the map slot is not loaded.
///
pub unsafe fn sl_g_getmappvp(m: i32) -> i32 {
    let ptr = get_map_ptr(m as u16);
    if ptr.is_null() || (*ptr).xs == 0 { return 0; }
    (*ptr).pvp as i32
}

/// Copy `map[m].title` into `buf` (null-terminated, at most `buflen` bytes including NUL).
///
/// Returns 1 on success, 0 if the map is not loaded or args are invalid.
pub unsafe fn sl_g_getmaptitle(m: i32, buf: *mut i8, buflen: i32) -> i32 {
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
pub unsafe fn sl_g_msg(bl: *mut std::ffi::c_void, color: i32, msg: *const i8, target: i32) {
    if bl.is_null() || msg.is_null() || target == 0 { return; }
    if let Some(arc) = map_id2sd_pc(target as u32) {
        let tsd = &mut *arc.write();
        clif_sendmsg(tsd as *mut _, color, msg);
    }
}

/// Return 1 if cell (x, y) on bl's map is passable from `side`, else 0.
///
pub unsafe fn sl_g_objectcanmove(bl: *mut std::ffi::c_void, x: i32, y: i32, side: i32) -> i32 {
    if bl.is_null() { return 0; }
    let m = (*(bl as *mut BlockList)).m as i32;
    if clif_object_canmove(m, x, y, side) != 0 { 0 } else { 1 }
}

/// Return 1 if the block at (x, y) can move from that cell toward `side`, else 0.
///
pub unsafe fn sl_g_objectcanmovefrom(bl: *mut std::ffi::c_void, x: i32, y: i32, side: i32) -> i32 {
    if bl.is_null() { return 0; }
    let m = (*(bl as *mut BlockList)).m as i32;
    if clif_object_canmove_from(m, x, y, side) != 0 { 0 } else { 1 }
}

/// Remove a floor item from the spatial grid and ID DB, broadcasting disappearance.
///
/// Does NOT free memory — the Lua object may still hold references.
pub unsafe fn sl_fl_delete(bl_ptr: *mut std::ffi::c_void) {
    use crate::game::pc::BL_PC;
    if bl_ptr.is_null() { return; }
    let bl = bl_ptr as *mut BlockList;
    if (*bl).bl_type as i32 == BL_PC { return; }
    map_delblock(bl);
    if (*bl).id > 0 { clif_lookgone(bl); }
    map_deliddb((*bl).id);
}

/// Remove block from the grid and the map ID database.
///
pub unsafe fn sl_g_deliddb(bl_ptr: *mut std::ffi::c_void) {
    if bl_ptr.is_null() { return; }
    let bl = bl_ptr as *mut BlockList;
    map_delblock(bl);
    map_deliddb((*bl).id);
}

/// No-op — permanent spawn tracking is handled in Lua.
///
pub unsafe fn sl_g_addpermanentspawn(_bl_ptr: *mut std::ffi::c_void) {}

/// Broadcast block's look packet to surrounding players.
///
pub unsafe fn sl_g_sendside(bl: *mut std::ffi::c_void) {
    if bl.is_null() { return; }
    clif_sendside(bl as *mut BlockList);
}

/// Play a sound effect at bl's position.
///
pub unsafe fn sl_g_playsound(bl: *mut std::ffi::c_void, sound: i32) {
    if bl.is_null() { return; }
    clif_playsound(bl as *mut BlockList, sound);
}

/// Delete a non-PC block from the world and free its memory.
///
/// Unlike `sl_fl_delete`, this removes the block from the world.
/// Deallocation is handled by `map_deliddb` (drops the Box from the typed map).
pub unsafe fn sl_g_delete_bl(bl_ptr: *mut std::ffi::c_void) {
    use crate::game::pc::BL_PC;
    if bl_ptr.is_null() { return; }
    let bl = bl_ptr as *mut BlockList;
    if (*bl).bl_type as i32 == BL_PC { return; }
    map_delblock(bl);
    if (*bl).id > 0 {
        clif_lookgone(bl);
    }
    // map_deliddb drops the Arc from the typed entity map.
    map_deliddb((*bl).id);
}

/// Broadcast an action animation at bl's position.
///
pub unsafe fn sl_g_sendaction(bl_ptr: *mut std::ffi::c_void, action: i32, speed: i32) {
    if bl_ptr.is_null() { return; }
    clif_sendaction(&mut *(bl_ptr as *mut BlockList), action, speed, 0);
}

/// Send a throw animation packet from bl's position toward (x, y).
///
/// Packet layout: opcode 0xAA, length 0x001B, type 0x16 subtype 0x03.
pub unsafe fn sl_g_throwblock(
    bl_ptr: *mut std::ffi::c_void,
    x: i32, y: i32,
    icon: i32, color: i32, action: i32,
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
pub unsafe fn sl_g_dropitem(bl_ptr: *mut std::ffi::c_void, item_id: i32, amount: i32, owner: i32) {
    if bl_ptr.is_null() { return; }
    let bl = bl_ptr as *mut BlockList;
    let id = item_id as u32;
    let sd = if owner != 0 {
        map_id2sd_pc(owner as u32).map(|arc| &mut *arc.write() as *mut MapSessionData).unwrap_or(std::ptr::null_mut())
    } else {
        std::ptr::null_mut()
    };
    let db = crate::database::item_db::search(id);
    crate::game::mob::rust_mob_dropitem(
        (*bl).id as u32, id, amount, db.dura, db.protected, 0,
        (*bl).m as i32, (*bl).x as i32, (*bl).y as i32, sd,
    );
}

/// Drop an item at a specific map coordinate, ignoring bl's position.
///
pub unsafe fn sl_g_dropitemxy(
    _bl_ptr: *mut std::ffi::c_void,
    item_id: i32, amount: i32,
    m: i32, x: i32, y: i32,
    owner: i32,
) {
    let id = item_id as u32;
    let sd = if owner != 0 {
        map_id2sd_pc(owner as u32).map(|arc| &mut *arc.write() as *mut MapSessionData).unwrap_or(std::ptr::null_mut())
    } else {
        std::ptr::null_mut()
    };
    let db = crate::database::item_db::search(id);
    crate::game::mob::rust_mob_dropitem(0, id, amount, db.dura, db.protected, 0, m, x, y, sd);
}

/// Insert a parcel into the Parcels table, assigning the next available slot.
///
pub unsafe fn sl_g_sendparcel(
    _bl_ptr: *mut std::ffi::c_void,
    receiver: i32, sender: i32,
    item: i32, amount: i32, owner: i32,
    engrave: *const i8, npcflag: i32,
) {
    let engrave_str: String = if engrave.is_null() {
        String::new()
    } else {
        std::ffi::CStr::from_ptr(engrave).to_string_lossy().into_owned()
    };
    let receiver_u = receiver as u32;
    let item_u = item as u32;
    let db = crate::database::item_db::search(item_u);
    let dura = db.dura;
    let prot = db.protected;
    // Fire-and-forget from LocalSet context: spawn_local avoids blocking the game thread.
    tokio::task::spawn_local(async move {
        let newest: i32 = sqlx::query_scalar::<_, i32>(
            "SELECT COALESCE(MAX(`ParPosition`), -1) FROM `Parcels` WHERE `ParChaIdDestination`=?"
        )
        .bind(receiver_u)
        .fetch_one(crate::database::get_pool()).await
        .unwrap_or(-1);
        let _ = sqlx::query(
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
        .bind(engrave_str)
        .bind(newest + 1)
        .bind(npcflag)
        .bind(prot)
        .bind(dura)
        .execute(crate::database::get_pool()).await;
    });
}

// ─── Task 1.4: NPC/Animation/Packet Broadcast Functions ──────────────────────

/// BL_PC type constant — matches C enum value.
const BL_PC_TYPE: i32 = 0x01;

/// Broadcast a spell/skill animation to all PCs in AREA around bl.
///
pub unsafe fn sl_g_sendanimation(bl_ptr: *mut std::ffi::c_void, anim: i32, times: i32) {
    if bl_ptr.is_null() { return; }
    let bl = bl_ptr as *mut BlockList;
    let m  = (*bl).m as i32;
    let x  = (*bl).x as i32;
    let y  = (*bl).y as i32;
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let slot = &*crate::database::map_db::raw_map_ptr().add(m as usize);
        let ids = block_grid::ids_in_area(grid, x, y, AreaType::Area, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if let Some(arc) = map_id2sd_pc(id) {
                let pc = &mut *arc.write();
                clif_sendanimation_inner(&mut pc.bl, anim, bl, times);
            }
        }
    }
}

/// Broadcast an animation at position (x, y) to all PCs in AREA around bl.
///
pub unsafe fn sl_g_sendanimxy(
    bl_ptr: *mut std::ffi::c_void,
    anim: i32,
    x: i32,
    y: i32,
    times: i32,
) {
    if bl_ptr.is_null() { return; }
    let bl = bl_ptr as *mut BlockList;
    let m  = (*bl).m as i32;
    let bx = (*bl).x as i32;
    let by = (*bl).y as i32;
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let slot = &*crate::database::map_db::raw_map_ptr().add(m as usize);
        let ids = block_grid::ids_in_area(grid, bx, by, AreaType::Area, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if let Some(arc) = map_id2sd_pc(id) {
                let pc = &mut *arc.write();
                clif_sendanimation_xy_inner(&mut pc.bl, anim, times, x, y);
            }
        }
    }
}

/// Broadcast a repeating animation to all PCs in AREA around bl.
///
/// `duration` is in milliseconds; divided by 1000 before sending on the wire.
pub unsafe fn sl_g_repeatanimation(bl_ptr: *mut std::ffi::c_void, anim: i32, duration: i32) {
    if bl_ptr.is_null() { return; }
    let bl = bl_ptr as *mut BlockList;
    let m  = (*bl).m as i32;
    let x  = (*bl).x as i32;
    let y  = (*bl).y as i32;
    // Integer division: sub-second durations (1-999 ms) truncate to wire value 0,
    // same as the C original. Callers should pass multiples of 1000.
    let wire_dur = if duration > 0 { duration / 1000 } else { duration };
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let slot = &*crate::database::map_db::raw_map_ptr().add(m as usize);
        let ids = block_grid::ids_in_area(grid, x, y, AreaType::Area, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if let Some(pc_arc) = map_id2sd_pc(id) {
                clif_sendanimation_inner(&mut pc_arc.write().bl, anim, bl, wire_dur);
            }
        }
    }
}

/// Send a self-targeted animation from `bl` to the single player at `target_id`.
///
/// Resolves the target's map/cell via `map_id2sd`, then broadcasts to that
/// Sends a self-animation to all players in the exact cell.
pub unsafe fn sl_g_selfanimation(
    bl_ptr: *mut std::ffi::c_void,
    target_id: i32,
    anim: i32,
    times: i32,
) {
    if bl_ptr.is_null() { return; }
    let bl = bl_ptr as *mut BlockList;
    let Some(arc) = map_id2sd_pc(target_id as u32) else { return; };
    let (m, tx, ty) = { let sd = arc.read(); (sd.bl.m as i32, sd.bl.x as i32, sd.bl.y as i32) };
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let cell_ids = grid.ids_at_tile(tx as u16, ty as u16);
        for id in cell_ids {
            if let Some(arc) = map_id2sd_pc(id) {
                let pc = &mut *arc.write();
                clif_sendanimation_inner(&mut pc.bl, anim, bl, times);
            }
        }
    }
}

/// Send a self-targeted XY animation to the single player at `target_id`.
///
/// Resolves the target's map/cell, then broadcasts the XY animation to that
/// Sends a self-animation at the specified (x,y) to players in the exact cell.
pub unsafe fn sl_g_selfanimationxy(
    _bl_ptr: *mut std::ffi::c_void,
    target_id: i32,
    anim: i32,
    x: i32,
    y: i32,
    times: i32,
) {
    let Some(arc) = map_id2sd_pc(target_id as u32) else { return; };
    let (m, sx, sy) = { let sd = arc.read(); (sd.bl.m as i32, sd.bl.x as i32, sd.bl.y as i32) };
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let cell_ids = grid.ids_at_tile(sx as u16, sy as u16);
        for id in cell_ids {
            if let Some(arc) = map_id2sd_pc(id) {
                let pc = &mut *arc.write();
                clif_sendanimation_xy_inner(&mut pc.bl, anim, times, x, y);
            }
        }
    }
}

/// Send a talk/speech packet from `bl` to all PCs in AREA.
///
pub unsafe fn sl_g_talk(bl_ptr: *mut std::ffi::c_void, talk_type: i32, msg: *const i8) {
    if bl_ptr.is_null() || msg.is_null() { return; }
    let bl = bl_ptr as *mut BlockList;
    let m  = (*bl).m as i32;
    let x  = (*bl).x as i32;
    let y  = (*bl).y as i32;
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let slot = &*crate::database::map_db::raw_map_ptr().add(m as usize);
        let ids = block_grid::ids_in_area(grid, x, y, AreaType::Area, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if let Some(arc) = map_id2sd_pc(id) {
                let pc = &mut *arc.write();
                clif_speak_inner(&raw mut pc.bl, msg, bl, talk_type);
            }
        }
    }
}

/// Send metadata to all online players.
///
/// Iterates every fd slot, finds live sessions, and calls `send_metalist`
/// for each.  `send_metalist` is still in C (map_parse.c); called via FFI.
///
pub unsafe fn sl_g_sendmeta() {
    for i in 0..crate::session::get_fd_max() {
        let sid = SessionId::from_raw(i);
        if !session_exists(sid) { continue; }
        if session_get_eof(sid) != 0 { continue; }
        let tsd = session_get_data(sid);
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
pub unsafe fn sl_g_throw(
    id: i32,
    m: i32,
    x: i32,
    y: i32,
    x2: i32,
    y2: i32,
    icon: i32,
    color: i32,
    action: i32,
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
/// Handles the Lua event to dynamically add an NPC to the map.
///
/// `npc_yname` may be null; defaults to `"nothing"` in that case.
pub unsafe fn sl_g_addnpc(
    name:     *const i8,
    m:        i32,
    x:        i32,
    y:        i32,
    subtype:  i32,
    timer:    i32,
    duration: i32,
    owner:    i32,
    movetime: i32,
    npc_yname: *const i8,
) {
    use crate::game::npc::{NpcData, BL_NPC, npc_get_new_npctempid};
    use crate::game::map_server::map_addiddb_npc;

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
        for i in 0..n { dst[i] = src[i] as i8; }
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
        for i in 0..n { dst[i] = yname[i] as i8; }
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
    (*raw).actiontime = timer as u32;
    (*raw).duration   = duration as u32;
    (*raw).owner      = owner as u32;
    (*raw).movetime   = movetime as u32;

    // Insert into NPC_MAP first — this moves the data into Arc<RwLock>,
    // freeing the original allocation. `raw` is dangling after this.
    let id = (*raw).bl.id;
    map_addiddb_npc(id, Box::from_raw(raw));
    // Get the live pointer from the Arc<RwLock>.
    let raw = crate::game::map_server::map_id2npc_ref(id)
        .expect("npc just inserted").data_ptr();
    map_addblock(&mut (*raw).bl);

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
/// After loading, calls `map_loadregistry` and broadcasts `sl_updatepeople`
/// to all PCs on the map so their client receives updated map metadata.
///
/// # Safety
/// The `map` global must have been initialised via `map_init` +
/// `map_initblock`. `m` must be a valid index in `0..MAP_SLOTS`.  `mapfile`
/// must be a valid null-terminated C string pointing to a readable file.
pub unsafe fn sl_g_setmap(
    m: i32,
    mapfile: *const i8,
    title: *const i8,
    bgm: i32,
    bgmtype: i32,
    pvp: i32,
    spell: i32,
    light: u8,
    weather: i32,
    sweeptime: i32,
    cantalk: i32,
    show_ghosts: i32,
    region: i32,
    indoor: i32,
    warpout: i32,
    bind: i32,
    reqlvl: i32,
    reqvita: i32,
    reqmana: i32,
) -> i32 {
    use crate::database::map_db::{GlobalReg, parse_map_file};
    use crate::database::map_db::map_loadregistry;

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
        for i in 0..n { dst[i] = src[i] as i8; }
        dst[n] = 0;
    }
    slot.bgm       = bgm as u16;
    slot.bgmtype   = bgmtype as u16;
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
        // Free old registry, then allocate a fresh zeroed one so map_loadregistry
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
    let new_bxs = ((tiles.xs as usize + BLOCK_SIZE - 1) / BLOCK_SIZE) as u16;
    let new_bys = ((tiles.ys as usize + BLOCK_SIZE - 1) / BLOCK_SIZE) as u16;
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
    } else {
        // Not previously loaded -- allocate fresh zeroed warp/registry.
        let mut warp_v: Vec<*mut WarpList>   = vec![std::ptr::null_mut(); new_block_count];
        slot.warp      = warp_v.as_mut_ptr();
        std::mem::forget(warp_v);

        let reg_layout = std::alloc::Layout::array::<GlobalReg>(MAX_MAPREG).unwrap();
        slot.registry = std::alloc::alloc_zeroed(reg_layout) as *mut GlobalReg;
    }

    // ── Recreate block grid for the (possibly resized) map ────────────────
    block_grid::create_grid(m as usize, slot.xs, slot.ys);

    // ── Registry + client update ────────────────────────────────────────────
    map_loadregistry(m);
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let slot = &*crate::database::map_db::raw_map_ptr().add(m as usize);
        let ids = block_grid::ids_in_area(grid, 0, 0, AreaType::SameMap, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if let Some(arc) = map_id2sd_pc(id) {
                let pc = &mut *arc.write();
                crate::game::scripting::sl_updatepeople(&raw mut pc.bl as *mut std::ffi::c_void, std::ptr::null_mut());
            }
        }
    }

    0
}
