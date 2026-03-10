use md5::{Digest, Md5};

/// Opcodes that use key1 (static XOR) on the client side.
const CL_KEY1_PACKETS: &[u8] = &[2, 3, 4, 11, 21, 38, 58, 66, 67, 75, 80, 87, 98, 113, 115, 123];

/// Opcodes that use key1 (static XOR) on the server side.
const SV_KEY1_PACKETS: &[u8] = &[2, 3, 10, 64, 68, 94, 96, 98, 102, 111];

/// Returns true if the opcode should use dynamic encryption (client-bound check).
pub fn is_key_client(opcode: u8) -> bool {
    !CL_KEY1_PACKETS.contains(&opcode)
}

/// Returns true if the opcode should use dynamic encryption (server-bound check).
pub fn is_key_server(opcode: u8) -> bool {
    !SV_KEY1_PACKETS.contains(&opcode)
}

/// Computes a single MD5 hex digest of `input` into `out[..32]`.
/// `out` must be at least 33 bytes. Returns false if the buffer is too short.
pub fn generate_hashvalues(input: &[u8], out: &mut [u8]) -> bool {
    if out.len() < 33 {
        return false;
    }
    let mut hasher = Md5::new();
    hasher.update(input);
    let digest = hasher.finalize();
    for (i, byte) in digest.iter().enumerate() {
        let hex = format!("{:02x}", byte);
        out[i * 2] = hex.as_bytes()[0];
        out[i * 2 + 1] = hex.as_bytes()[1];
    }
    out[32] = 0;
    true
}

/// Public generate_hash used elsewhere (e.g. password hashing).
pub fn generate_hash(name: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(name);
    hex::encode(hasher.finalize())
}

/// Builds a 1025-byte encryption lookup table from a name string.
///
/// Produces exactly 1024 usable bytes in 31 iterations (32 + 31×32 = 1024),
/// fitting safely in 1025 bytes. `generate_key2` only accesses indices masked
/// by `& 0x3FF` (0..1023).
pub fn populate_table(name: &[u8], table: &mut [u8]) -> bool {
    if table.len() < 0x401 {
        return false;
    }
    let mut hash = [0u8; 64];
    // Double-hash the name (two consecutive generate_hashvalues calls).
    if !generate_hashvalues(name, &mut hash) {
        return false;
    }
    let h1 = hash[..32].to_vec();
    if !generate_hashvalues(&h1, &mut hash) {
        return false;
    }
    table[..32].copy_from_slice(&hash[..32]);
    let mut current_len = 32usize;

    // 31 iterations fills indices 0..1023 — all that generate_key2 (& 0x3FF) can reach.
    for _ in 0..31 {
        let prev = table[..current_len].to_vec();
        if !generate_hashvalues(&prev, &mut hash) {
            return false;
        }
        table[current_len..current_len + 32].copy_from_slice(&hash[..32]);
        current_len += 32;
    }
    table[current_len] = 0; // null-terminate at byte 1024
    true
}

/// Appends 3 index bytes to the packet and updates the length field.
/// `packet` must be the full write buffer starting at byte 0 (opcode byte).
///
/// Packet layout (bytes):
///   [0] opcode  [1..2] big-endian payload len  [3] packet-id  [4] inc  [5..] data
///
/// After this call:
///   [psize+0] = k2_lo  [psize+1] = k1  [psize+2] = k2_hi
///   [1..2] updated to new big-endian length
pub fn set_packet_indexes(packet: &mut [u8]) -> usize {
    // USE_RANDOM_INDEXES is defined — use fixed deterministic values.
    let k1: u8 = (0x1337usize & 0x7FFF % 0x9B + 0x64) as u8 ^ 0x21;
    let k2: u16 = ((0x1337usize & 0x7FFF) as u16 + 0x100) ^ 0x7424;

    let psize = ((packet[1] as usize) << 8) | (packet[2] as usize);
    let psize = psize + 3;

    packet[psize] = (k2 & 0xFF) as u8;
    packet[psize + 1] = k1;
    packet[psize + 2] = ((k2 >> 8) & 0xFF) as u8;
    packet[1] = ((psize >> 8) & 0xFF) as u8;
    packet[2] = (psize & 0xFF) as u8;

    psize + 3
}

