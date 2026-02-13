//! Session management with async I/O
//!
//! This module replaces session.c with memory-safe async Rust implementation.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;
use tokio::sync::{Mutex, RwLock};

/// Buffer size constants
pub const RFIFO_SIZE: usize = 16 * 1024;
pub const WFIFO_SIZE: usize = 16 * 1024;

/// Maximum number of sessions
pub const MAX_SESSIONS: usize = 1024;

/// Maximum write buffer size (256KB)
const MAX_WDATA_SIZE: usize = 256 * 1024;

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

    #[error("Write buffer too large: fd={fd}, requested_pos={requested_pos}, max=262144")]
    WriteBufferTooLarge { fd: i32, requested_pos: usize },

    #[error("Session not found: fd={0}")]
    SessionNotFound(i32),

    #[error("Maximum sessions exceeded (limit: {MAX_SESSIONS})")]
    MaxSessionsExceeded,

    #[error("File descriptor overflow")]
    FdOverflow,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Callback function pointers for C interop
#[derive(Clone, Copy, Default)]
pub struct SessionCallbacks {
    /// Called when packet data is received and ready to parse
    pub parse: Option<unsafe extern "C" fn(i32) -> i32>,
    /// Called when session has been idle for too long
    pub timeout: Option<unsafe extern "C" fn(i32) -> i32>,
    /// Called when session is being shut down
    pub shutdown: Option<unsafe extern "C" fn(i32) -> i32>,
}

/// Global session manager (thread-safe)
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<i32, Arc<Mutex<Session>>>>>,
    next_fd: Arc<Mutex<i32>>,
    pub default_callbacks: Arc<Mutex<SessionCallbacks>>,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            next_fd: Arc::new(Mutex::new(1)), // Start at 1 (0 reserved)
            default_callbacks: Arc::new(Mutex::new(SessionCallbacks::default())),
        }
    }

    /// Allocate a new file descriptor
    pub async fn allocate_fd(&self) -> Result<i32, SessionError> {
        let mut next = self.next_fd.lock().await;
        let fd = *next;

        // Check against MAX_SESSIONS before incrementing
        if fd >= MAX_SESSIONS as i32 {
            return Err(SessionError::MaxSessionsExceeded);
        }

        *next = next.checked_add(1)
            .ok_or(SessionError::FdOverflow)?;

        Ok(fd)
    }

    /// Insert a session into the manager
    pub async fn insert_session(&self, fd: i32, session: Arc<Mutex<Session>>)
        -> Result<(), SessionError> {
        let mut sessions = self.sessions.write().await;

        if sessions.len() >= MAX_SESSIONS {
            return Err(SessionError::MaxSessionsExceeded);
        }

        sessions.insert(fd, session);
        Ok(())
    }

    /// Get a session by file descriptor
    pub async fn get_session(&self, fd: i32) -> Option<Arc<Mutex<Session>>> {
        let sessions = self.sessions.read().await;
        sessions.get(&fd).cloned()
    }

    /// Remove a session
    pub async fn remove_session(&self, fd: i32) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(&fd);
    }

    /// Get default callbacks
    pub async fn get_default_callbacks(&self) -> SessionCallbacks {
        let callbacks = self.default_callbacks.lock().await;
        *callbacks
    }

    /// Set default callbacks
    pub async fn set_default_callbacks(&self, callbacks: SessionCallbacks) {
        let mut default = self.default_callbacks.lock().await;
        *default = callbacks;
    }

    /// Get session count
    pub async fn session_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
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

/// Global Tokio runtime
pub static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Initialize the Tokio runtime
pub fn init_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        Runtime::new().expect("Failed to create Tokio runtime")
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
}

impl Session {
    /// Create a new session with the given file descriptor
    pub fn new(fd: i32) -> Self {
        Self {
            fd,
            socket: None,
            client_addr: None,
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
        }
    }

