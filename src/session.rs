//! Session management with async I/O
//!
//! This module replaces session.c with memory-safe async Rust implementation.

use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::os::unix::io::AsRawFd;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Mutex as StdMutex, RwLock};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;

/// Future returned by an async session callback.
/// Not Send — runs in-place inside the LocalSet session task.
pub type CallbackFuture = Pin<Box<dyn Future<Output = i32> + 'static>>;

/// An async session callback stored in SessionCallbacks.
/// The Fn itself is Send+Sync (no captured raw pointers); the Future it
/// returns is allowed to capture raw pointers because it runs in LocalSet.
pub type AsyncCallback = Arc<dyn Fn(i32) -> CallbackFuture + Send + Sync + 'static>;

/// Wrap a synchronous unsafe fn pointer as an AsyncCallback.
/// Use this for simple callbacks that don't need .await.
pub fn sync_callback(f: unsafe fn(i32) -> i32) -> AsyncCallback {
    std::sync::Arc::new(move |fd: i32| -> CallbackFuture {
        Box::pin(async move { unsafe { f(fd) } })
    })
}

/// Buffer size constants
pub const RFIFO_SIZE: usize = 16 * 1024;
pub const WFIFO_SIZE: usize = 16 * 1024;

/// Maximum read buffer size.
///
/// Inter-server connections (e.g. map→char) burst large payloads on connect
/// (map list, etc.) that can exceed RFIFO_SIZE.  Dropping bytes in a stream
/// protocol corrupts all subsequent packet framing, so we grow up to this
/// limit instead.  Connections that exceed it are closed, not silently truncated.
const MAX_RDATA_SIZE: usize = 64 * 1024;

/// Maximum number of sessions
pub const MAX_SESSIONS: usize = 1024;

/// Maximum write buffer size (4MB).
///
/// Must accommodate inter-server packets that compress struct mmo_charstatus
/// (~3MB uncompressed) via compressBound: the WFIFOHEAD call reserves the
/// worst-case compressed size before compress2 runs, which is ~3.17MB.
/// The old C session.c used dynamic realloc with no hard cap; 4MB matches
/// the original behaviour while providing a reasonable upper bound.
const MAX_WDATA_SIZE: usize = 4 * 1024 * 1024;

/// Error types for session operations
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Read out of bounds: fd={fd}, pos={pos}, size={size}")]
    ReadOutOfBounds { fd: i32, pos: usize, size: usize },

    #[error("Skip out of bounds: fd={fd}, skip={skip_len}, available={available}")]
    SkipOutOfBounds {
        fd: i32,
        skip_len: usize,
        available: usize,
    },

    #[error("Write commit too large: fd={fd}, requested={requested}, available={available}")]
    WriteCommitTooLarge {
        fd: i32,
        requested: usize,
        available: usize,
    },

    #[error("Write position overflow: fd={fd}, wdata_size={wdata_size}, pos={pos}")]
    WritePositionOverflow {
        fd: i32,
        wdata_size: usize,
        pos: usize,
    },

    #[error("Write buffer too large: fd={fd}, requested_pos={requested_pos}, max={max}")]
    WriteBufferTooLarge { fd: i32, requested_pos: usize, max: usize },

    #[error("Session not found: fd={0}")]
    SessionNotFound(i32),

    #[error("Maximum sessions exceeded (limit: {MAX_SESSIONS})")]
    MaxSessionsExceeded,

    #[error("File descriptor overflow")]
    FdOverflow,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Async callback function pointers for session lifecycle events
#[derive(Clone, Default)]
pub struct SessionCallbacks {
    /// Called once when a new connection is accepted (before any data is read)
    pub accept: Option<AsyncCallback>,
    /// Called when packet data is received and ready to parse
    pub parse: Option<AsyncCallback>,
    /// Called when session has been idle for too long
    pub timeout: Option<AsyncCallback>,
    /// Called when session is being shut down
    pub shutdown: Option<AsyncCallback>,
}

/// Global session manager (thread-safe, sync-accessible from C callbacks)
pub struct SessionManager {
    /// Active sessions: std::sync::RwLock so FFI can access without block_on
    sessions: RwLock<HashMap<i32, Arc<Mutex<Session>>>>,
    /// Next fd counter: atomic so FFI can allocate without block_on
    next_fd: AtomicI32,
    /// Default callbacks for new sessions: std::sync::Mutex
    pub default_callbacks: StdMutex<SessionCallbacks>,
    /// Pending listening sockets (std::net, converted to tokio at server start)
    pub listeners: StdMutex<HashMap<i32, std::net::TcpListener>>,
    /// Ordered list of listener fds
    pub listen_fds: StdMutex<Vec<i32>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            next_fd: AtomicI32::new(1), // 0 reserved
            default_callbacks: StdMutex::new(SessionCallbacks::default()),
            listeners: StdMutex::new(HashMap::new()),
            listen_fds: StdMutex::new(Vec::new()),
        }
    }

    /// Allocate a new file descriptor (sync)
    pub fn allocate_fd(&self) -> Result<i32, SessionError> {
        let fd = self.next_fd.fetch_add(1, Ordering::Relaxed);
        if fd > MAX_SESSIONS as i32 {
            return Err(SessionError::MaxSessionsExceeded);
        }
        Ok(fd)
    }

    /// Insert a session (sync)
    pub fn insert_session(&self, fd: i32, session: Arc<Mutex<Session>>) -> Result<(), SessionError> {
        let mut sessions = self.sessions.write().unwrap();
        if sessions.len() >= MAX_SESSIONS {
            return Err(SessionError::MaxSessionsExceeded);
        }
        sessions.insert(fd, session);
        Ok(())
    }

    /// Get a session by fd (sync)
    pub fn get_session(&self, fd: i32) -> Option<Arc<Mutex<Session>>> {
        self.sessions.read().unwrap().get(&fd).cloned()
    }

    /// Remove a session (sync)
    pub fn remove_session(&self, fd: i32) {
        self.sessions.write().unwrap().remove(&fd);
    }

    /// Get default callbacks (sync)
    pub fn get_default_callbacks(&self) -> SessionCallbacks {
        self.default_callbacks.lock().unwrap().clone()
    }

    /// Set default callbacks (sync)
    pub fn set_default_callbacks(&self, callbacks: SessionCallbacks) {
        *self.default_callbacks.lock().unwrap() = callbacks;
    }

    /// Get session count (sync)
    pub fn session_count(&self) -> usize {
        self.sessions.read().unwrap().len()
    }

    /// Get snapshot of all active session fds (sync)
    pub fn get_all_fds(&self) -> Vec<i32> {
        self.sessions.read().unwrap().keys().copied().collect()
    }

    /// Register a listener socket (sync, called before server starts)
    pub fn add_listener(&self, fd: i32, listener: std::net::TcpListener) {
        self.listeners.lock().unwrap().insert(fd, listener);
        self.listen_fds.lock().unwrap().push(fd);
    }

    /// Take ownership of a listener (sync, called by accept loop at startup)
    pub fn take_listener(&self, fd: i32) -> Option<std::net::TcpListener> {
        self.listeners.lock().unwrap().remove(&fd)
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Global session manager instance
pub static SESSION_MANAGER: OnceLock<SessionManager> = OnceLock::new();

/// Get the global session manager
pub fn get_session_manager() -> &'static SessionManager {
    SESSION_MANAGER.get_or_init(SessionManager::new)
}

