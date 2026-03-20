//! Session management — types, manager, and public accessors.

pub mod buffer;
pub mod io;

use parking_lot::RwLock;
use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex as StdMutex;
use std::sync::{Arc, OnceLock};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use crate::game::player::PlayerEntity;

pub use buffer::Session;
pub(crate) use io::{accept_loop, session_io_task, shutdown_all_sessions};

// ─── Types ─────────────────────────────────────────────────────────────────

/// Opaque session identifier. Wraps an internal `i32` counter that is
/// **not** an OS file descriptor despite the historical `fd` naming.
/// Use `.raw()` when an `i32` is needed at I/O or FFI boundaries.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SessionId(i32);

impl SessionId {
    pub const fn from_raw(raw: i32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sid={}", self.0)
    }
}

/// Future returned by an async session callback.
pub type CallbackFuture = Pin<Box<dyn Future<Output = i32> + 'static>>;

/// An async session callback stored in SessionCallbacks.
pub type AsyncCallback = Arc<dyn Fn(SessionId) -> CallbackFuture + Send + Sync + 'static>;

/// Wrap a synchronous unsafe fn pointer as an AsyncCallback.
pub fn sync_callback(f: unsafe fn(SessionId) -> i32) -> AsyncCallback {
    std::sync::Arc::new(move |fd: SessionId| -> CallbackFuture {
        Box::pin(async move { unsafe { f(fd) } })
    })
}

// ─── Constants ─────────────────────────────────────────────────────────────

pub const RFIFO_SIZE: usize = 16 * 1024;
pub const WFIFO_SIZE: usize = 16 * 1024;

/// Maximum read buffer size.
pub(crate) const MAX_RDATA_SIZE: usize = 64 * 1024;

pub const MAX_SESSIONS: usize = 1024;

/// Maximum write buffer size (4MB).
pub(crate) const MAX_WDATA_SIZE: usize = 4 * 1024 * 1024;

// ─── Error ─────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Read out of bounds: fd={fd}, pos={pos}, size={size}")]
    ReadOutOfBounds {
        fd: SessionId,
        pos: usize,
        size: usize,
    },

    #[error("Skip out of bounds: fd={fd}, skip={skip_len}, available={available}")]
    SkipOutOfBounds {
        fd: SessionId,
        skip_len: usize,
        available: usize,
    },

    #[error("Write commit too large: fd={fd}, requested={requested}, available={available}")]
    WriteCommitTooLarge {
        fd: SessionId,
        requested: usize,
        available: usize,
    },

    #[error("Write position overflow: fd={fd}, wdata_size={wdata_size}, pos={pos}")]
    WritePositionOverflow {
        fd: SessionId,
        wdata_size: usize,
        pos: usize,
    },

    #[error("Write buffer too large: fd={fd}, requested_pos={requested_pos}, max={max}")]
    WriteBufferTooLarge {
        fd: SessionId,
        requested_pos: usize,
        max: usize,
    },

    #[error("Session not found: fd={0}")]
    SessionNotFound(SessionId),

    #[error("Maximum sessions exceeded (limit: {MAX_SESSIONS})")]
    MaxSessionsExceeded,

    #[error("File descriptor overflow")]
    FdOverflow,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

// ─── Callbacks ─────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct SessionCallbacks {
    pub accept: Option<AsyncCallback>,
    pub parse: Option<AsyncCallback>,
    pub timeout: Option<AsyncCallback>,
    pub shutdown: Option<AsyncCallback>,
}

// ─── SessionManager ────────────────────────────────────────────────────────

