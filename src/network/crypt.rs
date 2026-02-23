use md5::{Digest, Md5};

/// Opcodes that use key1 (static XOR) on the client side.
const CL_KEY1_PACKETS: &[u8] = &[2, 3, 4, 11, 21, 38, 58, 66, 67, 75, 80, 87, 98, 113, 115, 123];

/// Opcodes that use key1 (static XOR) on the server side.
const SV_KEY1_PACKETS: &[u8] = &[2, 3, 10, 64, 68, 94, 96, 98, 102, 111];

/// Returns true if the opcode should use dynamic encryption (client-bound check).
/// Mirrors C `is_key_client`: returns false (0) when opcode IS in the list.
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
/// Mirrors C `populate_table`. The C implementation writes 1056 bytes into a
/// 1025-byte buffer (latent overflow), but `generate_key2` only accesses
/// indices masked by `& 0x3FF` (0..1023). We produce exactly 1024 usable bytes
/// in 31 iterations (32 + 31×32 = 1024), which fits safely in 1025 bytes.
pub fn populate_table(name: &[u8], table: &mut [u8]) -> bool {
    if table.len() < 0x401 {
        return false;
    }
    let mut hash = [0u8; 64];
    // Double-hash the name (mirrors two consecutive generate_hashvalues calls in C)
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
/// Mirrors C `set_packet_indexes`. `packet` must be the full write buffer
/// starting at byte 0 (opcode byte).
///
/// Packet layout (bytes):
///   [0] opcode  [1..2] big-endian payload len  [3] packet-id  [4] inc  [5..] data
///
/// After this call:
///   [psize+0] = k2_lo  [psize+1] = k1  [psize+2] = k2_hi
///   [1..2] updated to new big-endian length
pub fn set_packet_indexes(packet: &mut [u8]) -> usize {
    // USE_RANDOM_INDEXES is defined — use a fixed value matching C's #else branch
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
/// Mirrors C `generate_key2`.
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
/// Mirrors C `tk_crypt_dynamic`.
///
/// Packet layout: [0] opcode [1..2] big-endian total len [3] inc [4] packetInc [5..] data
pub fn tk_crypt_dynamic(buff: &mut [u8], key: &[u8]) {
    if buff.len() < 5 {
        return;
    }
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
