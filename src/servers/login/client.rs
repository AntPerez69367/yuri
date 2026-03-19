use std::sync::Arc;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;

use super::{LoginState, LGN_ERRPASS, LGN_ERRUSER};
use super::packet::{read_client_packet, build_message, build_version_ok, build_version_patch};
use crate::network::crypt::tk_crypt_static;

#[derive(Default)]
struct SessionData {
    name: String,
    pass: String,
    face: u8,
    sex: u8,
    country: u8,
    totem: u8,
    hair: u8,
    hair_color: u8,
    face_color: u8,
}


pub fn is_valid_name(s: &str) -> bool {
    s.len() >= 3 && s.len() <= 12 && s.chars().all(|c| c.is_ascii_alphabetic())
}

pub fn is_valid_password(s: &str) -> bool {
    s.len() >= 3 && s.len() <= 8 && s.chars().all(|c| c.is_ascii_alphanumeric())
}

pub async fn handle_client(
    state: Arc<LoginState>,
    mut stream: TcpStream,
    peer: SocketAddr,
    session_id: u16,
    first_packet: Vec<u8>,
) {
    let mut sd = SessionData::default();
    let mut queue: Vec<Vec<u8>> = vec![first_packet];

    loop {
        let mut pkt = if let Some(p) = queue.pop() {
            p
        } else {
            match read_client_packet(&mut stream).await {
                Ok(p) => p,
                Err(e) => {
                    tracing::info!("[login] [client_disconnect] session={} peer={} reason={}", session_id, peer, e);
                    return;
                }
            }
        };

        // Decrypt packet in place
        let xk = state.config.xor_key.as_bytes().to_vec();
        tk_crypt_static(&mut pkt, &xk);

        if pkt.len() < 4 {
            tracing::warn!("[login] [short_packet] session={} len={} raw={:02X?}", session_id, pkt.len(), &pkt[..]);
            return;
        }

        let cmd = pkt[3];
        tracing::debug!("[login] [packet_in] session={} cmd={:02X}", session_id, cmd);

        match cmd {
            0x00 => dispatch_version_check(&mut stream, &pkt, &state).await,
            0x02 => dispatch_register(&mut stream, &pkt, &state, &mut sd, session_id).await,
            0x03 => dispatch_login(&mut stream, &pkt, &state, &mut sd, session_id, &peer).await,
            0x04 => dispatch_create_char(&mut stream, &pkt, &state, &mut sd, session_id).await,
            0x10 => dispatch_heartbeat(&mut stream).await,
            0x26 => dispatch_change_pass(&mut stream, &pkt, &state, &mut sd, session_id).await,
            0x57 | 0x71 | 0x62 => {
                tracing::debug!("[login] [client_ping] session={} cmd={:02X} raw={:02X?}",
                    session_id, cmd, &pkt[..pkt.len().min(16)]);
            }
            0x7B => super::meta::dispatch_meta(&mut stream, &pkt, &state).await,
            _ => tracing::warn!("[login] [packet_unknown] cmd={:02X} session={}", cmd, session_id),
        }
    }
}

async fn dispatch_version_check(stream: &mut TcpStream, pkt: &[u8], state: &LoginState) {
    if pkt.len() < 9 { return; }
    // The version check packet is sent unencrypted by the client.
    // handle_client applied tk_crypt_static globally; re-apply here to reverse it
    // (XOR is its own inverse), restoring the original bytes before reading.
    let mut pkt = pkt.to_vec();
    tk_crypt_static(&mut pkt, state.config.xor_key.as_bytes());
    let ver  = u16::from_be_bytes([pkt[4], pkt[5]]);
    let deep = u16::from_be_bytes([pkt[7], pkt[8]]);
    tracing::info!("[login] [version_check] client_version={} patch={}", ver, deep);

    let xk = &state.config.xor_key;
    let nex = state.config.version as u16;
    let response = if ver == nex {
        build_version_ok(xk)
    } else {
        build_version_patch(nex, "http://www.google.com")
    };
    let _ = stream.write_all(&response).await;
}

async fn dispatch_heartbeat(stream: &mut TcpStream) {
    let pkt: &[u8] = &[0xAA, 0x00, 0x07, 0x60, 0x00, 0x55, 0xE0, 0xD8, 0xA2, 0xA0];
    let _ = stream.write_all(pkt).await;
}

