use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

async fn start_test_server() -> std::net::SocketAddr {
    use yuri::servers::login::LoginState;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let state = Arc::new(LoginState::test_only());

    tokio::spawn(async move {
        loop {
            let (stream, peer) = listener.accept().await.unwrap();
            let s = Arc::clone(&state);
            tokio::spawn(async move {
                LoginState::handle_new_connection(s, stream, peer).await;
            });
        }
    });

    addr
}

#[tokio::test]
async fn test_connect_banner() {
    let addr = start_test_server().await;
    let mut client = TcpStream::connect(addr).await.unwrap();
    let mut banner = vec![0u8; 22];
    client.read_exact(&mut banner).await.unwrap();
    assert_eq!(banner[0], 0xAA, "banner must start with 0xAA");
}

#[tokio::test]
async fn test_version_check_ok() {
    let addr = start_test_server().await;
    let mut client = TcpStream::connect(addr).await.unwrap();

    // drain banner (22 bytes)
    let mut banner = vec![0u8; 22];
    client.read_exact(&mut banner).await.unwrap();

    // send version-check packet (cmd=0x00), nex_version matches default (0)
    let pkt: &[u8] = &[0xAA, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    client.write_all(pkt).await.unwrap();

    let mut resp = vec![0u8; 1];
    client.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp[0], 0xAA);
}