/// Outgoing connections created from game callbacks, pending session_io_task spawn.
/// Game callbacks run synchronously inside the Tokio select! arm, so they cannot
/// use block_on or spawn_local directly. Instead they push fds here and
/// run_async_server drains this queue after each cron tick.
pub static PENDING_CONNECTIONS: OnceLock<StdMutex<Vec<i32>>> = OnceLock::new();

pub fn push_pending_connection(fd: i32) {
    PENDING_CONNECTIONS
        .get_or_init(|| StdMutex::new(Vec::new()))
        .lock()
        .unwrap()
        .push(fd);
}

fn drain_pending_connections() -> Vec<i32> {
    PENDING_CONNECTIONS
        .get()
        .map(|m| std::mem::take(&mut *m.lock().unwrap()))
        .unwrap_or_default()
}

/// Set up a new session from an established TCP connection (sync).
pub fn setup_connection(
    stream: TcpStream,
    addr: SocketAddr,
    manager: &SessionManager,
) -> Result<i32, SessionError> {
    let fd = manager.allocate_fd()?;

    let mut session = Session::new(fd);
    session.client_addr = Some(addr);
    session.client_addr_raw = match addr.ip() {
        std::net::IpAddr::V4(ipv4) => u32::from(ipv4).to_be(),
        _ => 0,
    };
    session.socket = Some(Arc::new(Mutex::new(stream)));
    session.callbacks = manager.get_default_callbacks();

    let session_arc = Arc::new(Mutex::new(session));
    manager.insert_session(fd, session_arc)?;

    tracing::info!("[session] New connection: fd={}, addr={}", fd, addr);
    #[cfg(not(test))]
    crate::session::update_fd_max_pub(fd);
    Ok(fd)
}

/// Global Tokio runtime
pub static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Initialize the Tokio runtime (single-threaded for C callback safety)
pub fn init_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime")
    })
}

/// Session state for a single client connection
pub struct Session {
    /// File descriptor (for C compatibility)
    pub fd: i32,

    /// TCP socket (Tokio async)
    pub socket: Option<Arc<Mutex<TcpStream>>>,

    /// Client address
    pub client_addr: Option<SocketAddr>,

    /// Client address as raw u32 (for C compatibility with sin_addr.s_addr)
    pub client_addr_raw: u32,

    /// Pending outgoing connection address.
    /// Set by rust_make_connection when called from inside the runtime.
    /// session_io_task performs the actual async connect before starting I/O.
    pub connect_addr: Option<SocketAddr>,

    /// Read buffer (FIFO)
    pub rdata: Vec<u8>,
    pub rdata_pos: usize,
    pub rdata_size: usize,

    /// Write buffer (FIFO)
    pub wdata: Vec<u8>,
    pub wdata_size: usize,

    /// Connection state (0=ok, 1=eof, 2=write error, 3=read error, etc.)
    pub eof: i32,

    /// Packet increment counter
    pub increment: u8,

    /// Last activity timestamp
    pub last_activity: Instant,

    /// Session-specific data (opaque pointer for C)
    ///
    /// This is a raw pointer to C-managed memory. The C code is responsible for:
    /// - Allocating the data when needed
    /// - Ensuring proper lifetime (must outlive the session)
    /// - Deallocating when the session is destroyed
    ///
    /// Rust code treats this as completely opaque and never dereferences it.
    pub session_data: Option<*mut std::ffi::c_void>,

    /// Callbacks
    pub callbacks: SessionCallbacks,

    /// Guards against double-invocation of the shutdown callback.
    /// Set to true the first time shutdown is called; subsequent callers skip it.
    shutdown_called: bool,

    /// Notified when C code writes data to this session's write buffer.
    /// session_io_task selects on this to flush pending writes immediately
    /// instead of waiting for the next read event.
    pub write_notify: Arc<tokio::sync::Notify>,

    /// When true, commit_write() skips notify_one(). Used by spawn_blocking
    /// callers (intif_mmo_tosd) that write many packets in sequence and need
    /// to prevent interleaved flushes from the async session_io_task.
    /// The caller is responsible for calling write_notify.notify_one() once
    /// after all writes are complete.
    pub suppress_notify: bool,
}

impl Session {
    /// Create a new session with the given file descriptor
    pub fn new(fd: i32) -> Self {
        Self {
            fd,
            socket: None,
            client_addr: None,
            client_addr_raw: 0,
            connect_addr: None,
            write_notify: Arc::new(tokio::sync::Notify::new()),
            rdata: Vec::with_capacity(RFIFO_SIZE),
            rdata_pos: 0,
            rdata_size: 0,
            wdata: Vec::with_capacity(WFIFO_SIZE),
            wdata_size: 0,
            eof: 0,
            increment: 0,
            last_activity: Instant::now(),
            session_data: None,
            callbacks: SessionCallbacks::default(),
            shutdown_called: false,
            suppress_notify: false,
        }
    }

    /// Read u8 with bounds checking
    pub fn read_u8(&self, pos: usize) -> Result<u8, SessionError> {
        let actual_pos = self.rdata_pos.checked_add(pos).ok_or(SessionError::ReadOutOfBounds {
            fd: self.fd,
            pos: usize::MAX,
            size: self.rdata_size,
        })?;

        if actual_pos >= self.rdata_size {
            return Err(SessionError::ReadOutOfBounds {
                fd: self.fd,
                pos: actual_pos,
                size: self.rdata_size,
            });
        }

        Ok(self.rdata[actual_pos])
    }

    /// Read u16 (little-endian) with bounds checking
    pub fn read_u16(&self, pos: usize) -> Result<u16, SessionError> {
        let actual_pos = self.rdata_pos.checked_add(pos).ok_or(SessionError::ReadOutOfBounds {
            fd: self.fd,
            pos: usize::MAX,
            size: self.rdata_size,
        })?;
        let end = actual_pos.checked_add(2).ok_or(SessionError::ReadOutOfBounds {
            fd: self.fd,
            pos: actual_pos,
            size: self.rdata_size,
        })?;

        if end > self.rdata_size {
            return Err(SessionError::ReadOutOfBounds {
                fd: self.fd,
                pos: actual_pos,
                size: self.rdata_size,
            });
        }

        Ok(u16::from_le_bytes([self.rdata[actual_pos], self.rdata[actual_pos + 1]]))
    }

    /// Read u32 (little-endian) with bounds checking
    pub fn read_u32(&self, pos: usize) -> Result<u32, SessionError> {
        let actual_pos = self.rdata_pos.checked_add(pos).ok_or(SessionError::ReadOutOfBounds {
            fd: self.fd,
            pos: usize::MAX,
            size: self.rdata_size,
        })?;
        let end = actual_pos.checked_add(4).ok_or(SessionError::ReadOutOfBounds {
            fd: self.fd,
            pos: actual_pos,
            size: self.rdata_size,
        })?;

        if end > self.rdata_size {
            return Err(SessionError::ReadOutOfBounds {
                fd: self.fd,
                pos: actual_pos,
                size: self.rdata_size,
            });
        }

        Ok(u32::from_le_bytes([
            self.rdata[actual_pos],
            self.rdata[actual_pos + 1],
            self.rdata[actual_pos + 2],
            self.rdata[actual_pos + 3],
        ]))
    }

    /// Get available bytes to read (like RFIFOREST)
    pub fn available(&self) -> usize {
        self.rdata_size - self.rdata_pos
    }

