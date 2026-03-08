// FFI bridge: C game logic (map_parse.c, pc.c, scripting.c) calls these when it
// needs to send packets to char_server. The Rust map_server binary sets MAP_STATE
// after startup via set_map_state(). Before that, calls are silently dropped.

use std::ffi::c_char;
use std::os::raw::c_int;
use std::sync::{Arc, OnceLock};
use tokio::runtime::Handle;
use crate::servers::map::{MapState, packet};

static MAP_STATE: OnceLock<Arc<MapState>> = OnceLock::new();

// Function pointer set by map_server.rs at startup so packet.rs can call
// intif_mmo_tosd without the library depending on libmap_game.a.
static MMO_TOSD_FN: OnceLock<unsafe extern "C" fn(i32, *mut u8) -> i32> = OnceLock::new();

/// Called by map_server.rs main() to register the intif_mmo_tosd C function.
pub fn set_mmo_tosd_fn(f: unsafe extern "C" fn(i32, *mut u8) -> i32) {
    let _ = MMO_TOSD_FN.set(f);
}

/// Call C intif_mmo_tosd with a raw mmo_charstatus buffer.
/// No-ops if the function was not registered (non-map_server binaries).
pub fn call_intif_mmo_tosd(fd: i32, raw: &mut Vec<u8>) -> i32 {
    if let Some(f) = MMO_TOSD_FN.get() {
        unsafe { f(fd, raw.as_mut_ptr()) }
    } else {
        0
    }
}

/// Called by map_server.rs main() after MapState is constructed.
pub fn set_map_state(state: Arc<MapState>) {
    let _ = MAP_STATE.set(state);
}

/// Send raw bytes to char_server via the Rust channel.
fn send(data: Vec<u8>) {
    if let Some(state) = MAP_STATE.get() {
        let s = Arc::clone(state);
        if let Ok(handle) = Handle::try_current() {
            handle.spawn(async move { packet::send_to_char(&s, data).await; });
        }
    }
}

/// 0x3003 — Request char data (map→char, 24 bytes).
/// C: intif_load(fd, id, name) — replaces WFIFOW/WFIFOSET dance.
///
/// Layout:
///   [0..2]  = 0x3003 cmd (LE)
///   [2..4]  = session_fd (u16 LE)
///   [4..8]  = char_id (u32 LE)
///   [8..24] = char_name (16 bytes, null-padded)
#[no_mangle]
pub unsafe extern "C" fn rust_intif_load(fd: i32, char_id: u32, name: *const c_char) {
    if name.is_null() { return; }
    let nb = std::ffi::CStr::from_ptr(name).to_bytes();
    let mut pkt = vec![0u8; 24];
    pkt[0] = 0x03; pkt[1] = 0x30; // 0x3003 LE
    pkt[2..4].copy_from_slice(&(fd as u16).to_le_bytes());
    pkt[4..8].copy_from_slice(&char_id.to_le_bytes());
    pkt[8..8 + nb.len().min(16)].copy_from_slice(&nb[..nb.len().min(16)]);
    send(pkt);
}

/// 0x3005 — Logout notification (map→char, 6 bytes).
/// C: intif_quit(sd) — replaces WFIFOW/WFIFOSET dance.
///
/// Layout:
///   [0..2] = 0x3005 cmd (LE)
///   [2..6] = char_id (u32 LE)
#[no_mangle]
pub unsafe extern "C" fn rust_intif_quit(char_id: u32) {
    let mut pkt = vec![0u8; 6];
    pkt[0] = 0x05; pkt[1] = 0x30; // 0x3005 LE
    pkt[2..6].copy_from_slice(&char_id.to_le_bytes());
    send(pkt);
}

/// 0x3004 — Save char (map→char, variable — zlib-compressed mmo_charstatus).
/// C: intif_save(sd) — C already does zlib compress2; passes raw packet bytes here.
///
/// Layout:
///   [0..2] = 0x3004 cmd (LE)
///   [2..6] = total_len (u32 LE)
///   [6..]  = zlib-compressed mmo_charstatus
///
/// `data` points to the already-built packet buffer; `len` is total_len.
#[no_mangle]
pub unsafe extern "C" fn rust_intif_save(data: *const u8, len: u32) {
    if data.is_null() || len < 6 { return; }
    let pkt = std::slice::from_raw_parts(data, len as usize).to_vec();
    send(pkt);
}

/// 0x3007 — Save-and-quit (map→char, variable — zlib-compressed mmo_charstatus).
/// C: intif_savequit(sd) — same pattern as rust_intif_save.
///
/// Layout:
///   [0..2] = 0x3007 cmd (LE)
///   [2..6] = total_len (u32 LE)
///   [6..]  = zlib-compressed mmo_charstatus
#[no_mangle]
pub unsafe extern "C" fn rust_intif_savequit(data: *const u8, len: u32) {
    if data.is_null() || len < 6 { return; }
    let pkt = std::slice::from_raw_parts(data, len as usize).to_vec();
    send(pkt);
}

// ─── Rust ports of intif_save / intif_savequit C static-inlines ──────────────
//
// These replace the C trampolines in c_src/rust_shims_map.c.
// They replicate the logic from the static-inline intif_save / intif_savequit
// in c_src/map_char.h:
//   1. Update sd->status positional/cosmetic fields from the live session state.
//   2. zlib-compress sd->status (the mmo_charstatus blob).
//   3. Prepend a 6-byte packet header (cmd LE + u32 total_len LE).
//   4. Forward the raw packet bytes to rust_intif_save / rust_intif_savequit.
//
// These symbols must be behind #[cfg(feature = "map-game")] because they
// reference MapSessionData which only exists in that feature's compilation unit.
#[cfg(feature = "map-game")]
mod intif_save_impl {
    use super::*;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write as _;
    use crate::game::pc::MapSessionData;
    use crate::game::block::map_is_loaded;

