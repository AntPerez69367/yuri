//! FFI bridge for session management
//!
//! C-compatible functions for session.c replacement

use std::os::raw::c_int;
use crate::session::{init_runtime, run_async_server};

/// Initialize and run the async game server
/// Blocks until server shuts down
///
/// # Safety
/// Must be called from C main thread
#[no_mangle]
pub unsafe extern "C" fn rust_server_run(port: u16) -> c_int {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    tracing::info!("[FFI] rust_server_run(port={})", port);

    // Get or create runtime
    let runtime = init_runtime();

    // Run server (blocks)
    match runtime.block_on(run_async_server(port)) {
        Ok(_) => {
            tracing::info!("[FFI] Server shutdown complete");
            0
        }
        Err(e) => {
            tracing::error!("[FFI] Server error: {}", e);
            -1
        }
    }
}

/// Create a listening socket on the specified port
/// Returns fd on success, -1 on failure
///
/// Note: In the async model, this is handled by run_async_server
/// This is here for compatibility but may not be used
#[no_mangle]
pub extern "C" fn rust_make_listen_port(_port: c_int) -> c_int {
    tracing::warn!("[FFI] rust_make_listen_port called but not implemented in async model");
    -1
}

/// Create an outgoing connection to ip:port
/// Returns fd on success, -1 on failure
///
/// TODO: Implement in later task for client connections
#[no_mangle]
pub extern "C" fn rust_make_connection(_ip: u32, _port: c_int) -> c_int {
    tracing::warn!("[FFI] rust_make_connection not yet implemented");
    -1
}

/// Close a session
#[no_mangle]
pub extern "C" fn rust_session_eof(fd: c_int) -> c_int {
    tracing::debug!("[FFI] rust_session_eof(fd={})", fd);

    let runtime = match crate::session::RUNTIME.get() {
        Some(rt) => rt,
        None => {
            tracing::error!("[FFI] Runtime not initialized");
            return -1;
        }
    };

    runtime.block_on(async {
        let manager = crate::session::get_session_manager();

        if let Some(session_arc) = manager.get_session(fd).await {
            let mut session = session_arc.lock().await;
            session.eof = 1;
            0
        } else {
            tracing::warn!("[FFI] Session {} not found", fd);
            -1
        }
    })
}

/// Read unsigned 8-bit value from read buffer
/// Returns 0 if out of bounds or invalid fd
#[no_mangle]
pub extern "C" fn rust_session_read_u8(fd: c_int, pos: usize) -> u8 {
    let runtime = match crate::session::RUNTIME.get() {
        Some(rt) => rt,
        None => return 0,
    };

    runtime.block_on(async {
        let manager = crate::session::get_session_manager();

        if let Some(session_arc) = manager.get_session(fd).await {
            let session = session_arc.lock().await;

            match session.read_u8(pos) {
                Ok(val) => val,
                Err(e) => {
                    tracing::error!("[FFI] read_u8 error: {}", e);
                    0
                }
            }
        } else {
            tracing::warn!("[FFI] Session {} not found", fd);
            0
        }
    })
}

/// Read unsigned 16-bit value (little-endian)
/// Returns 0 if out of bounds or invalid fd
#[no_mangle]
pub extern "C" fn rust_session_read_u16(fd: c_int, pos: usize) -> u16 {
    let runtime = match crate::session::RUNTIME.get() {
        Some(rt) => rt,
        None => return 0,
    };

    runtime.block_on(async {
        let manager = crate::session::get_session_manager();

        if let Some(session_arc) = manager.get_session(fd).await {
            let session = session_arc.lock().await;

            match session.read_u16(pos) {
                Ok(val) => val,
                Err(e) => {
                    tracing::error!("[FFI] read_u16 error: {}", e);
                    0
                }
            }
        } else {
            0
        }
    })
}

/// Read unsigned 32-bit value (little-endian)
/// Returns 0 if out of bounds or invalid fd
#[no_mangle]
pub extern "C" fn rust_session_read_u32(fd: c_int, pos: usize) -> u32 {
    let runtime = match crate::session::RUNTIME.get() {
        Some(rt) => rt,
        None => return 0,
    };

    runtime.block_on(async {
        let manager = crate::session::get_session_manager();

        if let Some(session_arc) = manager.get_session(fd).await {
            let session = session_arc.lock().await;

            match session.read_u32(pos) {
                Ok(val) => val,
                Err(e) => {
                    tracing::error!("[FFI] read_u32 error: {}", e);
                    0
                }
            }
        } else {
            0
        }
    })
}

