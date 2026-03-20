//! Cron-job timer and Lua hook dispatch.

use crate::game::lua::dispatch::dispatch;
use std::sync::atomic::AtomicI32;
use std::time::{SystemTime, UNIX_EPOCH};

/// Hour value from the previous cron-job tick; used to detect hour changes.
pub static OLD_HOUR: AtomicI32 = AtomicI32::new(0);

/// Minute value from the previous cron-job tick; used to detect minute changes.
pub static OLD_MINUTE: AtomicI32 = AtomicI32::new(0);

/// Timer ID returned by timer_insert for the cron-job callback.
pub static CRON_JOB_TIMER: AtomicI32 = AtomicI32::new(0);

/// Game loop callback — runs Lua cron hooks based on wall-clock seconds.
///
/// Called every 1000 ms from the Tokio select! loop.
/// Must be called on the Lua-owning thread (LocalSet).
///
/// # Safety
/// Must be called on the game thread (single-threaded game loop).
pub unsafe fn map_cronjob() {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    if t.is_multiple_of(60) {
        cron("cronJobMin");
    }
    if t.is_multiple_of(300) {
        cron("cronJob5Min");
    }
    if t.is_multiple_of(1800) {
        cron("cronJob30Min");
    }
    if t.is_multiple_of(3600) {
        cron("cronJobHour");
    }
    if t.is_multiple_of(86400) {
        cron("cronJobDay");
    }
    cron("cronJobSec");
}

#[inline]
fn cron(name: &str) {
    dispatch(name, None, &[]);
}
