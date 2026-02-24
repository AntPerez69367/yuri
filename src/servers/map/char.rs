use std::sync::Arc;
use super::MapState;

pub async fn connect_to_char(state: Arc<MapState>) {
    use tokio::time::{Duration, interval};
    let mut ticker = interval(Duration::from_secs(1));
    loop {
        ticker.tick().await;
        {
            let tx = state.char_tx.lock().await;
            if tx.is_some() {
                continue; // already connected
            }
        }
        let addr = format!("{}:{}", state.config.char_ip, state.config.char_port);
        tracing::info!("[map] [charif] Connecting to char server at {}", addr);
        match tokio::net::TcpStream::connect(&addr).await {
            Ok(stream) => run_char_connection(Arc::clone(&state), stream).await,
            Err(e) => tracing::warn!("[map] [charif] Connect failed: {}", e),
        }
    }
}

async fn run_char_connection(_state: Arc<MapState>, _stream: tokio::net::TcpStream) {
    // TODO: implement in Task 2
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
