//! DDoS / rate-limit protection
//!
//! Ports ConnectHistory from session.c to Rust.
//! Tracks connection attempts per IP and supports manual lockout.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Non-DDoS entries expire after 3× this interval (ms).
pub const DDOS_INTERVAL: u32 = 3 * 1000;

/// DDoS-locked entries are cleared after this interval (ms).
pub const DDOS_AUTORESET: u32 = 10 * 60 * 1000;

struct ConnectEntry {
    /// Tick (ms) when this entry was last updated.
    tick: u32,
    /// Whether this IP is in DDoS lockout.
    ddos: bool,
}

struct DdosState {
    /// Map from host-byte-order IPv4 to entry.
    entries: HashMap<u32, ConnectEntry>,
    /// Normal entry expiry interval (ms).
    ddos_interval: u32,
    /// Lockout entry expiry interval (ms).
    ddos_autoreset: u32,
}

impl DdosState {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            ddos_interval: DDOS_INTERVAL,
            ddos_autoreset: DDOS_AUTORESET,
        }
    }
}

static DDOS: OnceLock<Mutex<DdosState>> = OnceLock::new();

fn get_ddos() -> &'static Mutex<DdosState> {
    DDOS.get_or_init(|| Mutex::new(DdosState::new()))
}

/// Mark an IP as DDoS-locked.
///
/// `ip_net` is in network byte order (sin_addr.s_addr), matching what
/// `rust_session_get_client_ip` returns.
pub fn add_ip_lockout(ip_net: u32) {
    let ip = u32::from_be(ip_net);
    #[cfg(not(test))]
    let tick = unsafe { crate::ffi::timer::gettick() };
    #[cfg(test)]
    let tick: u32 = 0;
    let mut state = get_ddos().lock().unwrap();
    let entry = state.entries.entry(ip).or_insert(ConnectEntry {
        tick: 0,
        ddos: false,
    });
    entry.ddos = true;
    entry.tick = tick;
    tracing::info!(
        "[ddos] lockout ip={}.{}.{}.{}",
        (ip >> 24) & 0xFF,
        (ip >> 16) & 0xFF,
        (ip >> 8) & 0xFF,
        ip & 0xFF
    );
}

/// Returns true if the IP is currently DDoS-locked.
///
/// `ip_net` is in network byte order.
pub fn is_ip_locked(ip_net: u32) -> bool {
    let ip = u32::from_be(ip_net);
    let state = get_ddos().lock().unwrap();
    state.entries.get(&ip).map(|e| e.ddos).unwrap_or(false)
}

/// Prune stale connection history entries.
///
/// Called periodically by the timer system (every second).
/// Returns the number of remaining entries (matches C's return value).
pub fn connect_check_clear() -> i32 {
    #[cfg(not(test))]
    let tick = unsafe { crate::ffi::timer::gettick() };
    #[cfg(test)]
    let tick: u32 = u32::MAX; // expire everything in tests
    let mut state = get_ddos().lock().unwrap();
    let ddos_interval = state.ddos_interval;
    let ddos_autoreset = state.ddos_autoreset;

    state.entries.retain(|_, entry| {
        let age = tick.wrapping_sub(entry.tick);
        if entry.ddos {
            age <= ddos_autoreset
        } else {
            age <= ddos_interval * 3
        }
    });

    state.entries.len() as i32
}