/// Write unsigned 8-bit value to write buffer
/// Returns 0 on success, -1 on error
#[no_mangle]
pub extern "C" fn rust_session_write_u8(fd: c_int, pos: usize, val: u8) -> c_int {
    let runtime = match crate::session::RUNTIME.get() {
        Some(rt) => rt,
        None => return -1,
    };

    runtime.block_on(async {
        let manager = crate::session::get_session_manager();

        if let Some(session_arc) = manager.get_session(fd).await {
            let mut session = session_arc.lock().await;

            match session.write_u8(pos, val) {
                Ok(_) => 0,
                Err(e) => {
                    tracing::error!("[FFI] write_u8 error: {}", e);
                    -1
                }
            }
        } else {
            -1
        }
    })
}

/// Write unsigned 16-bit value (little-endian)
/// Returns 0 on success, -1 on error
#[no_mangle]
pub extern "C" fn rust_session_write_u16(fd: c_int, pos: usize, val: u16) -> c_int {
    let runtime = match crate::session::RUNTIME.get() {
        Some(rt) => rt,
        None => return -1,
    };

    runtime.block_on(async {
        let manager = crate::session::get_session_manager();

        if let Some(session_arc) = manager.get_session(fd).await {
            let mut session = session_arc.lock().await;

            match session.write_u16(pos, val) {
                Ok(_) => 0,
                Err(e) => {
                    tracing::error!("[FFI] write_u16 error: {}", e);
                    -1
                }
            }
        } else {
            -1
        }
    })
}

/// Write unsigned 32-bit value (little-endian)
/// Returns 0 on success, -1 on error
#[no_mangle]
pub extern "C" fn rust_session_write_u32(fd: c_int, pos: usize, val: u32) -> c_int {
    let runtime = match crate::session::RUNTIME.get() {
        Some(rt) => rt,
        None => return -1,
    };

    runtime.block_on(async {
        let manager = crate::session::get_session_manager();

        if let Some(session_arc) = manager.get_session(fd).await {
            let mut session = session_arc.lock().await;

            match session.write_u32(pos, val) {
                Ok(_) => 0,
                Err(e) => {
                    tracing::error!("[FFI] write_u32 error: {}", e);
                    -1
                }
            }
        } else {
            -1
        }
    })
}

/// Skip N bytes in read buffer (like RFIFOSKIP)
/// Returns 0 on success, -1 on error
#[no_mangle]
pub extern "C" fn rust_session_skip(fd: c_int, len: usize) -> c_int {
    let runtime = match crate::session::RUNTIME.get() {
        Some(rt) => rt,
        None => return -1,
    };

    runtime.block_on(async {
        let manager = crate::session::get_session_manager();

        if let Some(session_arc) = manager.get_session(fd).await {
            let mut session = session_arc.lock().await;

            match session.skip(len) {
                Ok(_) => 0,
                Err(e) => {
                    tracing::error!("[FFI] skip error: {}", e);
                    -1
                }
            }
        } else {
            -1
        }
    })
}

/// Get number of unread bytes (like RFIFOREST)
#[no_mangle]
pub extern "C" fn rust_session_available(fd: c_int) -> usize {
    let runtime = match crate::session::RUNTIME.get() {
        Some(rt) => rt,
        None => return 0,
    };

    runtime.block_on(async {
        let manager = crate::session::get_session_manager();

        if let Some(session_arc) = manager.get_session(fd).await {
            let session = session_arc.lock().await;
            session.available()
        } else {
            0
        }
    })
}

/// Commit write buffer (like WFIFOSET)
/// Returns 0 on success, -1 on error
#[no_mangle]
pub extern "C" fn rust_session_commit(fd: c_int, len: usize) -> c_int {
    let runtime = match crate::session::RUNTIME.get() {
        Some(rt) => rt,
        None => return -1,
    };

    runtime.block_on(async {
        let manager = crate::session::get_session_manager();

        if let Some(session_arc) = manager.get_session(fd).await {
            let mut session = session_arc.lock().await;

            match session.commit_write(len) {
                Ok(_) => 0,
                Err(e) => {
                    tracing::error!("[FFI] commit error: {}", e);
                    -1
                }
            }
        } else {
            -1
        }
    })
}

/// Flush write buffer - trigger send
/// In async model, writes happen automatically
/// This is a no-op for compatibility
#[no_mangle]
pub extern "C" fn rust_session_flush(_fd: c_int) -> c_int {
    // Writes happen automatically in the async loop
    0
}

/// Get a raw pointer to the read buffer at offset (like RFIFOP)
/// Returns NULL if fd invalid or out of bounds
///
/// # Safety
/// The returned pointer is only valid until the next FFI call that modifies the session.
/// Caller must not hold this pointer across other rust_session_* calls.
#[no_mangle]
pub extern "C" fn rust_session_rdata_ptr(fd: c_int, pos: usize) -> *const u8 {
    let runtime = match crate::session::RUNTIME.get() {
        Some(rt) => rt,
        None => return std::ptr::null(),
    };

    runtime.block_on(async {
        let manager = crate::session::get_session_manager();

        if let Some(session_arc) = manager.get_session(fd).await {
            let session = session_arc.lock().await;

            match session.rdata_ptr(pos) {
                Ok(ptr) => ptr,
                Err(e) => {
                    tracing::error!("[FFI] rdata_ptr error: {}", e);
                    std::ptr::null()
                }
            }
        } else {
            std::ptr::null()
        }
    })
}