    /// Write u8 with automatic buffer growth
    pub fn write_u8(&mut self, pos: usize, val: u8) -> Result<(), SessionError> {
        let actual_pos = self
            .wdata_size
            .checked_add(pos)
            .ok_or(SessionError::WritePositionOverflow {
                fd: self.fd,
                wdata_size: self.wdata_size,
                pos,
            })?;

        let end = actual_pos + 1;
        if end > MAX_WDATA_SIZE {
            return Err(SessionError::WriteBufferTooLarge {
                fd: self.fd,
                requested_pos: end,
                max: MAX_WDATA_SIZE,
            });
        }

        // Auto-grow in 1KB chunks, clamped to MAX_WDATA_SIZE
        if end > self.wdata.len() {
            self.wdata.resize(end.saturating_add(1024).min(MAX_WDATA_SIZE), 0);
        }

        self.wdata[actual_pos] = val;
        Ok(())
    }

    /// Write u16 (little-endian) with automatic buffer growth
    pub fn write_u16(&mut self, pos: usize, val: u16) -> Result<(), SessionError> {
        let actual_pos = self
            .wdata_size
            .checked_add(pos)
            .ok_or(SessionError::WritePositionOverflow {
                fd: self.fd,
                wdata_size: self.wdata_size,
                pos,
            })?;

        let end = actual_pos + 2;
        if end > MAX_WDATA_SIZE {
            return Err(SessionError::WriteBufferTooLarge {
                fd: self.fd,
                requested_pos: end,
                max: MAX_WDATA_SIZE,
            });
        }

        if end > self.wdata.len() {
            self.wdata.resize(end.saturating_add(1024).min(MAX_WDATA_SIZE), 0);
        }

        let bytes = val.to_le_bytes();
        self.wdata[actual_pos..actual_pos + 2].copy_from_slice(&bytes);

        Ok(())
    }

    /// Write u32 (little-endian) with automatic buffer growth
    pub fn write_u32(&mut self, pos: usize, val: u32) -> Result<(), SessionError> {
        let actual_pos = self
            .wdata_size
            .checked_add(pos)
            .ok_or(SessionError::WritePositionOverflow {
                fd: self.fd,
                wdata_size: self.wdata_size,
                pos,
            })?;

        let end = actual_pos + 4;
        if end > MAX_WDATA_SIZE {
            return Err(SessionError::WriteBufferTooLarge {
                fd: self.fd,
                requested_pos: end,
                max: MAX_WDATA_SIZE,
            });
        }

        if end > self.wdata.len() {
            self.wdata.resize(end.saturating_add(1024).min(MAX_WDATA_SIZE), 0);
        }

        let bytes = val.to_le_bytes();
        self.wdata[actual_pos..actual_pos + 4].copy_from_slice(&bytes);

        Ok(())
    }

    /// Commit write buffer (like WFIFOSET)
    pub fn commit_write(&mut self, len: usize) -> Result<(), SessionError> {
        let new_size = self.wdata_size.checked_add(len).ok_or(
            SessionError::WriteBufferTooLarge {
                fd: self.fd,
                requested_pos: usize::MAX,
                max: MAX_WDATA_SIZE,
            },
        )?;

        if new_size > MAX_WDATA_SIZE {
            return Err(SessionError::WriteBufferTooLarge {
                fd: self.fd,
                requested_pos: new_size,
                max: MAX_WDATA_SIZE,
            });
        }

        let available = self.wdata.len().checked_sub(self.wdata_size).unwrap_or(0);
        if new_size > self.wdata.len() {
            return Err(SessionError::WriteCommitTooLarge {
                fd: self.fd,
                requested: len,
                available,
            });
        }

        self.wdata_size = new_size;
        // Wake session_io_task so it flushes immediately rather than waiting for
        // the next read event. This is critical when a C parse callback writes
        // to a *different* session's buffer (e.g. login server writing to char_fd
        // while handling a client packet).
        // Skip notification when suppress_notify is set — the caller will
        // batch-notify after all writes are done.
        if !self.suppress_notify {
            self.write_notify.notify_one();
        }
        Ok(())
    }

    /// Skip N bytes in read buffer (like RFIFOSKIP)
    pub fn skip(&mut self, len: usize) -> Result<(), SessionError> {
        let new_pos = self.rdata_pos.saturating_add(len);

        if new_pos > self.rdata_size {
            return Err(SessionError::SkipOutOfBounds {
                fd: self.fd,
                skip_len: len,
                available: self.rdata_size - self.rdata_pos,
            });
        }

        self.rdata_pos = new_pos;

        // Auto-compact when fully read
        if self.rdata_pos == self.rdata_size {
            self.rdata_pos = 0;
            self.rdata_size = 0;
            self.rdata.clear();
        }

        Ok(())
    }

    /// Get a raw pointer to the read buffer at the given offset (like RFIFOP)
    ///
    /// # Safety
    /// The returned pointer is only valid while the Session lock is held.
    /// The caller must not read past `available()` bytes from this pointer.
    pub fn rdata_ptr(&self, pos: usize) -> Result<*const u8, SessionError> {
        let actual_pos = self.rdata_pos.checked_add(pos).ok_or(SessionError::ReadOutOfBounds {
            fd: self.fd,
            pos: usize::MAX,
            size: self.rdata_size,
        })?;

        if actual_pos >= self.rdata_size {
            return Err(SessionError::ReadOutOfBounds {
                fd: self.fd,
                pos: actual_pos,
                size: self.rdata_size,
            });
        }

        Ok(self.rdata.as_ptr().wrapping_add(actual_pos))
    }

    /// Get a mutable raw pointer to the write buffer at the given offset (like WFIFOP)
    ///
    /// # Safety
    /// The returned pointer is only valid while the Session lock is held.
    /// The caller must call `commit_write()` after writing to commit the data.
    pub fn wdata_ptr(&mut self, pos: usize) -> Result<*mut u8, SessionError> {
        let actual_pos = self
            .wdata_size
            .checked_add(pos)
            .ok_or(SessionError::WritePositionOverflow {
                fd: self.fd,
                wdata_size: self.wdata_size,
                pos,
            })?;

        let end = actual_pos + 1;
        if end > MAX_WDATA_SIZE {
            return Err(SessionError::WriteBufferTooLarge {
                fd: self.fd,
                requested_pos: end,
                max: MAX_WDATA_SIZE,
            });
        }

        // Ensure buffer is large enough, clamped to MAX_WDATA_SIZE
        if end > self.wdata.len() {
            self.wdata.resize(end.saturating_add(1024).min(MAX_WDATA_SIZE), 0);
        }

        Ok(self.wdata.as_mut_ptr().wrapping_add(actual_pos))
    }

    /// Ensure write buffer has room for `size` bytes (like WFIFOHEAD)
    pub fn ensure_wdata_capacity(&mut self, size: usize) -> Result<(), SessionError> {
        let needed = self
            .wdata_size
            .checked_add(size)
            .ok_or(SessionError::WritePositionOverflow {
                fd: self.fd,
                wdata_size: self.wdata_size,
                pos: size,
            })?;

        if needed > MAX_WDATA_SIZE {
            return Err(SessionError::WriteBufferTooLarge {
                fd: self.fd,
                requested_pos: needed,
                max: MAX_WDATA_SIZE,
            });
        }

        if needed > self.wdata.len() {
            self.wdata.resize(needed.saturating_add(1024).min(MAX_WDATA_SIZE), 0);
        }

        Ok(())
    }

