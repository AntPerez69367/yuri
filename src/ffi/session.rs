//! FFI bridge for session management
//!
//! C-compatible functions for session.c replacement.
//!
//! ## Design: No block_on() in FFI
//!
//! All FFI functions are callable from two contexts:
//!   1. Before rust_server_run() — C init code, block_on() would be safe but we avoid it
//!   2. Inside the running Tokio runtime — timer callbacks, C parse callbacks
//!      block_on() panics here ("cannot start a runtime from within a runtime")
//!
//! Solution: SessionManager uses std::sync primitives (RwLock, Mutex, AtomicI32).
//! Individual sessions use tokio::sync::Mutex, accessed via try_lock() from FFI.
//! try_lock() always succeeds in practice because:
//!   - parse callbacks run after session_io_task releases the session lock
//!   - timer callbacks run synchronously in the select! arm; all async tasks are suspended

use std::sync::Arc;
use std::os::raw::c_int;
use tokio::sync::Mutex;
use crate::session::{init_runtime, run_async_server, Session};

/// Called by C's session.c to register the fd_max update function.
/// Rust calls this callback whenever a new session is created so that
/// C code using `for (i=0; i<fd_max; i++)` loops stays correct.
static FD_MAX_UPDATER: std::sync::OnceLock<unsafe extern "C" fn(c_int)> = std::sync::OnceLock::new();

#[no_mangle]
pub unsafe extern "C" fn rust_register_fd_max_updater(cb: unsafe extern "C" fn(c_int)) {
    let _ = FD_MAX_UPDATER.set(cb);
}

/// Update C's fd_max via the registered callback.
pub fn update_fd_max_pub(fd: i32) {
    if let Some(cb) = FD_MAX_UPDATER.get() {
        unsafe { cb(fd); }
    }
}

fn update_fd_max(fd: i32) {
    update_fd_max_pub(fd);
}

/// Helper: access a session synchronously from C FFI without block_on().
/// Uses try_lock() which works from both inside and outside the Tokio runtime.
fn with_session<F, R>(fd: i32, default: R, f: F) -> R
where
    F: FnOnce(&mut Session) -> R,
{
    let manager = crate::session::get_session_manager();
    if let Some(session_arc) = manager.get_session(fd) {
        match session_arc.try_lock() {
            Ok(mut guard) => f(&mut guard),
            Err(_) => {
                tracing::warn!("[FFI] fd={} session lock contention (try_lock failed)", fd);
                default
            }
        }
    } else {
        default
    }
}

/// Initialize and run the async game server.
/// Blocks until server shuts down.
///
/// # Safety
/// Must be called from C main thread after do_init() has registered listeners.
#[no_mangle]
pub unsafe extern "C" fn rust_server_run(port: u16) -> c_int {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    tracing::info!("[FFI] rust_server_run(port={})", port);

    let runtime = init_runtime();

    // LocalSet is required for spawn_local (used by accept_loop and session_io_task)
    let local = tokio::task::LocalSet::new();

    match local.block_on(runtime, run_async_server(port)) {
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

/// Create a listening socket on the specified port.
/// Returns fd on success, -1 on failure.
///
/// Binds a std::net::TcpListener (sync, safe from any context) and stores it
/// in the SessionManager. Converted to tokio::net::TcpListener at server start.
#[no_mangle]
pub extern "C" fn rust_make_listen_port(port: c_int) -> c_int {
    tracing::info!("[FFI] rust_make_listen_port(port={})", port);

    let addr = format!("0.0.0.0:{}", port);
    let std_listener = match std::net::TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("[FFI] Failed to bind port {}: {}", port, e);
            return -1;
        }
    };

    let manager = crate::session::get_session_manager();
    let fd = match manager.allocate_fd() {
        Ok(fd) => fd,
        Err(e) => {
            tracing::error!("[FFI] Failed to allocate fd for listener: {}", e);
            return -1;
        }
    };

    tracing::info!("[FFI] Listener bound on port {}, fd={}", port, fd);
    manager.add_listener(fd, std_listener);
    update_fd_max(fd);
    fd
}

