use std::sync::Arc;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;

use super::{LoginState, CharResponse, LGN_ERRDB, LGN_ERRPASS, LGN_ERRUSER};
use super::packet::{read_client_packet, build_message, build_version_ok, build_version_patch};
use crate::network::crypt::tk_crypt_static;

struct SessionData {
    name: String,
    pass: String,
    face: u8,
    sex: u8,
    country: u8,
    totem: u8,
    hair: u8,
    hair_color: u8,
    face_color: u8,
}

impl Default for SessionData {
    fn default() -> Self {
        Self {
            name: String::new(), pass: String::new(),
            face: 0, sex: 0, country: 0, totem: 0,
            hair: 0, hair_color: 0, face_color: 0,
        }
    }
}

pub fn is_valid_name(s: &str) -> bool {
    s.len() >= 3 && s.len() <= 12 && s.chars().all(|c| c.is_ascii_alphabetic())
}

pub fn is_valid_password(s: &str) -> bool {
    s.len() >= 3 && s.len() <= 8 && s.chars().all(|c| c.is_ascii_alphanumeric())
}

pub async fn handle_client(
    state: Arc<LoginState>,
    mut stream: TcpStream,
    peer: SocketAddr,
    session_id: u16,
    first_packet: Vec<u8>,
) {
    let mut sd = SessionData::default();
    let mut queue: Vec<Vec<u8>> = vec![first_packet];

    loop {
        let mut pkt = if let Some(p) = queue.pop() {
            p
        } else {
            match read_client_packet(&mut stream).await {
                Ok(p) => p,
                Err(_) => return,
            }
        };

        // Decrypt packet in place
        let xk = state.config.xor_key.as_bytes().to_vec();
        tk_crypt_static(&mut pkt, &xk);

        if pkt.len() < 4 {
            return;
        }

        let cmd = pkt[3];
        tracing::debug!("[login] [packet_in] session={} cmd={:02X}", session_id, cmd);

        match cmd {
            0x00 => dispatch_version_check(&mut stream, &pkt, &state).await,
            0x02 => dispatch_register(&mut stream, &pkt, &state, &mut sd, session_id).await,
            0x03 => dispatch_login(&mut stream, &pkt, &state, &mut sd, session_id, &peer).await,
            0x04 => dispatch_create_char(&mut stream, &pkt, &state, &mut sd, session_id).await,
            0x10 => dispatch_heartbeat(&mut stream).await,
            0x26 => dispatch_change_pass(&mut stream, &pkt, &state, &mut sd, session_id).await,
            0x57 | 0x71 | 0x62 => {}
            0x7B => super::meta::dispatch_meta(&mut stream, &pkt, &state).await,
            _ => tracing::warn!("[login] [packet_unknown] cmd={:02X} session={}", cmd, session_id),
        }
    }
}

async fn dispatch_version_check(stream: &mut TcpStream, pkt: &[u8], state: &LoginState) {
    if pkt.len() < 9 { return; }
    let ver  = u16::from_be_bytes([pkt[4], pkt[5]]);
    let deep = u16::from_be_bytes([pkt[7], pkt[8]]);
    tracing::info!("[login] [version_check] client_version={} patch={}", ver, deep);

    let xk = &state.config.xor_key;
    let nex = state.config.version as u16;
    let response = if ver == nex {
        build_version_ok(xk)
    } else {
        build_version_patch(nex, "http://www.google.com")
    };
    let _ = stream.write_all(&response).await;
}

async fn dispatch_heartbeat(stream: &mut TcpStream) {
    let pkt: &[u8] = &[0xAA, 0x00, 0x07, 0x60, 0x00, 0x55, 0xE0, 0xD8, 0xA2, 0xA0];
    let _ = stream.write_all(pkt).await;
}

