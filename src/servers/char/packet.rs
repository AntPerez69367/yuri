use anyhow::Result;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

pub use crate::network::read_framed_packet;

/// Read one raw packet from a plain (non-0xAA-framed) interserver stream.
/// Reads exactly `len` bytes starting with a 2-byte LE command already known.
pub async fn read_exact_bytes(stream: &mut TcpStream, len: usize) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    Ok(buf)
}

/// Read a 2-byte LE command word from a stream.
pub async fn read_cmd(stream: &mut TcpStream) -> Result<u16> {
    let mut b = [0u8; 2];
    stream.read_exact(&mut b).await?;
    Ok(u16::from_le_bytes(b))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_cmd_le_parse() {
        let bytes = [0x00u8, 0x30]; // 0x3000 in LE
        let cmd = u16::from_le_bytes(bytes);
        assert_eq!(cmd, 0x3000);
    }
}
