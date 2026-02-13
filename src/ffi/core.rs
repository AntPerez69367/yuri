//! FFI bindings for core module (C interop)
//!
//! This module provides C-compatible wrapper functions for core.rs
//! These will be removed once the C code is fully migrated to Rust.

use crate::core::{SharedServerState, Signal, SERVER_TICK_RATE_NS};
use std::os::raw::c_int;
use std::sync::Mutex;

/// Global server state accessible from C
/// This is a singleton that C code can interact with
static GLOBAL_SERVER_STATE: Mutex<Option<SharedServerState>> = Mutex::new(None);

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

/// Type for C termination callback functions
pub type CTermFunc = extern "C" fn();

/// Set the termination function callback
/// This replaces set_termfunc() from core.c
///
/// # Safety
/// The callback function pointer must be valid for the lifetime of the server
#[no_mangle]
pub unsafe extern "C" fn rust_set_termfunc(func: Option<CTermFunc>) {
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
#[no_mangle]
pub unsafe extern "C" fn rust_handle_signal(signum: c_int) {
    if let Some(signal) = Signal::from_signal_num(signum) {
        eprintln!("[core] [signal] Received signal: {:?}", signal);

        if signal.should_shutdown() {
            if let Some(state_lock) = GLOBAL_SERVER_STATE.lock().unwrap().as_ref() {
                let mut state = state_lock.lock().unwrap();

                // Call termination function if set
                state.call_term_func();

                // Request shutdown
                state.request_shutdown();
            }
        }
    }
}

/// Check if server shutdown has been requested
/// Returns 1 if shutdown requested, 0 otherwise
#[no_mangle]
pub extern "C" fn rust_should_shutdown() -> c_int {
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
            rust_set_termfunc(Some(test_callback));
            rust_handle_signal(libc::SIGTERM);
            assert!(CALLED);
        }

        rust_core_cleanup();
    }
}