    /// Copy data from read buffer into a destination buffer (safe RFIFOP + memcpy)
    pub fn read_buf(&self, pos: usize, dst: &mut [u8]) -> Result<(), SessionError> {
        let actual_pos = self.rdata_pos.checked_add(pos).ok_or(SessionError::ReadOutOfBounds {
            fd: self.fd,
            pos: usize::MAX,
            size: self.rdata_size,
        })?;
        let end = actual_pos.checked_add(dst.len()).ok_or(SessionError::ReadOutOfBounds {
            fd: self.fd,
            pos: actual_pos,
            size: self.rdata_size,
        })?;

        if end > self.rdata_size {
            return Err(SessionError::ReadOutOfBounds {
                fd: self.fd,
                pos: actual_pos,
                size: self.rdata_size,
            });
        }

        dst.copy_from_slice(&self.rdata[actual_pos..end]);
        Ok(())
    }

    /// Copy data into the write buffer (safe WFIFOP + memcpy)
    pub fn write_buf(&mut self, pos: usize, src: &[u8]) -> Result<(), SessionError> {
        let actual_pos = self
            .wdata_size
            .checked_add(pos)
            .ok_or(SessionError::WritePositionOverflow {
                fd: self.fd,
                wdata_size: self.wdata_size,
                pos,
            })?;

        let end = actual_pos + src.len();

        if end > MAX_WDATA_SIZE {
            return Err(SessionError::WriteBufferTooLarge {
                fd: self.fd,
                requested_pos: end,
                max: MAX_WDATA_SIZE,
            });
        }

        if end > self.wdata.len() {
            self.wdata.resize(end.saturating_add(1024).min(MAX_WDATA_SIZE), 0);
        }

        self.wdata[actual_pos..end].copy_from_slice(src);
        Ok(())
    }

    /// Compacts the read buffer by moving unread data to the beginning.
    pub fn flush_read_buffer(&mut self) {
        if self.rdata_pos == self.rdata_size {
            self.rdata_pos = 0;
            self.rdata_size = 0;
            self.rdata.clear();
        } else if self.rdata_pos > 0 {
            self.rdata.copy_within(self.rdata_pos..self.rdata_size, 0);
            self.rdata_size -= self.rdata_pos;
            self.rdata_pos = 0;
            self.rdata.truncate(self.rdata_size);
        }
    }
}

// SAFETY: session_data is an opaque pointer to C-managed memory. It is only accessed
// by C callbacks which provide their own synchronization. The pointer itself is Send/Sync
// safe because it's just an address - the actual data synchronization is handled externally
// by C code. All other fields (Vec, Option, primitives) are already Send/Sync.
unsafe impl Send for Session {}
unsafe impl Sync for Session {}

/// Run the async game server.
///
/// Replaces the C main loop in core.c:
/// - Spawns accept tasks for all registered listeners
/// - Drives mob/npc/cron/ddos/throttle ticks via Tokio intervals
/// - Session I/O is handled by per-connection tasks (session_io_task)
/// - Drains PENDING_CONNECTIONS after each cron tick (for connections
///   made from game callbacks via rust_make_connection)
pub async fn run_async_server(_port: u16) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("[rust_server] Starting event loop");

    let manager = get_session_manager();

    // Take all registered std::net listeners, convert to tokio, spawn accept tasks
    let listen_fds = manager.listen_fds.lock().unwrap().clone();

    for fd in listen_fds {
        if let Some(std_listener) = manager.take_listener(fd) {
            std_listener.set_nonblocking(true)?;
            let listener = tokio::net::TcpListener::from_std(std_listener)?;
            tracing::info!("[rust_server] Spawning accept loop for listener fd={}", fd);
            tokio::task::spawn_local(accept_loop(listener, fd));
        }
    }

    let mut mob_tick      = tokio::time::interval(Duration::from_millis(50));
    let mut npc_tick      = tokio::time::interval(Duration::from_millis(100));
    let mut cron_tick     = tokio::time::interval(Duration::from_secs(1));
    cron_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut ddos_tick     = tokio::time::interval(Duration::from_secs(1));
    let mut throttle_tick = tokio::time::interval(Duration::from_secs(600));

    loop {
        tokio::select! {
            _ = mob_tick.tick() => {
                #[cfg(not(test))]
                unsafe { crate::game::mob::rust_mob_timer_spawns(); }
            }
            _ = npc_tick.tick() => {
                #[cfg(not(test))]
                unsafe { crate::game::npc::npc_runtimers(); }
            }
            _ = cron_tick.tick() => {
                #[cfg(not(test))]
                unsafe { crate::game::map_server::rust_map_cronjob(); }

                // Spawn I/O tasks for connections made from callbacks.
                for fd in drain_pending_connections() {
                    tracing::debug!("[rust_server] Spawning io task for pending fd={}", fd);
                    tokio::task::spawn_local(session_io_task(fd));
                }

                // Check shutdown signal
                #[cfg(not(test))]
                if crate::core::rust_should_shutdown() != 0 {
                    tracing::info!("[rust_server] Shutdown requested");
                    break;
                }
            }
            _ = ddos_tick.tick() => {
                #[cfg(not(test))]
                crate::session::rust_connect_check_clear();
            }
            _ = throttle_tick.tick() => {
                #[cfg(not(test))]
                crate::session::rust_remove_throttle();
            }
        }
    }

    #[allow(unreachable_code)]
    {
        shutdown_all_sessions().await;
    }

    Ok(())
}

/// Accept loop for a single listener socket
async fn accept_loop(listener: tokio::net::TcpListener, _listen_fd: i32) {
    let local_addr = listener.local_addr().map(|a| a.to_string()).unwrap_or_else(|_| "unknown".to_string());
    tracing::info!("[accept] Listening on fd={} addr={}", _listen_fd, local_addr);

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                // Reject DDoS-locked IPs before allocating any resources.
                let ip_net = match addr.ip() {
                    std::net::IpAddr::V4(ipv4) => u32::from(ipv4).to_be(),
                    _ => 0,
                };
                if crate::network::ddos::is_ip_locked(ip_net) {
                    tracing::warn!("[accept] DDoS-locked IP {}, refusing connection", addr);
                    continue;
                }
                if crate::network::throttle::is_throttled(ip_net) {
                    tracing::warn!("[accept] Throttled IP {}, refusing connection", addr);
                    continue;
                }
                apply_socket_opts(&stream);
                tracing::info!("[accept] New connection from {} on listener fd={}", addr, _listen_fd);
                tokio::task::spawn_local(session_io_task_from_accept(stream, addr));
            }
            Err(e) => {
                tracing::error!("[accept] fd={} accept error: {}", _listen_fd, e);
            }
        }
    }
}

/// Apply the same socket options as the old C `setsocketopts()`.
///
/// - `SO_REUSEADDR` / `SO_REUSEPORT` (unix): allows the port to be reused
///   after a quick server restart.
/// - `IPPROTO_TCP / 0`: matches what the C code did (TCP_NODELAY was
///   intentionally commented out; the `0` call was kept as-is).
/// - `SO_LINGER` with `l_onoff=0`: graceful close, no hard timeout.
fn apply_socket_opts(stream: &TcpStream) {
    let fd = stream.as_raw_fd();
    let yes: libc::c_int = 1;
    unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEADDR,
            &yes as *const _ as *const libc::c_void,
            std::mem::size_of_val(&yes) as libc::socklen_t,
        );
        #[cfg(target_os = "linux")]
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            &yes as *const _ as *const libc::c_void,
            std::mem::size_of_val(&yes) as libc::socklen_t,
        );
        // Matches C's setsockopt(fd, IPPROTO_TCP, 0, ...) (TCP_NODELAY was
        // commented out in the original; the zero option-name is kept verbatim).
        libc::setsockopt(
            fd,
            libc::IPPROTO_TCP,
            0,
            &yes as *const _ as *const libc::c_void,
            std::mem::size_of_val(&yes) as libc::socklen_t,
        );
        let linger = libc::linger {
            l_onoff: 0,
            l_linger: 0,
        };
        if libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_LINGER,
            &linger as *const _ as *const libc::c_void,
            std::mem::size_of_val(&linger) as libc::socklen_t,
        ) != 0
        {
            tracing::warn!("[accept] Unable to set SO_LINGER for fd={}", fd);
        }
    }
}

