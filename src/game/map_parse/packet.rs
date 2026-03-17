//! Inline FIFO helpers for packet I/O.


use crate::session::{
    session_exists, session_increment,
    get_session_manager, SessionId,
};

// ─── Send-target constants (enum in map_parse.h) ─────────────────────────────
// Re-exported from crate::common::constants::network for callers that import
// these via `super::packet::*` or named imports from this module.

pub use crate::common::constants::network::{
    ALL_CLIENT, SAMESRV, SAMEMAP, SAMEMAP_WOS,
    AREA, AREA_WOS, SAMEAREA, SAMEAREA_WOS,
    CORNER, SELF,
};
// Note: AREA_WOC not present in C source (map_parse.h enum ends at SELF = 9)

// ─── Byte-order helpers ───────────────────────────────────────────────────────

#[inline]
pub fn swap16(x: u16) -> u16 { x.swap_bytes() }

#[inline]
pub fn swap32(x: u32) -> u32 { x.swap_bytes() }

// ─── Recv-buffer (RFIFO) helpers ─────────────────────────────────────────────

/// Read one byte from the session recv buffer at `pos`. Mirrors `RFIFOB`.
#[inline]
pub fn rfifob(fd: SessionId, pos: usize) -> u8 {
    if let Some(s) = get_session_manager().get_session(fd) {
        if let Ok(session) = s.try_lock() {
            let bytes = session.rdata_bytes();
            if pos < bytes.len() { return bytes[pos]; }
        }
    }
    0
}

/// Read two bytes (little-endian u16) from recv buffer at `pos`. Mirrors `RFIFOW`.
#[inline]
pub fn rfifow(fd: SessionId, pos: usize) -> u16 {
    if let Some(s) = get_session_manager().get_session(fd) {
        if let Ok(session) = s.try_lock() {
            let bytes = session.rdata_bytes();
            if pos + 1 < bytes.len() {
                return u16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
            }
        }
    }
    0
}

/// Read four bytes (little-endian u32) from recv buffer at `pos`. Mirrors `RFIFOL`.
#[inline]
pub fn rfifol(fd: SessionId, pos: usize) -> u32 {
    if let Some(s) = get_session_manager().get_session(fd) {
        if let Ok(session) = s.try_lock() {
            let bytes = session.rdata_bytes();
            if pos + 3 < bytes.len() {
                return u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
            }
        }
    }
    0
}

/// Raw pointer into recv buffer at `pos`. Mirrors `RFIFOP`.
///
/// # Safety
/// The returned pointer is only valid while no other code modifies the session.
#[inline]
pub unsafe fn rfifop(fd: SessionId, pos: usize) -> *const u8 {
    if let Some(s) = get_session_manager().get_session(fd) {
        if let Ok(session) = s.try_lock() {
            let bytes = session.rdata_bytes();
            if pos < bytes.len() { return bytes.as_ptr().add(pos); }
        }
    }
    std::ptr::null()
}

/// Number of unprocessed bytes in recv buffer. Mirrors `RFIFOREST`.
#[inline]
pub fn rfiforest(fd: SessionId) -> i32 {
    if let Some(s) = get_session_manager().get_session(fd) {
        if let Ok(session) = s.try_lock() {
            return session.available() as i32;
        }
    }
    0
}

/// Consume `len` bytes from recv buffer. Mirrors `RFIFOSKIP`.
#[inline]
pub fn rfifoskip(fd: SessionId, len: usize) {
    if let Some(s) = get_session_manager().get_session(fd) {
        if let Ok(mut session) = s.try_lock() {
            if let Err(e) = session.skip(len) {
                tracing::error!("[session] skip error: {}", e);
            }
        }
    }
}

// ─── Send-buffer (WFIFO) helpers ─────────────────────────────────────────────

/// Reserve `size` bytes in send buffer. Mirrors `WFIFOHEAD`.
#[inline]
pub fn wfifohead(fd: SessionId, size: usize) {
    if let Some(s) = get_session_manager().get_session(fd) {
        if let Ok(mut session) = s.try_lock() {
            if let Err(e) = session.ensure_wdata_capacity(size) {
                tracing::error!("[session] wfifohead error: {}", e);
            }
        }
    }
}

/// Write one byte to send buffer at `pos`. Mirrors `WFIFOB(fd, pos) = val`.
#[inline]
pub fn wfifob(fd: SessionId, pos: usize, val: u8) {
    if let Some(s) = get_session_manager().get_session(fd) {
        if let Ok(mut session) = s.try_lock() {
            if let Err(e) = session.write_u8(pos, val) {
                tracing::error!("[session] wfifob error: {}", e);
            }
        }
    }
}