/// Create an outgoing connection to ip:port.
/// Returns fd on success, -1 on failure.
///
/// Safe to call from inside the Tokio runtime (timer callbacks, parse callbacks).
/// The actual TCP connect happens asynchronously in session_io_task after this returns.
/// ip is in network byte order (matching sin_addr.s_addr).
#[no_mangle]
pub extern "C" fn rust_make_connection(ip: u32, port: c_int) -> c_int {
    // ip is sin_addr.s_addr: network byte order (big-endian).
    // u32::from_be() converts from network to host byte order for Ipv4Addr construction.
    let ipv4 = std::net::Ipv4Addr::from(u32::from_be(ip));
    let addr = std::net::SocketAddr::new(std::net::IpAddr::V4(ipv4), port as u16);

    tracing::info!("[FFI] rust_make_connection queuing outgoing connection to {}", addr);

    let manager = crate::session::get_session_manager();

    let fd = match manager.allocate_fd() {
        Ok(fd) => fd,
        Err(e) => {
            tracing::error!("[FFI] allocate_fd failed: {}", e);
            return -1;
        }
    };

    let mut session = Session::new(fd);
    session.client_addr = Some(addr);
    // Store in network byte order — same value C passed in, ready to return via get_client_ip
    session.client_addr_raw = ip;
    // Signal session_io_task to perform the actual async connect
    session.connect_addr = Some(addr);
    session.callbacks = manager.get_default_callbacks();

    let session_arc = Arc::new(Mutex::new(session));
    if let Err(e) = manager.insert_session(fd, session_arc) {
        tracing::error!("[FFI] insert_session failed: {}", e);
        return -1;
    }

    // Queue fd for session_io_task spawn after the current timer tick completes.
    // run_async_server drains this queue after each timer_do() call.
    crate::session::push_pending_connection(fd);

    tracing::info!("[FFI] Queued outgoing connection to {}, fd={}", addr, fd);
    update_fd_max(fd);
    fd
}

/// Mark a session for closing.
#[no_mangle]
pub extern "C" fn rust_session_eof(fd: c_int) -> c_int {
    with_session(fd, -1, |session| {
        session.eof = 1;
        0
    })
}

/// Read unsigned 8-bit value from read buffer.
/// Returns 0 if out of bounds or session not found.
#[no_mangle]
pub extern "C" fn rust_session_read_u8(fd: c_int, pos: usize) -> u8 {
    with_session(fd, 0, |session| {
        session.read_u8(pos).unwrap_or_else(|e| {
            tracing::error!("[FFI] read_u8 error: {}", e);
            0
        })
    })
}

/// Read unsigned 16-bit value (little-endian).
/// Returns 0 if out of bounds or session not found.
#[no_mangle]
pub extern "C" fn rust_session_read_u16(fd: c_int, pos: usize) -> u16 {
    with_session(fd, 0, |session| {
        session.read_u16(pos).unwrap_or_else(|e| {
            tracing::error!("[FFI] read_u16 error: {}", e);
            0
        })
    })
}

/// Read unsigned 32-bit value (little-endian).
/// Returns 0 if out of bounds or session not found.
#[no_mangle]
pub extern "C" fn rust_session_read_u32(fd: c_int, pos: usize) -> u32 {
    with_session(fd, 0, |session| {
        session.read_u32(pos).unwrap_or_else(|e| {
            tracing::error!("[FFI] read_u32 error: {}", e);
            0
        })
    })
}

/// Write unsigned 8-bit value to write buffer.
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub extern "C" fn rust_session_write_u8(fd: c_int, pos: usize, val: u8) -> c_int {
    with_session(fd, -1, |session| {
        session.write_u8(pos, val).map(|_| 0).unwrap_or_else(|e| {
            tracing::error!("[FFI] write_u8 error: {}", e);
            -1
        })
    })
}

/// Write unsigned 16-bit value (little-endian).
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub extern "C" fn rust_session_write_u16(fd: c_int, pos: usize, val: u16) -> c_int {
    with_session(fd, -1, |session| {
        session.write_u16(pos, val).map(|_| 0).unwrap_or_else(|e| {
            tracing::error!("[FFI] write_u16 error: {}", e);
            -1
        })
    })
}

/// Write unsigned 32-bit value (little-endian).
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub extern "C" fn rust_session_write_u32(fd: c_int, pos: usize, val: u32) -> c_int {
    with_session(fd, -1, |session| {
        session.write_u32(pos, val).map(|_| 0).unwrap_or_else(|e| {
            tracing::error!("[FFI] write_u32 error: {}", e);
            -1
        })
    })
}

/// Skip N bytes in read buffer (like RFIFOSKIP).
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub extern "C" fn rust_session_skip(fd: c_int, len: usize) -> c_int {
    with_session(fd, -1, |session| {
        session.skip(len).map(|_| 0).unwrap_or_else(|e| {
            tracing::error!("[FFI] skip error: {}", e);
            -1
        })
    })
}

/// Get number of unread bytes (like RFIFOREST).
#[no_mangle]
pub extern "C" fn rust_session_available(fd: c_int) -> usize {
    with_session(fd, 0, |session| session.available())
}

/// Commit write buffer (like WFIFOSET).
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub extern "C" fn rust_session_commit(fd: c_int, len: usize) -> c_int {
    with_session(fd, -1, |session| {
        session.commit_write(len).map(|_| 0).unwrap_or_else(|e| {
            tracing::error!("[FFI] commit error: {}", e);
            -1
        })
    })
}