async fn dispatch_register(
    stream: &mut TcpStream,
    pkt: &[u8],
    state: &LoginState,
    sd: &mut SessionData,
    session_id: u16,
) {
    let xk = state.config.xor_key.as_bytes();
    if pkt.len() < 6 { return; }
    let name_len = pkt[5] as usize;
    if pkt.len() < 6 + name_len + 1 { return; }
    let name = std::str::from_utf8(&pkt[6..6 + name_len])
        .unwrap_or("").trim_end_matches('\0').to_string();

    if !is_valid_name(&name) {
        let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_ERRUSER], xk)).await;
        return;
    }

    let pass_len = pkt[6 + name_len] as usize;
    if pkt.len() < 7 + name_len + pass_len { return; }
    let pass = std::str::from_utf8(&pkt[7 + name_len..7 + name_len + pass_len])
        .unwrap_or("").trim_end_matches('\0').to_string();

    if !is_valid_password(&pass) {
        let _ = stream.write_all(&build_message(0x05, &state.messages.0[LGN_ERRPASS], xk)).await;
        return;
    }

    sd.name = name.clone();
    sd.pass = pass;

    let pool = &state.world.as_ref().unwrap().db;
    let used = crate::servers::char::db::is_name_used(pool, &name).await.unwrap_or(true);
    if used {
        let _ = stream.write_all(&build_message(0x03, &state.messages.0[super::LGN_USEREXIST], xk)).await;
    } else {
        let _ = stream.write_all(&build_message(0x00, "\x00", xk)).await;
    }

    let _ = session_id; // preserved for future logging
}

async fn dispatch_login(
    stream: &mut TcpStream,
    pkt: &[u8],
    state: &LoginState,
    sd: &mut SessionData,
    session_id: u16,
    peer: &SocketAddr,
) {
    let xk = state.config.xor_key.as_bytes();
    if pkt.len() < 6 { return; }
    let name_len = pkt[5] as usize;
    if pkt.len() < 6 + name_len + 1 { return; }
    let name = std::str::from_utf8(&pkt[6..6 + name_len])
        .unwrap_or("").trim_end_matches('\0').to_string();

    if !is_valid_name(&name) {
        let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_ERRUSER], xk)).await;
        return;
    }

    let pass_len = pkt[6 + name_len] as usize;
    if pkt.len() < 7 + name_len + pass_len { return; }
    let pass = std::str::from_utf8(&pkt[7 + name_len..7 + name_len + pass_len])
        .unwrap_or("").trim_end_matches('\0').to_string();

    if !is_valid_password(&pass) {
        let _ = stream.write_all(&build_message(0x05, &state.messages.0[LGN_ERRPASS], xk)).await;
        return;
    }

    // Maintenance and require_reg checks
    if let Some(pool) = &state.db {
        if super::db::get_maintenance_mode(pool).await {
            let gm = super::db::get_char_gm_level(pool, &name).await;
            if gm == 0 {
                let _ = stream.write_all(&build_message(0x03,
                    "Server is undergoing maintenance. Please visit www.website.com or the facebook group for more details.",
                    xk)).await;
                return;
            }
        }
        if state.config.require_reg != 0
            && super::db::get_account_for_char(pool, &name).await == 0 {
                let _ = stream.write_all(&build_message(0x03,
                    "You must attach your character to an account to play.\n\nPlease visit www.website.com to attach your character to an account.",
                    xk)).await;
                return;
            }
    }

    sd.name = name.clone();
    sd.pass = pass.clone();

    dispatch_login_direct(stream, state, session_id, peer, &name, &pass).await;
}

