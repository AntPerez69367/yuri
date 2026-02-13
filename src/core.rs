//! Core server functionality (replaces core.c)
//!
//! This module provides:
//! - Server lifecycle management
//! - Signal handling
//! - Core constants and utilities
//! - Termination callback system

use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Server tick rate in nanoseconds (10ms = 10,000,000 ns)
/// This controls how fast the main server loop runs
pub const SERVER_TICK_RATE_NS: u64 = 10_000_000;

/// Server tick rate as a Duration for convenience
pub const SERVER_TICK_RATE: Duration = Duration::from_nanos(SERVER_TICK_RATE_NS);

/// Type alias for termination callback functions
/// These are called when the server receives SIGTERM/SIGINT
pub type TermFunc = Box<dyn Fn() + Send + 'static>;

/// Global server state
pub struct ServerState {
    /// Flag indicating if shutdown has been requested
    pub shutdown_requested: bool,
    /// Optional termination callback
    pub term_func: Option<TermFunc>,
}

impl ServerState {
    /// Create a new ServerState
    pub fn new() -> Self {
        ServerState {
            shutdown_requested: false,
            term_func: None,
        }
    }

    /// Request server shutdown
    pub fn request_shutdown(&mut self) {
        self.shutdown_requested = true;
    }

    /// Check if shutdown has been requested
    pub fn should_shutdown(&self) -> bool {
        self.shutdown_requested
    }

    /// Set the termination callback function
    pub fn set_term_func<F>(&mut self, func: F)
    where
        F: Fn() + Send + 'static,
    {
        self.term_func = Some(Box::new(func));
    }

    /// Call the termination function if set
    pub fn call_term_func(&self) {
        if let Some(ref func) = self.term_func {
            func();
        }
    }
}

impl Default for ServerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe global server state
/// This allows signal handlers and other threads to access server state
pub type SharedServerState = Arc<Mutex<ServerState>>;

/// Create a new shared server state
pub fn create_server_state() -> SharedServerState {
    Arc::new(Mutex::new(ServerState::new()))
}

/// Signal types that can trigger server shutdown
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// SIGINT (Ctrl+C)
    Interrupt,
    /// SIGTERM (graceful shutdown)
    Terminate,
    /// SIGPIPE (broken pipe - usually ignored)
    Pipe,
}

impl Signal {
    /// Convert a libc signal number to our Signal enum
    pub fn from_signal_num(signum: libc::c_int) -> Option<Self> {
        match signum {
            libc::SIGINT => Some(Signal::Interrupt),
            libc::SIGTERM => Some(Signal::Terminate),
            libc::SIGPIPE => Some(Signal::Pipe),
            _ => None,
        }
    }

    /// Check if this signal should trigger shutdown
    pub fn should_shutdown(&self) -> bool {
        matches!(self, Signal::Interrupt | Signal::Terminate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_state_creation() {
        let state = ServerState::new();
        assert!(!state.should_shutdown());
        assert!(state.term_func.is_none());
    }

    #[test]
    fn test_server_state_shutdown() {
        let mut state = ServerState::new();
        assert!(!state.should_shutdown());

        state.request_shutdown();
        assert!(state.should_shutdown());
    }

    #[test]
    fn test_term_func() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let mut state = ServerState::new();
        state.set_term_func(move || {
            called_clone.store(true, Ordering::SeqCst);
        });

        assert!(!called.load(Ordering::SeqCst));
        state.call_term_func();
        assert!(called.load(Ordering::SeqCst));
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
