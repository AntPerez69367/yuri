//! Map server lifecycle — shutdown, reload, and countdown timer.

use crate::common::traits::LegacyEntity;
use crate::core::request_shutdown;
use crate::game::entity_store::{map_id2sd_pc, map_termiddb};
use crate::game::floor_items::map_clritem;
use crate::game::map_char::intif_save_impl::sl_intif_save;
use crate::game::pc::MapSessionData;
use crate::session::{
    get_session_manager, session_call_parse, session_exists, session_get_data, session_get_eof,
    session_set_eof, SessionId,
};
use std::sync::atomic::{AtomicI32, Ordering};

// ---------------------------------------------------------------------------
// map_savechars
// ---------------------------------------------------------------------------

/// Save all online character sessions to the char server.
///
/// # Safety
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn map_savechars(_none: i32, _nonetoo: i32) -> i32 {
    for x in 0..crate::session::get_fd_max() {
        let fd = SessionId::from_raw(x);
        if !session_exists(fd) {
            continue;
        }
        if session_get_eof(fd) != 0 {
            continue;
        }
        if let Some(sd) = session_get_data(fd) {
            sl_intif_save(&mut *sd.write() as *mut MapSessionData);
        }
    }
    0
}

// ---------------------------------------------------------------------------
// map_do_term
// ---------------------------------------------------------------------------

/// Shuts down the map server: save characters, free all map tile/grid
/// allocations, and terminate all subsystem databases.
///
/// # Safety
/// Must be called exactly once at shutdown, on the game thread, after all
/// clients have been disconnected.
pub unsafe fn map_do_term() {
    use crate::database::map_db::WarpList;
    use crate::database::map_db::{GlobalReg, MAP_SLOTS, MAX_MAPREG};

    map_savechars(0, 0);
    map_clritem();
    map_termiddb();

    // Free per-slot tile arrays (Rust Vec alloc) and block grid arrays.
    if !crate::database::map_db::raw_map_ptr().is_null() {
        let slots =
            std::slice::from_raw_parts_mut(crate::database::map_db::raw_map_ptr(), MAP_SLOTS);
        for slot in slots.iter_mut() {
            let cells = slot.xs as usize * slot.ys as usize;
            let bcells = slot.bxs as usize * slot.bys as usize;

            if !slot.tile.is_null() && cells > 0 {
                drop(Vec::from_raw_parts(slot.tile, cells, cells));
                slot.tile = std::ptr::null_mut();
            }
            if !slot.obj.is_null() && cells > 0 {
                drop(Vec::from_raw_parts(slot.obj, cells, cells));
                slot.obj = std::ptr::null_mut();
            }
            if !slot.map.is_null() && cells > 0 {
                drop(Vec::from_raw_parts(slot.map, cells, cells));
                slot.map = std::ptr::null_mut();
            }
            if !slot.pass.is_null() && cells > 0 {
                drop(Vec::from_raw_parts(slot.pass, cells, cells));
                slot.pass = std::ptr::null_mut();
            }
            // block/block_mob are no longer allocated (block_grid handles spatial indexing).
            if !slot.warp.is_null() && bcells > 0 {
                drop(Vec::<*mut WarpList>::from_raw_parts(
                    slot.warp, bcells, bcells,
                ));
                slot.warp = std::ptr::null_mut();
            }
            if !slot.registry.is_null() {
                let layout = std::alloc::Layout::array::<GlobalReg>(MAX_MAPREG)
                    .expect("GlobalReg layout overflow");
                std::alloc::dealloc(slot.registry as *mut u8, layout);
                slot.registry = std::ptr::null_mut();
            }
        }
    }

    crate::game::block::map_termblock();
    crate::database::item_db::term();
    crate::database::magic_db::term();
    crate::database::class_db::term();
    println!("[map] Map Server Shutdown");
}

// ---------------------------------------------------------------------------
// map_reload
// ---------------------------------------------------------------------------

