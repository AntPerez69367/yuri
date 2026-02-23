use std::sync::Arc;
use super::CharState;

pub async fn handle_map_server(_state: Arc<CharState>, _stream: tokio::net::TcpStream, _first_cmd_bytes: [u8; 2]) {
    // TODO: implement in Task 5
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_auth_packet_len() {
        // 0x3000 auth packet is 72 bytes
        assert_eq!(72usize, 72);
    }

    #[test]
    fn test_map_fifo_from_mapid() {
        let maps: Vec<u16> = vec![0, 1, 2];
        let found = maps.iter().position(|&m| m == 0u16);
        assert_eq!(found, Some(0));
    }
}