/// Get a mutable raw pointer to the write buffer at offset (like WFIFOP)
/// Returns NULL if fd invalid or out of bounds
///
/// # Safety
/// The returned pointer is only valid until the next FFI call that modifies the session.
/// Caller must call rust_session_commit() after writing.
#[no_mangle]
pub extern "C" fn rust_session_wdata_ptr(fd: c_int, pos: usize) -> *mut u8 {
    let runtime = match crate::session::RUNTIME.get() {
        Some(rt) => rt,
        None => return std::ptr::null_mut(),
    };

    runtime.block_on(async {
        let manager = crate::session::get_session_manager();

        if let Some(session_arc) = manager.get_session(fd).await {
            let mut session = session_arc.lock().await;

            match session.wdata_ptr(pos) {
                Ok(ptr) => ptr,
                Err(e) => {
                    tracing::error!("[FFI] wdata_ptr error: {}", e);
                    std::ptr::null_mut()
                }
            }
        } else {
            std::ptr::null_mut()
        }
    })
}

/// Ensure write buffer has room for `size` bytes (like WFIFOHEAD)
/// Returns 0 on success, -1 on error
#[no_mangle]
pub extern "C" fn rust_session_wfifohead(fd: c_int, size: usize) -> c_int {
    let runtime = match crate::session::RUNTIME.get() {
        Some(rt) => rt,
        None => return -1,
    };

    runtime.block_on(async {
        let manager = crate::session::get_session_manager();

        if let Some(session_arc) = manager.get_session(fd).await {
            let mut session = session_arc.lock().await;

            match session.ensure_wdata_capacity(size) {
                Ok(_) => 0,
                Err(e) => {
                    tracing::error!("[FFI] wfifohead error: {}", e);
                    -1
                }
            }
        } else {
            -1
        }
    })
}

/// Flush read buffer - compact unread data (like RFIFOFLUSH)
#[no_mangle]
pub extern "C" fn rust_session_rfifoflush(fd: c_int) -> c_int {
    let runtime = match crate::session::RUNTIME.get() {
        Some(rt) => rt,
        None => return -1,
    };

    runtime.block_on(async {
        let manager = crate::session::get_session_manager();

        if let Some(session_arc) = manager.get_session(fd).await {
            let mut session = session_arc.lock().await;
            session.flush_read_buffer();
            0
        } else {
            -1
        }
    })
}

/// Set default parse callback for all new sessions
///
/// # Safety
/// Callback must be a valid C function pointer
#[no_mangle]
pub unsafe extern "C" fn rust_session_set_default_parse(
    callback: unsafe extern "C" fn(c_int) -> c_int,
) {
    tracing::info!("[FFI] Setting default parse callback");

    let runtime = match crate::session::RUNTIME.get() {
        Some(rt) => rt,
        None => {
            tracing::error!("[FFI] Runtime not initialized");
            return;
        }
    };

    runtime.block_on(async {
        let manager = crate::session::get_session_manager();
        let mut callbacks = manager.default_callbacks.lock().await;
        callbacks.parse = Some(callback);
    });
}

/// Set default timeout callback
///
/// # Safety
/// Callback must be a valid C function pointer
#[no_mangle]
pub unsafe extern "C" fn rust_session_set_default_timeout(
    callback: unsafe extern "C" fn(c_int) -> c_int,
) {
    tracing::info!("[FFI] Setting default timeout callback");

    let runtime = match crate::session::RUNTIME.get() {
        Some(rt) => rt,
        None => return,
    };

    runtime.block_on(async {
        let manager = crate::session::get_session_manager();
        let mut callbacks = manager.default_callbacks.lock().await;
        callbacks.timeout = Some(callback);
    });
}

/// Set default shutdown callback
///
/// # Safety
/// Callback must be a valid C function pointer
#[no_mangle]
pub unsafe extern "C" fn rust_session_set_default_shutdown(
    callback: unsafe extern "C" fn(c_int) -> c_int,
) {
    tracing::info!("[FFI] Setting default shutdown callback");

    let runtime = match crate::session::RUNTIME.get() {
        Some(rt) => rt,
        None => return,
    };

    runtime.block_on(async {
        let manager = crate::session::get_session_manager();
        let mut callbacks = manager.default_callbacks.lock().await;
        callbacks.shutdown = Some(callback);
    });
}
