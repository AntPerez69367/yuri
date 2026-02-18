//! Connection throttle system
//!
//! Ports the stThrottle linked list from session.c to Rust.
//! Tracks per-IP connection counts and blocks repeat offenders.
//! Resets every 10 minutes via a timer callback (matching C's Remove_Throttle).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

struct ThrottleState {
    /// Map from host-byte-order IPv4 to connection count.
    counts: HashMap<u32, u32>,
}

impl ThrottleState {
    fn new() -> Self {
        Self {
            counts: HashMap::new(),
        }
    }
}

static THROTTLE: OnceLock<Mutex<ThrottleState>> = OnceLock::new();

fn get_throttle() -> &'static Mutex<ThrottleState> {
    THROTTLE.get_or_init(|| Mutex::new(ThrottleState::new()))
}

/// Record a connection attempt from an IP (increment count).
///
/// `ip_net` is in network byte order (sin_addr.s_addr).
pub fn add_throttle(ip_net: u32) {
    let ip = u32::from_be(ip_net);
    let mut state = get_throttle().lock().unwrap();
    *state.counts.entry(ip).or_insert(0) += 1;
    tracing::debug!(
        "[throttle] add ip={}.{}.{}.{} count={}",
        (ip >> 24) & 0xFF,
        (ip >> 16) & 0xFF,
        (ip >> 8) & 0xFF,
        ip & 0xFF,
        state.counts[&ip],
    );
}

/// Returns true if this IP has been throttled (count > 0).
///
/// `ip_net` is in network byte order.
pub fn is_throttled(ip_net: u32) -> bool {
    let ip = u32::from_be(ip_net);
    let state = get_throttle().lock().unwrap();
    state.counts.get(&ip).copied().unwrap_or(0) > 0
}

/// Reset all throttle counts (matches C's Remove_Throttle).
///
/// Called as a timer callback every 10 minutes.
pub fn remove_throttle() {
    let mut state = get_throttle().lock().unwrap();
    state.counts.clear();
    tracing::debug!("[throttle] cleared all entries");
}
