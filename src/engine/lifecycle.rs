//! Server lifecycle — state management, signals, and shutdown.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Server tick rate in nanoseconds (10ms = 10,000,000 ns)
pub const SERVER_TICK_RATE_NS: u64 = 10_000_000;

/// Server tick rate as a Duration for convenience
pub const SERVER_TICK_RATE: Duration = Duration::from_nanos(SERVER_TICK_RATE_NS);

/// Global server state
pub struct ServerState {
    pub shutdown_requested: bool,
}

impl ServerState {
    pub fn new() -> Self {
        ServerState {
            shutdown_requested: false,
        }
    }

    pub fn request_shutdown(&mut self) {
        self.shutdown_requested = true;
    }

    pub fn should_shutdown(&self) -> bool {
        self.shutdown_requested
    }
}

impl Default for ServerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe global server state
pub type SharedServerState = Arc<Mutex<ServerState>>;

/// Create a new shared server state
pub fn create_server_state() -> SharedServerState {
    Arc::new(Mutex::new(ServerState::new()))
}

/// Signal types that can trigger server shutdown
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Interrupt,
    Terminate,
    Pipe,
}

impl Signal {
    pub fn from_signal_num(signum: libc::c_int) -> Option<Self> {
        match signum {
            libc::SIGINT => Some(Signal::Interrupt),
            libc::SIGTERM => Some(Signal::Terminate),
            libc::SIGPIPE => Some(Signal::Pipe),
            _ => None,
        }
    }

    pub fn should_shutdown(&self) -> bool {
        matches!(self, Signal::Interrupt | Signal::Terminate)
    }
}

// ─── Global state ──────────────────────────────────────────────────────────

static GLOBAL_SERVER_STATE: Mutex<Option<SharedServerState>> = Mutex::new(None);
static SHUTDOWN_PENDING: AtomicBool = AtomicBool::new(false);

pub fn core_init() {
    let state = create_server_state();
    *GLOBAL_SERVER_STATE.lock().unwrap() = Some(state);
}

pub fn core_cleanup() {
    *GLOBAL_SERVER_STATE.lock().unwrap() = None;
}

/// # Safety
/// Should only be called from signal handlers
pub unsafe fn handle_signal(signum: i32) {
    if let Some(signal) = Signal::from_signal_num(signum) {
        if signal.should_shutdown() {
            SHUTDOWN_PENDING.store(true, Ordering::SeqCst);
        }
    }
}

pub fn request_shutdown() {
    if let Some(state_lock) = GLOBAL_SERVER_STATE.lock().unwrap().as_ref() {
        let mut state = state_lock.lock().unwrap();
        state.request_shutdown();
    }
}

pub fn should_shutdown() -> bool {
    if SHUTDOWN_PENDING.load(Ordering::SeqCst) {
        tracing::info!("[engine] Processing shutdown signal");
        SHUTDOWN_PENDING.store(false, Ordering::SeqCst);
        request_shutdown();
    }
    if let Some(state_lock) = GLOBAL_SERVER_STATE.lock().unwrap().as_ref() {
        let state = state_lock.lock().unwrap();
        if state.should_shutdown() { return true; }
    }
    false
}

pub fn get_tick_rate_ns() -> u64 {
    SERVER_TICK_RATE_NS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_state_creation() {
        let state = ServerState::new();
        assert!(!state.should_shutdown());
    }

    #[test]
    fn test_server_state_shutdown() {
        let mut state = ServerState::new();
        assert!(!state.should_shutdown());
        state.request_shutdown();
        assert!(state.should_shutdown());
    }

    #[test]
    fn test_signal_conversion() {
        assert_eq!(Signal::from_signal_num(libc::SIGINT), Some(Signal::Interrupt));
        assert_eq!(Signal::from_signal_num(libc::SIGTERM), Some(Signal::Terminate));
        assert_eq!(Signal::from_signal_num(libc::SIGPIPE), Some(Signal::Pipe));
        assert_eq!(Signal::from_signal_num(999), None);
    }

    #[test]
    fn test_signal_should_shutdown() {
        assert!(Signal::Interrupt.should_shutdown());
        assert!(Signal::Terminate.should_shutdown());
        assert!(!Signal::Pipe.should_shutdown());
    }

    #[test]
    fn test_shared_server_state() {
        let state = create_server_state();
        {
            let mut s = state.lock().unwrap();
            assert!(!s.should_shutdown());
            s.request_shutdown();
        }
        {
            let s = state.lock().unwrap();
            assert!(s.should_shutdown());
        }
    }

    #[test]
    fn test_constants() {
        assert_eq!(SERVER_TICK_RATE_NS, 10_000_000);
        assert_eq!(SERVER_TICK_RATE, Duration::from_nanos(10_000_000));
        assert_eq!(SERVER_TICK_RATE, Duration::from_millis(10));
    }
}
