//! Per-session state and packet buffer I/O.

use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use crate::game::player::PlayerEntity;

use super::{SessionCallbacks, SessionError, SessionId, MAX_WDATA_SIZE, RFIFO_SIZE, WFIFO_SIZE};

/// Session state for a single client connection
pub struct Session {
    /// Session identifier
    pub fd: SessionId,

    /// TCP socket (Tokio async)
    pub socket: Option<Arc<Mutex<TcpStream>>>,

    /// Client address
    pub client_addr: Option<std::net::SocketAddr>,

    /// Client IPv4 address as network-order u32 (used for DDoS/throttle lookups)
    pub client_addr_raw: u32,

    /// Pending outgoing connection address.
    /// Set by make_connection when called from inside the runtime.
    /// session_io_task performs the actual async connect before starting I/O.
    pub connect_addr: Option<std::net::SocketAddr>,

    /// Read buffer (FIFO)
    pub rdata: Vec<u8>,
    pub rdata_pos: usize,
    pub rdata_size: usize,

    /// Write buffer (FIFO)
    pub wdata: Vec<u8>,
    pub wdata_size: usize,

    /// Connection state (0=active, 1=server-eof, 2=write error, 3=read error, 4=peer closed)
    pub eof: i32,

    /// Packet increment counter
    pub increment: u8,

    /// Last activity timestamp
    pub last_activity: Instant,

    /// Non-owning pointer to this session's PlayerEntity (owned by PLAYER_MAP).
    /// Only set for player connections; None for listener/inter-server sessions.
    pub session_data: Option<Arc<PlayerEntity>>,

    /// Callbacks
    pub callbacks: SessionCallbacks,

    /// Guards against double-invocation of the shutdown callback.
    /// Set to true the first time shutdown is called; subsequent callers skip it.
    pub(crate) shutdown_called: bool,

    /// Notified when data is written to this session's write buffer.
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
    /// Create a new session with the given session identifier
    pub fn new(fd: SessionId) -> Self {
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
        let actual_pos = self
            .rdata_pos
            .checked_add(pos)
            .ok_or(SessionError::ReadOutOfBounds {
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
        let actual_pos = self
            .rdata_pos
            .checked_add(pos)
            .ok_or(SessionError::ReadOutOfBounds {
                fd: self.fd,
                pos: usize::MAX,
                size: self.rdata_size,
            })?;
        let end = actual_pos
            .checked_add(2)
            .ok_or(SessionError::ReadOutOfBounds {
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

        Ok(u16::from_le_bytes([
            self.rdata[actual_pos],
            self.rdata[actual_pos + 1],
        ]))
    }

    /// Read u32 (little-endian) with bounds checking
    pub fn read_u32(&self, pos: usize) -> Result<u32, SessionError> {
        let actual_pos = self
            .rdata_pos
            .checked_add(pos)
            .ok_or(SessionError::ReadOutOfBounds {
                fd: self.fd,
                pos: usize::MAX,
                size: self.rdata_size,
            })?;
        let end = actual_pos
            .checked_add(4)
            .ok_or(SessionError::ReadOutOfBounds {
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

    /// Get the available read data as a byte slice.
    /// Returns `&rdata[rdata_pos..rdata_size]`.
    pub fn rdata_bytes(&self) -> &[u8] {
        &self.rdata[self.rdata_pos..self.rdata_size]
    }

    /// Write u8 with automatic buffer growth
    pub fn write_u8(&mut self, pos: usize, val: u8) -> Result<(), SessionError> {
        let actual_pos =
            self.wdata_size
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

        if end > self.wdata.len() {
            self.wdata
                .resize(end.saturating_add(1024).min(MAX_WDATA_SIZE), 0);
        }

        self.wdata[actual_pos] = val;
        Ok(())
    }

    /// Write u16 (little-endian) with automatic buffer growth
    pub fn write_u16(&mut self, pos: usize, val: u16) -> Result<(), SessionError> {
        let actual_pos =
            self.wdata_size
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
            self.wdata
                .resize(end.saturating_add(1024).min(MAX_WDATA_SIZE), 0);
        }

        let bytes = val.to_le_bytes();
        self.wdata[actual_pos..actual_pos + 2].copy_from_slice(&bytes);

        Ok(())
    }

    /// Write u32 (little-endian) with automatic buffer growth
    pub fn write_u32(&mut self, pos: usize, val: u32) -> Result<(), SessionError> {
        let actual_pos =
            self.wdata_size
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
            self.wdata
                .resize(end.saturating_add(1024).min(MAX_WDATA_SIZE), 0);
        }

        let bytes = val.to_le_bytes();
        self.wdata[actual_pos..actual_pos + 4].copy_from_slice(&bytes);

        Ok(())
    }

    /// Commit write buffer (like WFIFOSET)
    pub fn commit_write(&mut self, len: usize) -> Result<(), SessionError> {
        let new_size =
            self.wdata_size
                .checked_add(len)
                .ok_or(SessionError::WriteBufferTooLarge {
                    fd: self.fd,
                    requested_pos: usize::MAX,
                    max: MAX_WDATA_SIZE,
                })?;

        if new_size > MAX_WDATA_SIZE {
            return Err(SessionError::WriteBufferTooLarge {
                fd: self.fd,
                requested_pos: new_size,
                max: MAX_WDATA_SIZE,
            });
        }

        let available = self.wdata.len().saturating_sub(self.wdata_size);
        if new_size > self.wdata.len() {
            return Err(SessionError::WriteCommitTooLarge {
                fd: self.fd,
                requested: len,
                available,
            });
        }

        self.wdata_size = new_size;
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
        let actual_pos = self
            .rdata_pos
            .checked_add(pos)
            .ok_or(SessionError::ReadOutOfBounds {
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
        let actual_pos =
            self.wdata_size
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

        if end > self.wdata.len() {
            self.wdata
                .resize(end.saturating_add(1024).min(MAX_WDATA_SIZE), 0);
        }

        Ok(self.wdata.as_mut_ptr().wrapping_add(actual_pos))
    }

    /// Ensure write buffer has room for `size` bytes (like WFIFOHEAD)
    pub fn ensure_wdata_capacity(&mut self, size: usize) -> Result<(), SessionError> {
        let needed =
            self.wdata_size
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
            self.wdata
                .resize(needed.saturating_add(1024).min(MAX_WDATA_SIZE), 0);
        }

        Ok(())
    }

    /// Copy data from read buffer into a destination buffer (safe RFIFOP + memcpy)
    pub fn read_buf(&self, pos: usize, dst: &mut [u8]) -> Result<(), SessionError> {
        let actual_pos = self
            .rdata_pos
            .checked_add(pos)
            .ok_or(SessionError::ReadOutOfBounds {
                fd: self.fd,
                pos: usize::MAX,
                size: self.rdata_size,
            })?;
        let end = actual_pos
            .checked_add(dst.len())
            .ok_or(SessionError::ReadOutOfBounds {
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
        let actual_pos =
            self.wdata_size
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
            self.wdata
                .resize(end.saturating_add(1024).min(MAX_WDATA_SIZE), 0);
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

// SAFETY: session_data is a raw pointer used for polymorphic session storage.
// It is only accessed while the Session mutex is held. The pointer itself is
// Send/Sync safe because it's just an address. All other fields are already Send/Sync.
// TODO: Replace session_data with a typed enum to eliminate this unsafe impl.
unsafe impl Send for Session {}
unsafe impl Sync for Session {}
