use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use super::MapState;
use super::packet::{PKT_LENS, dispatch};

const MAX_PKT_LEN: usize = 16 * 1024 * 1024;

pub async fn connect_to_char(state: Arc<MapState>) {
    use tokio::time::{Duration, interval};
    let mut ticker = interval(Duration::from_secs(1));
    loop {
        ticker.tick().await;
        {
            let tx = state.char_tx.lock().await;
            if tx.is_some() { continue; }
        }
        let addr = format!("{}:{}", state.config.char_ip, state.config.char_port);
        tracing::info!("[map] [charif] Connecting to char server at {}", addr);
        match TcpStream::connect(&addr).await {
            Ok(stream) => run_char_connection(Arc::clone(&state), stream).await,
            Err(e) => tracing::warn!("[map] [charif] Connect failed: {}", e),
        }
    }
}

async fn run_char_connection(state: Arc<MapState>, mut stream: TcpStream) {
    // Send registration: 0x3000 (72 bytes)
    // [0..2]=cmd, [2..34]=char_id (32 bytes), [34..66]=char_pw (32 bytes),
    // [66..70]=map_ip (u32 LE), [70..72]=map_port (u16 LE)
    let mut pkt = vec![0u8; 72];
    pkt[0] = 0x00; pkt[1] = 0x30; // cmd 0x3000 LE
    let cid = state.config.char_id.as_bytes();
    let cpw = state.config.char_pw.as_bytes();
    pkt[2..2 + cid.len().min(32)].copy_from_slice(&cid[..cid.len().min(32)]);
    pkt[34..34 + cpw.len().min(32)].copy_from_slice(&cpw[..cpw.len().min(32)]);
    // map_ip as raw bytes in network byte order (big-endian) — matches C convention
    // where IPs are stored as u32 in network order and written directly to packets.
    let map_ip_u32: u32 = state.config.map_ip.parse::<std::net::Ipv4Addr>()
        .map(u32::from)
        .unwrap_or(0);
    pkt[66..70].copy_from_slice(&map_ip_u32.to_be_bytes());
    pkt[70..72].copy_from_slice(&state.config.map_port.to_le_bytes());

    if stream.write_all(&pkt).await.is_err() { return; }

    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(64);
    {
        let mut ct = state.char_tx.lock().await;
        *ct = Some(tx);
    }

    let (mut rh, mut wh) = stream.into_split();

    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if wh.write_all(&msg).await.is_err() { break; }
        }
    });

    loop {
        let mut cmd_bytes = [0u8; 2];
        if rh.read_exact(&mut cmd_bytes).await.is_err() { break; }
        let cmd = u16::from_le_bytes(cmd_bytes);

        let table_idx = (cmd as usize).wrapping_sub(0x3800);
        if table_idx >= PKT_LENS.len() || PKT_LENS[table_idx] == 0 {
            tracing::warn!("[map] [charif] unknown cmd={:04X}", cmd);
            break;
        }

        let (pkt_len, len_bytes) = if PKT_LENS[table_idx] == -1 {
            let mut lbuf = [0u8; 4];
            if rh.read_exact(&mut lbuf).await.is_err() { break; }
            let declared = u32::from_le_bytes(lbuf) as usize;
            if declared == 0 || declared > MAX_PKT_LEN {
                tracing::error!("[map] [charif] cmd={:04X} len={} out of range", cmd, declared);
                break;
            }
            (declared, Some(lbuf))
        } else {
            (PKT_LENS[table_idx] as usize, None)
        };

        let already_read = 2 + if len_bytes.is_some() { 4 } else { 0 };
        let rest_len = pkt_len.saturating_sub(already_read);
        let mut rest = vec![0u8; rest_len];
        if rh.read_exact(&mut rest).await.is_err() { break; }

        let mut full_pkt = Vec::with_capacity(pkt_len);
        full_pkt.extend_from_slice(&cmd_bytes);
        if let Some(lb) = len_bytes { full_pkt.extend_from_slice(&lb); }
        full_pkt.extend_from_slice(&rest);

        tracing::info!("[map] [charif] recv cmd={:04X} len={}", cmd, pkt_len);
        dispatch(&state, cmd, &full_pkt).await;
    }

    {
        let mut ct = state.char_tx.lock().await;
        *ct = None;
    }
    writer.abort();
    tracing::warn!("[map] [charif] Char server connection lost, reconnecting...");
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_reg_packet_layout() {
        let mut pkt = vec![0u8; 72];
        pkt[0] = 0x00; pkt[1] = 0x30;
        let cid = b"testid";
        pkt[2..2 + cid.len()].copy_from_slice(cid);
        assert_eq!(u16::from_le_bytes([pkt[0], pkt[1]]), 0x3000);
        assert_eq!(&pkt[2..8], b"testid");
        assert_eq!(pkt.len(), 72);
    }
}