/// Derives a 9-byte session key from the packet trailer and the lookup table.
pub fn generate_key2(packet: &[u8], table: &[u8], keyout: &mut [u8; 10], fromclient: bool) {
    let psize = ((packet[1] as usize) << 8) | (packet[2] as usize);
    let mut k1 = packet[psize + 1] as u32;
    let mut k2 = ((packet[psize + 2] as u32) << 8) | (packet[psize] as u32);

    if fromclient {
        k1 ^= 0x25;
        k2 ^= 0x2361;
    } else {
        k1 ^= 0x21;
        k2 ^= 0x7424;
    }

    k1 *= k1;

    for i in 0..9usize {
        keyout[i] = table[((k1 * i as u32 + k2) & 0x3FF) as usize];
        k1 = k1.wrapping_add(3);
    }
    keyout[9] = 0;
}

/// XOR-encrypts/decrypts packet data in-place using a 9-byte key.
///
/// Packet layout: [0] opcode [1..2] big-endian total len [3] inc [4] packetInc [5..] data
pub fn tk_crypt_dynamic(buff: &mut [u8], key: &[u8]) {
    if buff.len() < 5 || key.is_empty() {
        return;
    }
    // Pad key to 9 bytes (null-terminated, matching the expected key array layout).
    let mut k9 = [0u8; 9];
    k9[..key.len().min(9)].copy_from_slice(&key[..key.len().min(9)]);
    let key = &k9;

    let packet_len = (((buff[1] as u32) << 8) | (buff[2] as u32)).saturating_sub(5) as usize;
    let packet_inc = buff[4];

    if packet_len > 65535 || buff.len() < 5 + packet_len {
        return;
    }

    let data = &mut buff[5..5 + packet_len];
    let mut group: u32 = 0;
    let mut group_count: u32 = 0;

    for i in 0..packet_len {
        data[i] ^= key[i % 9];
        let key_val = (group % 256) as u8;
        if key_val != packet_inc {
            data[i] ^= key_val;
        }
        data[i] ^= packet_inc;

        group_count += 1;
        if group_count == 9 {
            group += 1;
            group_count = 0;
        }
    }
}

/// XOR-encrypts/decrypts packet data using the static `xor_key` from config.
pub fn tk_crypt_static(buff: &mut [u8], xor_key: &[u8]) {
    tk_crypt_dynamic(buff, xor_key);
}

/// Encrypts the pending write buffer for `fd` and returns the total byte count to commit.
///
/// Algorithm:
/// 1. Read the original payload length from `wbuf[1..2]` (big-endian u16).
/// 2. Append 3 index bytes via `set_packet_indexes` (updates `wbuf[1..2]` to new length).
/// 3. Apply dynamic or static XOR encryption.
/// 4. Return `new_payload_len + 3` = total bytes to commit (including the 3-byte framing header).
///
/// # Safety
/// `fd` must be a valid session fd with pending write data staged by `wfifohead`.
pub unsafe fn encrypt(fd: SessionId) -> i32 {
    use crate::config::config;
    use crate::session::{session_get_data, get_session_manager};

    let sd = session_get_data(fd);
    if sd.is_null() {
        tracing::error!("[encrypt] sd is NULL for fd={}", fd);
        return 1;
    }

    if let Some(s) = get_session_manager().get_session(fd) {
        if let Ok(mut session) = s.try_lock() {
            let wpos = session.wdata_size;
            if wpos >= session.wdata.len() {
                tracing::error!("[encrypt] write buffer empty for fd={}", fd);
                return 1;
            }

            // Original payload length from packet header bytes 1–2 (big-endian).
            // After set_packet_indexes the header is updated; total slice = original + 6
            // (3-byte framing header + 3 index bytes appended by set_packet_indexes).
            let original_len = u16::from_be_bytes([session.wdata[wpos + 1], session.wdata[wpos + 2]]) as usize;
            let total_size = original_len + 6;
            let buf_slice = &mut session.wdata[wpos..wpos + total_size];

            set_packet_indexes(buf_slice);

            if is_key_server(buf_slice[3]) {
                let enc_hash = std::slice::from_raw_parts(
                    (*sd).EncHash.as_ptr() as *const u8,
                    (*sd).EncHash.len(),
                );
                let mut key = [0u8; 10];
                generate_key2(buf_slice, enc_hash, &mut key, false);
                tk_crypt_dynamic(buf_slice, &key);
            } else {
                tk_crypt_static(buf_slice, config().xor_key.as_bytes());
            }

            return u16::from_be_bytes([buf_slice[1], buf_slice[2]]) as i32 + 3;
        }
    }

    tracing::error!("[encrypt] session not found for fd={}", fd);
    1
}

