use std::fs;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use flate2::Crc;
use std::io::Write;

use super::LoginState;
use crate::network::crypt::{set_packet_indexes, tk_crypt_static};

fn compute_crc32(data: &[u8]) -> u32 {
    let mut crc = Crc::new();
    crc.update(data);
    crc.sum()
}

fn zlib_compress(data: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap_or(());
    encoder.finish().unwrap_or_default()
}

pub async fn dispatch_meta(stream: &mut TcpStream, pkt: &[u8], state: &LoginState) {
    if pkt.len() < 6 { return; }
    match pkt[5] {
        0 => send_meta_file(stream, pkt, state).await,
        1 => send_meta_list(stream, state).await,
        _ => {}
    }
}

async fn send_meta_file(stream: &mut TcpStream, pkt: &[u8], state: &LoginState) {
    if pkt.len() < 7 { return; }
    let fname_len = pkt[6] as usize;
    if pkt.len() < 7 + fname_len { return; }
    let fname = match std::str::from_utf8(&pkt[7..7 + fname_len]) {
        Ok(s) => s.trim_end_matches('\0').to_string(),
        Err(_) => return,
    };

    let path = format!("{}{}", state.config.meta_dir, fname);
    let data = match fs::read(&path) {
        Ok(d) => d,
        Err(_) => return,
    };

    let crc = compute_crc32(&data);
    let compressed = zlib_compress(&data);
    let clen = compressed.len() as u16;

    // Body: [cmd=0x6F][??=0][mode=0][name_len][name][crc32_4B][clen_2B][data][0x00]
    let inner_len = 3 + 1 + fname_len + 4 + 2 + compressed.len() + 1;
    let total = inner_len + 3;
    let buf_size = total + 3; // +3 for set_packet_indexes
    let mut buf = vec![0u8; buf_size];

    buf[0] = 0xAA;
    buf[3] = 0x6F;
    buf[4] = 0x00;
    buf[5] = 0x00; // mode = file data
    buf[6] = fname_len as u8;
    buf[7..7 + fname_len].copy_from_slice(fname.as_bytes());

    let mut off = 7 + fname_len;
    buf[off..off + 4].copy_from_slice(&crc.to_be_bytes());
    off += 4;
    buf[off..off + 2].copy_from_slice(&clen.to_be_bytes());
    off += 2;
    buf[off..off + compressed.len()].copy_from_slice(&compressed);
    off += compressed.len();
    buf[off] = 0x00;

    let payload = off + 1 - 3;
    buf[1] = (payload >> 8) as u8;
    buf[2] = (payload & 0xFF) as u8;

    set_packet_indexes(&mut buf);
    tk_crypt_static(&mut buf, state.config.xor_key.as_bytes());
    let _ = stream.write_all(&buf[..total + 3]).await;
}

async fn send_meta_list(stream: &mut TcpStream, state: &LoginState) {
    let xk = state.config.xor_key.as_bytes();
    let files = &state.config.meta;

    let entry_size: usize = files.iter().map(|f| 1 + f.len() + 4).sum();
    let payload = 3 + 2 + entry_size; // [0x6F][??][mode=1][count_2B] + entries
    let total = payload + 3;
    let buf_size = total + 3;
    let mut buf = vec![0u8; buf_size];

    buf[0] = 0xAA;
    buf[1] = (payload >> 8) as u8;
    buf[2] = (payload & 0xFF) as u8;
    buf[3] = 0x6F;
    buf[4] = 0x00;
    buf[5] = 0x01; // mode = list
    let count = files.len() as u16;
    buf[6..8].copy_from_slice(&count.to_be_bytes());

    let mut off = 8;
    for fname in files {
        let path = format!("{}{}", state.config.meta_dir, fname);
        let data = fs::read(&path).unwrap_or_default();
        let crc = compute_crc32(&data);

        buf[off] = fname.len() as u8;
        off += 1;
        buf[off..off + fname.len()].copy_from_slice(fname.as_bytes());
        off += fname.len();
        buf[off..off + 4].copy_from_slice(&crc.to_be_bytes());
        off += 4;
    }

    set_packet_indexes(&mut buf);
    tk_crypt_static(&mut buf, xk);
    let _ = stream.write_all(&buf[..total + 3]).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32_empty() {
        assert_eq!(compute_crc32(&[]), 0);
    }

    #[test]
    fn test_zlib_compress_roundtrip() {
        let data = b"hello world hello world hello world";
        let compressed = zlib_compress(data);
        assert!(!compressed.is_empty());
        // compressed repeating data should be smaller
        assert!(compressed.len() < data.len());
    }
}
