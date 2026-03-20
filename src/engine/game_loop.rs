//! Main game loop — drives all game ticks and network I/O scheduling.

use std::time::Duration;

use crate::session::{
    accept_loop, drain_pending_connections, get_session_manager, session_io_task,
    shutdown_all_sessions,
};

/// Run the main game loop.
///
/// - Spawns accept tasks for all registered listeners
/// - Drives timer/mob/npc/cron/ddos/throttle ticks via Tokio intervals
/// - Session I/O is handled by per-connection tasks (`session_io_task`)
/// - Drains pending connections after each cron tick (for connections
///   made from game callbacks via `make_connection`)
pub async fn run_game_loop(_port: u16) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("[engine] Starting game loop");

    let manager = get_session_manager();

    // Take all registered std::net listeners, convert to tokio, spawn accept tasks.
    let listen_fds = manager.listen_fds.lock().unwrap().clone();

    for fd in listen_fds {
        if let Some(std_listener) = manager.take_listener(fd) {
            std_listener.set_nonblocking(true)?;
            let listener = tokio::net::TcpListener::from_std(std_listener)?;
            tracing::info!("[engine] Spawning accept loop for listener fd={}", fd);
            tokio::task::spawn_local(accept_loop(listener, fd));
        }
    }

    let mut timer_tick = tokio::time::interval(Duration::from_millis(10));
    let mut mob_tick = tokio::time::interval(Duration::from_millis(50));
    let mut npc_tick = tokio::time::interval(Duration::from_millis(100));
    let mut cron_tick = tokio::time::interval(Duration::from_secs(1));
    cron_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut ddos_tick = tokio::time::interval(Duration::from_secs(1));
    let mut throttle_tick = tokio::time::interval(Duration::from_secs(600));

    loop {
        tokio::select! {
            _ = timer_tick.tick() => {
                crate::game::time_util::timer_do(crate::game::time_util::gettick());
            }
            _ = mob_tick.tick() => {
                unsafe { crate::game::mob::mob_timer_spawns(); }
            }
            _ = npc_tick.tick() => {
                unsafe { crate::game::npc::npc_runtimers(); }
            }
            _ = cron_tick.tick() => {
                unsafe { crate::game::map_server::map_cronjob(); }

                // Spawn I/O tasks for connections made from callbacks.
                for fd in drain_pending_connections() {
                    tracing::debug!("[engine] Spawning io task for pending fd={}", fd);
                    tokio::task::spawn_local(session_io_task(fd));
                }

                // Check shutdown signal.
                if crate::engine::should_shutdown() {
                    tracing::info!("[engine] Shutdown requested");
                    break;
                }
            }
            _ = ddos_tick.tick() => {
                crate::network::ddos::connect_check_clear();
            }
            _ = throttle_tick.tick() => {
                crate::network::throttle::remove_throttle();
            }
        }
    }

    #[allow(unreachable_code)]
    {
        shutdown_all_sessions().await;
    }

    Ok(())
}