/// Decrypts the incoming read buffer for `fd` in-place.
///
/// # Safety
/// `fd` must be a valid session fd with a complete incoming packet in the read buffer.
/// The `*const u8 → *mut u8` cast for in-place XOR is safe here because
/// packet dispatch is single-threaded and no other thread aliases this buffer.
pub unsafe fn decrypt(fd: SessionId) -> i32 {
    use crate::config::config;
    use crate::session::{session_get_data, get_session_manager};

    let sd = session_get_data(fd);
    if sd.is_null() {
        return 1;
    }

    if let Some(s) = get_session_manager().get_session(fd) {
        if let Ok(mut session) = s.try_lock() {
            let pos = session.rdata_pos;
            let size = session.rdata_size;
            if pos >= size { return 0; }
            let buf_slice = &mut session.rdata[pos..size];

            let enc_hash = std::slice::from_raw_parts(
                (*sd).EncHash.as_ptr() as *const u8,
                (*sd).EncHash.len(),
            );

            if is_key_client(buf_slice[3]) {
                let mut key = [0u8; 10];
                generate_key2(buf_slice, enc_hash, &mut key, true);
                tk_crypt_dynamic(buf_slice, &key);
            } else {
                tk_crypt_static(buf_slice, config().xor_key.as_bytes());
            }
        }
    }

    0
}

// ─── Meta file packet senders ─────────────────────────────────────────────────

unsafe fn metacrc_path(path: &str) -> u32 {
    use flate2::Crc;
    let data = std::fs::read(path).unwrap_or_default();
    let mut crc = Crc::new();
    crc.update(&data);
    crc.sum()
}

unsafe fn send_metafile_impl(fd: SessionId, file: &str) {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;
    use crate::config::config;
    use crate::game::map_parse::packet::{wfifohead, wfifob, wfifop, wfifoset};

    let cfg = config();
    let path = format!("{}{}", cfg.meta_dir, file);

    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(_) => return,
    };

    let checksum = metacrc_path(&path);
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    let _ = enc.write_all(&data);
    let compressed = enc.finish().unwrap_or_default();
    let clen = compressed.len() as u16;

    let fname = file.as_bytes();
    let name_len = fname.len();
    let len = (name_len + 1) + 4 + 2 + compressed.len() + 1;
    let total = len + 9;
    wfifohead(fd, total);

    wfifob(fd, 0, 0xAA);
    let plen = (len + 3) as u16;
    wfifob(fd, 1, (plen >> 8) as u8);
    wfifob(fd, 2, (plen & 0xFF) as u8);
    wfifob(fd, 3, 0x6F);
    wfifob(fd, 4, 0);
    wfifob(fd, 5, 0);
    wfifob(fd, 6, name_len as u8);
    for (i, &b) in fname.iter().enumerate() {
        wfifob(fd, 7 + i, b);
    }
    wfifob(fd, 7 + name_len, 0);

    let mut off = 7 + name_len + 1;
    let cs = checksum.to_be_bytes();
    wfifob(fd, off, cs[0]); wfifob(fd, off+1, cs[1]); wfifob(fd, off+2, cs[2]); wfifob(fd, off+3, cs[3]);
    off += 4;
    let cl = clen.to_be_bytes();
    wfifob(fd, off, cl[0]); wfifob(fd, off+1, cl[1]);
    off += 2;
    let dst = wfifop(fd, off);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(compressed.as_ptr(), dst, compressed.len());
    }
    off += compressed.len();
    wfifob(fd, off, 0);

    wfifoset(fd, encrypt(fd) as usize);
}