/// Write two bytes (little-endian) to send buffer at `pos`. Mirrors `WFIFOW(fd, pos) = val`.
#[inline]
pub fn wfifow(fd: SessionId, pos: usize, val: u16) {
    if let Some(s) = get_session_manager().get_session(fd) {
        if let Ok(mut session) = s.try_lock() {
            let actual = session.wdata_size + pos;
            if actual + 1 < session.wdata.len() {
                let bytes = val.to_le_bytes();
                session.wdata[actual] = bytes[0];
                session.wdata[actual + 1] = bytes[1];
            }
        }
    }
}

/// Write four bytes (little-endian) to send buffer at `pos`. Mirrors `WFIFOL(fd, pos) = val`.
#[inline]
pub fn wfifol(fd: SessionId, pos: usize, val: u32) {
    if let Some(s) = get_session_manager().get_session(fd) {
        if let Ok(mut session) = s.try_lock() {
            let actual = session.wdata_size + pos;
            if actual + 3 < session.wdata.len() {
                let bytes = val.to_le_bytes();
                session.wdata[actual] = bytes[0];
                session.wdata[actual + 1] = bytes[1];
                session.wdata[actual + 2] = bytes[2];
                session.wdata[actual + 3] = bytes[3];
            }
        }
    }
}

/// Raw mutable pointer into send buffer at `pos`. Mirrors `WFIFOP`.
///
/// # Safety
/// The returned pointer is only valid while no other code modifies the session.
#[inline]
pub unsafe fn wfifop(fd: SessionId, pos: usize) -> *mut u8 {
    if let Some(s) = get_session_manager().get_session(fd) {
        if let Ok(mut session) = s.try_lock() {
            match session.wdata_ptr(pos) {
                Ok(p) => return p,
                Err(e) => { tracing::error!("[session] wfifop error: {}", e); }
            }
        }
    }
    std::ptr::null_mut()
}

/// Safe slice view into the write-FIFO buffer at `offset` for `len` bytes.
/// Returns `None` if the session is gone or wfifop returns null.
///
/// # Safety
/// The returned slice is only valid while no other code modifies the session.
/// Caller must hold exclusive access (no concurrent wfifo calls on this fd).
#[inline]
pub unsafe fn wfifo_slice(fd: SessionId, offset: usize, len: usize) -> Option<&'static mut [u8]> {
    let ptr = wfifop(fd, offset);
    if ptr.is_null() {
        return None;
    }
    Some(std::slice::from_raw_parts_mut(ptr, len))
}

/// Commit `size` bytes from send buffer to the wire. Mirrors `WFIFOSET`.
#[inline]
pub fn wfifoset(fd: SessionId, size: usize) {
    if let Some(s) = get_session_manager().get_session(fd) {
        if let Ok(mut session) = s.try_lock() {
            if let Err(e) = session.commit_write(size) {
                tracing::error!("[session] wfifoset error: {}", e);
            }
        }
    }
}

/// Write the standard 5-byte packet header and reserve `packet_size` additional bytes.
/// Header layout:
///   [0]   = 0xAA magic
///   [1..2] = packet_size as big-endian u16
///   [3]   = packet_id
///   [4]   = per-session increment byte
#[inline]
pub fn wfifoheader(fd: SessionId, packet_id: u8, packet_size: u16) {
    debug_assert!(packet_size >= 2, "wfifoheader: packet_size must be >= 2 (header needs 5 bytes, reserves packet_size+3)");
    if !session_exists(fd) { return; }
    wfifohead(fd, packet_size as usize + 3);
    wfifob(fd, 0, 0xAA);
    // packet_size stored big-endian
    if let Some(s) = get_session_manager().get_session(fd) {
        if let Ok(mut session) = s.try_lock() {
            let actual = session.wdata_size + 1;
            if actual + 1 < session.wdata.len() {
                let bytes = packet_size.to_be_bytes();
                session.wdata[actual] = bytes[0];
                session.wdata[actual + 1] = bytes[1];
            }
        }
    }
    wfifob(fd, 3, packet_id);
    wfifob(fd, 4, session_increment(fd));
}


pub use crate::network::crypt::{decrypt, encrypt};
pub use crate::game::client::{clif_send, clif_sendtogm};

