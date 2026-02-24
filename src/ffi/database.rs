//! FFI bridge for database pool initialization.

use std::os::raw::{c_char, c_int};

/// Called from C's do_init() before any *_init() calls.
/// url format: "mysql://user:pass@host:port/db"
#[no_mangle]
pub extern "C" fn rust_db_connect(url: *const c_char) -> c_int {
    // Initialize tracing here so logs from map/db init are captured.
    // try_init() is a no-op if rust_server_run already called it.
    let _ = tracing_subscriber::fmt()
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if url.is_null() {
            tracing::error!("[db] Connect called with null URL");
            return -1;
        }
        let url_str = match unsafe { std::ffi::CStr::from_ptr(url) }.to_str() {
            Ok(s) => s.to_owned(),
            Err(e) => {
                tracing::error!("[db] Connect URL is not valid UTF-8: {}", e);
                return -1;
            }
        };
        match crate::database::connect(&url_str) {
            Ok(()) => 0,
            Err(e) => {
                tracing::error!("[db] Connect failed: {}", e);
                -1
            }
        }
    })) {
        Ok(v) => v,
        Err(_) => {
            tracing::error!("[db] Connect panicked");
            -1
        }
    }
}