pub struct SessionManager {
    sessions: RwLock<HashMap<SessionId, Arc<Mutex<Session>>>>,
    next_id: AtomicI32,
    pub default_callbacks: StdMutex<SessionCallbacks>,
    pub listeners: StdMutex<HashMap<i32, std::net::TcpListener>>,
    pub listen_fds: StdMutex<Vec<i32>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            next_id: AtomicI32::new(1),
            default_callbacks: StdMutex::new(SessionCallbacks::default()),
            listeners: StdMutex::new(HashMap::new()),
            listen_fds: StdMutex::new(Vec::new()),
        }
    }

    pub fn allocate_id(&self) -> Result<SessionId, SessionError> {
        let raw = self.next_id.fetch_add(1, Ordering::Relaxed);
        if raw > MAX_SESSIONS as i32 {
            return Err(SessionError::MaxSessionsExceeded);
        }
        Ok(SessionId::from_raw(raw))
    }

    pub fn insert_session(
        &self,
        id: SessionId,
        session: Arc<Mutex<Session>>,
    ) -> Result<(), SessionError> {
        let mut sessions = self.sessions.write();
        if sessions.len() >= MAX_SESSIONS {
            return Err(SessionError::MaxSessionsExceeded);
        }
        sessions.insert(id, session);
        Ok(())
    }

    pub fn get_session(&self, id: SessionId) -> Option<Arc<Mutex<Session>>> {
        self.sessions.read().get(&id).cloned()
    }

    pub fn remove_session(&self, id: SessionId) {
        self.sessions.write().remove(&id);
    }

    pub fn get_default_callbacks(&self) -> SessionCallbacks {
        self.default_callbacks.lock().unwrap().clone()
    }

    pub fn set_default_callbacks(&self, callbacks: SessionCallbacks) {
        *self.default_callbacks.lock().unwrap() = callbacks;
    }

    pub fn session_count(&self) -> usize {
        self.sessions.read().len()
    }

    pub fn get_all_fds(&self) -> Vec<SessionId> {
        self.sessions.read().keys().copied().collect()
    }

    pub fn add_listener(&self, fd: i32, listener: std::net::TcpListener) {
        self.listeners.lock().unwrap().insert(fd, listener);
        self.listen_fds.lock().unwrap().push(fd);
    }

    pub fn take_listener(&self, fd: i32) -> Option<std::net::TcpListener> {
        self.listeners.lock().unwrap().remove(&fd)
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Globals ───────────────────────────────────────────────────────────────

pub static SESSION_MANAGER: OnceLock<SessionManager> = OnceLock::new();

pub fn get_session_manager() -> &'static SessionManager {
    SESSION_MANAGER.get_or_init(SessionManager::new)
}

/// Outgoing connections created from game callbacks, pending session_io_task spawn.
pub static PENDING_CONNECTIONS: OnceLock<StdMutex<Vec<SessionId>>> = OnceLock::new();

pub fn push_pending_connection(fd: SessionId) {
    PENDING_CONNECTIONS
        .get_or_init(|| StdMutex::new(Vec::new()))
        .lock()
        .unwrap()
        .push(fd);
}

pub(crate) fn drain_pending_connections() -> Vec<SessionId> {
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
) -> Result<SessionId, SessionError> {
    let fd = manager.allocate_id()?;

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
    update_fd_max_pub(fd);
    Ok(fd)
}

// ─── Public API ────────────────────────────────────────────────────────────

static FD_MAX: AtomicI32 = AtomicI32::new(0);

pub fn get_fd_max() -> i32 {
    FD_MAX.load(Ordering::Relaxed)
}

pub fn update_fd_max_pub(fd: SessionId) {
    let next = fd.raw() + 1;
    FD_MAX.fetch_max(next, Ordering::Relaxed);
}

fn with_session<F, R>(fd: SessionId, default: R, f: F) -> R
where
    F: FnOnce(&mut Session) -> R,
{
    let manager = get_session_manager();
    if let Some(session_arc) = manager.get_session(fd) {
        match session_arc.try_lock() {
            Ok(mut guard) => f(&mut guard),
            Err(_) => {
                tracing::error!("[session] with_session fd={} try_lock failed — session contended, returning default", fd);
                default
            }
        }
    } else {
        default
    }
}

pub fn make_listen_port(port: i32) -> SessionId {
    tracing::info!("[session] make_listen_port(port={})", port);

    let addr = format!("0.0.0.0:{}", port);
    let std_listener = match std::net::TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("[session] Failed to bind port {}: {}", port, e);
            return SessionId::from_raw(-1);
        }
    };

    let manager = get_session_manager();
    let fd = match manager.allocate_id() {
        Ok(fd) => fd,
        Err(e) => {
            tracing::error!("[session] Failed to allocate id for listener: {}", e);
            return SessionId::from_raw(-1);
        }
    };

    tracing::info!("[session] Listener bound on port {}, fd={}", port, fd);
    manager.add_listener(fd.raw(), std_listener);
    update_fd_max_pub(fd);
    fd
}

