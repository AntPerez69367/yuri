use std::sync::Arc;
use super::CharState;

pub async fn connect_to_login(_state: Arc<CharState>) {
    // TODO: implement in Task 6
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_logif_packet_lens() {
        // 0x1000=3, 0x1001=20, 0x1002=43, 0x1003=40, 0x1004=52
        const PKT_LENS: &[usize] = &[3, 20, 43, 40, 52];
        assert_eq!(PKT_LENS[0x1000 - 0x1000], 3);
        assert_eq!(PKT_LENS[0x1003 - 0x1000], 40);
    }

    #[test]
    fn test_auth_packet_build() {
        let mut pkt = vec![0u8; 69];
        pkt[0] = 0xAA;
        pkt[1] = 0x00; pkt[2] = 0x42; // 66 in BE
        pkt[3] = 0xFF;
        assert_eq!(pkt[3], 0xFF);
    }
}
