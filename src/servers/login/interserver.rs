use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use super::{
    LoginState, CharResponse,
    LGN_WRONGPASS, LGN_WRONGUSER, LGN_USEREXIST, LGN_ERRDB,
    LGN_NEWCHAR, LGN_CHGPASS, LGN_DBLLOGIN, LGN_BANNED, LGN_ERRSERVER,
};
use super::packet::{build_message, build_intif_auth_response};
use crate::network::crypt::{set_packet_indexes, tk_crypt_static};

const PKT_LENS: [usize; 6] = [69, 5, 5, 27, 5, 0];

pub async fn promote_to_charserver(state: Arc<LoginState>, mut stream: TcpStream, first: Vec<u8>) {
    // Reject if char server already connected
    {
        let tx = state.char_tx.lock().await;
        if tx.is_some() {
            let _ = stream.write_all(&build_intif_auth_response(false)).await;
            return;
        }
    }

    if first.len() < 69 {
        let _ = stream.write_all(&build_intif_auth_response(false)).await;
        return;
    }

    // Decrypt: char_server encrypts the packet with tk_crypt_static before sending
    let mut first = first;
    tk_crypt_static(&mut first, state.config.xor_key.as_bytes());

    let login_id = std::str::from_utf8(&first[5..37]).unwrap_or("").trim_end_matches('\0');
    let login_pw = std::str::from_utf8(&first[37..69]).unwrap_or("").trim_end_matches('\0');

    if login_id != state.config.login_id || login_pw != state.config.login_pw {
        let _ = stream.write_all(&build_intif_auth_response(false)).await;
        tracing::warn!("[login] [char_auth_failed] id={}", login_id);
        return;
    }

    let _ = stream.write_all(&build_intif_auth_response(true)).await;
    tracing::info!("[login] [char_server_connect] Char Server accepted.");

    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(64);
    {
        let mut ct = state.char_tx.lock().await;
        *ct = Some(tx);
    }

    let (mut read_half, mut write_half) = stream.into_split();

    // Spawn writer: forwards messages from client tasks to char server
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            tracing::debug!("[login] [intif_writer] writing {} bytes to char server: {:02X?}",
                msg.len(), &msg[..msg.len().min(20)]);
            if write_half.write_all(&msg).await.is_err() {
                tracing::error!("[login] [intif_writer] write failed, breaking");
                break;
            }
        }
    });

    // Reader loop: receives char server responses and routes to pending client tasks
    loop {
        let mut cmd_bytes = [0u8; 2];
        if read_half.read_exact(&mut cmd_bytes).await.is_err() {
            break;
        }
        let cmd = u16::from_le_bytes(cmd_bytes);

        let idx = (cmd as usize).wrapping_sub(0x2000);
        if idx >= PKT_LENS.len() || PKT_LENS[idx] == 0 {
            tracing::warn!("[login] [intif_unknown_cmd] cmd={:04X}", cmd);
            continue;
        }

        let pkt_len = PKT_LENS[idx];
        let mut rest = vec![0u8; pkt_len - 2];
        if read_half.read_exact(&mut rest).await.is_err() {
            break;
        }

        let mut pkt = Vec::with_capacity(pkt_len);
        pkt.extend_from_slice(&cmd_bytes);
        pkt.extend_from_slice(&rest);

        let session_id = u16::from_le_bytes([pkt[2], pkt[3]]);
        tracing::debug!("[login] [intif_recv] cmd={:04X} session={} pkt_len={} raw={:02X?}",
            cmd, session_id, pkt.len(), &pkt[..pkt.len().min(27)]);
        let resp = CharResponse { session_id, data: pkt };

        let sender = {
            let pending = state.pending.lock().await;
            pending.get(&session_id).cloned()
        };
        if let Some(tx) = sender {
            match tx.try_send(resp) {
                Ok(()) => tracing::debug!("[login] [intif_route] session={} routed to client", session_id),
                Err(e) => tracing::error!("[login] [intif_route] session={} send FAILED: {}", session_id, e),
            }
        } else {
            tracing::warn!("[login] [intif_no_pending] session={} no pending client (pending count=?)", session_id);
        }
    }

    {
        let mut ct = state.char_tx.lock().await;
        *ct = None;
    }
    writer.abort();
    tracing::info!("[login] [char_server_disconnect] Char Server connection lost.");
}

