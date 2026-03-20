//! Session I/O tasks — accept loop, per-session read/write, flush, shutdown.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use super::{get_session_manager, setup_connection, SessionId, SessionManager, MAX_RDATA_SIZE};

/// Accept loop for a single listener socket
pub(crate) async fn accept_loop(listener: tokio::net::TcpListener, _listen_fd: i32) {
    let local_addr = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    tracing::info!(
        "[accept] Listening on fd={} addr={}",
        _listen_fd,
        local_addr
    );

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                let ip_net = match addr.ip() {
                    std::net::IpAddr::V4(ipv4) => u32::from(ipv4).to_be(),
                    _ => 0,
                };
                if crate::network::ddos::is_ip_locked(ip_net) {
                    tracing::warn!("[accept] DDoS-locked IP {}, refusing connection", addr);
                    continue;
                }
                if crate::network::throttle::is_throttled(ip_net) {
                    tracing::warn!("[accept] Throttled IP {}, refusing connection", addr);
                    continue;
                }
                apply_socket_opts(&stream);
                tracing::info!(
                    "[accept] New connection from {} on listener fd={}",
                    addr,
                    _listen_fd
                );
                tokio::task::spawn_local(session_io_task_from_accept(stream, addr));
            }
            Err(e) => {
                tracing::error!("[accept] fd={} accept error: {}", _listen_fd, e);
            }
        }
    }
}

/// Apply socket options for accepted connections.
fn apply_socket_opts(stream: &TcpStream) {
    let sock = socket2::SockRef::from(stream);
    sock.set_reuse_address(true).ok();
    #[cfg(target_os = "linux")]
    sock.set_reuse_port(true).ok();
    sock.set_linger(None).ok();
}

/// Set up session from an accepted connection and run its I/O task.
async fn session_io_task_from_accept(stream: TcpStream, addr: SocketAddr) {
    let manager = get_session_manager();
    let fd = match setup_connection(stream, addr, manager) {
        Ok(fd) => fd,
        Err(e) => {
            tracing::error!("[session] Failed to set up connection from {}: {}", addr, e);
            return;
        }
    };

    let accept_cb = {
        match manager.get_session(fd) {
            Some(arc) => arc.try_lock().ok().and_then(|s| s.callbacks.accept.clone()),
            None => None,
        }
    };
    if let Some(cb) = accept_cb {
        cb(fd).await;
        flush_wdata_to_socket(fd, manager).await;
    }

    session_io_task(fd).await;
}

/// Flush session write buffer to socket immediately.
async fn flush_wdata_to_socket(fd: SessionId, manager: &SessionManager) {
    let session_arc = match manager.get_session(fd) {
        Some(a) => a,
        None => return,
    };

    let (socket_arc, wdata) = {
        let mut session = session_arc.lock().await;
        let socket_arc = match session.socket.as_ref() {
            Some(s) => s.clone(),
            None => return,
        };
        let wdata = if session.wdata_size > 0 {
            let prev_size = session.wdata_size;
            let data = session.wdata[..prev_size].to_vec();
            session.wdata[..prev_size].fill(0);
            session.wdata_size = 0;
            data
        } else {
            return;
        };
        (socket_arc, wdata)
    };

    let mut socket = socket_arc.lock().await;
    if let Err(e) = socket.write_all(&wdata).await {
        tracing::error!("[session] fd={} flush write error: {}", fd, e);
        if let Some(arc) = manager.get_session(fd) {
            arc.lock().await.eof = 2;
        }
    }
}