/// Respond to a client meta-file request with the compressed file data.
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`crate::game::pc::MapSessionData`].
pub unsafe fn send_meta(sd: *mut crate::game::pc::MapSessionData) -> i32 {
    use crate::game::map_parse::packet::{rfifob};
    if sd.is_null() { return 0; }
    let fd = (*sd).fd;
    let name_len = rfifob(fd, 6) as usize;
    let mut buf = vec![0u8; name_len];
    for i in 0..name_len {
        buf[i] = rfifob(fd, 7 + i);
    }
    let file = String::from_utf8_lossy(&buf).into_owned();
    send_metafile_impl(fd, &file);
    0
}

/// Send the list of meta files and their CRC32 checksums to the client.
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`crate::game::pc::MapSessionData`].
pub unsafe fn send_metalist(sd: *mut crate::game::pc::MapSessionData) -> i32 {
    use flate2::Crc;
    use crate::config::config;
    use crate::game::map_parse::packet::{wfifohead, wfifob, wfifoset};

    if sd.is_null() { return 0; }
    let fd = (*sd).fd;
    let cfg = config();
    let files = &cfg.meta;
    let meta_dir = &cfg.meta_dir;

    let entry_bytes: usize = files.iter().map(|f| 1 + f.len() + 4).sum();
    let len = 2 + entry_bytes;
    let total = len + 10;
    wfifohead(fd, total);

    wfifob(fd, 0, 0xAA);
    wfifob(fd, 3, 0x6F);
    wfifob(fd, 4, 0);
    wfifob(fd, 5, 1); // subtype: list
    let count = files.len() as u16;
    wfifob(fd, 6, (count >> 8) as u8);
    wfifob(fd, 7, (count & 0xFF) as u8);

    let mut off = 8;
    for fname in files.iter() {
        let fbytes = fname.as_bytes();
        wfifob(fd, off, fbytes.len() as u8);
        off += 1;
        for (i, &b) in fbytes.iter().enumerate() {
            wfifob(fd, off + i, b);
        }
        off += fbytes.len();
        let path = format!("{}{}", meta_dir, fname);
        let mut crc = Crc::new();
        crc.update(&std::fs::read(&path).unwrap_or_default());
        let cs = crc.sum().to_be_bytes();
        wfifob(fd, off, cs[0]); wfifob(fd, off+1, cs[1]); wfifob(fd, off+2, cs[2]); wfifob(fd, off+3, cs[3]);
        off += 4;
    }

    let plen = (len + 4) as u16;
    wfifob(fd, 1, (plen >> 8) as u8);
    wfifob(fd, 2, (plen & 0xFF) as u8);

    wfifoset(fd, encrypt(fd) as usize);
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_key_server() {
        assert!(!is_key_server(2));   // in SV list → static
        assert!(is_key_server(99));   // not in list → dynamic
    }

    #[test]
    fn test_is_key_client() {
        assert!(!is_key_client(2));   // in CL list → static
        assert!(is_key_client(99));   // not in list → dynamic
    }

    #[test]
    fn test_generate_hashvalues() {
        let mut out = [0u8; 33];
        assert!(generate_hashvalues(b"hello", &mut out));
        // MD5("hello") = 5d41402abc4b2a76b9719d911017c592
        assert_eq!(&out[..32], b"5d41402abc4b2a76b9719d911017c592");
    }

    #[test]
    fn test_populate_table_length() {
        let mut table = vec![0u8; 0x401];
        assert!(populate_table(b"testkey", &mut table));
        // table should be non-empty hex chars
        assert!(table[..32].iter().all(|&b| (b'0'..=b'9').contains(&b) || (b'a'..=b'f').contains(&b)));
    }

    #[test]
    fn test_tk_crypt_roundtrip() {
        // encrypt then decrypt with same key should recover original
        let key = b"testkey\x00\x00";
        let original = b"Hello, world!!";
        // build a minimal packet: [opcode][len_hi][len_lo][id][inc][data...]
        let data_len = original.len();
        let total = 5 + data_len;
        let mut packet = vec![0u8; total];
        packet[0] = 0xAA;
        packet[1] = ((total as u16) >> 8) as u8;
        packet[2] = (total as u16 & 0xFF) as u8;
        packet[3] = 0x01;
        packet[4] = 0x00; // packet_inc
        packet[5..].copy_from_slice(original);

        tk_crypt_dynamic(&mut packet, key);
        tk_crypt_dynamic(&mut packet, key); // XOR twice = identity
        assert_eq!(&packet[5..], original);
    }
}

