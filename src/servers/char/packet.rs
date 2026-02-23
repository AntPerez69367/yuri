use anyhow::{bail, Result};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

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

/// Read one 0xAA-framed packet (same as login/packet.rs).
pub async fn read_framed_packet(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let mut header = [0u8; 3];
    stream.read_exact(&mut header).await?;
    if header[0] != 0xAA {
        bail!("expected 0xAA header, got {:02X}", header[0]);
    }
    let payload_len = u16::from_be_bytes([header[1], header[2]]) as usize;
    let total = payload_len + 3;
    let mut buf = vec![0u8; total];
    buf[..3].copy_from_slice(&header);
    stream.read_exact(&mut buf[3..]).await?;
    Ok(buf)
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
