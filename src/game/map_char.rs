//! Map-char inter-server communication.
//!
//! Contains `intif_mmo_tosd` — the login landing function that installs a
//! freshly-received `MmoCharStatus` into a live session and fires the full
//! player-login sequence.

#![allow(non_snake_case, dead_code, unused_variables)]

use std::ptr;

use crate::common::player::PlayerData;
use crate::database::{blocking_run_async, get_pool};
use crate::database::map_db::BlockList;
use crate::game::pc::MapSessionData;
use crate::servers::char::charstatus::MmoCharStatus;

// ---------------------------------------------------------------------------
// Constants mirrored from map_server.h / map_parse.h
// ---------------------------------------------------------------------------

const SFLAG_FULLSTATS: i32 = 0x40;
const SFLAG_HPMP: i32      = 0x20;
const SFLAG_XPMONEY: i32   = 0x10;

const BL_ALL: i32 = 0x0F;
const BL_PC:  i32 = 0x01;

// enum { LOOK_GET, LOOK_SEND } from map_parse.h
const LOOK_GET: i32 = 0;

// optFlag_walkthrough = 128 (from map_server.h)
const OPT_WALKTHROUGH: u64 = 128;

// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------

use crate::game::map_server::map_fd;

use crate::session::{session_set_eof, get_session_manager, SessionId};
use crate::network::crypt::crypt_populate_table;
use crate::game::pc::{
    pc_setpos, pc_loadmagic, pc_starttimer, pc_requestmp,
    pc_loaditem, pc_loadequip, pc_magic_startup,
    pc_calcstat, pc_checklevel,
};
use crate::game::map_parse::visual::{clif_spawn, clif_mob_look_start, clif_mob_look_close};
use crate::game::client::visual::broadcast_update_state;

use crate::game::block::AreaType;
use crate::game::block_grid;
use crate::game::map_parse::visual::clif_object_look_sub_inner;

// ---------------------------------------------------------------------------
// intif_mmo_tosd
// ---------------------------------------------------------------------------