pub async fn dispatch_char_response(
    stream: &mut TcpStream,
    state: &LoginState,
    resp: &CharResponse,
) {
    let pkt = &resp.data;
    let xk = state.config.xor_key.as_bytes();

    if pkt.len() < 2 { return; }
    let cmd = u16::from_le_bytes([pkt[0], pkt[1]]);

    match cmd {
        0x2001 => {
            if pkt.len() < 5 { return; }
            match pkt[4] {
                0x01 => { let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_USEREXIST], xk)).await; }
                0x00 => { let _ = stream.write_all(&build_message(0x00, "\x00", xk)).await; }
                _    => { let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_ERRDB], xk)).await; }
            }
        }
        0x2002 => {
            if pkt.len() < 5 { return; }
            match pkt[4] {
                0x01 => { let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_USEREXIST], xk)).await; }
                0x00 => { let _ = stream.write_all(&build_message(0x00, &state.messages.0[LGN_NEWCHAR], xk)).await; }
                _    => { let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_ERRDB], xk)).await; }
            }
        }
        0x2003 => {
            if pkt.len() < 27 { return; }
            let name_2003 = std::str::from_utf8(&pkt[5..21]).unwrap_or("?").trim_end_matches('\0');
            let ip_bytes = &pkt[21..25];
            let port_bytes = &pkt[25..27];
            tracing::debug!("[login] [intif_connectconfirm] result={:#04X} name={} ip_bytes={:02X?} port_bytes={:02X?}",
                pkt[4], name_2003, ip_bytes, port_bytes);
            match pkt[4] {
                0x00 => send_auth_success(stream, state, pkt).await,
                0x01 => { let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_ERRDB], xk)).await; }
                0x02 => { let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_WRONGUSER], xk)).await; }
                0x03 => { let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_WRONGPASS], xk)).await; }
                0x04 => { let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_BANNED], xk)).await; }
                0x05 => { let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_ERRSERVER], xk)).await; }
                0x06 => { let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_DBLLOGIN], xk)).await; }
                _    => tracing::warn!("[login] [intif_connectconfirm] unknown result={}", pkt[4]),
            }
        }
        0x2004 => {
            if pkt.len() < 5 { return; }
            match pkt[4] {
                0x00 => { let _ = stream.write_all(&build_message(0x00, &state.messages.0[LGN_CHGPASS], xk)).await; }
                0x01 => { let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_ERRDB], xk)).await; }
                0x02 => { let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_WRONGUSER], xk)).await; }
                0x03 => { let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_WRONGPASS], xk)).await; }
                _    => {}
            }
        }
        _ => tracing::warn!("[login] [dispatch_char_response] unknown cmd={:04X}", cmd),
    }
}