async fn dispatch_login_direct(
    stream: &mut TcpStream,
    state: &LoginState,
    session_id: u16,
    peer: &SocketAddr,
    name: &str,
    pass: &str,
) {
    let world = state.world.as_ref().unwrap();
    let xk = state.config.xor_key.as_bytes();
    let pool = &world.db;

    // Password verification (from char/login.rs:handle_login)
    let stored_hash = match crate::servers::char::db::get_char_password(pool, name).await {
        Ok(Some(h)) => h,
        Ok(None) => {
            let _ = stream.write_all(&build_message(0x03, &state.messages.0[super::LGN_WRONGUSER], xk)).await;
            return;
        }
        Err(e) => {
            tracing::error!("[login] [direct] db error: {}", e);
            let _ = stream.write_all(&build_message(0x03, &state.messages.0[super::LGN_ERRDB], xk)).await;
            return;
        }
    };

    let (mast_ok, mast_hash) = match crate::servers::char::db::get_master_password(pool).await {
        Ok(Some((mhash, exp))) => (crate::servers::char::db::ismastpass(pass, &mhash, exp).await, Some(mhash)),
        _ => (false, None),
    };

    if !crate::servers::char::db::ispass(name, pass, &stored_hash).await && !mast_ok {
        let _ = stream.write_all(&build_message(0x03, &state.messages.0[super::LGN_WRONGPASS], xk)).await;
        return;
    }

    // Bcrypt rehash (background, from char/login.rs:221-267)
    if !mast_ok && crate::servers::char::db::is_legacy_hash(&stored_hash) {
        let pool2 = pool.clone();
        let pass2 = pass.to_owned();
        let name2 = name.to_owned();
        tokio::spawn(async move {
            if let Ok(new_hash) = crate::servers::char::db::hash_password(&pass2).await {
                let _ = sqlx::query("UPDATE `Character` SET `ChaPassword` = ? WHERE `ChaName` = ?")
                    .bind(&new_hash).bind(&name2).execute(&pool2).await;
            }
        });
    }
    if mast_ok {
        if let Some(mhash) = mast_hash {
            if crate::servers::char::db::is_legacy_hash(&mhash) {
                let pool2 = pool.clone();
                let pass2 = pass.to_owned();
                tokio::spawn(async move {
                    if let Ok(new_hash) = crate::servers::char::db::hash_password(&pass2).await {
                        let _ = sqlx::query("UPDATE `AdminPassword` SET `AdmPassword` = ? WHERE `AdmId` = 1")
                            .bind(&new_hash).execute(&pool2).await;
                    }
                });
            }
        }
    }

    // Char lookup (from char/login.rs:269-273)
    let char_info = match crate::servers::char::db::char_login_lookup(pool, name).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            let _ = stream.write_all(&build_message(0x03, &state.messages.0[super::LGN_WRONGUSER], xk)).await;
            return;
        }
        Err(e) => {
            tracing::error!("[login] [direct] lookup error: {}", e);
            let _ = stream.write_all(&build_message(0x03, &state.messages.0[super::LGN_ERRDB], xk)).await;
            return;
        }
    };

    // Ban check (from char/login.rs:277-281)
    if char_info.banned || crate::servers::char::db::is_account_banned(pool, char_info.char_id).await {
        let _ = stream.write_all(&build_message(0x03, &state.messages.0[super::LGN_BANNED], xk)).await;
        return;
    }

    // Duplicate login check (from char/login.rs:297-313)
    if world.online.contains(&char_info.char_id) {
        let _ = stream.write_all(&build_message(0x03, &state.messages.0[super::LGN_DBLLOGIN], xk)).await;
        // Send kick request to LocalSet
        let _ = world.kick_tx.send(crate::world::KickRequest { char_id: char_info.char_id }).await;
        return;
    }

    // Insert auth token (replaces 0x3802 authadd)
    let normalized_name = name.to_lowercase();
    world.auth_db.insert(normalized_name, crate::world::AuthEntry {
        account_id: 0, // not used in current flow
        char_id: char_info.char_id,
        char_name: name.to_string(),
        client_ip: match peer.ip() {
            std::net::IpAddr::V4(v4) => u32::from(v4),
            _ => 0,
        },
        expires: std::time::Instant::now() + std::time::Duration::from_secs(30),
    });

    // Send auth success redirect to client (from interserver.rs:send_auth_success)
    super::interserver::send_auth_success_direct(stream, &state.config, name, session_id).await;
}