/// Set up session from an accepted connection and run its I/O task.
/// Calls the accept callback (e.g. clif_accept) before entering the I/O loop
/// so the server can send its initial handshake packet.
async fn session_io_task_from_accept(stream: TcpStream, addr: SocketAddr) {
    let manager = get_session_manager();
    let fd = match setup_connection(stream, addr, manager) {
        Ok(fd) => fd,
        Err(e) => {
            tracing::error!("[session] Failed to set up connection from {}: {}", addr, e);
            return;
        }
    };

    // Call the accept callback — servers use this to send the initial handshake.
    // The callback may write to the session's write buffer; we flush it below.
    let accept_cb = {
        match manager.get_session(fd) {
            Some(arc) => arc.try_lock().ok().and_then(|s| s.callbacks.accept.clone()),
            None => None,
        }
    };
    if let Some(cb) = accept_cb {
        cb(fd).await;

        // Flush whatever the accept callback wrote
        flush_wdata_to_socket(fd, manager).await;
    }

    session_io_task(fd).await;
}

/// Flush session write buffer to socket immediately (used after accept callback).
async fn flush_wdata_to_socket(fd: i32, manager: &SessionManager) {
    let session_arc = match manager.get_session(fd) {
        Some(a) => a,
        None => return,
    };

    let (socket_arc, wdata) = {
        let mut session = session_arc.lock().await;
        let socket_arc = match session.socket.as_ref() {
            Some(s) => s.clone(),
            None => return,
        };
        let wdata = if session.wdata_size > 0 {
            let prev_size = session.wdata_size;
            let data = session.wdata[..prev_size].to_vec();
            // Zero the flushed region before resetting the logical length.
            // This prevents stale payload bytes from appearing in the next
            // packet if C only partially overwrites the committed range.
            // Don't call wdata.clear() — keep the allocation intact so that
            // raw pointers returned by WFIFOP (rust_session_wdata_ptr) remain
            // valid even if a flush races with C code writing to the buffer.
            session.wdata[..prev_size].fill(0);
            session.wdata_size = 0;
            data
        } else {
            return;
        };
        (socket_arc, wdata)
    };

    let mut socket = socket_arc.lock().await;
    if let Err(e) = socket.write_all(&wdata).await {
        tracing::error!("[session] fd={} flush write error: {}", fd, e);
        if let Some(arc) = manager.get_session(fd) {
            arc.lock().await.eof = 2;
        }
    }
}

/// Per-session I/O task.
///
/// For outgoing connections (made via rust_make_connection from timer callbacks),
/// the TCP connect is deferred: session.connect_addr is set and socket is None.
/// This task performs the actual connect before entering the I/O loop.
async fn session_io_task(fd: i32) {
    let manager = get_session_manager();
    let session_arc = match manager.get_session(fd) {
        Some(s) => s,
        None => {
            tracing::error!("[session] fd={} not found in manager", fd);
            return;
        }
    };

    // Handle deferred outgoing connection (set by rust_make_connection)
    let connect_addr = {
        let session = session_arc.lock().await;
        if session.socket.is_none() { session.connect_addr } else { None }
    };

    if let Some(addr) = connect_addr {
        match TcpStream::connect(addr).await {
            Ok(stream) => {
                session_arc.lock().await.socket = Some(Arc::new(Mutex::new(stream)));
                tracing::info!("[session] fd={} connected to {}", fd, addr);
                // Flush any write data queued before the connection was established
                // (e.g. auth packet written by check_connect_login before connect completes)
                flush_wdata_to_socket(fd, manager).await;
            }
            Err(e) => {
                tracing::error!("[session] fd={} connect to {} failed: {}", fd, addr, e);
                let shutdown_cb = {
                    let mut session = session_arc.lock().await;
                    if session.shutdown_called {
                        None
                    } else {
                        session.shutdown_called = true;
                        session.callbacks.shutdown.clone()
                    }
                };
                if let Some(cb) = shutdown_cb {
                    cb(fd).await;
                }
                manager.remove_session(fd);
                return;
            }
        }
    }

    let mut read_buf = vec![0u8; 4096];

    // Get the write_notify Arc once (it never changes for the lifetime of the session)
    let write_notify = session_arc.lock().await.write_notify.clone();

    loop {
        // Check eof
        let eof = {
            let session = session_arc.lock().await;
            session.eof
        };
        if eof != 0 {
            tracing::info!("[session] fd={} server-initiated eof={}, invoking parse for cleanup", fd, eof);
            // Give C one final parse call so clif_handle_disconnect / clif_closeit
            // can run and free the player's session_data (sd).  This
            // what happens for peer-initiated closes (Ok(0) branch below).
            let parse_cb = {
                let session = session_arc.lock().await;
                session.callbacks.parse.clone()
            };
            if let Some(cb) = parse_cb {
                cb(fd).await;
            }
            break;
        }

        // Get socket reference
        let socket_arc = {
            let session = session_arc.lock().await;
            match session.socket.as_ref() {
                Some(s) => s.clone(),
                None => break,
            }
        };

        // Select on either incoming data OR a write_notify signal.
        // write_notify fires when C code commits data to this session's write
        // buffer from another session's parse callback (e.g. login server
        // writing to char_fd while handling a client packet).
        enum Event {
            Read(std::io::Result<usize>),
            WriteReady,
        }

        let event = {
            let mut socket = socket_arc.lock().await;
            tokio::select! {
                result = socket.read(&mut read_buf) => Event::Read(result),
                _ = write_notify.notified() => Event::WriteReady,
            }
        };

        match event {
            Event::WriteReady => {
                flush_wdata_to_socket(fd, manager).await;
            }
            Event::Read(Ok(0)) => {
                // Peer closed connection — set eof and give C one last parse call
                {
                    let mut session = session_arc.lock().await;
                    session.eof = 4;
                }
                let parse_cb = {
                    let session = session_arc.lock().await;
                    session.callbacks.parse.clone()
                };
                if let Some(cb) = parse_cb {
                    cb(fd).await;
                }
                break;
            }
            Event::Read(Ok(n)) => {
                // Append data and update activity timestamp.
                //
                // Dropping bytes in a stream protocol corrupts all subsequent
                // packet framing, so we grow up to MAX_RDATA_SIZE instead of
                // silently truncating.  If that limit is exceeded we close the
                // connection rather than corrupt it.
                let overflow = {
                    let mut session = session_arc.lock().await;
                    let new_size = session.rdata_size + n;
                    if new_size > MAX_RDATA_SIZE {
                        tracing::warn!(
                            "[session] fd={} rdata overflow ({} bytes), closing connection",
                            fd, new_size
                        );
                        session.eof = 3;
                        true
                    } else {
                        session.rdata.extend_from_slice(&read_buf[..n]);
                        session.rdata_size += n;
                        session.last_activity = Instant::now();
                        false
                    }
                };
                if overflow {
                    break;
                }

                // Call C parse callback in a loop until all packets are consumed.
                // The C parser processes ONE packet per call (RFIFOSKIP at the end).
                // Multiple packets may arrive in a single read(), so we loop.
                // Break if: no bytes available, parser needs more data (ret==2),
                // or no progress was made (avoids infinite loop on unknown packets).
                let parse_cb = {
                    let session = session_arc.lock().await;
                    session.callbacks.parse.clone()
                };
                if let Some(cb) = parse_cb {
                    loop {
                        let available = {
                            let session = session_arc.lock().await;
                            session.available()
                        };
                        if available == 0 { break; }

                        let ret = cb(fd).await;
                        if ret == 2 { break; }

                        let (new_available, eof) = {
                            let session = session_arc.lock().await;
                            (session.available(), session.eof)
                        };
                        if eof != 0 || new_available >= available { break; }
                    }
                }

                // Flush this session's write buffer (may have been written by parse cb)
                flush_wdata_to_socket(fd, manager).await;

                // Compact read buffer
                {
                    let mut session = session_arc.lock().await;
                    session.flush_read_buffer();
                }
            }
            Event::Read(Err(e)) => {
                tracing::error!("[session] fd={} read error: {}", fd, e);
                let mut session = session_arc.lock().await;
                session.eof = 3;
                break;
            }
        }
    }

    // Invoke C shutdown callback then remove session.
    // The flag prevents a double-call if shutdown_all_sessions races here.
    let shutdown_cb = {
        let mut session = session_arc.lock().await;
        if session.shutdown_called {
            None
        } else {
            session.shutdown_called = true;
            session.callbacks.shutdown.clone()
        }
    };
    if let Some(cb) = shutdown_cb {
        cb(fd).await;
    }
    manager.remove_session(fd);
    tracing::info!("[session] fd={} closed", fd);
}

