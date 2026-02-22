use tokio::net::TcpStream;
use super::LoginState;

pub async fn dispatch_meta(_stream: &mut TcpStream, _pkt: &[u8], _state: &LoginState) {}
