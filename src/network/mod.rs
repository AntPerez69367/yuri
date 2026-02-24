pub mod acl;
pub mod crypt;
pub mod ddos;
pub mod throttle;

use anyhow::{bail, Result};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

/// Read one 0xAA-framed packet from `stream`.
/// Returns the full buffer including the 3-byte header.
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