async fn dispatch_create_char(
    stream: &mut TcpStream,
    pkt: &[u8],
    state: &LoginState,
    sd: &mut SessionData,
    session_id: u16,
) {
    if sd.name.is_empty() || sd.pass.is_empty() { return; }
    if pkt.len() < 13 { return; }

    sd.face       = pkt[6];
    sd.hair       = pkt[7];
    sd.face_color = pkt[8];
    sd.hair_color = pkt[9];
    sd.sex        = pkt[10];
    sd.totem      = pkt[12];
    sd.country    = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() % 2) as u8;

    let xk = state.config.xor_key.as_bytes();

    let pool = &state.world.as_ref().unwrap().db;
    let cfg = &state.config;
    let res = crate::servers::char::db::create_char(
        pool,
        crate::servers::char::db::CreateCharParams {
            name: &sd.name, pass: &sd.pass,
            totem: sd.totem, sex: sd.sex % 2, country: sd.country,
            face: sd.face as u16, hair: sd.hair as u16,
            face_color: sd.face_color as u16, hair_color: sd.hair_color as u16,
            start_m: cfg.start_point.m as u32, start_x: cfg.start_point.x as u32, start_y: cfg.start_point.y as u32,
        },
    ).await;
    if res == 0 {
        let _ = stream.write_all(&build_message(0x00, &state.messages.0[super::LGN_NEWCHAR], xk)).await;
    } else {
        let _ = stream.write_all(&build_message(0x03, &state.messages.0[super::LGN_USEREXIST], xk)).await;
    }

    let _ = session_id; // preserved for future logging
}

async fn dispatch_change_pass(
    stream: &mut TcpStream,
    pkt: &[u8],
    state: &LoginState,
    sd: &mut SessionData,
    session_id: u16,
) {
    let xk = state.config.xor_key.as_bytes();
    if pkt.len() < 6 { return; }
    let name_len = pkt[5] as usize;
    if name_len > 16 { return; }
    let old_off = 6 + name_len;
    if pkt.len() <= old_off { return; }
    let old_pass_len = pkt[old_off] as usize;
    if old_pass_len > 16 { return; }
    let new_off = old_off + 1 + old_pass_len;
    if pkt.len() <= new_off { return; }
    let new_pass_len = pkt[new_off] as usize;
    if !(3..=8).contains(&new_pass_len) { return; }

    let name = std::str::from_utf8(&pkt[6..6 + name_len]).unwrap_or("").trim_end_matches('\0');
    if !is_valid_name(name) {
        let _ = stream.write_all(&build_message(0x03, &state.messages.0[LGN_ERRUSER], xk)).await;
        return;
    }

    sd.name = name.to_string();

    let pool = &state.world.as_ref().unwrap().db;
    let old_pass = std::str::from_utf8(&pkt[old_off + 1..old_off + 1 + old_pass_len])
        .unwrap_or("").trim_end_matches('\0');
    let new_pass = std::str::from_utf8(&pkt[new_off + 1..new_off + 1 + new_pass_len])
        .unwrap_or("").trim_end_matches('\0');
    let res = crate::servers::char::db::set_char_password(pool, name, old_pass, new_pass).await;
    match res {
         0 => { let _ = stream.write_all(&build_message(0x00, &state.messages.0[super::LGN_CHGPASS], xk)).await; }
        -2 => { let _ = stream.write_all(&build_message(0x03, &state.messages.0[super::LGN_WRONGUSER], xk)).await; }
        -3 => { let _ = stream.write_all(&build_message(0x03, &state.messages.0[super::LGN_WRONGPASS], xk)).await; }
         _ => { let _ = stream.write_all(&build_message(0x03, &state.messages.0[super::LGN_ERRDB], xk)).await; }
    }

    let _ = session_id; // preserved for future logging
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_name_chars_only_letters() {
        assert!(is_valid_name("Alice"));
        assert!(!is_valid_name("ali123"));
        assert!(!is_valid_name("a"));
    }

    #[test]
    fn test_valid_password_allows_alnum() {
        assert!(is_valid_password("abc123"));
        assert!(!is_valid_password("ab"));
        assert!(!is_valid_password("ab!"));
    }

    #[test]
    fn test_valid_name_length_bounds() {
        assert!(is_valid_name("abc"));          // min 3
        assert!(is_valid_name("abcdefghijkl")); // max 12
        assert!(!is_valid_name("ab"));           // too short
        assert!(!is_valid_name("abcdefghijklm")); // too long
    }
}