/// Flush write buffer - no-op in async model (writes happen in session_io_task).
#[no_mangle]
pub extern "C" fn rust_session_flush(_fd: c_int) -> c_int {
    0
}

/// Get a raw pointer to the read buffer at offset (like RFIFOP).
/// Returns NULL if fd invalid or out of bounds.
///
/// # Safety
/// The returned pointer is valid only while the C call stack holds no other FFI calls
/// that could modify this session's read buffer (skip, flush). In practice this is
/// safe because C parse callbacks operate on a single session at a time.
#[no_mangle]
pub extern "C" fn rust_session_rdata_ptr(fd: c_int, pos: usize) -> *const u8 {
    with_session(fd, std::ptr::null(), |session| {
        session.rdata_ptr(pos).unwrap_or_else(|e| {
            tracing::error!("[FFI] rdata_ptr error: {}", e);
            std::ptr::null()
        })
    })
}

/// Get a mutable raw pointer to the write buffer at offset (like WFIFOP).
/// Returns NULL if fd invalid or out of bounds.
///
/// # Safety
/// Caller must call rust_session_commit() after writing to advance wdata_size.
#[no_mangle]
pub extern "C" fn rust_session_wdata_ptr(fd: c_int, pos: usize) -> *mut u8 {
    with_session(fd, std::ptr::null_mut(), |session| {
        session.wdata_ptr(pos).unwrap_or_else(|e| {
            tracing::error!("[FFI] wdata_ptr error: {}", e);
            std::ptr::null_mut()
        })
    })
}

/// Ensure write buffer has room for `size` bytes (like WFIFOHEAD).
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub extern "C" fn rust_session_wfifohead(fd: c_int, size: usize) -> c_int {
    with_session(fd, -1, |session| {
        session.ensure_wdata_capacity(size).map(|_| 0).unwrap_or_else(|e| {
            tracing::error!("[FFI] wfifohead error: {}", e);
            -1
        })
    })
}

/// Flush read buffer - compact unread data (like RFIFOFLUSH).
#[no_mangle]
pub extern "C" fn rust_session_rfifoflush(fd: c_int) -> c_int {
    with_session(fd, -1, |session| {
        session.flush_read_buffer();
        0
    })
}

/// Set default accept callback — called once per new incoming connection.
/// Use this for initial handshake packets (server speaks first).
///
/// # Safety
/// Callback must be a valid C function pointer.
#[no_mangle]
pub unsafe extern "C" fn rust_session_set_default_accept(
    callback: unsafe extern "C" fn(c_int) -> c_int,
) {
    tracing::info!("[FFI] Setting default accept callback");
    let manager = crate::session::get_session_manager();
    manager.default_callbacks.lock().unwrap().accept = Some(callback);
}

/// Set default parse callback for all new sessions.
///
/// # Safety
/// Callback must be a valid C function pointer.
#[no_mangle]
pub unsafe extern "C" fn rust_session_set_default_parse(
    callback: unsafe extern "C" fn(c_int) -> c_int,
) {
    tracing::info!("[FFI] Setting default parse callback");
    let manager = crate::session::get_session_manager();
    manager.default_callbacks.lock().unwrap().parse = Some(callback);
}

/// Set default timeout callback.
///
/// # Safety
/// Callback must be a valid C function pointer.
#[no_mangle]
pub unsafe extern "C" fn rust_session_set_default_timeout(
    callback: unsafe extern "C" fn(c_int) -> c_int,
) {
    tracing::info!("[FFI] Setting default timeout callback");
    let manager = crate::session::get_session_manager();
    manager.default_callbacks.lock().unwrap().timeout = Some(callback);
}

/// Set default shutdown callback.
///
/// # Safety
/// Callback must be a valid C function pointer.
#[no_mangle]
pub unsafe extern "C" fn rust_session_set_default_shutdown(
    callback: unsafe extern "C" fn(c_int) -> c_int,
) {
    tracing::info!("[FFI] Setting default shutdown callback");
    let manager = crate::session::get_session_manager();
    manager.default_callbacks.lock().unwrap().shutdown = Some(callback);
}

/// Get session_data pointer (opaque void* for C).
#[no_mangle]
pub extern "C" fn rust_session_get_data(fd: c_int) -> *mut std::ffi::c_void {
    with_session(fd, std::ptr::null_mut(), |session| {
        session.session_data.unwrap_or(std::ptr::null_mut())
    })
}

/// Set session_data pointer.
#[no_mangle]
pub extern "C" fn rust_session_set_data(fd: c_int, data: *mut std::ffi::c_void) {
    with_session(fd, (), |session| {
        session.session_data = if data.is_null() { None } else { Some(data) };
    });
}

/// Get session eof flag.
#[no_mangle]
pub extern "C" fn rust_session_get_eof(fd: c_int) -> c_int {
    with_session(fd, -1, |session| session.eof)
}

