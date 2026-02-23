use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};
use super::{CharState, LoginEntry};
use super::db;
use crate::network::crypt::tk_crypt_static;

// Packet length table for 0x1000–0x1006 (0 = end/unused)
const PKT_LENS: &[usize] = &[3, 20, 43, 40, 52, 0, 0];

pub async fn connect_to_login(state: Arc<CharState>) {
    let mut ticker = interval(Duration::from_secs(10));
    loop {
        ticker.tick().await;
        {
            let login_tx = state.login_tx.lock().await;
            if let Some(tx) = login_tx.as_ref() {
                // Already connected — send keepalive
                let _ = tx.send(vec![0xFF, 0x01]).await;
                continue;
            }
        }

        let addr = format!("{}:{}", state.config.login_ip, state.config.login_port);
        tracing::info!("[char] [logif] Connecting to login server at {}", addr);

        match TcpStream::connect(&addr).await {
            Ok(stream) => {
                run_login_connection(Arc::clone(&state), stream).await;
            }
            Err(e) => {
                tracing::warn!("[char] [logif] Connect failed: {}", e);
            }
        }
    }
}

async fn run_login_connection(state: Arc<CharState>, mut stream: TcpStream) {
    // Send auth handshake: 0xAA + BE_len(66) + 0xFF + RAND_INC(0) + login_id(32) + login_pw(32)
    // The Rust login server reads exactly 69 bytes (3-byte 0xAA header + 66 payload).
    // The C client sends 72 (69 + 3 set_packet_indexes trailer), but the Rust login server
    // doesn't read the trailer — sending 72 would leave 3 stale bytes corrupting the stream.
    let mut pkt = vec![0u8; 69];
    pkt[0] = 0xAA;
    pkt[1] = 0x00; pkt[2] = 0x42; // 66 in big-endian
    pkt[3] = 0xFF;
    pkt[4] = 0x00; // RAND_INC placeholder
    let lid = state.config.login_id.as_bytes();
    let lpw = state.config.login_pw.as_bytes();
    let lid_len = lid.len().min(32);
    let lpw_len = lpw.len().min(32);
    pkt[5..5 + lid_len].copy_from_slice(&lid[..lid_len]);
    pkt[37..37 + lpw_len].copy_from_slice(&lpw[..lpw_len]);
    let xk = state.config.xor_key.as_bytes();
    tk_crypt_static(&mut pkt, xk);
    if stream.write_all(&pkt).await.is_err() {
        return;
    }

    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(64);
    {
        let mut lt = state.login_tx.lock().await;
        *lt = Some(tx);
    }

    let (mut rh, mut wh) = stream.into_split();

    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if wh.write_all(&msg).await.is_err() {
                break;
            }
        }
    });

    loop {
        let mut cmd_bytes = [0u8; 2];
        if rh.read_exact(&mut cmd_bytes).await.is_err() {
            break;
        }

        // 0xAA-framed packets (banner, keep-alive responses): skip them.
        // We already read [0xAA, hi_byte]. Read 1 more byte for lo_byte of the BE length,
        // then skip `payload_len` bytes.
        if cmd_bytes[0] == 0xAA {
            let mut lo = [0u8; 1];
            if rh.read_exact(&mut lo).await.is_err() {
                break;
            }
            let skip = u16::from_be_bytes([cmd_bytes[1], lo[0]]) as usize;
            let mut discard = vec![0u8; skip];
            if rh.read_exact(&mut discard).await.is_err() {
                break;
            }
            continue;
        }

        let cmd = u16::from_le_bytes(cmd_bytes);
        let idx = (cmd as usize).wrapping_sub(0x1000);
        if idx >= PKT_LENS.len() || PKT_LENS[idx] == 0 {
            tracing::warn!("[char] [logif] unknown cmd={:04X}, dropping connection", cmd);
            break;
        }

        let rest_len = PKT_LENS[idx].saturating_sub(2);
        let mut rest = vec![0u8; rest_len];
        if rh.read_exact(&mut rest).await.is_err() {
            break;
        }

        let mut pkt = Vec::with_capacity(PKT_LENS[idx]);
        pkt.extend_from_slice(&cmd_bytes);
        pkt.extend_from_slice(&rest);

        dispatch_login_packet(&state, cmd, &pkt).await;
    }

    {
        let mut lt = state.login_tx.lock().await;
        *lt = None;
    }
    writer.abort();
    tracing::warn!("[char] [logif] Login server connection lost");
}

