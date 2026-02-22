use anyhow::{bail, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::network::crypt::{set_packet_indexes, tk_crypt_static};

/// Reads one complete 0xAA-framed packet from the stream.
/// Returns the raw bytes (including the 3-byte header).
pub async fn read_client_packet(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let mut header = [0u8; 3];
    stream.read_exact(&mut header).await?;
    if header[0] != 0xAA {
        bail!("expected 0xAA header, got {:02X}", header[0]);
    }
    let payload_len = u16::from_be_bytes([header[1], header[2]]) as usize;
    let total = payload_len + 3;
    let mut buf = vec![0u8; total];
    buf[..3].copy_from_slice(&header);
    stream.read_exact(&mut buf[3..]).await?;
    Ok(buf)
}

/// Builds a `clif_message` packet: 0xAA-framed, cmd=0x02, encrypted.
/// `code`: sub-command (0x00=ok, 0x03=error, 0x05=pass-error)
pub fn build_message(code: u8, text: &str, xor_key: &[u8]) -> Vec<u8> {
    let text_bytes = text.as_bytes();
    let payload_len = text_bytes.len() + 6;
    let total = payload_len + 3;
    let mut buf = vec![0u8; total + 3]; // +3 for set_packet_indexes trailer
    buf[0] = 0xAA;
    buf[1] = ((payload_len >> 8) & 0xFF) as u8;
    buf[2] = (payload_len & 0xFF) as u8;
    buf[3] = 0x02; // cmd
    buf[4] = 0x02;
    buf[5] = code;
    buf[6] = text_bytes.len() as u8;
    buf[7..7 + text_bytes.len()].copy_from_slice(text_bytes);
    set_packet_indexes(&mut buf);
    tk_crypt_static(&mut buf, xor_key);
    buf[..total].to_vec()
}

/// Builds the version-OK response (20 bytes, unencrypted).
/// Sends the xor_key back to the client.
pub fn build_version_ok(xor_key: &str) -> Vec<u8> {
    let mut buf = vec![0u8; 20];
    buf[0] = 0xAA;
    buf[1] = 0x00;
    buf[2] = 0x11;
    buf[3] = 0x00;
    buf[4] = 0x00;
    buf[5] = 0x27;
    buf[6] = 0x4F;
    buf[7] = 0x8A;
    buf[8] = 0x4A;
    buf[9] = 0x00;
    buf[10] = 0x09;
    let key_bytes = xor_key.as_bytes();
    let copy_len = key_bytes.len().min(9);
    buf[11..11 + copy_len].copy_from_slice(&key_bytes[..copy_len]);
    buf
}

/// Builds the version-mismatch (patch) response (47 bytes).
pub fn build_version_patch(nex_version: u16, patch_url: &str) -> Vec<u8> {
    let mut buf = vec![0u8; 47];
    buf[0] = 0xAA;
    let payload = 0x29u16;
    buf[1] = (payload >> 8) as u8;
    buf[2] = (payload & 0xFF) as u8;
    buf[3] = 0x00;
    buf[4] = 0x02;
    buf[5] = (nex_version >> 8) as u8;
    buf[6] = (nex_version & 0xFF) as u8;
    buf[7] = 0x01;
    buf[8] = 0x23;
    let url = patch_url.as_bytes();
    let copy = url.len().min(38);
    buf[9..9 + copy].copy_from_slice(&url[..copy]);
    buf
}

/// Builds the interserver accept/reject packet (3 bytes, LE cmd=0x1000).
pub fn build_intif_auth_response(accepted: bool) -> Vec<u8> {
    vec![0x00, 0x10, if accepted { 0x00 } else { 0x01 }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_version_ok_length() {
        let pkt = build_version_ok("testkey12");
        assert_eq!(pkt.len(), 20);
        assert_eq!(pkt[0], 0xAA);
    }

    #[test]
    fn test_build_version_patch_length() {
        let pkt = build_version_patch(100, "http://example.com");
        assert_eq!(pkt.len(), 47);
        assert_eq!(pkt[4], 0x02); // type field
    }

    #[test]
    fn test_build_intif_auth_response() {
        assert_eq!(build_intif_auth_response(true),  vec![0x00, 0x10, 0x00]);
        assert_eq!(build_intif_auth_response(false), vec![0x00, 0x10, 0x01]);
    }

    #[test]
    fn test_build_message_starts_with_aa() {
        let pkt = build_message(0x03, "oops", b"key\x00\x00\x00\x00\x00\x00");
        assert_eq!(pkt[0], 0xAA);
        assert_eq!(pkt[3], 0x02); // cmd
    }
}
