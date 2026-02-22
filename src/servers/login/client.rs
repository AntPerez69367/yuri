// stub
use std::sync::Arc;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use super::LoginState;

pub async fn handle_client(
    _state: Arc<LoginState>,
    _stream: TcpStream,
    _peer: SocketAddr,
    _session_id: u16,
    _first_packet: Vec<u8>,
) {}
