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

#[tokio::test]
async fn test_char_server_bad_auth_rejected() {
    use yuri::servers::login::packet::build_intif_auth_response;

    let addr = start_test_server().await;
    let mut char_client = TcpStream::connect(addr).await.unwrap();

    // drain banner
    let mut banner = vec![0u8; 22];
    char_client.read_exact(&mut banner).await.unwrap();

    // Send char auth packet: 0xAA-framed, cmd=0xFF, 69 bytes total, wrong credentials
    let mut auth = vec![0u8; 69];
    auth[0] = 0xAA;
    auth[1] = 0x00; auth[2] = 0x42; // payload len = 66 = 0x42
    auth[3] = 0xFF; // cmd
    auth[5..14].copy_from_slice(b"wrong_id\0");
    auth[37..46].copy_from_slice(b"wrong_pw\0");
    char_client.write_all(&auth).await.unwrap();

    // Expect reject: [0x00, 0x10, 0x01]
    let mut resp = vec![0u8; 3];
    char_client.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, build_intif_auth_response(false));
}
