//! FFI bridge for database pool initialization.

use std::os::raw::{c_char, c_int};

/// Called from C's do_init() before any *_init() calls.
/// url format: "mysql://user:pass@host:port/db"
#[no_mangle]
pub extern "C" fn rust_db_connect(url: *const c_char) -> c_int {
    let url_str = unsafe { std::ffi::CStr::from_ptr(url).to_str().unwrap().to_owned() };
    match crate::database::connect(&url_str) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!("[db] Connect failed: {}", e);
            -1
        }
    }
}
