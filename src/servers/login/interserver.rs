// stub
use std::sync::Arc;
use tokio::net::TcpStream;
use super::LoginState;

pub async fn promote_to_charserver(
    _state: Arc<LoginState>,
    _stream: TcpStream,
    _first: Vec<u8>,
) {}
