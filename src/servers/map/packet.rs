use std::sync::Arc;
use super::MapState;

/// Packet length table for incoming 0x3800–0x3811 packets from char_server.
/// Index = cmd - 0x3800. -1 = variable (read 4-byte len at offset 2). 0 = unknown.
pub const PKT_LENS: &[i32] = &[
    4,   // 0x3800 accept
    -1,  // 0x3801 mapset (variable)
    38,  // 0x3802 authadd
    -1,  // 0x3803 charload (variable, zlib)
    6,   // 0x3804 checkonline
    -1,  // 0x3805 unused
    255, // 0x3806 unused
    -1,  // 0x3807 unused
    5,   // 0x3808 deletepostresponse
    -1,  // 0x3809 showpostresponse (variable)
    -1,  // 0x380A userlist (variable)
    6,   // 0x380B boardpostresponse
    6,   // 0x380C nmailwriteresponse
    8,   // 0x380D findmp
    6,   // 0x380E setmp
    -1,  // 0x380F readpost (variable)
    255, // 0x3810 unused
    30,  // 0x3811
];

pub async fn dispatch(state: &Arc<MapState>, cmd: u16, pkt: &[u8]) {
    match cmd {
        0x3800 => handle_accept(state, pkt).await,
        0x3801 => { /* mapset — no-op in C, intif_parse_mapset is commented out */ }
        0x3802 => handle_authadd(state, pkt).await,
        0x3803 => handle_charload(state, pkt).await,
        0x3804 => handle_checkonline(state, pkt).await,
        0x3808..=0x380F => forward_to_c(state, cmd, pkt).await,
        _ => tracing::warn!("[map] [charif] unhandled cmd={:04X}", cmd),
    }
}

/// 0x3800 — char_server accepted our registration.
/// C: intif_parse_accept — sends back 0x3001 with map list.
async fn handle_accept(state: &Arc<MapState>, pkt: &[u8]) {
    if pkt.len() < 4 { return; }
    if pkt[2] != 0 {
        tracing::warn!("[map] [charif] char_server rejected connection result={}", pkt[2]);
        return;
    }
    let server_id = pkt[3];
    tracing::info!("[map] [charif] Connected to Char Server server_id={}", server_id);

    // Send 0x3001: register our map list.
    // Packet: [0..2]=0x3001 cmd, [2..6]=total_len, [6..8]=map_count, [8..]=map_ids (u16 each)
    // Sending empty list — will be filled once FFI for map enumeration is available.
    let map_count: u16 = 0;
    let total_len: u32 = 8 + (map_count as u32) * 2;
    let mut resp = Vec::with_capacity(total_len as usize);
    resp.extend_from_slice(&0x3001u16.to_le_bytes());
    resp.extend_from_slice(&total_len.to_le_bytes());
    resp.extend_from_slice(&map_count.to_le_bytes());
    send_to_char(state, resp).await;
}

/// 0x3802 — char_server is routing a player to this map server.
/// C: intif_parse_authadd — adds to auth_db, sends 0x3002 ack with char name.
async fn handle_authadd(state: &Arc<MapState>, pkt: &[u8]) {
    if pkt.len() < 38 { return; }
    // Layout: [2..4]=session_fd, [4..8]=account_id, [8..24]=char_name (16 bytes),
    //         [34..38]=client_ip
    let session_fd  = u16::from_le_bytes([pkt[2], pkt[3]]);
    let account_id  = u32::from_le_bytes([pkt[4], pkt[5], pkt[6], pkt[7]]);
    let char_name   = read_str(pkt, 8, 16);
    let client_ip   = u32::from_le_bytes([pkt[34], pkt[35], pkt[36], pkt[37]]);

    {
        let mut auth = state.auth_db.lock().await;
        auth.insert(char_name.clone(), super::AuthEntry {
            char_name: char_name.clone(),
            account_id,
            client_ip,
            expires: std::time::Instant::now() + std::time::Duration::from_secs(30),
        });
    }

    // Ack: 0x3002 (20 bytes): [0..2]=cmd, [2..4]=session_fd, [4..20]=char_name (16 bytes)
    let mut resp = vec![0u8; 20];
    resp[0] = 0x02; resp[1] = 0x30; // 0x3002 LE
    resp[2] = pkt[2]; resp[3] = pkt[3]; // session_fd passthrough
    let nb = char_name.as_bytes();
    resp[4..4 + nb.len().min(16)].copy_from_slice(&nb[..nb.len().min(16)]);
    tracing::info!("[map] [charif] authadd name={} session_fd={}", char_name, session_fd);
    send_to_char(state, resp).await;
}

