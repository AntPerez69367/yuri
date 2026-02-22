// stub
use std::sync::Arc;
use tokio::net::TcpStream;
use super::LoginState;

pub async fn promote_to_charserver(
    _state: Arc<LoginState>,
    _stream: TcpStream,
    _first: Vec<u8>,
) {}

pub async fn dispatch_char_response(
    _stream: &mut tokio::net::TcpStream,
    _state: &super::LoginState,
    _resp: &super::CharResponse,
) {}