/// Installs an `MmoCharStatus` received from the char-server into a live
/// map-server session and fires the full player-login sequence.
///
pub unsafe fn intif_mmo_tosd(fd: i32, p: *const MmoCharStatus) -> i32 {
    let sid = SessionId::from_raw(fd);
    // Ignore packets arriving on the inter-server socket itself.
    if fd == map_fd.load(std::sync::atomic::Ordering::Relaxed) {
        return 0;
    }

    // Null status pointer means the char-server signaled an error.
    if p.is_null() {
        session_set_eof(sid, 7);
        return 0;
    }

    // SAFETY: Box::new_zeroed allocates MapSessionData (~3MB) on the heap without
    // a stack intermediary. assume_init is safe for numeric/pointer fields.
    // PlayerData (String/Vec/HashMap) is immediately overwritten via ptr::write below.
    let mut sd_box: Box<MapSessionData> = unsafe { Box::new_zeroed().assume_init() };
    let sd: *mut MapSessionData = sd_box.as_mut() as *mut MapSessionData;

    // SAFETY: player contains String/Vec/HashMap — zeroed bytes are UB.
    // Write a valid default before any code path can read or drop it.
    ptr::write(ptr::addr_of_mut!((*sd).player), PlayerData::default());

    // Copy MmoCharStatus into sd->status.
    ptr::copy_nonoverlapping(p, ptr::addr_of_mut!((*sd).status), 1);

    // Populate player from the legacy status data we just copied.
    (*sd).player = PlayerData::from_mmo_char_status(&(*sd).status);

    // Attach to session.
    (*sd).fd = sid;
    if let Some(session_arc) = get_session_manager().get_session(sid) {
        if let Ok(mut session) = session_arc.try_lock() {
            session.session_data = Some(sd);
        }
    }

    // Build the per-session encryption hash table from the character name.
    // player.identity.name is a String; copy into a null-terminated stack buffer.
    {
        let mut name_buf = [0u8; 16];
        let name = (*sd).player.identity.name.as_bytes();
        let n = name.len().min(15);
        name_buf[..n].copy_from_slice(&name[..n]);
        crypt_populate_table(
            name_buf.as_ptr() as *const i8,
            (*sd).EncHash.as_mut_ptr(),
            0x401, // sizeof(sd->EncHash)
        );
    }

    // Set up the block-list header.
    (*sd).bl.id   = (*sd).player.identity.id;
    (*sd).bl.prev = ptr::null_mut();
    (*sd).bl.next = ptr::null_mut();

    // Visual / display defaults.
    (*sd).disguise       = (*sd).player.appearance.disguise;
    (*sd).disguise_color = (*sd).player.appearance.disguise_color;
    (*sd).viewx = 8;
    (*sd).viewy = 7;

    // Copy IP address from the String in player.identity into the legacy C array.
    {
        let ip_bytes = (*sd).player.identity.ipaddress.as_bytes();
        let n = ip_bytes.len().min(254);
        ptr::copy_nonoverlapping(
            ip_bytes.as_ptr() as *const i8,
            (*sd).ipaddress.as_mut_ptr(),
            n,
        );
        // Ensure null termination.
        (*sd).ipaddress[n] = 0;
    }

    // Query the Character table for the stored map position.
    // ChaMapId is `int(10) unsigned` → u32; ChaX/ChaY likewise.
    let char_id = (*sd).player.identity.id;
    let pos_opt: Option<(u32, u32, u32)> = blocking_run_async(async move {
        let pool = get_pool();
        sqlx::query_as::<_, (u32, u32, u32)>(
            "SELECT `ChaMapId`, `ChaX`, `ChaY` FROM `Character` WHERE `ChaId` = ?",
        )
        .bind(char_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
    });

    // Apply the loaded position into last_pos.
    if let Some((map_id, cx, cy)) = pos_opt {
        (*sd).player.identity.last_pos.m = map_id as u16;
        (*sd).player.identity.last_pos.x = cx as u16;
        (*sd).player.identity.last_pos.y = cy as u16;
    }

    // GM players walk through blocked tiles.
    if (*sd).player.identity.gm_level != 0 {
        (*sd).optFlags |= OPT_WALKTHROUGH;
    }

    // Fall back to map 0 / spawn point if the target map is not loaded.
    if !crate::game::block::map_is_loaded((*sd).player.identity.last_pos.m as i32) {
        (*sd).player.identity.last_pos.m = 0;
        (*sd).player.identity.last_pos.x = 8;
        (*sd).player.identity.last_pos.y = 7;
    }

    // Place the player on the map (adds to block grid).
    pc_setpos(
        sd,
        (*sd).player.identity.last_pos.m as i32,
        (*sd).player.identity.last_pos.x as i32,
        (*sd).player.identity.last_pos.y as i32,
    );

    // Load magic timers and start session timers.
    pc_loadmagic(sd);
    pc_starttimer(sd);
    pc_requestmp(sd);

    // Send initial login packets to the client.
    // Functions now ported to Rust — call via module path.
    use crate::game::map_parse::player_state::{
        clif_sendack, clif_sendtime, clif_sendid, clif_sendmapinfo,
        clif_sendstatus, clif_mystaytus_by_addr, clif_refresh, clif_sendxy,
        clif_getchararea, clif_retrieveprofile,
    };
    let fd = (*sd).fd;
    tracing::info!("[map] [login] fd={} step=sendack", fd);
    clif_sendack(sd);
    tracing::info!("[map] [login] fd={} step=sendtime", fd);
    clif_sendtime(sd);
    tracing::info!("[map] [login] fd={} step=sendid", fd);
    clif_sendid(sd);
    tracing::info!("[map] [login] fd={} step=sendmapinfo", fd);
    clif_sendmapinfo(sd);
    tracing::info!("[map] [login] fd={} step=sendstatus", fd);
    clif_sendstatus(sd, SFLAG_FULLSTATS | SFLAG_HPMP | SFLAG_XPMONEY);
    tracing::info!("[map] [login] fd={} step=mystaytus_1", fd);
    crate::database::blocking_run_async(clif_mystaytus_by_addr(sd as usize));
    tracing::info!("[map] [login] fd={} step=spawn", fd);
    clif_spawn(sd);
    tracing::info!("[map] [login] fd={} step=refresh", fd);
    clif_refresh(sd);
    tracing::info!("[map] [login] fd={} step=sendxy", fd);
    clif_sendxy(sd);
    tracing::info!("[map] [login] fd={} step=getchararea", fd);
    clif_getchararea(sd);

    // Broadcast visible entities to the new player.
    tracing::info!("[map] [login] fd={} step=mob_look_start", fd);
    clif_mob_look_start(sd);
    if let Some(grid) = block_grid::get_grid((*sd).bl.m as usize) {
        let slot = &*crate::database::map_db::raw_map_ptr().add((*sd).bl.m as usize);
        let ids = block_grid::ids_in_area(grid, (*sd).bl.x as i32, (*sd).bl.y as i32, AreaType::SameArea, slot.xs as i32, slot.ys as i32);
        for id in ids {
            let bl_ptr = crate::game::map_server::map_id2bl_ref(id);
            if !bl_ptr.is_null() {
                clif_object_look_sub_inner(bl_ptr, LOOK_GET, sd as *mut BlockList);
            }
        }
    }
    clif_mob_look_close(sd);

    // Load inventory and equipment.
    tracing::info!("[map] [login] fd={} step=loaditem", fd);
    pc_loaditem(sd);
    tracing::info!("[map] [login] fd={} step=loadequip", fd);
    pc_loadequip(sd);

    // Initialise magic system state for this session.
    tracing::info!("[map] [login] fd={} step=magic_startup", fd);
    pc_magic_startup(sd);

    // Register the player in the global ID database and mark online.
    tracing::info!("[map] [login] fd={} step=addiddb", fd);
    // Store player in typed map — moves sd_box data into Arc<RwLock>,
    // freeing the original Box. sd is dangling after this call.
    let sd_id = (*sd).bl.id;
    crate::game::map_server::map_addiddb_player(sd_id, sd_box);
    // Get the live pointer from the Arc<RwLock>.
    let sd: *mut crate::game::pc::MapSessionData =
        crate::game::map_server::map_id2sd_pc(sd_id)
            .expect("player just inserted").data_ptr();
    // Update session.session_data — the old pointer was into the freed Box.
    if let Some(session_arc) = crate::session::get_session_manager().get_session((*sd).fd) {
        if let Ok(mut session) = session_arc.try_lock() {
            session.session_data = Some(sd);
        }
    }
    // mmo_setonline returns true when the "login" Lua hook should fire.
    // We capture that here and fire the hook AFTER blocking_run_async returns so
    // Lua's DB calls (which also use blocking_run_async) don't deadlock on DB_RUNTIME.
    let fire_login_hook = crate::database::blocking_run_async(
        crate::database::assert_send(crate::game::map_server::mmo_setonline((*sd).player.identity.id, 1))
    );
    if fire_login_hook {
        let name_str = (*sd).player.identity.name.as_str();
        let raw_ip = crate::session::session_get_client_ip(fd);
        let addr = format!("{}.{}.{}.{}", raw_ip & 0xff, (raw_ip >> 8) & 0xff,
            (raw_ip >> 16) & 0xff, (raw_ip >> 24) & 0xff);
        println!("[map] [login] name={} addr={}", name_str, addr);
        let bl_ptr = ptr::addr_of_mut!((*sd).bl);
        crate::game::scripting::doscript_blargs(
            b"login\0".as_ptr() as *const i8,
            std::ptr::null(),
            &[bl_ptr],
        );
    }

    // Final stat calculation and state broadcast.
    tracing::info!("[map] [login] fd={} step=calcstat", fd);
    pc_calcstat(sd);
    pc_checklevel(sd);
    tracing::info!("[map] [login] fd={} step=mystaytus_2", fd);
    crate::database::blocking_run_async(clif_mystaytus_by_addr(sd as usize));

    // Send our state to all PCs in the area.
    tracing::info!("[map] [login] fd={} step=updatestate", fd);
    broadcast_update_state(sd);

    tracing::info!("[map] [login] fd={} step=retrieveprofile", fd);
    clif_retrieveprofile(sd);
    tracing::info!("[map] [login] fd={} step=done", fd);
    0
}



use std::sync::{Arc, OnceLock};
use tokio::runtime::Handle;
use crate::servers::map::{MapState, packet};

static MAP_STATE: OnceLock<Arc<MapState>> = OnceLock::new();

/// Call intif_mmo_tosd with a raw mmo_charstatus buffer.
/// The buffer is reinterpreted as *const MmoCharStatus (same ABI, same layout).
pub fn call_intif_mmo_tosd(fd: i32, raw: &mut Vec<u8>) -> i32 {
    let p = raw.as_ptr() as *const crate::servers::char::charstatus::MmoCharStatus;
    unsafe { intif_mmo_tosd(fd, p) }
}

/// Kept for binary compatibility: called by map_server.rs main(), now a no-op
/// since intif_mmo_tosd is called directly.
pub fn set_mmo_tosd_fn(_f: unsafe fn(i32, *mut u8) -> i32) {}

/// Called by map_server.rs main() after MapState is constructed.
pub fn set_map_state(state: Arc<MapState>) {
    let _ = MAP_STATE.set(state);
}

pub fn send(data: Vec<u8>) {
    if let Some(state) = MAP_STATE.get() {
        let s = Arc::clone(state);
        if let Ok(handle) = Handle::try_current() {
            handle.spawn(async move { packet::send_to_char(&s, data).await; });
        }
    }
}

/// 0x3003 — Request char data (map→char, 24 bytes).
pub unsafe fn intif_load(fd: i32, char_id: u32, name: *const i8) {
    if name.is_null() { return; }
    let nb = std::ffi::CStr::from_ptr(name).to_bytes();
    let mut pkt = vec![0u8; 24];
    pkt[0] = 0x03; pkt[1] = 0x30;
    pkt[2..4].copy_from_slice(&(fd as u16).to_le_bytes());
    pkt[4..8].copy_from_slice(&char_id.to_le_bytes());
    pkt[8..8 + nb.len().min(16)].copy_from_slice(&nb[..nb.len().min(16)]);
    send(pkt);
}

/// 0x3005 — Logout notification (map→char, 6 bytes).
pub unsafe fn intif_quit(char_id: u32) {
    let mut pkt = vec![0u8; 6];
    pkt[0] = 0x05; pkt[1] = 0x30;
    pkt[2..6].copy_from_slice(&char_id.to_le_bytes());
    send(pkt);
}

/// 0x3004 — Save char (map→char, variable).
pub unsafe fn intif_save(data: *const u8, len: u32) {
    if data.is_null() || len < 6 { return; }
    let pkt = std::slice::from_raw_parts(data, len as usize).to_vec();
    send(pkt);
}

/// 0x3007 — Save-and-quit (map→char, variable).
pub unsafe fn intif_savequit(data: *const u8, len: u32) {
    if data.is_null() || len < 6 { return; }
    let pkt = std::slice::from_raw_parts(data, len as usize).to_vec();
    send(pkt);
}

pub mod intif_save_impl {
    use super::*;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write as _;
    use crate::game::pc::MapSessionData;
    use crate::game::block::map_is_loaded;

    unsafe fn compress_status(sd: *mut MapSessionData, cmd: u16) -> Option<Vec<u8>> {
        if sd.is_null() { return None; }
        let status_ptr = &(*sd).status as *const _ as *const u8;
        let status_len = std::mem::size_of_val(&(*sd).status);
        let raw = std::slice::from_raw_parts(status_ptr, status_len);
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::fast());
        enc.write_all(raw).ok()?;
        let compressed = enc.finish().ok()?;
        let total: u32 = (6 + compressed.len()) as u32;
        let mut pkt = Vec::with_capacity(total as usize);
        pkt.push((cmd & 0xff) as u8);
        pkt.push((cmd >> 8) as u8);
        pkt.extend_from_slice(&total.to_le_bytes());
        pkt.extend_from_slice(&compressed);
        Some(pkt)
    }

    pub unsafe fn sl_intif_save(sd: *mut MapSessionData) -> i32 {
        if sd.is_null() { return -1; }

        // Sync runtime shadow fields into player before serialization.
        (*sd).player.identity.last_pos.m = (*sd).bl.m;
        (*sd).player.identity.last_pos.x = (*sd).bl.x;
        (*sd).player.identity.last_pos.y = (*sd).bl.y;
        (*sd).player.appearance.disguise       = (*sd).disguise;
        (*sd).player.appearance.disguise_color = (*sd).disguise_color;

        // Sync player → status for wire serialization (zlib raw bytes).
        (*sd).player.sync_to_legacy(&mut (*sd).status);

        match compress_status(sd, 0x3004) {
            Some(pkt) => { intif_save(pkt.as_ptr(), pkt.len() as u32); 0 }
            None      => -1,
        }
    }

    pub unsafe fn sl_intif_savequit(sd: *mut MapSessionData) -> i32 {
        if sd.is_null() { return -1; }

        // Dest-pos fallback: if the target map isn't loaded, warp to current position.
        if !map_is_loaded((*sd).player.identity.dest_pos.m as i32) {
            if (*sd).player.identity.dest_pos.m == 0 {
                (*sd).player.identity.dest_pos.m = (*sd).bl.m;
                (*sd).player.identity.dest_pos.x = (*sd).bl.x;
                (*sd).player.identity.dest_pos.y = (*sd).bl.y;
            }
            (*sd).player.identity.last_pos = (*sd).player.identity.dest_pos;
        } else {
            (*sd).player.identity.last_pos.m = (*sd).bl.m;
            (*sd).player.identity.last_pos.x = (*sd).bl.x;
            (*sd).player.identity.last_pos.y = (*sd).bl.y;
        }

        // Sync runtime shadow fields.
        (*sd).player.appearance.disguise       = (*sd).disguise;
        (*sd).player.appearance.disguise_color = (*sd).disguise_color;

        // Sync player → status for wire serialization.
        (*sd).player.sync_to_legacy(&mut (*sd).status);

        match compress_status(sd, 0x3007) {
            Some(pkt) => { intif_savequit(pkt.as_ptr(), pkt.len() as u32); 0 }
            None      => -1,
        }
    }
}
