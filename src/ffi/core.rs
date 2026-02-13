//! FFI bindings for core module (C interop)
//!
//! This module provides C-compatible wrapper functions for core.rs
//! These will be removed once the C code is fully migrated to Rust.

use crate::core::{SharedServerState, Signal, SERVER_TICK_RATE_NS};
use std::os::raw::c_int;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Global server state accessible from C
/// This is a singleton that C code can interact with
static GLOBAL_SERVER_STATE: Mutex<Option<SharedServerState>> = Mutex::new(None);

/// Atomic flag set by signal handler, checked by main loop
/// This allows async-signal-safe signal handling
static SHUTDOWN_PENDING: AtomicBool = AtomicBool::new(false);

/// Initialize the global server state
/// This should be called once at server startup
#[no_mangle]
pub extern "C" fn rust_core_init() {
    let state = crate::core::create_server_state();
    *GLOBAL_SERVER_STATE.lock().unwrap() = Some(state);
}

/// Clean up the global server state
/// This should be called at server shutdown
#[no_mangle]
pub extern "C" fn rust_core_cleanup() {
    *GLOBAL_SERVER_STATE.lock().unwrap() = None;
}

/// Set the termination function callback
/// This replaces set_termfunc() from core.c
///
/// # Safety
/// The callback function pointer must be valid for the lifetime of the server
/// Pass NULL to clear the termination function
///
/// Note: We use Option<extern "C" fn()> directly here because:
/// 1. In Rust, Option<fn> has guaranteed NULL representation (None = NULL pointer)
/// 2. This is the standard way to represent nullable function pointers in FFI
/// 3. cbindgen will generate the correct C signature
#[no_mangle]
pub unsafe extern "C" fn rust_set_termfunc(func: Option<extern "C" fn()>) {
    if let Some(state_lock) = GLOBAL_SERVER_STATE.lock().unwrap().as_ref() {
        let mut state = state_lock.lock().unwrap();

        if let Some(f) = func {
            state.set_term_func(move || {
                f();
            });
        } else {
            state.term_func = None;
        }
    }
}

/// Handle a signal (called from C signal handlers)
/// This replaces handle_signal() from core.c
///
/// # Safety
/// Should only be called from signal handlers
///
/// # Async-signal-safety
/// This function is async-signal-safe - it only sets an atomic flag.
/// The actual shutdown processing happens in rust_should_shutdown()
/// which is called from the main loop.
#[no_mangle]
pub unsafe extern "C" fn rust_handle_signal(signum: c_int) {
    if let Some(signal) = Signal::from_signal_num(signum) {
        if signal.should_shutdown() {
            // Only set atomic flag - async-signal-safe!
            // No I/O, no mutex locking, no allocations
            SHUTDOWN_PENDING.store(true, Ordering::SeqCst);
        }
        // SIGPIPE is ignored (doesn't trigger shutdown)
    }
}

/// Request server shutdown
/// This should be called by C code to trigger graceful shutdown
/// Equivalent to the old `server_shutdown = 1` pattern
#[no_mangle]
pub extern "C" fn rust_request_shutdown() {
    if let Some(state_lock) = GLOBAL_SERVER_STATE.lock().unwrap().as_ref() {
        let mut state = state_lock.lock().unwrap();
        state.request_shutdown();
    }
}

/// Check if server shutdown has been requested
/// Returns 1 if shutdown requested, 0 otherwise
///
/// This function also processes pending shutdown requests from signals.
/// It performs the non-async-signal-safe work (logging, mutex locking,
/// calling termination callbacks) that couldn't be done in the signal handler.
#[no_mangle]
pub extern "C" fn rust_should_shutdown() -> c_int {
    // Check if signal handler set the shutdown flag
    if SHUTDOWN_PENDING.load(Ordering::SeqCst) {
        // Process shutdown in a signal-safe context (main loop)
        // This is where we can safely do I/O, locking, etc.
        eprintln!("[core] [signal] Processing shutdown signal");

        if let Some(state_lock) = GLOBAL_SERVER_STATE.lock().unwrap().as_ref() {
            let mut state = state_lock.lock().unwrap();

            // Call termination function if set
            state.call_term_func();

            // Request shutdown
            state.request_shutdown();
        }

        // Clear the pending flag (processed)
        SHUTDOWN_PENDING.store(false, Ordering::SeqCst);
    }

    // Check current shutdown state
    if let Some(state_lock) = GLOBAL_SERVER_STATE.lock().unwrap().as_ref() {
        let state = state_lock.lock().unwrap();
        if state.should_shutdown() {
            return 1;
        }
    }
    0
}

/// Get the server tick rate in nanoseconds
#[no_mangle]
pub extern "C" fn rust_get_tick_rate_ns() -> u64 {
    SERVER_TICK_RATE_NS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_init_cleanup() {
        rust_core_init();

        {
            let state_opt = GLOBAL_SERVER_STATE.lock().unwrap();
            assert!(state_opt.is_some());
        }

        rust_core_cleanup();

        {
            let state_opt = GLOBAL_SERVER_STATE.lock().unwrap();
            assert!(state_opt.is_none());
        }
    }

    #[test]
    fn test_should_shutdown() {
        rust_core_init();

        assert_eq!(rust_should_shutdown(), 0);

        // Simulate signal
        unsafe {
            rust_handle_signal(libc::SIGTERM);
        }

        assert_eq!(rust_should_shutdown(), 1);

        rust_core_cleanup();
    }

    #[test]
    fn test_request_shutdown() {
        rust_core_init();

        assert_eq!(rust_should_shutdown(), 0);

        // Request shutdown from C code
        rust_request_shutdown();

        assert_eq!(rust_should_shutdown(), 1);

        rust_core_cleanup();
    }

    #[test]
    fn test_get_tick_rate() {
        assert_eq!(rust_get_tick_rate_ns(), 10_000_000);
    }

    #[test]
    fn test_set_termfunc() {
        static mut CALLED: bool = false;

        extern "C" fn test_callback() {
            unsafe {
                CALLED = true;
            }
        }

        rust_core_init();

        unsafe {
            // Pass the function wrapped in Some() - None would be like passing NULL
            rust_set_termfunc(Some(test_callback));
            rust_handle_signal(libc::SIGTERM);
            assert!(CALLED);
        }

        rust_core_cleanup();
    }
}