async fn dispatch_register(
    stream: &mut TcpStream,
    pkt: &[u8],
    state: &LoginState,
    sd: &mut SessionData,
    session_id: u16,
) {
    let xk = state.config.xor_key.as_bytes();
    if pkt.len() < 6 { return; }
    let name_len = pkt[5] as usize;
    if pkt.len() < 6 + name_len + 1 { return; }
    let name = std::str::from_utf8(&pkt[6..6 + name_len])
        .unwrap_or("").trim_end_matches('\0').to_string();

    if !is_valid_name(&name) {
        let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_ERRUSER], xk)).await;
        return;
    }

    let pass_len = pkt[6 + name_len] as usize;
    if pkt.len() < 7 + name_len + pass_len { return; }
    let pass = std::str::from_utf8(&pkt[7 + name_len..7 + name_len + pass_len])
        .unwrap_or("").trim_end_matches('\0').to_string();

    if !is_valid_password(&pass) {
        let _ = stream.write_all(&build_message(0x05, &state.messages.0[LGN_ERRPASS], xk)).await;
        return;
    }

    sd.name = name.clone();
    sd.pass = pass;

    let mut msg = vec![0u8; 20];
    msg[0] = 0x01; msg[1] = 0x10;
    msg[2] = (session_id & 0xFF) as u8;
    msg[3] = (session_id >> 8) as u8;
    let nb = name.as_bytes();
    msg[4..4 + nb.len().min(16)].copy_from_slice(&nb[..nb.len().min(16)]);

    forward_to_char(state, stream, msg, session_id, xk, &state.messages.0[LGN_ERRDB]).await;
}

async fn dispatch_login(
    stream: &mut TcpStream,
    pkt: &[u8],
    state: &LoginState,
    sd: &mut SessionData,
    session_id: u16,
    peer: &SocketAddr,
) {
    let xk = state.config.xor_key.as_bytes();
    if pkt.len() < 6 { return; }
    let name_len = pkt[5] as usize;
    if pkt.len() < 6 + name_len + 1 { return; }
    let name = std::str::from_utf8(&pkt[6..6 + name_len])
        .unwrap_or("").trim_end_matches('\0').to_string();

    if !is_valid_password(&name) {
        let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_ERRUSER], xk)).await;
        return;
    }

    let pass_len = pkt[6 + name_len] as usize;
    if pkt.len() < 7 + name_len + pass_len { return; }
    let pass = std::str::from_utf8(&pkt[7 + name_len..7 + name_len + pass_len])
        .unwrap_or("").trim_end_matches('\0').to_string();

    if !is_valid_password(&pass) {
        let _ = stream.write_all(&build_message(0x05, &state.messages.0[LGN_ERRPASS], xk)).await;
        return;
    }

    // Maintenance and require_reg checks
    if let Some(pool) = &state.db {
        if super::db::get_maintenance_mode(pool).await {
            let gm = super::db::get_char_gm_level(pool, &name).await;
            if gm == 0 {
                let _ = stream.write_all(&build_message(0x03,
                    "Server is undergoing maintenance. Please visit www.website.com or the facebook group for more details.",
                    xk)).await;
                return;
            }
        }
        if state.config.require_reg != 0 {
            if super::db::get_account_for_char(pool, &name).await == 0 {
                let _ = stream.write_all(&build_message(0x03,
                    "You must attach your character to an account to play.\n\nPlease visit www.website.com to attach your character to an account.",
                    xk)).await;
                return;
            }
        }
    }

    sd.name = name.clone();
    sd.pass = pass.clone();

    let mut msg = vec![0u8; 40];
    msg[0] = 0x03; msg[1] = 0x10;
    msg[2] = (session_id & 0xFF) as u8;
    msg[3] = (session_id >> 8) as u8;
    let nb = name.as_bytes();
    msg[4..4 + nb.len().min(16)].copy_from_slice(&nb[..nb.len().min(16)]);
    let pb = pass.as_bytes();
    msg[20..20 + pb.len().min(16)].copy_from_slice(&pb[..pb.len().min(16)]);
    if let std::net::IpAddr::V4(v4) = peer.ip() {
        msg[36..40].copy_from_slice(&v4.octets());
    }

    forward_to_char(state, stream, msg, session_id, xk, &state.messages.0[LGN_ERRDB]).await;
}