    /// Read u8 with bounds checking
    pub fn read_u8(&self, pos: usize) -> Result<u8, SessionError> {
        let actual_pos = self.rdata_pos + pos;

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
        let actual_pos = self.rdata_pos + pos;

        if actual_pos + 2 > self.rdata_size {
            return Err(SessionError::ReadOutOfBounds {
                fd: self.fd,
                pos: actual_pos,
                size: self.rdata_size,
            });
        }

        let bytes = [self.rdata[actual_pos], self.rdata[actual_pos + 1]];

        Ok(u16::from_le_bytes(bytes))
    }

    /// Read u32 (little-endian) with bounds checking
    pub fn read_u32(&self, pos: usize) -> Result<u32, SessionError> {
        let actual_pos = self.rdata_pos + pos;

        if actual_pos + 4 > self.rdata_size {
            return Err(SessionError::ReadOutOfBounds {
                fd: self.fd,
                pos: actual_pos,
                size: self.rdata_size,
            });
        }

        let bytes = [
            self.rdata[actual_pos],
            self.rdata[actual_pos + 1],
            self.rdata[actual_pos + 2],
            self.rdata[actual_pos + 3],
        ];

        Ok(u32::from_le_bytes(bytes))
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

        if actual_pos >= MAX_WDATA_SIZE {
            return Err(SessionError::WriteBufferTooLarge {
                fd: self.fd,
                requested_pos: actual_pos,
            });
        }

        // Auto-grow in 1KB chunks
        if actual_pos + 1 > self.wdata.len() {
            self.wdata.resize(actual_pos + 1024, 0);
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

        if actual_pos >= MAX_WDATA_SIZE {
            return Err(SessionError::WriteBufferTooLarge {
                fd: self.fd,
                requested_pos: actual_pos,
            });
        }

        if actual_pos + 2 > self.wdata.len() {
            self.wdata.resize(actual_pos + 1024, 0);
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

        if actual_pos >= MAX_WDATA_SIZE {
            return Err(SessionError::WriteBufferTooLarge {
                fd: self.fd,
                requested_pos: actual_pos,
            });
        }

        if actual_pos + 4 > self.wdata.len() {
            self.wdata.resize(actual_pos + 1024, 0);
        }

        let bytes = val.to_le_bytes();
        self.wdata[actual_pos..actual_pos + 4].copy_from_slice(&bytes);

        Ok(())
    }

    /// Commit write buffer (like WFIFOSET)
    pub fn commit_write(&mut self, len: usize) -> Result<(), SessionError> {
        let new_size = self.wdata_size + len;

        if new_size > self.wdata.len() {
            return Err(SessionError::WriteCommitTooLarge {
                fd: self.fd,
                requested: len,
                available: self.wdata.len() - self.wdata_size,
            });
        }

        self.wdata_size = new_size;
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

    /// Compacts the read buffer by moving unread data to the beginning.
    ///
    /// If all data has been consumed (rdata_pos == rdata_size), clears the buffer.
    /// Otherwise, moves unread bytes to the front and updates positions.
    ///
    /// This is equivalent to the C macro RFIFOFLUSH.
    ///
    /// # Note
    /// This operation is infallible - it always succeeds.
    pub fn flush_read_buffer(&mut self) {
        if self.rdata_pos == self.rdata_size {
            // All data read - reset
            self.rdata_pos = 0;
            self.rdata_size = 0;
            self.rdata.clear();
        } else if self.rdata_pos > 0 {
            // Compact: move unread data to front
            self.rdata.copy_within(self.rdata_pos..self.rdata_size, 0);
            self.rdata_size -= self.rdata_pos;
            self.rdata_pos = 0;
        }
    }
}

// SAFETY: session_data is an opaque pointer to C-managed memory. It is only accessed
// by C callbacks which provide their own synchronization. The pointer itself is Send/Sync
// safe because it's just an address - the actual data synchronization is handled externally
// by C code. All other fields (Vec, Option, primitives) are already Send/Sync.
unsafe impl Send for Session {}
unsafe impl Sync for Session {}

/// Run the async game server
pub async fn run_async_server(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("[rust_server] Starting on port {}", port);

    // Initialize session manager
    get_session_manager();

    // Bind to port
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!("[rust_server] Listening on port {}", port);

    // Accept loop
    loop {
        // Check for shutdown signal
        #[cfg(not(test))]
        if crate::ffi::core::rust_should_shutdown() != 0 {
            tracing::info!("[rust_server] Shutdown requested");
            break;
        }

        // Accept connection with timeout
        match tokio::time::timeout(Duration::from_millis(100), listener.accept()).await {
            Ok(Ok((socket, addr))) => {
                tracing::debug!("[rust_server] New connection from {}", addr);
                // Spawn handler task (will implement in next task)
                tokio::spawn(handle_connection(socket, addr));
            }
            Ok(Err(e)) => {
                tracing::error!("[rust_server] Accept error: {}", e);
            }
            Err(_) => {
                // Timeout - check shutdown flag again
                continue;
            }
        }
    }

    // Graceful shutdown
    shutdown_all_sessions().await;

    Ok(())
}

/// Shutdown all active sessions
async fn shutdown_all_sessions() {
    tracing::info!("[rust_server] Shutting down all sessions");

    let manager = get_session_manager();
    let sessions = manager.sessions.read().await;

    for (fd, session_arc) in sessions.iter() {
        let session = session_arc.lock().await;

        // Call shutdown callback if set
        if let Some(shutdown_cb) = session.callbacks.shutdown {
            tracing::debug!("[rust_server] Calling shutdown callback for fd={}", fd);
            drop(session); // Release lock before C call
            unsafe { shutdown_cb(*fd) };
        }
    }
}

/// Handle a single client connection
async fn handle_connection(socket: TcpStream, addr: SocketAddr) {
    // TODO: Add throttle check in later task

    // Create session
    let manager = get_session_manager();
    let fd = match manager.allocate_fd().await {
        Ok(fd) => fd,
        Err(e) => {
            tracing::error!("[session] Failed to allocate FD: {}", e);
            return;
        }
    };

    let mut session = Session::new(fd);
    session.socket = Some(Arc::new(Mutex::new(socket)));
    session.client_addr = Some(addr);

    // Apply default callbacks
    session.callbacks = manager.get_default_callbacks().await;

    let session_arc = Arc::new(Mutex::new(session));
    if let Err(e) = manager.insert_session(fd, session_arc.clone()).await {
        tracing::error!("[session] Failed to insert session: {}", e);
        return;
    }

    tracing::info!("[session] New connection: fd={}, addr={}", fd, addr);

    // Main session loop
    if let Err(e) = session_loop(fd, session_arc.clone()).await {
        tracing::error!("[session] fd={} error: {}", fd, e);
    }

    // Cleanup
    manager.remove_session(fd).await;
    tracing::info!("[session] Closed: fd={}", fd);
}

/// Main event loop for a single session
async fn session_loop(
    fd: i32,
    session_arc: Arc<Mutex<Session>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut read_buf = vec![0u8; 4096];
    let mut timeout_check = tokio::time::interval(Duration::from_secs(1));

    loop {
        let session = session_arc.lock().await;

        // Check if session marked for closure
        if session.eof != 0 {
            tracing::debug!("[session] fd={} eof={}, closing", fd, session.eof);
            break;
        }

        // Get socket (must exist)
        let socket_arc = session
            .socket
            .as_ref()
            .ok_or("Socket not initialized")?
            .clone();

        drop(session); // Release session lock

        tokio::select! {
            // Read from socket
            result = async {
                let mut socket = socket_arc.lock().await;
                socket.read(&mut read_buf).await
            } => {
                let mut session = session_arc.lock().await;

                match result {
                    Ok(0) => {
                        // Connection closed
                        tracing::debug!("[session] fd={} connection closed by peer", fd);
                        session.eof = 4;
                        break;
                    }
                    Ok(n) => {
                        // Append to read buffer
                        session.rdata.extend_from_slice(&read_buf[..n]);
                        session.rdata_size += n;
                        session.last_activity = Instant::now();

                        tracing::trace!("[session] fd={} read {} bytes", fd, n);

                        // Call parse callback
                        if let Some(parse_cb) = session.callbacks.parse {
                            drop(session);

                            tracing::trace!("[session] fd={} calling parse callback", fd);
                            unsafe { parse_cb(fd) };
                        }
                    }
                    Err(e) => {
                        tracing::error!("[session] fd={} read error: {}", fd, e);
                        session.eof = 3;
                        break;
                    }
                }
            }

            // Write to socket (if data pending)
            _ = async {
                let session = session_arc.lock().await;

                if session.wdata_size > 0 {
                    let data_to_send = session.wdata[..session.wdata_size].to_vec();
                    drop(session);

                    let mut socket = socket_arc.lock().await;
                    match socket.write_all(&data_to_send).await {
                        Ok(_) => {
                            let mut session = session_arc.lock().await;
                            tracing::trace!("[session] fd={} sent {} bytes", fd, data_to_send.len());
                            session.wdata.clear();
                            session.wdata_size = 0;
                        }
                        Err(e) => {
                            let mut session = session_arc.lock().await;
                            tracing::error!("[session] fd={} write error: {}", fd, e);
                            session.eof = 2;
                        }
                    }
                }
            } => {}

            // Timeout check
            _ = timeout_check.tick() => {
                let session = session_arc.lock().await;

                let idle = session.last_activity.elapsed();
                if idle > Duration::from_secs(60) {
                    tracing::warn!("[session] fd={} timeout (idle {}s)", fd, idle.as_secs());

                    // Call timeout callback
                    if let Some(timeout_cb) = session.callbacks.timeout {
                        drop(session);

                        unsafe { timeout_cb(fd) };
                    }
                }
            }
        }
    }

    Ok(())
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

        // Can't commit more than buffer has
        assert!(session.commit_write(1023).is_err());
    }

    #[test]
    fn test_write_buffer_size_limit() {
        let mut session = Session::new(1);

        // Writing beyond 256KB should fail
        let result = session.write_u8(300_000, 0xFF);
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

    #[tokio::test]
    async fn test_session_manager_allocate_fd() {
        let manager = SessionManager::new();

        let fd1 = manager.allocate_fd().await.unwrap();
        let fd2 = manager.allocate_fd().await.unwrap();

        assert!(fd1 > 0);
        assert!(fd2 > 0);
        assert_ne!(fd1, fd2);
    }

    #[tokio::test]
    async fn test_session_manager_insert_and_get() {
        let manager = SessionManager::new();

        let session = Session::new(5);
        let session_arc = Arc::new(Mutex::new(session));

        manager.insert_session(5, session_arc.clone()).await.unwrap();

        let retrieved = manager.get_session(5).await;
        assert!(retrieved.is_some());

        let session_arc_retrieved = retrieved.unwrap();
        let sess = session_arc_retrieved.lock().await;
        assert_eq!(sess.fd, 5);
    }

    #[tokio::test]
    async fn test_session_manager_remove() {
        let manager = SessionManager::new();

        let session = Session::new(10);
        manager.insert_session(10, Arc::new(Mutex::new(session))).await.unwrap();

        assert!(manager.get_session(10).await.is_some());

        manager.remove_session(10).await;

        assert!(manager.get_session(10).await.is_none());
    }

    #[tokio::test]
    async fn test_session_manager_max_sessions() {
        let manager = SessionManager::new();

        // Fill to limit
        for i in 0..MAX_SESSIONS {
            let session = Session::new(i as i32);
            manager.insert_session(i as i32, Arc::new(Mutex::new(session)))
                .await
                .unwrap();
        }

        // Next insert should fail
        let session = Session::new(9999);
        let result = manager.insert_session(9999, Arc::new(Mutex::new(session))).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(SessionError::MaxSessionsExceeded)));
    }
}