/// 0x3803 — char_server sent a zlib-compressed mmo_charstatus for a player session.
/// C: intif_parse_charload — decompresses and calls intif_mmo_tosd(fd, status).
/// We decompress here; the FFI call to game logic is a TODO until map_parse.c is ported.
async fn handle_charload(_state: &Arc<MapState>, pkt: &[u8]) {
    if pkt.len() < 8 { return; }
    let session_fd = u16::from_le_bytes([pkt[6], pkt[7]]);
    let compressed = &pkt[8..];

    use std::io::Read;
    use flate2::read::ZlibDecoder;
    let mut dec = ZlibDecoder::new(compressed);
    let mut raw = Vec::new();
    if dec.read_to_end(&mut raw).is_err() {
        tracing::warn!("[map] [charif] charload: zlib decompression failed");
        return;
    }
    // TODO: forward decompressed mmo_charstatus to C via rust_intif_mmo_tosd (added in Task 6)
    tracing::info!("[map] [charif] charload session_fd={} bytes={} (mmo_tosd TODO)", session_fd, raw.len());
}

/// 0x3804 — char_server is checking / forcing a player offline.
async fn handle_checkonline(_state: &Arc<MapState>, pkt: &[u8]) {
    if pkt.len() < 6 { return; }
    let char_id = u32::from_le_bytes([pkt[2], pkt[3], pkt[4], pkt[5]]);
    // TODO: kick the player from the map once map_parse.c FFI is wired
    tracing::info!("[map] [charif] checkonline char_id={} (kick TODO)", char_id);
}

/// Board/mail response packets (0x3808–0x380F) are forwarded to map_parse.c via C handler.
/// Once map_parse.c is ported, implement handlers here directly.
async fn forward_to_c(_state: &Arc<MapState>, cmd: u16, _pkt: &[u8]) {
    tracing::debug!("[map] [charif] forward_to_c cmd={:04X} (TODO: call C handler)", cmd);
}

fn read_str(src: &[u8], offset: usize, len: usize) -> String {
    let end = (offset + len).min(src.len());
    let s = &src[offset..end];
    let nul = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    String::from_utf8_lossy(&s[..nul]).into_owned()
}

pub async fn send_to_char(state: &Arc<MapState>, msg: Vec<u8>) {
    let ct = state.char_tx.lock().await;
    if let Some(tx) = ct.as_ref() {
        let _ = tx.send(msg).await;
    }
}

/// Expire auth tokens older than 30 seconds (mirrors C auth_timer).
pub async fn expire_auth(state: &Arc<MapState>) {
    let now = std::time::Instant::now();
    let mut auth = state.auth_db.lock().await;
    auth.retain(|_, e| e.expires > now);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_pkt_lens_accept() {
        assert_eq!(PKT_LENS[0], 4);
    }
    #[test]
    fn test_pkt_lens_authadd() {
        assert_eq!(PKT_LENS[2], 38);
    }
    #[test]
    fn test_pkt_lens_variable() {
        assert_eq!(PKT_LENS[1], -1);
    }
    #[test]
    fn test_parse_authadd_name() {
        let mut pkt = vec![0u8; 38];
        pkt[0] = 0x02; pkt[1] = 0x38;
        pkt[4..8].copy_from_slice(&42u32.to_le_bytes()); // account_id=42
        pkt[8..14].copy_from_slice(b"Yuria\0");
        let account_id = u32::from_le_bytes([pkt[4], pkt[5], pkt[6], pkt[7]]);
        let name = read_str(&pkt, 8, 16);
        assert_eq!(account_id, 42);
        assert_eq!(name, "Yuria");
    }
    #[test]
    fn test_read_str_nul_terminated() {
        let src = b"hello\0extra";
        assert_eq!(read_str(src, 0, 11), "hello");
    }
    #[test]
    fn test_read_str_full() {
        let src = b"abcdefghijklmnop";
        assert_eq!(read_str(src, 0, 16), "abcdefghijklmnop");
    }
}