use std::ffi::CStr;
use std::slice;

use crate::session::{SessionId, RFIFO_SIZE, WFIFO_SIZE};

/// Whether the opcode uses dynamic encryption (client-side check).
pub fn rust_crypt_is_key_client(opcode: i32) -> bool {
    is_key_client(opcode as u8)
}

/// Whether the opcode uses dynamic encryption (server-side check).
pub fn rust_crypt_is_key_server(opcode: i32) -> bool {
    is_key_server(opcode as u8)
}

/// Generates an MD5 hex digest of `name` into `buffer` (must be ≥33 bytes).
pub unsafe fn rust_crypt_generate_hashvalues(
    name: *const i8,
    buffer: *mut i8,
    buflen: i32,
) -> *mut i8 {
    if name.is_null() || buffer.is_null() || buflen < 33 {
        return std::ptr::null_mut();
    }
    let name_bytes = CStr::from_ptr(name).to_bytes();
    let buf = slice::from_raw_parts_mut(buffer as *mut u8, buflen as usize);
    if generate_hashvalues(name_bytes, buf) { buffer } else { std::ptr::null_mut() }
}

/// Builds the 1025-byte encryption lookup table from `name`.
pub unsafe fn rust_crypt_populate_table(
    name: *const i8,
    table: *mut i8,
    tablelen: i32,
) -> *mut i8 {
    if name.is_null() || table.is_null() || tablelen < 0x401 {
        return std::ptr::null_mut();
    }
    let name_bytes = CStr::from_ptr(name).to_bytes();
    let buf = slice::from_raw_parts_mut(table as *mut u8, tablelen as usize);
    if populate_table(name_bytes, buf) { table } else { std::ptr::null_mut() }
}

/// Appends 3 index bytes to `packet` and updates its length field.
pub unsafe fn rust_crypt_set_packet_indexes(packet: *mut u8) -> i32 {
    if packet.is_null() { return 0; }
    let psize = ((*packet.add(1) as usize) << 8) | (*packet.add(2) as usize);
    if psize == 0 || psize + 6 > WFIFO_SIZE { return 0; }
    let buf_size = psize + 3 + 3;
    let buf = slice::from_raw_parts_mut(packet, buf_size);
    set_packet_indexes(buf) as i32
}

/// Derives a 9-byte session key into `keyout[0..10]` (NUL at [9]).
pub unsafe fn rust_crypt_generate_key2(
    packet: *mut u8,
    table: *const i8,
    keyout: *mut i8,
    fromclient: i32,
) -> *mut i8 {
    if packet.is_null() || table.is_null() || keyout.is_null() {
        return std::ptr::null_mut();
    }
    let psize = ((*packet.add(1) as usize) << 8) | (*packet.add(2) as usize);
    if psize == 0 || psize + 3 > RFIFO_SIZE { return std::ptr::null_mut(); }
    let packet_buf = slice::from_raw_parts(packet, psize + 3);
    let table_buf = slice::from_raw_parts(table as *const u8, 0x401);
    let mut key = [0u8; 10];
    generate_key2(packet_buf, table_buf, &mut key, fromclient != 0);
    let out = slice::from_raw_parts_mut(keyout as *mut u8, 10);
    out.copy_from_slice(&key);
    keyout
}

/// XOR-encrypts/decrypts `buff` in-place using a 9-byte `key`.
pub unsafe fn rust_crypt_dynamic(buff: *mut u8, key: *const i8) {
    if buff.is_null() || key.is_null() { return; }
    let total = ((*buff.add(1) as usize) << 8) | (*buff.add(2) as usize);
    if total < 5 || total > RFIFO_SIZE { return; }
    let buf = slice::from_raw_parts_mut(buff, total);
    let key_bytes = slice::from_raw_parts(key as *const u8, 9);
    tk_crypt_dynamic(buf, key_bytes);
}

/// XOR-encrypts/decrypts `buff` using the static xor_key.
pub unsafe fn rust_crypt_static(buff: *mut u8, xor_key: *const i8) {
    rust_crypt_dynamic(buff, xor_key);
}
