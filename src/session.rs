//! Session management with async I/O
//!
//! This module replaces session.c with memory-safe async Rust implementation.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

/// Buffer size constants
pub const RFIFO_SIZE: usize = 16 * 1024;
pub const WFIFO_SIZE: usize = 16 * 1024;

/// Maximum number of sessions
pub const MAX_SESSIONS: usize = 1024;

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

    #[error("Session not found: fd={0}")]
    SessionNotFound(i32),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Callback function pointers for C interop
#[derive(Clone, Copy, Default)]
pub struct SessionCallbacks {
    pub parse: Option<unsafe extern "C" fn(i32) -> i32>,
    pub timeout: Option<unsafe extern "C" fn(i32) -> i32>,
    pub shutdown: Option<unsafe extern "C" fn(i32) -> i32>,
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
}

// Marker for Send/Sync - session_data is managed carefully
unsafe impl Send for Session {}
unsafe impl Sync for Session {}

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
}
