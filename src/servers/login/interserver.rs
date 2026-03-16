use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use crate::network::crypt::{set_packet_indexes, tk_crypt_static};

pub async fn send_auth_success_direct(
    stream: &mut TcpStream,
    config: &crate::config::ServerConfig,
    char_name: &str,
    session_id: u16,
) {
    let xk = config.xor_key.as_bytes();

    // Packet 1: session-ok
    let mut buf1 = vec![0u8; 11];
    buf1[0] = 0xAA;
    buf1[1] = 0x00; buf1[2] = 0x05;
    buf1[3] = 0x02;
    buf1[4] = 0x17;
    buf1[5] = 0x00; buf1[6] = 0x00; buf1[7] = 0x00;
    set_packet_indexes(&mut buf1);
    tk_crypt_static(&mut buf1, xk);
    if stream.write_all(&buf1[..11]).await.is_err() { return; }

    // Packet 2: redirect to map server
    let map_ip: u32 = config.map_ip
        .parse::<std::net::Ipv4Addr>()
        .map(|a| u32::from_be_bytes(a.octets()))
        .unwrap_or(0);
    let map_port = config.map_port;

    let char_name_len = char_name.len();
    let payload_len = char_name_len + 24;
    let total = payload_len + 3;
    let mut buf2 = vec![0u8; total + 3];
    buf2[0] = 0xAA;
    buf2[1] = (payload_len >> 8) as u8;
    buf2[2] = (payload_len & 0xFF) as u8;
    buf2[3] = 0x03;
    buf2[4..8].copy_from_slice(&map_ip.to_le_bytes());
    buf2[8..10].copy_from_slice(&map_port.to_be_bytes());
    buf2[10] = (char_name_len + 16) as u8;
    buf2[11..13].copy_from_slice(&9u16.to_be_bytes());
    let xk_copy = xk.len().min(9);
    buf2[13..13 + xk_copy].copy_from_slice(&xk[..xk_copy]);
    buf2[22] = char_name_len as u8;
    buf2[23..23 + char_name_len].copy_from_slice(char_name.as_bytes());
    buf2[23 + char_name_len..27 + char_name_len].copy_from_slice(&(session_id as u32).to_be_bytes());

    set_packet_indexes(&mut buf2);
    let _ = stream.write_all(&buf2[..total + 3]).await;
    tracing::info!("[login] [direct] redirect sent: map={}:{} name={} session={}",
        config.map_ip, map_port, char_name, session_id);
}