/// Per-session I/O task.
pub(crate) async fn session_io_task(fd: SessionId) {
    let manager = get_session_manager();
    let session_arc = match manager.get_session(fd) {
        Some(s) => s,
        None => {
            tracing::error!("[session] fd={} not found in manager", fd);
            return;
        }
    };

    let connect_addr = {
        let session = session_arc.lock().await;
        if session.socket.is_none() {
            session.connect_addr
        } else {
            None
        }
    };

    if let Some(addr) = connect_addr {
        match TcpStream::connect(addr).await {
            Ok(stream) => {
                session_arc.lock().await.socket = Some(Arc::new(Mutex::new(stream)));
                tracing::info!("[session] fd={} connected to {}", fd, addr);
                flush_wdata_to_socket(fd, manager).await;
            }
            Err(e) => {
                tracing::error!("[session] fd={} connect to {} failed: {}", fd, addr, e);
                let shutdown_cb = {
                    let mut session = session_arc.lock().await;
                    if session.shutdown_called {
                        None
                    } else {
                        session.shutdown_called = true;
                        session.callbacks.shutdown.clone()
                    }
                };
                if let Some(cb) = shutdown_cb {
                    cb(fd).await;
                }
                manager.remove_session(fd);
                return;
            }
        }
    }

    let mut read_buf = vec![0u8; 4096];
    let write_notify = session_arc.lock().await.write_notify.clone();

    loop {
        let eof = {
            let session = session_arc.lock().await;
            session.eof
        };
        if eof != 0 {
            tracing::info!(
                "[session] fd={} server-initiated eof={}, invoking parse for cleanup",
                fd,
                eof
            );
            let parse_cb = {
                let session = session_arc.lock().await;
                session.callbacks.parse.clone()
            };
            if let Some(cb) = parse_cb {
                cb(fd).await;
            }
            break;
        }

        let socket_arc = {
            let session = session_arc.lock().await;
            match session.socket.as_ref() {
                Some(s) => s.clone(),
                None => break,
            }
        };

        enum Event {
            Read(std::io::Result<usize>),
            WriteReady,
        }

        let event = {
            let mut socket = socket_arc.lock().await;
            tokio::select! {
                result = socket.read(&mut read_buf) => Event::Read(result),
                _ = write_notify.notified() => Event::WriteReady,
            }
        };

        match event {
            Event::WriteReady => {
                flush_wdata_to_socket(fd, manager).await;
            }
            Event::Read(Ok(0)) => {
                {
                    let mut session = session_arc.lock().await;
                    session.eof = 4;
                }
                let parse_cb = {
                    let session = session_arc.lock().await;
                    session.callbacks.parse.clone()
                };
                if let Some(cb) = parse_cb {
                    cb(fd).await;
                }
                break;
            }
            Event::Read(Ok(n)) => {
                let overflow = {
                    let mut session = session_arc.lock().await;
                    let new_size = session.rdata_size + n;
                    if new_size > MAX_RDATA_SIZE {
                        tracing::warn!(
                            "[session] fd={} rdata overflow ({} bytes), closing connection",
                            fd,
                            new_size
                        );
                        session.eof = 3;
                        true
                    } else {
                        session.rdata.extend_from_slice(&read_buf[..n]);
                        session.rdata_size += n;
                        session.last_activity = Instant::now();
                        false
                    }
                };
                if overflow {
                    break;
                }

                let parse_cb = {
                    let session = session_arc.lock().await;
                    session.callbacks.parse.clone()
                };
                if let Some(cb) = parse_cb {
                    loop {
                        let available = {
                            let session = session_arc.lock().await;
                            session.available()
                        };
                        if available == 0 {
                            break;
                        }

                        let ret = cb(fd).await;
                        if ret == 2 {
                            break;
                        }

                        let (new_available, eof) = {
                            let session = session_arc.lock().await;
                            (session.available(), session.eof)
                        };
                        if eof != 0 || new_available >= available {
                            break;
                        }
                    }
                }

                flush_wdata_to_socket(fd, manager).await;

                {
                    let mut session = session_arc.lock().await;
                    session.flush_read_buffer();
                }
            }
            Event::Read(Err(e)) => {
                tracing::error!("[session] fd={} read error: {}", fd, e);
                let mut session = session_arc.lock().await;
                session.eof = 3;
                break;
            }
        }
    }

    let shutdown_cb = {
        let mut session = session_arc.lock().await;
        if session.shutdown_called {
            None
        } else {
            session.shutdown_called = true;
            session.callbacks.shutdown.clone()
        }
    };
    if let Some(cb) = shutdown_cb {
        cb(fd).await;
    }
    manager.remove_session(fd);
    tracing::info!("[session] fd={} closed", fd);
}

/// Shutdown all active sessions (called on server exit)
pub(crate) async fn shutdown_all_sessions() {
    tracing::info!("[engine] Shutting down all sessions");

    let manager = get_session_manager();
    let fds = manager.get_all_fds();

    for fd in fds {
        if let Some(session_arc) = manager.get_session(fd) {
            let shutdown_cb = {
                let mut session = session_arc.lock().await;
                if session.shutdown_called {
                    None
                } else {
                    session.shutdown_called = true;
                    session.callbacks.shutdown.clone()
                }
            };
            if let Some(cb) = shutdown_cb {
                tracing::debug!("[engine] Calling shutdown callback for fd={}", fd);
                cb(fd).await;
            }
            manager.remove_session(fd);
        }
    }
}
