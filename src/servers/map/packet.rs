use crate::network::crypt::encrypt;
use crate::game::map_parse::packet::{wfifop, wfifohead, wfifoset};
use crate::session::SessionId;

// ─── Packet builder ──────────────────────────────────────────────────────────

/// Safe builder for client-bound 0xAA packets.  Assembles the payload into a
/// `Vec<u8>`, then flushes to the session FIFO in a single `unsafe` block.
pub(crate) struct ClientPacket {
    pub(crate) buf: Vec<u8>,
}

impl ClientPacket {
    /// Start a new 0xAA/0x31 board packet with the given sub-type byte at [5].
    pub(crate) fn board(sub5: u8) -> Self {
        // [0]=0xAA, [1..2]=len placeholder, [3]=0x31, [4]=3, [5]=sub5
        let buf = vec![0xAA, 0, 0, 0x31, 3, sub5];
        Self { buf }
    }

    pub(crate) fn put_u8(&mut self, v: u8) { self.buf.push(v); }
    pub(crate) fn put_u16_be(&mut self, v: u16) { self.buf.extend_from_slice(&v.to_be_bytes()); }

    pub(crate) fn put_str(&mut self, s: &str) {
        let b = s.as_bytes();
        debug_assert!(b.len() <= 255, "put_str: string too long ({} bytes)", b.len());
        self.buf.push(b.len() as u8);
        self.buf.extend_from_slice(b);
    }

    pub(crate) fn put_str_u16_be(&mut self, s: &str) {
        let b = s.as_bytes();
        self.buf.extend_from_slice(&(b.len() as u16).to_be_bytes());
        self.buf.extend_from_slice(b);
    }

    /// Finalize length field and send to the player's session fd.
    /// [1..2] stores the payload length after [3] (i.e. buf.len() - 3),
    /// which encrypt() reads and adds 6 to compute total wire size.
    pub(crate) fn send(mut self, fd: SessionId) {
        let len = (self.buf.len() - 3) as u16;
        let len_be = len.to_be_bytes();
        self.buf[1] = len_be[0];
        self.buf[2] = len_be[1];

        unsafe {
            wfifohead(fd, self.buf.len() + 64);
            let p = wfifop(fd, 0);
            if p.is_null() { return; }
            std::ptr::copy_nonoverlapping(self.buf.as_ptr(), p, self.buf.len());
            let enc_len = encrypt(fd);
            if enc_len <= 0 {
                tracing::warn!("[map] [packet] encrypt failed fd={} rc={}", fd, enc_len);
                return;
            }
            wfifoset(fd, enc_len as usize);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_packet_board_header() {
        let pkt = ClientPacket::board(0);
        assert_eq!(&pkt.buf[..6], &[0xAA, 0, 0, 0x31, 3, 0]);
    }

    #[test]
    fn test_client_packet_put_str() {
        let mut pkt = ClientPacket::board(0);
        pkt.put_str("hello");
        assert_eq!(pkt.buf[6], 5); // length byte
        assert_eq!(&pkt.buf[7..12], b"hello");
    }
}