/// Shutdown all active sessions (called on server exit)
async fn shutdown_all_sessions() {
    tracing::info!("[rust_server] Shutting down all sessions");

    let manager = get_session_manager();
    let fds = manager.get_all_fds();

    for fd in fds {
        if let Some(session_arc) = manager.get_session(fd) {
            let shutdown_cb = {
                let mut session = session_arc.lock().await;
                if session.shutdown_called {
                    None
                } else {
                    session.shutdown_called = true;
                    session.callbacks.shutdown.clone()
                }
            };
            if let Some(cb) = shutdown_cb {
                tracing::debug!("[rust_server] Calling shutdown callback for fd={}", fd);
                cb(fd).await;
            }
            manager.remove_session(fd);
        }
    }
}

// ─── Public API exports ────────────────────────────────────────────────────


/// The maximum open fd seen so far. Updated whenever a new session is accepted.
/// Previously split across bin/map_server.rs + a C callback; now owned here.
static FD_MAX: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// Returns the current fd_max (highest fd + 1 seen so far).
pub fn get_fd_max() -> i32 {
    FD_MAX.load(std::sync::atomic::Ordering::Relaxed)
}

/// No-op: fd_max is now managed internally. Kept for call-site compatibility.
///
/// # Safety
/// The caller must pass a valid function pointer (or null). This function ignores it.
pub unsafe fn rust_register_fd_max_updater(_cb: unsafe fn(i32)) {}

/// Update fd_max if `fd` is larger.
pub fn update_fd_max_pub(fd: i32) {
    let next = fd + 1;
    FD_MAX.fetch_max(next, std::sync::atomic::Ordering::Relaxed);
}

fn update_fd_max(fd: i32) {
    update_fd_max_pub(fd);
}

/// Helper: access a session synchronously.
fn with_session<F, R>(fd: i32, default: R, f: F) -> R
where
    F: FnOnce(&mut Session) -> R,
{
    let manager = get_session_manager();
    if let Some(session_arc) = manager.get_session(fd) {
        match session_arc.try_lock() {
            Ok(mut guard) => f(&mut guard),
            Err(_) => {
                let mut guard = session_arc.blocking_lock();
                f(&mut guard)
            }
        }
    } else {
        default
    }
}

/// Initialize and run the async game server.
///
/// # Safety
/// Must be called from a non-async context. Initialises the Tokio runtime internally.
pub unsafe fn rust_server_run(port: u16) -> i32 {
    let _ = tracing_subscriber::fmt()
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    tracing::info!("[FFI] rust_server_run(port={})", port);

    let runtime = init_runtime();
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

pub fn rust_make_listen_port(port: i32) -> i32 {
    tracing::info!("[FFI] rust_make_listen_port(port={})", port);

    let addr = format!("0.0.0.0:{}", port);
    let std_listener = match std::net::TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("[FFI] Failed to bind port {}: {}", port, e);
            return -1;
        }
    };

    let manager = get_session_manager();
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

pub fn rust_make_connection(ip: u32, port: i32) -> i32 {
    let ipv4 = std::net::Ipv4Addr::from(u32::from_be(ip));
    let addr = std::net::SocketAddr::new(std::net::IpAddr::V4(ipv4), port as u16);

    tracing::info!("[FFI] rust_make_connection queuing outgoing connection to {}", addr);

    let manager = get_session_manager();

    let fd = match manager.allocate_fd() {
        Ok(fd) => fd,
        Err(e) => {
            tracing::error!("[FFI] allocate_fd failed: {}", e);
            return -1;
        }
    };

    let mut session = Session::new(fd);
    session.client_addr = Some(addr);
    session.client_addr_raw = ip;
    session.connect_addr = Some(addr);
    session.callbacks = manager.get_default_callbacks();

    let session_arc = std::sync::Arc::new(tokio::sync::Mutex::new(session));
    if let Err(e) = manager.insert_session(fd, session_arc) {
        tracing::error!("[FFI] insert_session failed: {}", e);
        return -1;
    }

    push_pending_connection(fd);

    tracing::info!("[FFI] Queued outgoing connection to {}, fd={}", addr, fd);
    update_fd_max(fd);
    fd
}

pub fn rust_session_eof(fd: i32) -> i32 {
    with_session(fd, -1, |session| { session.eof = 1; 0 })
}

pub fn rust_session_read_u8(fd: i32, pos: usize) -> u8 {
    with_session(fd, 0, |session| {
        session.read_u8(pos).unwrap_or_else(|e| { tracing::error!("[FFI] read_u8 error: {}", e); 0 })
    })
}

pub fn rust_session_read_u16(fd: i32, pos: usize) -> u16 {
    with_session(fd, 0, |session| {
        session.read_u16(pos).unwrap_or_else(|e| { tracing::error!("[FFI] read_u16 error: {}", e); 0 })
    })
}

pub fn rust_session_read_u32(fd: i32, pos: usize) -> u32 {
    with_session(fd, 0, |session| {
        session.read_u32(pos).unwrap_or_else(|e| { tracing::error!("[FFI] read_u32 error: {}", e); 0 })
    })
}

pub fn rust_session_write_u8(fd: i32, pos: usize, val: u8) -> i32 {
    with_session(fd, -1, |session| {
        session.write_u8(pos, val).map(|_| 0).unwrap_or_else(|e| { tracing::error!("[FFI] write_u8 error: {}", e); -1 })
    })
}

pub fn rust_session_write_u16(fd: i32, pos: usize, val: u16) -> i32 {
    with_session(fd, -1, |session| {
        session.write_u16(pos, val).map(|_| 0).unwrap_or_else(|e| { tracing::error!("[FFI] write_u16 error: {}", e); -1 })
    })
}