/// Set session eof flag.
#[no_mangle]
pub extern "C" fn rust_session_set_eof(fd: c_int, eof: c_int) {
    with_session(fd, (), |session| {
        session.eof = eof;
    });
}

/// Get client IP address as u32 (network byte order, matches sin_addr.s_addr).
#[no_mangle]
pub extern "C" fn rust_session_get_client_ip(fd: c_int) -> u32 {
    with_session(fd, 0, |session| session.client_addr_raw)
}

/// Get session increment value (packet sequence counter).
#[no_mangle]
pub extern "C" fn rust_session_get_increment(fd: c_int) -> u8 {
    with_session(fd, 0, |session| session.increment)
}

/// Increment packet counter and return new value.
#[no_mangle]
pub extern "C" fn rust_session_increment(fd: c_int) -> u8 {
    with_session(fd, 0, |session| {
        session.increment = session.increment.wrapping_add(1);
        session.increment
    })
}

/// Check if session exists (returns 1 if exists, 0 if not).
#[no_mangle]
pub extern "C" fn rust_session_exists(fd: c_int) -> c_int {
    let manager = crate::session::get_session_manager();
    if manager.get_session(fd).is_some() { 1 } else { 0 }
}

/// Override the parse callback for a specific session.
///
/// # Safety
/// Callback must be a valid C function pointer.
#[no_mangle]
pub unsafe extern "C" fn rust_session_set_parse(
    fd: c_int,
    callback: unsafe extern "C" fn(c_int) -> c_int,
) {
    with_session(fd, (), |session| {
        session.callbacks.parse = Some(callback);
    });
}

/// Override the shutdown callback for a specific session.
///
/// # Safety
/// Callback must be a valid C function pointer.
#[no_mangle]
pub unsafe extern "C" fn rust_session_set_shutdown(
    fd: c_int,
    callback: unsafe extern "C" fn(c_int) -> c_int,
) {
    with_session(fd, (), |session| {
        session.callbacks.shutdown = Some(callback);
    });
}

/// Call the parse callback for a session.
#[no_mangle]
pub extern "C" fn rust_session_call_parse(fd: c_int) {
    let parse_cb = with_session(fd, None, |session| session.callbacks.parse);
    if let Some(cb) = parse_cb {
        unsafe { cb(fd); }
    }
}

/// Log a message from C code through Rust's tracing system.
/// level: 0=error, 1=warn, 2=info, 3=debug
///
/// # Safety
/// msg must be a valid null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn rust_log_c(level: c_int, msg: *const std::os::raw::c_char) {
    if msg.is_null() {
        return;
    }
    let s = std::ffi::CStr::from_ptr(msg).to_string_lossy();
    match level {
        0 => tracing::error!("{}", s),
        1 => tracing::warn!("{}", s),
        3 => tracing::debug!("{}", s),
        _ => tracing::info!("{}", s),
    }
}

/// Get a snapshot of all active session fds (for iteration in C).
/// Writes fds into caller-provided buffer, returns count written.
#[no_mangle]
pub extern "C" fn rust_session_get_all_fds(buf: *mut c_int, buf_len: c_int) -> c_int {
    if buf.is_null() || buf_len <= 0 {
        return 0;
    }

    let fds = crate::session::get_session_manager().get_all_fds();
    let count = (fds.len() as i32).min(buf_len);
    for (i, &fd) in fds.iter().take(count as usize).enumerate() {
        unsafe { *buf.add(i) = fd; }
    }
    count
}

/// Mark an IP as DDoS-locked.
///
/// `ip` is in network byte order (sin_addr.s_addr), as returned by
/// `rust_session_get_client_ip`.
#[no_mangle]
pub extern "C" fn rust_add_ip_lockout(ip: u32) {
    crate::network::ddos::add_ip_lockout(ip);
}

/// Timer callback: prune stale DDoS history entries.
///
/// Registered with timer_insert at server startup (interval 1 s).
/// Signature matches C's `int (*func)(int, int)`.
#[no_mangle]
pub extern "C" fn rust_connect_check_clear(_id: c_int, _data: c_int) -> c_int {
    crate::network::ddos::connect_check_clear()
}

/// Record a throttled connection attempt from an IP.
///
/// `ip` is in network byte order (sin_addr.s_addr), as returned by
/// `rust_session_get_client_ip`.
#[no_mangle]
pub extern "C" fn rust_add_throttle(ip: u32) {
    crate::network::throttle::add_throttle(ip);
}

/// Timer callback: reset all throttle counts.
///
/// Registered with timer_insert at server startup (interval 10 min).
/// Signature matches C's `int (*func)(int, int)`.
#[no_mangle]
pub extern "C" fn rust_remove_throttle(_id: c_int, _data: c_int) -> c_int {
    crate::network::throttle::remove_throttle();
    0
}