async fn dispatch_login_packet(state: &Arc<CharState>, cmd: u16, pkt: &[u8]) {
    match cmd {
        0x1000 => {
            if pkt.len() >= 3 && pkt[2] != 0 {
                tracing::warn!("[char] [logif] Login server rejected connection result={}", pkt[2]);
            } else {
                tracing::info!("[char] [logif] Connected to Login Server");
            }
        }
        0x1001 => handle_usedname(state, pkt).await,
        0x1002 => handle_newchar(state, pkt).await,
        0x1003 => handle_login(state, pkt).await,
        0x1004 => handle_setpass(state, pkt).await,
        _ => tracing::warn!("[char] [logif] unhandled cmd={:04X}", cmd),
    }
}

async fn handle_usedname(state: &Arc<CharState>, pkt: &[u8]) {
    if pkt.len() < 20 {
        return;
    }
    let name = std::str::from_utf8(&pkt[4..20]).unwrap_or("").trim_end_matches('\0');
    let used = db::is_name_used(&state.db, name).await.unwrap_or(true);
    let mut resp = [0u8; 5];
    resp[0] = 0x01; resp[1] = 0x20; // cmd 0x2001 LE
    resp[2] = pkt[2]; resp[3] = pkt[3]; // session_id passthrough
    resp[4] = if used { 1 } else { 0 };
    send_to_login(state, resp.to_vec()).await;
}

async fn handle_newchar(state: &Arc<CharState>, pkt: &[u8]) {
    if pkt.len() < 43 {
        return;
    }
    let name = std::str::from_utf8(&pkt[4..20]).unwrap_or("").trim_end_matches('\0');
    let pass = std::str::from_utf8(&pkt[20..36]).unwrap_or("").trim_end_matches('\0');
    let cfg = &state.config;
    let res = db::create_char(
        &state.db, name, pass,
        pkt[39],          // totem
        pkt[37] % 2,      // sex
        pkt[38],          // country/nation
        pkt[36] as u16,   // face
        pkt[40] as u16,   // hair
        pkt[42] as u16,   // face_color
        pkt[41] as u16,   // hair_color
        cfg.start_point.m as u32,
        cfg.start_point.x as u32,
        cfg.start_point.y as u32,
    ).await;
    let mut resp = [0u8; 5];
    resp[0] = 0x02; resp[1] = 0x20; // cmd 0x2002 LE
    resp[2] = pkt[2]; resp[3] = pkt[3];
    resp[4] = res as u8;
    send_to_login(state, resp.to_vec()).await;
}