pub fn rust_session_write_u32(fd: i32, pos: usize, val: u32) -> i32 {
    with_session(fd, -1, |session| {
        session.write_u32(pos, val).map(|_| 0).unwrap_or_else(|e| { tracing::error!("[FFI] write_u32 error: {}", e); -1 })
    })
}

pub fn rust_session_skip(fd: i32, len: usize) -> i32 {
    with_session(fd, -1, |session| {
        session.skip(len).map(|_| 0).unwrap_or_else(|e| { tracing::error!("[FFI] skip error: {}", e); -1 })
    })
}

pub fn rust_session_available(fd: i32) -> usize {
    with_session(fd, 0, |session| session.available())
}

pub fn rust_session_commit(fd: i32, len: usize) -> i32 {
    with_session(fd, -1, |session| {
        session.commit_write(len).map(|_| 0).unwrap_or_else(|e| { tracing::error!("[FFI] commit error: {}", e); -1 })
    })
}

pub fn rust_session_flush(_fd: i32) -> i32 { 0 }

pub fn rust_session_rdata_ptr(fd: i32, pos: usize) -> *const u8 {
    with_session(fd, std::ptr::null(), |session| {
        session.rdata_ptr(pos).unwrap_or_else(|e| { tracing::error!("[FFI] rdata_ptr error: {}", e); std::ptr::null() })
    })
}

pub fn rust_session_wdata_ptr(fd: i32, pos: usize) -> *mut u8 {
    with_session(fd, std::ptr::null_mut(), |session| {
        session.wdata_ptr(pos).unwrap_or_else(|e| { tracing::error!("[FFI] wdata_ptr error: {}", e); std::ptr::null_mut() })
    })
}

pub fn rust_session_wfifohead(fd: i32, size: usize) -> i32 {
    with_session(fd, -1, |session| {
        session.ensure_wdata_capacity(size).map(|_| 0).unwrap_or_else(|e| { tracing::error!("[FFI] wfifohead error: {}", e); -1 })
    })
}

pub fn rust_session_rfifoflush(fd: i32) -> i32 {
    with_session(fd, -1, |session| { session.flush_read_buffer(); 0 })
}

pub unsafe fn rust_session_set_default_accept(
    callback: unsafe fn(i32) -> i32,
) {
    tracing::info!("[FFI] Setting default accept callback");
    let manager = get_session_manager();
    manager.default_callbacks.lock().unwrap().accept = Some(sync_callback(callback));
}

pub fn rust_session_set_default_parse(
    callback: AsyncCallback,
) {
    tracing::info!("[FFI] Setting default parse callback");
    let manager = get_session_manager();
    manager.default_callbacks.lock().unwrap().parse = Some(callback);
}

pub unsafe fn rust_session_set_default_timeout(
    callback: unsafe fn(i32) -> i32,
) {
    tracing::info!("[FFI] Setting default timeout callback");
    let manager = get_session_manager();
    manager.default_callbacks.lock().unwrap().timeout = Some(sync_callback(callback));
}

pub unsafe fn rust_session_set_default_shutdown(
    callback: unsafe fn(i32) -> i32,
) {
    tracing::info!("[FFI] Setting default shutdown callback");
    let manager = get_session_manager();
    manager.default_callbacks.lock().unwrap().shutdown = Some(sync_callback(callback));
}

pub fn rust_session_get_data(fd: i32) -> *mut std::ffi::c_void {
    with_session(fd, std::ptr::null_mut(), |session| {
        session.session_data.unwrap_or(std::ptr::null_mut())
    })
}

pub fn rust_session_set_data(fd: i32, data: *mut std::ffi::c_void) {
    with_session(fd, (), |session| {
        session.session_data = if data.is_null() { None } else { Some(data) };
    });
}

pub fn rust_session_get_eof(fd: i32) -> i32 {
    with_session(fd, -1, |session| session.eof)
}

pub fn rust_session_set_eof(fd: i32, eof: i32) {
    with_session(fd, (), |session| { session.eof = eof; });
}

pub fn rust_session_get_client_ip(fd: i32) -> u32 {
    with_session(fd, 0, |session| session.client_addr_raw)
}

pub fn rust_session_get_increment(fd: i32) -> u8 {
    with_session(fd, 0, |session| session.increment)
}

pub fn rust_session_increment(fd: i32) -> u8 {
    with_session(fd, 0, |session| {
        session.increment = session.increment.wrapping_add(1);
        session.increment
    })
}

pub fn rust_session_exists(fd: i32) -> i32 {
    let manager = get_session_manager();
    if manager.get_session(fd).is_some() { 1 } else { 0 }
}

pub unsafe fn rust_session_set_parse(
    fd: i32,
    callback: unsafe fn(i32) -> i32,
) {
    let cb = sync_callback(callback);
    with_session(fd, (), |session| { session.callbacks.parse = Some(cb.clone()); });
}

pub unsafe fn rust_session_set_shutdown(
    fd: i32,
    callback: unsafe fn(i32) -> i32,
) {
    let cb = sync_callback(callback);
    with_session(fd, (), |session| { session.callbacks.shutdown = Some(cb.clone()); });
}

/// Invoke the async parse callback for session `fd`, awaiting its completion.
///
/// This function is `async` because `AsyncCallback` returns a future that cannot
/// be driven without an async context.  It is NOT `#[no_mangle]` and is not
/// called from C — all C-side parse dispatch was removed when `sl_compat.c` was
/// migrated.  The only remaining Rust call site is `map_reset_timer` (in
/// `map_server.rs`), which is a sync `TimerFn` callback and therefore uses
/// `tokio::task::spawn_local` to schedule the async work on the current LocalSet.
///
/// Within the normal session read-loop (`session_task`) the callback is called
/// with `cb(fd).await` directly — `rust_session_call_parse` is only needed when
/// the call site is itself synchronous (i.e. cannot `.await`).
pub async fn rust_session_call_parse(fd: i32) {
    let parse_cb = with_session(fd, None, |session| session.callbacks.parse.clone());
    if let Some(cb) = parse_cb {
        cb(fd).await;
    }
}

pub unsafe fn rust_log_c(level: i32, msg: *const i8) {
    if msg.is_null() { return; }
    let s = std::ffi::CStr::from_ptr(msg).to_string_lossy();
    match level {
        0 => tracing::error!("{}", s),
        1 => tracing::warn!("{}", s),
        3 => tracing::debug!("{}", s),
        _ => tracing::info!("{}", s),
    }
}

pub unsafe fn rust_session_get_all_fds(buf: *mut i32, buf_len: i32) -> i32 {
    if buf.is_null() || buf_len <= 0 { return 0; }
    let fds = get_session_manager().get_all_fds();
    let count = (fds.len() as i32).min(buf_len);
    for (i, &fd) in fds.iter().take(count as usize).enumerate() {
        unsafe { *buf.add(i) = fd; }
    }
    count
}

pub fn rust_add_ip_lockout(ip: u32) {
    crate::network::ddos::add_ip_lockout(ip);
}

pub fn rust_connect_check_clear() {
    crate::network::ddos::connect_check_clear();
}

pub fn rust_add_throttle(ip: u32) {
    crate::network::throttle::add_throttle(ip);
}