async fn dispatch_create_char(
    stream: &mut TcpStream,
    pkt: &[u8],
    state: &LoginState,
    sd: &mut SessionData,
    session_id: u16,
) {
    if sd.name.is_empty() || sd.pass.is_empty() { return; }
    if pkt.len() < 13 { return; }

    sd.face       = pkt[6];
    sd.hair       = pkt[7];
    sd.face_color = pkt[8];
    sd.hair_color = pkt[9];
    sd.sex        = pkt[10];
    sd.totem      = pkt[12];
    sd.country    = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() % 2) as u8;

    let xk = state.config.xor_key.as_bytes();
    let mut msg = vec![0u8; 43];
    msg[0] = 0x02; msg[1] = 0x10;
    msg[2] = (session_id & 0xFF) as u8;
    msg[3] = (session_id >> 8) as u8;
    let nb = sd.name.as_bytes();
    msg[4..4 + nb.len().min(16)].copy_from_slice(&nb[..nb.len().min(16)]);
    let pb = sd.pass.as_bytes();
    msg[20..20 + pb.len().min(16)].copy_from_slice(&pb[..pb.len().min(16)]);
    msg[36] = sd.face; msg[37] = sd.sex; msg[38] = sd.country;
    msg[39] = sd.totem; msg[40] = sd.hair; msg[41] = sd.hair_color; msg[42] = sd.face_color;

    forward_to_char(state, stream, msg, session_id, xk, &state.messages.0[LGN_ERRDB]).await;
}

async fn dispatch_change_pass(
    stream: &mut TcpStream,
    pkt: &[u8],
    state: &LoginState,
    sd: &mut SessionData,
    session_id: u16,
) {
    let xk = state.config.xor_key.as_bytes();
    if pkt.len() < 6 { return; }
    let name_len = pkt[5] as usize;
    if name_len > 16 { return; }
    let old_off = 6 + name_len;
    if pkt.len() <= old_off { return; }
    let old_pass_len = pkt[old_off] as usize;
    if old_pass_len > 16 { return; }
    let new_off = old_off + 1 + old_pass_len;
    if pkt.len() <= new_off { return; }
    let new_pass_len = pkt[new_off] as usize;
    if new_pass_len > 8 || new_pass_len < 3 { return; }

    let name = std::str::from_utf8(&pkt[6..6 + name_len]).unwrap_or("").trim_end_matches('\0');
    if !is_valid_password(name) {
        let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_ERRUSER], xk)).await;
        return;
    }

    sd.name = name.to_string();

    let mut msg = vec![0u8; 52];
    msg[0] = 0x04; msg[1] = 0x10;
    msg[2] = (session_id & 0xFF) as u8;
    msg[3] = (session_id >> 8) as u8;
    msg[4..4 + name_len.min(16)].copy_from_slice(&pkt[6..6 + name_len.min(16)]);
    msg[20..20 + old_pass_len.min(16)].copy_from_slice(&pkt[old_off + 1..old_off + 1 + old_pass_len.min(16)]);
    msg[36..36 + new_pass_len.min(16)].copy_from_slice(&pkt[new_off + 1..new_off + 1 + new_pass_len.min(16)]);

    forward_to_char(state, stream, msg, session_id, xk, &state.messages.0[LGN_ERRDB]).await;
}

async fn forward_to_char(
    state: &LoginState,
    stream: &mut TcpStream,
    msg: Vec<u8>,
    session_id: u16,
    xk: &[u8],
    err_db_msg: &str,
) {
    use tokio::sync::oneshot;

    let (tx, rx) = oneshot::channel::<CharResponse>();
    {
        let mut pending = state.pending.lock().await;
        pending.insert(session_id, tx);
    }

    let sent = {
        let tx_guard = state.char_tx.lock().await;
        if let Some(tx) = &*tx_guard {
            tx.send(msg).await.is_ok()
        } else {
            false
        }
    };

    if !sent {
        let _ = stream.write_all(&build_message(0x03, err_db_msg, xk)).await;
        let mut pending = state.pending.lock().await;
        pending.remove(&session_id);
        return;
    }

    match tokio::time::timeout(std::time::Duration::from_secs(10), rx).await {
        Ok(Ok(resp)) => {
            super::interserver::dispatch_char_response(stream, state, &resp).await;
        }
        _ => {
            let _ = stream.write_all(&build_message(0x03, err_db_msg, xk)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_name_chars_only_letters() {
        assert!(is_valid_name("Alice"));
        assert!(!is_valid_name("ali123"));
        assert!(!is_valid_name("a"));
    }

    #[test]
    fn test_valid_password_allows_alnum() {
        assert!(is_valid_password("abc123"));
        assert!(!is_valid_password("ab"));
        assert!(!is_valid_password("ab!"));
    }

    #[test]
    fn test_valid_name_length_bounds() {
        assert!(is_valid_name("abc"));          // min 3
        assert!(is_valid_name("abcdefghijkl")); // max 12
        assert!(!is_valid_name("ab"));           // too short
        assert!(!is_valid_name("abcdefghijklm")); // too long
    }
}