/// Reload all map data (tile, registry) and notify all online players.
///
/// # Safety
/// Must be called on the game thread. `maps_dir` and `server_id` are read from
/// `crate::config::config()`.
pub unsafe fn map_reload() -> i32 {
    use crate::database::map_db::map_reload;

    let cfg = crate::config::config();
    let serverid = cfg.server_id;
    if map_reload(&cfg.maps_dir, serverid) != 0 {
        tracing::error!("[map] map_reload failed");
        return -1;
    }

    let n = crate::database::map_db::map_n.load(Ordering::Relaxed) as usize;
    // Map IDs are sparse — must iterate all slots, not just 0..map_n.
    for i in 0..crate::database::map_db::MAP_SLOTS {
        // map_isloaded(i): registry pointer is non-null iff the map was loaded.
        let slot = &*crate::database::map_db::raw_map_ptr().add(i);
        if !slot.registry.is_null() {
            // TODO: broadcast viewport refresh to all players on this map
        }
    }

    tracing::info!("[map] Map reload finished. {} maps loaded", n);
    0
}

// ---------------------------------------------------------------------------
// Shutdown countdown timer
// ---------------------------------------------------------------------------

/// Running countdown value (milliseconds remaining until shutdown).
static RESET_TIMER_REMAINING: AtomicI32 = AtomicI32::new(0);

/// Accumulated elapsed ms since the last broadcast.
static RESET_TIMER_DIFF: AtomicI32 = AtomicI32::new(0);

/// Shutdown countdown timer callback.
///
/// `v1` — initial countdown in ms (only used on first call when `reset == 0`).
/// `v2` — elapsed ms since the last call (timer interval, typically 250).
///
/// Returns 1 when shutdown is triggered, 0 otherwise.
///
/// # Safety
/// Must be called on the game thread. Accesses the global session table and
/// `crate::session::get_fd_max()`. Both are single-threaded game globals.
pub unsafe fn map_reset_timer(v1: i32, v2: i32) -> i32 {
    let mut remaining = RESET_TIMER_REMAINING.load(Ordering::Relaxed);
    let mut diff = RESET_TIMER_DIFF.load(Ordering::Relaxed);

    if remaining == 0 {
        remaining = v1;
    }

    remaining -= v2;
    diff += v2;
    RESET_TIMER_REMAINING.store(remaining, Ordering::Relaxed);
    RESET_TIMER_DIFF.store(diff, Ordering::Relaxed);

    if remaining <= 250 {
        let msg = c"Chaos is rising up. Please re-enter in a few seconds.";
        crate::game::map_parse::chat::clif_broadcast(msg.as_ptr(), -1);
    }

    if remaining <= 0 {
        // Disconnect all active sessions, then request shutdown.
        for x in 0..crate::session::get_fd_max() {
            let fd = SessionId::from_raw(x);
            if session_exists(fd) {
                let sd = session_get_data(fd);
                if let Some(ref sd_arc) = sd {
                    if session_get_eof(fd) == 0 {
                        let player_id = sd_arc.id;
                        crate::database::blocking_run_async(crate::database::assert_send(
                            async move {
                                if let Some(pe) = map_id2sd_pc(player_id) {
                                    crate::game::client::handlers::clif_handle_disconnect(&pe)
                                        .await;
                                }
                            },
                        ));
                        tokio::task::spawn_local(session_call_parse(fd));
                        if let Some(s) = get_session_manager().get_session(fd) {
                            if let Ok(mut session) = s.try_lock() {
                                session.flush_read_buffer();
                            }
                        }
                        session_set_eof(fd, 1);
                    }
                }
            }
        }
        request_shutdown();
        RESET_TIMER_REMAINING.store(0, Ordering::Relaxed);
        RESET_TIMER_DIFF.store(0, Ordering::Relaxed);
        return 1;
    }

    if remaining <= 60_000 {
        if diff >= 10_000 {
            let msg = format!("Reset in {} seconds\0", remaining / 1000);
            crate::game::map_parse::chat::clif_broadcast(msg.as_ptr() as *const i8, -1);
            RESET_TIMER_DIFF.store(0, Ordering::Relaxed);
        }
    } else if remaining <= 3_600_000 {
        if diff >= 300_000 {
            let msg = format!("Reset in {} minutes\0", remaining / 60_000);
            crate::game::map_parse::chat::clif_broadcast(msg.as_ptr() as *const i8, -1);
            RESET_TIMER_DIFF.store(0, Ordering::Relaxed);
        }
    } else if remaining > 3_600_000 && diff >= 3_600_000 {
        let msg = format!("Reset in {} hours\0", remaining / 3_600_000);
        crate::game::map_parse::chat::clif_broadcast(msg.as_ptr() as *const i8, -1);
        RESET_TIMER_DIFF.store(0, Ordering::Relaxed);
    }

    0
}
