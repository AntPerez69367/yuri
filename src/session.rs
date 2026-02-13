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
    /// Called when packet data is received and ready to parse
    pub parse: Option<unsafe extern "C" fn(i32) -> i32>,
    /// Called when session has been idle for too long
    pub timeout: Option<unsafe extern "C" fn(i32) -> i32>,
    /// Called when session is being shut down
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
}

// SAFETY: session_data is an opaque pointer to C-managed memory. It is only accessed
// by C callbacks which provide their own synchronization. The pointer itself is Send/Sync
// safe because it's just an address - the actual data synchronization is handled externally
// by C code. All other fields (Vec, Option, primitives) are already Send/Sync.
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
}