pub fn make_connection(ip: u32, port: i32) -> SessionId {
    let ipv4 = std::net::Ipv4Addr::from(u32::from_be(ip));
    let addr = std::net::SocketAddr::new(std::net::IpAddr::V4(ipv4), port as u16);

    tracing::info!(
        "[session] make_connection queuing outgoing connection to {}",
        addr
    );

    let manager = get_session_manager();

    let fd = match manager.allocate_id() {
        Ok(fd) => fd,
        Err(e) => {
            tracing::error!("[session] allocate_id failed: {}", e);
            return SessionId::from_raw(-1);
        }
    };

    let mut session = Session::new(fd);
    session.client_addr = Some(addr);
    session.client_addr_raw = ip;
    session.connect_addr = Some(addr);
    session.callbacks = manager.get_default_callbacks();

    let session_arc = std::sync::Arc::new(tokio::sync::Mutex::new(session));
    if let Err(e) = manager.insert_session(fd, session_arc) {
        tracing::error!("[session] insert_session failed: {}", e);
        return SessionId::from_raw(-1);
    }

    push_pending_connection(fd);

    tracing::info!(
        "[session] Queued outgoing connection to {}, fd={}",
        addr,
        fd
    );
    update_fd_max_pub(fd);
    fd
}

pub fn session_get_data(fd: SessionId) -> Option<Arc<PlayerEntity>> {
    with_session(fd, None, |session| session.session_data.clone())
}

pub fn session_get_eof(fd: SessionId) -> i32 {
    with_session(fd, -1, |session| session.eof)
}

pub fn session_set_eof(fd: SessionId, eof: i32) {
    with_session(fd, (), |session| {
        session.eof = eof;
    });
}

pub fn session_get_client_ip(fd: SessionId) -> u32 {
    with_session(fd, 0, |session| session.client_addr_raw)
}

pub fn session_increment(fd: SessionId) -> u8 {
    with_session(fd, 0, |session| {
        session.increment = session.increment.wrapping_add(1);
        session.increment
    })
}

pub fn session_exists(fd: SessionId) -> bool {
    get_session_manager().get_session(fd).is_some()
}

