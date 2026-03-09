//! Safe inline helpers mirroring the C FIFO macros from `c_src/session.h`,
//! plus extern "C" declarations for C functions that remain unported.

use std::os::raw::c_int;

use crate::session::{
    rust_session_available, rust_session_commit, rust_session_increment,
    rust_session_rdata_ptr, rust_session_skip, rust_session_wdata_ptr,
    rust_session_wfifohead, rust_session_exists,
};

// ─── Send-target constants (enum in map_parse.h) ─────────────────────────────

pub const ALL_CLIENT: c_int  = 0;
pub const SAMESRV: c_int     = 1;
pub const SAMEMAP: c_int     = 2;
pub const SAMEMAP_WOS: c_int = 3;
pub const AREA: c_int        = 4;
pub const AREA_WOS: c_int    = 5;
pub const SAMEAREA: c_int    = 6;
pub const SAMEAREA_WOS: c_int = 7;
pub const CORNER: c_int      = 8;
pub const SELF: c_int        = 9;
// Note: AREA_WOC not present in C source (map_parse.h enum ends at SELF = 9)

// ─── Byte-order helpers ───────────────────────────────────────────────────────

#[inline]
pub fn swap16(x: u16) -> u16 { x.swap_bytes() }

#[inline]
pub fn swap32(x: u32) -> u32 { x.swap_bytes() }

// ─── Recv-buffer (RFIFO) helpers ─────────────────────────────────────────────

/// Read one byte from the session recv buffer at `pos`. Mirrors `RFIFOB`.
#[inline]
pub unsafe fn rfifob(fd: c_int, pos: usize) -> u8 {
    let p = rust_session_rdata_ptr(fd, pos);
    if p.is_null() { 0 } else { *p }
}

/// Read two bytes (little-endian u16) from recv buffer at `pos`. Mirrors `RFIFOW`.
#[inline]
pub unsafe fn rfifow(fd: c_int, pos: usize) -> u16 {
    let p = rust_session_rdata_ptr(fd, pos);
    if p.is_null() { return 0; }
    u16::from_le_bytes([*p, *p.add(1)])
}

/// Read four bytes (little-endian u32) from recv buffer at `pos`. Mirrors `RFIFOL`.
#[inline]
pub unsafe fn rfifol(fd: c_int, pos: usize) -> u32 {
    let p = rust_session_rdata_ptr(fd, pos);
    if p.is_null() { return 0; }
    u32::from_le_bytes([*p, *p.add(1), *p.add(2), *p.add(3)])
}

/// Raw pointer into recv buffer at `pos`. Mirrors `RFIFOP`.
#[inline]
pub unsafe fn rfifop(fd: c_int, pos: usize) -> *const u8 {
    rust_session_rdata_ptr(fd, pos)
}

/// Number of unprocessed bytes in recv buffer. Mirrors `RFIFOREST`.
#[inline]
pub unsafe fn rfiforest(fd: c_int) -> c_int {
    rust_session_available(fd) as c_int
}

/// Consume `len` bytes from recv buffer. Mirrors `RFIFOSKIP`.
#[inline]
pub unsafe fn rfifoskip(fd: c_int, len: usize) {
    rust_session_skip(fd, len);
}

// ─── Send-buffer (WFIFO) helpers ─────────────────────────────────────────────

/// Reserve `size` bytes in send buffer. Mirrors `WFIFOHEAD`.
#[inline]
pub unsafe fn wfifohead(fd: c_int, size: usize) {
    rust_session_wfifohead(fd, size);
}

/// Write one byte to send buffer at `pos`. Mirrors `WFIFOB(fd, pos) = val`.
#[inline]
pub unsafe fn wfifob(fd: c_int, pos: usize, val: u8) {
    let p = rust_session_wdata_ptr(fd, pos);
    if !p.is_null() { *p = val; }
}

/// Write two bytes (little-endian) to send buffer at `pos`. Mirrors `WFIFOW(fd, pos) = val`.
#[inline]
pub unsafe fn wfifow(fd: c_int, pos: usize, val: u16) {
    let p = rust_session_wdata_ptr(fd, pos) as *mut u16;
    if !p.is_null() { p.write_unaligned(val.to_le()); }
}

/// Write four bytes (little-endian) to send buffer at `pos`. Mirrors `WFIFOL(fd, pos) = val`.
#[inline]
pub unsafe fn wfifol(fd: c_int, pos: usize, val: u32) {
    let p = rust_session_wdata_ptr(fd, pos) as *mut u32;
    if !p.is_null() { p.write_unaligned(val.to_le()); }
}

/// Commit `size` bytes from send buffer to the wire. Mirrors `WFIFOSET`.
#[inline]
pub unsafe fn wfifoset(fd: c_int, size: usize) {
    rust_session_commit(fd, size);
}

/// Write the standard 5-byte packet header and reserve `packet_size` additional bytes.
/// Header layout (from `WFIFOHEADER` in session.h):
///   [0]   = 0xAA magic
///   [1..2] = packet_size as big-endian u16
///   [3]   = packet_id
///   [4]   = per-session increment byte
/// Mirrors the C `static inline int WFIFOHEADER(fd, packetID, packetSize)`.
#[inline]
pub unsafe fn wfifoheader(fd: c_int, packet_id: u8, packet_size: u16) {
    debug_assert!(packet_size >= 2, "wfifoheader: packet_size must be >= 2 (header needs 5 bytes, reserves packet_size+3)");
    if rust_session_exists(fd) == 0 { return; }
    rust_session_wfifohead(fd, packet_size as usize + 3);
    wfifob(fd, 0, 0xAA);
    // packet_size stored big-endian (SWAP16 in C)
    let p = rust_session_wdata_ptr(fd, 1) as *mut u16;
    if !p.is_null() { p.write_unaligned(packet_size.to_be()); }
    wfifob(fd, 3, packet_id);
    wfifob(fd, 4, rust_session_increment(fd));
}

// ─── Direct Rust imports (replacing extern "C" declarations) ─────────────────

pub use crate::network::crypt::{decrypt, encrypt};
pub use crate::game::client::{clif_send, clif_sendtogm};