pub fn rust_remove_throttle() {
    crate::network::throttle::remove_throttle();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_new() {
        let session = Session::new(1);
        assert_eq!(session.fd, 1);
        assert_eq!(session.eof, 0);
        assert_eq!(session.rdata_pos, 0);
        assert_eq!(session.rdata_size, 0);
        assert_eq!(session.wdata_size, 0);
    }

    #[test]
    fn test_read_u8_bounds_check() {
        let mut session = Session::new(1);
        session.rdata = vec![0x12, 0x34, 0x56];
        session.rdata_size = 3;
        session.rdata_pos = 0;

        // Valid reads
        assert_eq!(session.read_u8(0).unwrap(), 0x12);
        assert_eq!(session.read_u8(2).unwrap(), 0x56);

        // Out of bounds
        assert!(session.read_u8(3).is_err());
        assert!(session.read_u8(100).is_err());
    }

    #[test]
    fn test_read_u16_little_endian() {
        let mut session = Session::new(1);
        session.rdata = vec![0x34, 0x12, 0x78, 0x56];
        session.rdata_size = 4;

        assert_eq!(session.read_u16(0).unwrap(), 0x1234);
        assert_eq!(session.read_u16(2).unwrap(), 0x5678);

        // Not enough bytes
        assert!(session.read_u16(3).is_err());
    }

    #[test]
    fn test_read_u32_little_endian() {
        let mut session = Session::new(1);
        session.rdata = vec![0x78, 0x56, 0x34, 0x12];
        session.rdata_size = 4;

        assert_eq!(session.read_u32(0).unwrap(), 0x12345678);

        // Not enough bytes
        assert!(session.read_u32(1).is_err());
    }

    #[test]
    fn test_write_u8_auto_grow() {
        let mut session = Session::new(1);

        // Write at position 0
        assert!(session.write_u8(0, 0xAA).is_ok());

        // Write beyond current buffer - should auto-grow
        assert!(session.write_u8(100, 0xBB).is_ok());

        // Buffer should have grown
        assert!(session.wdata.len() >= 101);
    }

    #[test]
    fn test_write_u16_little_endian() {
        let mut session = Session::new(1);

        assert!(session.write_u16(0, 0x1234).is_ok());

        // Verify little-endian byte order
        assert_eq!(session.wdata[0], 0x34);
        assert_eq!(session.wdata[1], 0x12);
    }

    #[test]
    fn test_write_u32_little_endian() {
        let mut session = Session::new(1);

        assert!(session.write_u32(0, 0x12345678).is_ok());

        // Verify little-endian
        assert_eq!(session.wdata[0], 0x78);
        assert_eq!(session.wdata[1], 0x56);
        assert_eq!(session.wdata[2], 0x34);
        assert_eq!(session.wdata[3], 0x12);
    }

    #[test]
    fn test_commit_write() {
        let mut session = Session::new(1);

        session.write_u8(0, 0xAA).unwrap();
        session.write_u8(1, 0xBB).unwrap();

        // Commit 2 bytes
        assert!(session.commit_write(2).is_ok());
        assert_eq!(session.wdata_size, 2);

        // Can't commit more than buffer has (buffer auto-grew to 1025 bytes
        // due to write_u8's 1024-byte padding, so 1024 exceeds remaining capacity)
        assert!(session.commit_write(1024).is_err());
    }

    #[test]
    fn test_write_buffer_size_limit() {
        let mut session = Session::new(1);

        // Writing beyond 4MB should fail
        let result = session.write_u8(5_000_000, 0xFF);
        assert!(matches!(
            result,
            Err(SessionError::WriteBufferTooLarge { .. })
        ));
    }

    #[test]
    fn test_write_overflow_check() {
        let mut session = Session::new(1);
        session.wdata_size = usize::MAX - 10;

        // This should overflow and be caught
        let result = session.write_u8(100, 0xFF);
        assert!(matches!(
            result,
            Err(SessionError::WritePositionOverflow { .. })
        ));
    }

    #[test]
    fn test_skip_bounds_check() {
        let mut session = Session::new(1);
        session.rdata = vec![1, 2, 3, 4, 5];
        session.rdata_size = 5;
        session.rdata_pos = 0;

        // Valid skip
        assert!(session.skip(2).is_ok());
        assert_eq!(session.rdata_pos, 2);

        // Read should now start at pos 2
        assert_eq!(session.read_u8(0).unwrap(), 3);

        // Out of bounds skip
        assert!(session.skip(10).is_err());
    }

    #[test]
    fn test_skip_auto_compact() {
        let mut session = Session::new(1);
        session.rdata = vec![1, 2, 3, 4, 5];
        session.rdata_size = 5;

        // Skip to end
        assert!(session.skip(5).is_ok());

        // Should auto-compact (clear buffer)
        assert_eq!(session.rdata_pos, 0);
        assert_eq!(session.rdata_size, 0);
        assert_eq!(session.rdata.len(), 0);
    }

    #[test]
    fn test_flush_read_buffer() {
        let mut session = Session::new(1);
        session.rdata = vec![1, 2, 3, 4, 5, 6];
        session.rdata_size = 6;
        session.rdata_pos = 0;

        // Skip first 2 bytes
        session.skip(2).unwrap();

        // Flush should compact
        session.flush_read_buffer();

        assert_eq!(session.rdata_pos, 0);
        assert_eq!(session.rdata_size, 4);
        assert_eq!(session.rdata[0], 3);  // Data moved to front
    }

    #[test]
    fn test_skip_rejects_overflow() {
        let mut session = Session::new(1);
        session.rdata_size = 100;
        session.rdata_pos = 50;

        // Attempt to skip a huge amount that would overflow
        let result = session.skip(usize::MAX);

        assert!(result.is_err());
        match result {
            Err(SessionError::SkipOutOfBounds { skip_len, available, .. }) => {
                assert_eq!(skip_len, usize::MAX);
                assert_eq!(available, 50);
            }
            _ => panic!("Expected SkipOutOfBounds error"),
        }
    }

    #[test]
    fn test_session_manager_allocate_fd() {
        let manager = SessionManager::new();

        let fd1 = manager.allocate_fd().unwrap();
        let fd2 = manager.allocate_fd().unwrap();

        assert!(fd1 > 0);
        assert!(fd2 > 0);
        assert_ne!(fd1, fd2);
    }

    #[test]
    fn test_session_manager_insert_and_get() {
        let manager = SessionManager::new();

        let session = Session::new(5);
        let session_arc = Arc::new(Mutex::new(session));

        manager.insert_session(5, session_arc.clone()).unwrap();

        let retrieved = manager.get_session(5);
        assert!(retrieved.is_some());

        let arc = retrieved.unwrap();
        let sess = arc.try_lock().unwrap();
        assert_eq!(sess.fd, 5);
    }

    #[test]
    fn test_session_manager_remove() {
        let manager = SessionManager::new();

        let session = Session::new(10);
        manager.insert_session(10, Arc::new(Mutex::new(session))).unwrap();

        assert!(manager.get_session(10).is_some());

        manager.remove_session(10);

        assert!(manager.get_session(10).is_none());
    }

    #[test]
    fn test_session_manager_max_sessions() {
        let manager = SessionManager::new();

        // Fill to limit
        for i in 0..MAX_SESSIONS {
            let session = Session::new(i as i32);
            manager.insert_session(i as i32, Arc::new(Mutex::new(session)))
                .unwrap();
        }

        // Next insert should fail
        let session = Session::new(9999);
        let result = manager.insert_session(9999, Arc::new(Mutex::new(session)));
        assert!(result.is_err());
        assert!(matches!(result, Err(SessionError::MaxSessionsExceeded)));
    }
}