/// Invoke the async parse callback for session `fd`, awaiting its completion.
pub async fn session_call_parse(fd: SessionId) {
    let parse_cb = with_session(fd, None, |session| session.callbacks.parse.clone());
    if let Some(cb) = parse_cb {
        cb(fd).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sid(raw: i32) -> SessionId {
        SessionId::from_raw(raw)
    }

    #[test]
    fn test_session_new() {
        let session = Session::new(sid(1));
        assert_eq!(session.fd, sid(1));
        assert_eq!(session.eof, 0);
        assert_eq!(session.rdata_pos, 0);
        assert_eq!(session.rdata_size, 0);
        assert_eq!(session.wdata_size, 0);
    }

    #[test]
    fn test_read_u8_bounds_check() {
        let mut session = Session::new(sid(1));
        session.rdata = vec![0x12, 0x34, 0x56];
        session.rdata_size = 3;
        session.rdata_pos = 0;

        assert_eq!(session.read_u8(0).unwrap(), 0x12);
        assert_eq!(session.read_u8(2).unwrap(), 0x56);
        assert!(session.read_u8(3).is_err());
        assert!(session.read_u8(100).is_err());
    }

    #[test]
    fn test_read_u16_little_endian() {
        let mut session = Session::new(sid(1));
        session.rdata = vec![0x34, 0x12, 0x78, 0x56];
        session.rdata_size = 4;

        assert_eq!(session.read_u16(0).unwrap(), 0x1234);
        assert_eq!(session.read_u16(2).unwrap(), 0x5678);
        assert!(session.read_u16(3).is_err());
    }

    #[test]
    fn test_read_u32_little_endian() {
        let mut session = Session::new(sid(1));
        session.rdata = vec![0x78, 0x56, 0x34, 0x12];
        session.rdata_size = 4;

        assert_eq!(session.read_u32(0).unwrap(), 0x12345678);
        assert!(session.read_u32(1).is_err());
    }

    #[test]
    fn test_write_u8_auto_grow() {
        let mut session = Session::new(sid(1));
        assert!(session.write_u8(0, 0xAA).is_ok());
        assert!(session.write_u8(100, 0xBB).is_ok());
        assert!(session.wdata.len() >= 101);
    }

    #[test]
    fn test_write_u16_little_endian() {
        let mut session = Session::new(sid(1));
        assert!(session.write_u16(0, 0x1234).is_ok());
        assert_eq!(session.wdata[0], 0x34);
        assert_eq!(session.wdata[1], 0x12);
    }

    #[test]
    fn test_write_u32_little_endian() {
        let mut session = Session::new(sid(1));
        assert!(session.write_u32(0, 0x12345678).is_ok());
        assert_eq!(session.wdata[0], 0x78);
        assert_eq!(session.wdata[1], 0x56);
        assert_eq!(session.wdata[2], 0x34);
        assert_eq!(session.wdata[3], 0x12);
    }

    #[test]
    fn test_commit_write() {
        let mut session = Session::new(sid(1));
        session.write_u8(0, 0xAA).unwrap();
        session.write_u8(1, 0xBB).unwrap();
        assert!(session.commit_write(2).is_ok());
        assert_eq!(session.wdata_size, 2);
        assert!(session.commit_write(1024).is_err());
    }

    #[test]
    fn test_write_buffer_size_limit() {
        let mut session = Session::new(sid(1));
        let result = session.write_u8(5_000_000, 0xFF);
        assert!(matches!(
            result,
            Err(SessionError::WriteBufferTooLarge { .. })
        ));
    }

    #[test]
    fn test_write_overflow_check() {
        let mut session = Session::new(sid(1));
        session.wdata_size = usize::MAX - 10;
        let result = session.write_u8(100, 0xFF);
        assert!(matches!(
            result,
            Err(SessionError::WritePositionOverflow { .. })
        ));
    }

    #[test]
    fn test_skip_bounds_check() {
        let mut session = Session::new(sid(1));
        session.rdata = vec![1, 2, 3, 4, 5];
        session.rdata_size = 5;
        session.rdata_pos = 0;

        assert!(session.skip(2).is_ok());
        assert_eq!(session.rdata_pos, 2);
        assert_eq!(session.read_u8(0).unwrap(), 3);
        assert!(session.skip(10).is_err());
    }

    #[test]
    fn test_skip_auto_compact() {
        let mut session = Session::new(sid(1));
        session.rdata = vec![1, 2, 3, 4, 5];
        session.rdata_size = 5;
        assert!(session.skip(5).is_ok());
        assert_eq!(session.rdata_pos, 0);
        assert_eq!(session.rdata_size, 0);
        assert_eq!(session.rdata.len(), 0);
    }

    #[test]
    fn test_flush_read_buffer() {
        let mut session = Session::new(sid(1));
        session.rdata = vec![1, 2, 3, 4, 5, 6];
        session.rdata_size = 6;
        session.rdata_pos = 0;
        session.skip(2).unwrap();
        session.flush_read_buffer();
        assert_eq!(session.rdata_pos, 0);
        assert_eq!(session.rdata_size, 4);
        assert_eq!(session.rdata[0], 3);
    }

    #[test]
    fn test_skip_rejects_overflow() {
        let mut session = Session::new(sid(1));
        session.rdata_size = 100;
        session.rdata_pos = 50;
        let result = session.skip(usize::MAX);
        assert!(result.is_err());
        match result {
            Err(SessionError::SkipOutOfBounds {
                skip_len,
                available,
                ..
            }) => {
                assert_eq!(skip_len, usize::MAX);
                assert_eq!(available, 50);
            }
            _ => panic!("Expected SkipOutOfBounds error"),
        }
    }

    #[test]
    fn test_session_manager_allocate_id() {
        let manager = SessionManager::new();
        let fd1 = manager.allocate_id().unwrap();
        let fd2 = manager.allocate_id().unwrap();
        assert!(fd1.raw() > 0);
        assert!(fd2.raw() > 0);
        assert_ne!(fd1, fd2);
    }

    #[test]
    fn test_session_manager_insert_and_get() {
        let manager = SessionManager::new();
        let id = sid(5);
        let session = Session::new(id);
        let session_arc = Arc::new(Mutex::new(session));
        manager.insert_session(id, session_arc.clone()).unwrap();
        let retrieved = manager.get_session(id);
        assert!(retrieved.is_some());
        let arc = retrieved.unwrap();
        let sess = arc.try_lock().unwrap();
        assert_eq!(sess.fd, id);
    }

    #[test]
    fn test_session_manager_remove() {
        let manager = SessionManager::new();
        let id = sid(10);
        let session = Session::new(id);
        manager
            .insert_session(id, Arc::new(Mutex::new(session)))
            .unwrap();
        assert!(manager.get_session(id).is_some());
        manager.remove_session(id);
        assert!(manager.get_session(id).is_none());
    }

    #[test]
    fn test_session_manager_max_sessions() {
        let manager = SessionManager::new();
        for i in 0..MAX_SESSIONS {
            let id = sid(i as i32);
            let session = Session::new(id);
            manager
                .insert_session(id, Arc::new(Mutex::new(session)))
                .unwrap();
        }
        let id = sid(9999);
        let session = Session::new(id);
        let result = manager.insert_session(id, Arc::new(Mutex::new(session)));
        assert!(result.is_err());
        assert!(matches!(result, Err(SessionError::MaxSessionsExceeded)));
    }
}
