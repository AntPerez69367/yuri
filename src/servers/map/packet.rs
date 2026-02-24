/// Packet length table for incoming 0x3800–0x3811 packets from char_server.
/// Index = cmd - 0x3800. -1 = variable (read 4-byte len at offset 2). 0 = unknown.
pub const PKT_LENS: &[i32] = &[
    4,   // 0x3800 accept (result + server_id)
    -1,  // 0x3801 mapset (variable)
    38,  // 0x3802 authadd (char_name + account_id + client_ip)
    -1,  // 0x3803 charload (variable, zlib-compressed mmo_charstatus)
    6,   // 0x3804 checkonline (char_id)
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
    -1,  // 0x380F readpost (variable — boards_read_post_1 + 2)
    255, // 0x3810 unused
    30,  // 0x3811
];

pub async fn dispatch(
    _state: &std::sync::Arc<super::MapState>,
    cmd: u16,
    _pkt: &[u8],
) {
    tracing::warn!("[map] [charif] unhandled cmd={:04X}", cmd);
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
}