async fn handle_login(state: &Arc<CharState>, pkt: &[u8]) {
    if pkt.len() < 40 {
        tracing::warn!("[char] [login] pkt too short: {}", pkt.len());
        return;
    }
    let name = std::str::from_utf8(&pkt[4..20]).unwrap_or("").trim_end_matches('\0');
    let pass = std::str::from_utf8(&pkt[20..36]).unwrap_or("").trim_end_matches('\0');
    tracing::debug!("[char] [login] attempt name={}", name);

    let mut resp = vec![0u8; 27];
    resp[0] = 0x03; resp[1] = 0x20; // cmd 0x2003 LE
    resp[2] = pkt[2]; resp[3] = pkt[3];

    // Verify password
    tracing::info!("[char] [login] checking password");
    let stored_hash = match db::get_char_password(&state.db, name).await {
        Ok(Some(h)) => h,
        Ok(None) => { tracing::warn!("[char] [login] no user"); resp[4] = 0x02; send_to_login(state, resp).await; return; }
        Err(e)    => { tracing::warn!("[char] [login] db err: {}", e); resp[4] = 0x01; send_to_login(state, resp).await; return; }
    };

    let mast_ok = match db::get_master_password(&state.db).await {
        Ok(Some((mhash, exp))) => db::ismastpass(pass, &mhash, exp),
        _ => false,
    };

    if !db::ispass(name, pass, &stored_hash) && !mast_ok {
        tracing::warn!("[char] [login] wrong password");
        resp[4] = 0x03;
        send_to_login(state, resp).await;
        return;
    }
    tracing::info!("[char] [login] password ok");

    let char_info = match db::char_login_lookup(&state.db, name).await {
        Ok(Some(c)) => c,
        Ok(None) => { tracing::warn!("[char] [login] char not found"); resp[4] = 0x02; send_to_login(state, resp).await; return; }
        Err(e)    => { tracing::warn!("[char] [login] lookup err: {}", e); resp[4] = 0x01; send_to_login(state, resp).await; return; }
    };
    tracing::info!("[char] [login] char_id={} map_id={}", char_info.char_id, char_info.map_id);

    tracing::info!("[char] [login] checking ban");
    if char_info.banned || db::is_account_banned(&state.db, char_info.char_id).await {
        resp[4] = 0x04;
        send_to_login(state, resp).await;
        return;
    }

    // Find map server that hosts this character's map
    let map_idx = {
        let servers = state.map_servers.lock().await;
        servers.iter().enumerate()
            .find(|(_, s)| s.as_ref().map(|f| f.maps.contains(&(char_info.map_id as u16))).unwrap_or(false))
            .map(|(i, _)| i)
    };

    let map_idx = match map_idx {
        Some(i) => i,
        None => { resp[4] = 0x05; send_to_login(state, resp).await; return; }
    };

    // Check if already online — lock online, record result, then drop before locking map_servers.
    let already_online = {
        let online = state.online.lock().await;
        online.contains_key(&char_info.char_id)
    };
    if already_online {
        resp[4] = 0x06;
        send_to_login(state, resp).await;
        // Force-kick on map server (0x3804)
        let servers = state.map_servers.lock().await;
        if let Some(Some(s)) = servers.get(map_idx) {
            let mut kick = vec![0u8; 6];
            kick[0] = 0x04; kick[1] = 0x38; // 0x3804 LE
            kick[2..6].copy_from_slice(&char_info.char_id.to_le_bytes());
            let _ = s.tx.send(kick).await;
        }
        return;
    }

    // Route player: send 0x3802 to map server
    let char_id_le = char_info.char_id.to_le_bytes();
    let nlen = name.len().min(16);
    let mut map_msg = vec![0u8; 38];
    map_msg[0] = 0x02; map_msg[1] = 0x38; // 0x3802 LE
    map_msg[2] = pkt[2]; map_msg[3] = pkt[3]; // session_id
    map_msg[4..8].copy_from_slice(&char_id_le);
    map_msg[8..8 + nlen].copy_from_slice(&name.as_bytes()[..nlen]);
    // client IP at pkt[36..40] — forward to map server
    if pkt.len() >= 40 {
        map_msg[34..38].copy_from_slice(&pkt[36..40]);
    }

    {
        let servers = state.map_servers.lock().await;
        if let Some(Some(s)) = servers.get(map_idx) {
            let _ = s.tx.send(map_msg).await;

            resp[4] = 0x00;
            let nlen = name.len().min(16);
            resp[5..5 + nlen].copy_from_slice(&name.as_bytes()[..nlen]);
            resp[21..25].copy_from_slice(&s.ip.to_le_bytes());
            resp[25..27].copy_from_slice(&s.port.to_le_bytes());
        } else {
            resp[4] = 0x05;
            send_to_login(state, resp).await;
            return;
        }
    }

    send_to_login(state, resp).await;

    // Register as online
    {
        let mut online = state.online.lock().await;
        online.insert(char_info.char_id, LoginEntry {
            map_server_idx: map_idx,
            char_name: name.to_string(),
        });
    }
}

async fn handle_setpass(state: &Arc<CharState>, pkt: &[u8]) {
    if pkt.len() < 52 {
        return;
    }
    let name    = std::str::from_utf8(&pkt[4..20]).unwrap_or("").trim_end_matches('\0');
    let pass    = std::str::from_utf8(&pkt[20..36]).unwrap_or("").trim_end_matches('\0');
    let newpass = std::str::from_utf8(&pkt[36..52]).unwrap_or("").trim_end_matches('\0');
    let res = db::set_char_password(&state.db, name, pass, newpass).await;
    let mut resp = [0u8; 5];
    resp[0] = 0x04; resp[1] = 0x20; // cmd 0x2004 LE
    resp[2] = pkt[2]; resp[3] = pkt[3];
    resp[4] = res.unsigned_abs() as u8;
    send_to_login(state, resp.to_vec()).await;
}

async fn send_to_login(state: &Arc<CharState>, msg: Vec<u8>) {
    let lt = state.login_tx.lock().await;
    if let Some(tx) = lt.as_ref() {
        let _ = tx.send(msg).await;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_logif_packet_lens() {
        assert_eq!(super::PKT_LENS[0x1000 - 0x1000], 3);
        assert_eq!(super::PKT_LENS[0x1003 - 0x1000], 40);
    }

    #[test]
    fn test_auth_packet_build() {
        let mut pkt = vec![0u8; 69];
        pkt[0] = 0xAA;
        pkt[1] = 0x00; pkt[2] = 0x42; // 66 in BE
        pkt[3] = 0xFF;
        assert_eq!(pkt[3], 0xFF);
        assert_eq!(u16::from_be_bytes([pkt[1], pkt[2]]), 66);
    }
}