async fn send_auth_success(stream: &mut TcpStream, state: &LoginState, pkt: &[u8]) {
    let xk = state.config.xor_key.as_bytes();
    tracing::debug!("[login] [send_auth_success] sending redirect to client");

    // Packet 1: session-ok (11 bytes: 8 payload + 3 index bytes from set_packet_indexes)
    let mut buf1 = vec![0u8; 11]; // 8 + 3 for set_packet_indexes
    buf1[0] = 0xAA;
    buf1[1] = 0x00; buf1[2] = 0x05;
    buf1[3] = 0x02;
    buf1[4] = 0x17;
    buf1[5] = 0x00; buf1[6] = 0x00; buf1[7] = 0x00;
    set_packet_indexes(&mut buf1);
    tk_crypt_static(&mut buf1, xk);
    if let Err(e) = stream.write_all(&buf1[..11]).await {
        tracing::error!("[login] [send_auth_success] buf1 write error: {}", e);
        return;
    }
    tracing::debug!("[login] [send_auth_success] buf1 ok: {:02X?}", &buf1[..11]);

    // Packet 2: char redirect
    // pkt: [0..1]=cmd [2..3]=session_id [4]=result [5..21]=char_name [21..25]=map_ip [25..27]=map_port
    let char_name = std::str::from_utf8(&pkt[5..21]).unwrap_or("").trim_end_matches('\0');
    // The char server writes map_ip via WFIFOL (native LE uint32). We must mirror C login_char.c
    // which does SWAP32(RFIFOL(fd, 21)) before writing to the client: read as LE, byte-swap.
    // The game client reverses this with RFIFOL + htonl to recover the NBO IP for connect().
    let map_ip_bytes = u32::from_be_bytes([pkt[21], pkt[22], pkt[23], pkt[24]]).to_le_bytes();
    let char_port  = u16::from_le_bytes([pkt[25], pkt[26]]);
    let session_id = u16::from_le_bytes([pkt[2], pkt[3]]);
    tracing::debug!("[map_ip] bytes={:?}", map_ip_bytes);
    // Packet layout:
    // [0xAA][len_BE_2B][0x03][map_ip_NBO_4B][char_port_BE_2B][name_len_1B]
    // [9_BE_2B][xk_9B][char_name_len_1B][char_name_NB][session_id_BE_4B]
    // + 3 index bytes from set_packet_indexes
    //
    // xk is always 9 bytes (C uses strcpy which includes the null terminator).
    // session_id is written after char_name as SWAP32(session_id) = BE u32.
    let char_name_len = char_name.len();
    let name_len_field = char_name_len + 16; // matches C: strlen(thing) + 16
    let xk_bytes = state.config.xor_key.as_bytes();
    // C uses strcpy which copies all chars; [13..22) = full xor_key (9 bytes).
    // Position [22] is then overwritten by char_name_len, matching C behavior.
    let xk_copy_len = xk_bytes.len().min(9);

    // payload = cmd(1) + map_ip(4) + char_port(2) + name_len(1) + xk_len_field(2)
    //         + xk(9) + char_name_len(1) + char_name(N) + session_id(4)
    let payload_len = char_name_len + 24; // (char_name_len + 16) + 8
    let total = payload_len + 3;
    let mut buf2 = vec![0u8; total + 3]; // +3 for set_packet_indexes trailer
    buf2[0] = 0xAA;
    buf2[1] = (payload_len >> 8) as u8;
    buf2[2] = (payload_len & 0xFF) as u8;
    buf2[3] = 0x03;
    buf2[4..8].copy_from_slice(&map_ip_bytes);  // NBO bytes: client reads directly as IPv4
    buf2[8..10].copy_from_slice(&char_port.to_be_bytes());
    buf2[10] = name_len_field as u8;
    buf2[11..13].copy_from_slice(&9u16.to_be_bytes()); // xk_len always 9
    buf2[13..13 + xk_copy_len].copy_from_slice(&xk_bytes[..xk_copy_len]);
    // buf2[22] overwrites any null from a short key (< 9 chars), matching C strcpy + WFIFOB behavior
    buf2[22] = char_name_len as u8;
    buf2[23..23 + char_name_len].copy_from_slice(char_name.as_bytes());
    buf2[23 + char_name_len..27 + char_name_len].copy_from_slice(&(session_id as u32).to_be_bytes());

    set_packet_indexes(&mut buf2);

    // Log the IP from pkt[21..25] which are the original NBO bytes (127=pkt[21] for 127.0.0.1).
    let ip_str = format!("{}.{}.{}.{}", pkt[21], pkt[22], pkt[23], pkt[24]);
    tracing::debug!("[login] [send_auth_success] redirect: map_ip={} map_port={} char_name={} session_id={}",
        ip_str, char_port, char_name, session_id);
    tracing::debug!("[login] [send_auth_success] buf2 hex: {:02X?}", &buf2[..total + 3]);

    if let Err(e) = stream.write_all(&buf2[..total + 3]).await {
        tracing::error!("[login] [send_auth_success] buf2 write error: {}", e);
        return;
    }
    tracing::debug!("[login] [send_auth_success] redirect sent OK ({} bytes), client should now connect to {}:{}", total + 3, ip_str, char_port);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_intif_cmd() {
        let pkt = vec![0x03u8, 0x20, 0x05, 0x00, 0x00];
        let cmd = u16::from_le_bytes([pkt[0], pkt[1]]);
        assert_eq!(cmd, 0x2003);
        let session_id = u16::from_le_bytes([pkt[2], pkt[3]]);
        assert_eq!(session_id, 5);
    }

    #[test]
    fn test_packet_len_table() {
        assert_eq!(PKT_LENS[0x2003 - 0x2000], 27);
        assert_eq!(PKT_LENS[0x2004 - 0x2000],  5);
        assert_eq!(PKT_LENS[0x2001 - 0x2000],  5);
    }
}