    /// Helper: compress `sd->status` with zlib level-1 and build a packet.
    /// `cmd` is the 2-byte little-endian command word (0x3004 or 0x3007).
    /// Returns the packet bytes on success, None on failure.
    unsafe fn compress_status(sd: *mut MapSessionData, cmd: u16) -> Option<Vec<u8>> {
        if sd.is_null() { return None; }
        // Raw bytes of the mmo_charstatus POD struct
        let status_ptr = &(*sd).status as *const _ as *const u8;
        let status_len = std::mem::size_of_val(&(*sd).status);
        let raw = std::slice::from_raw_parts(status_ptr, status_len);

        let mut enc = ZlibEncoder::new(Vec::new(), Compression::fast());
        enc.write_all(raw).ok()?;
        let compressed = enc.finish().ok()?;

        let total: u32 = (6 + compressed.len()) as u32;
        let mut pkt = Vec::with_capacity(total as usize);
        // 2-byte command (LE)
        pkt.push((cmd & 0xff) as u8);
        pkt.push((cmd >> 8) as u8);
        // 4-byte total length (LE)
        pkt.extend_from_slice(&total.to_le_bytes());
        pkt.extend_from_slice(&compressed);
        Some(pkt)
    }

    /// `int sl_intif_save(void *sd)` — Rust replacement for the C shim.
    ///
    /// Mirrors `intif_save` in `c_src/map_char.h`:
    ///   sets last_pos from bl, copies disguise fields, compresses, sends 0x3004.
    #[no_mangle]
    pub unsafe extern "C" fn rust_sl_intif_save(sd: *mut std::ffi::c_void) -> c_int {
        let sd = sd as *mut MapSessionData;
        if sd.is_null() { return -1; }
        (*sd).status.last_pos.m = (*sd).bl.m;
        (*sd).status.last_pos.x = (*sd).bl.x;
        (*sd).status.last_pos.y = (*sd).bl.y;
        (*sd).status.disguise       = (*sd).disguise;
        (*sd).status.disguise_color = (*sd).disguise_color;
        match compress_status(sd, 0x3004) {
            Some(pkt) => { rust_intif_save(pkt.as_ptr(), pkt.len() as u32); 0 }
            None      => -1,
        }
    }

    /// `int sl_intif_savequit(void *sd)` — Rust replacement for the C shim.
    ///
    /// Mirrors `intif_savequit` in `c_src/map_char.h`:
    ///   updates last_pos (preferring dest_pos if it's on an unloaded map),
    ///   copies disguise fields, compresses, sends 0x3007.
    #[no_mangle]
    pub unsafe extern "C" fn rust_sl_intif_savequit(sd: *mut std::ffi::c_void) -> c_int {
        let sd = sd as *mut MapSessionData;
        if sd.is_null() { return -1; }
        if !map_is_loaded((*sd).status.dest_pos.m as i32) {
            if (*sd).status.dest_pos.m == 0 {
                (*sd).status.dest_pos.m = (*sd).bl.m;
                (*sd).status.dest_pos.x = (*sd).bl.x;
                (*sd).status.dest_pos.y = (*sd).bl.y;
            }
            (*sd).status.last_pos.m = (*sd).status.dest_pos.m;
            (*sd).status.last_pos.x = (*sd).status.dest_pos.x;
            (*sd).status.last_pos.y = (*sd).status.dest_pos.y;
        } else {
            (*sd).status.last_pos.m = (*sd).bl.m;
            (*sd).status.last_pos.x = (*sd).bl.x;
            (*sd).status.last_pos.y = (*sd).bl.y;
        }
        (*sd).status.disguise       = (*sd).disguise;
        (*sd).status.disguise_color = (*sd).disguise_color;
        match compress_status(sd, 0x3007) {
            Some(pkt) => { rust_intif_savequit(pkt.as_ptr(), pkt.len() as u32); 0 }
            None      => -1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intif_load_packet_layout() {
        // Verify the packet we'd build for intif_load
        let mut pkt = vec![0u8; 24];
        pkt[0] = 0x03; pkt[1] = 0x30;
        pkt[2..4].copy_from_slice(&(42u16).to_le_bytes()); // fd=42
        pkt[4..8].copy_from_slice(&(999u32).to_le_bytes()); // char_id=999
        pkt[8..14].copy_from_slice(b"Yuria\0");
        assert_eq!(u16::from_le_bytes([pkt[0], pkt[1]]), 0x3003);
        assert_eq!(u16::from_le_bytes([pkt[2], pkt[3]]), 42);
        assert_eq!(u32::from_le_bytes([pkt[4], pkt[5], pkt[6], pkt[7]]), 999);
        assert_eq!(&pkt[8..13], b"Yuria");
        assert_eq!(pkt.len(), 24);
    }

    #[test]
    fn test_intif_quit_packet_layout() {
        let mut pkt = vec![0u8; 6];
        pkt[0] = 0x05; pkt[1] = 0x30;
        pkt[2..6].copy_from_slice(&(12345u32).to_le_bytes());
        assert_eq!(u16::from_le_bytes([pkt[0], pkt[1]]), 0x3005);
        assert_eq!(u32::from_le_bytes([pkt[2], pkt[3], pkt[4], pkt[5]]), 12345);
    }
}
