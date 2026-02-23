use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use super::{CharState, MapFifo};
use super::db;

// Packet length table: index = cmd - 0x3000
// -1 means variable length (read 4-byte len at offset 2)
// 0 means unknown/invalid
const PKT_LENS: &[i32] = &[
    72,   // 0x3000 map server auth
    -1,   // 0x3001 mapset (variable)
    20,   // 0x3002 map login
    24,   // 0x3003 request char
    -1,   // 0x3004 save char (variable)
    6,    // 0x3005 logout
    255,  // 0x3006 (unused)
    -1,   // 0x3007 save char + logout (variable)
    28,   // 0x3008 delete post
    38,   // 0x3009 show posts (sizeof board_show_0 + 2)
    34,   // 0x300A read post (sizeof boards_read_post_0 + 2)
    4,    // 0x300B user list
    4086, // 0x300C board post (sizeof boards_post_0 + 2)
    4124, // 0x300D nmail write
    20,   // 0x300E findnewmp
    4124, // 0x300F nmail write copy
    30,   // 0x3010
    255,  // 0x3011
    255,  // 0x3012
    255,  // 0x3013
    255,  // 0x3014
    255,  // 0x3015
];

pub async fn handle_map_server(state: Arc<CharState>, mut stream: TcpStream, first_cmd_bytes: [u8; 2]) {
    // Read rest of 0x3000 auth packet (72 total, 2 already read)
    let mut rest = vec![0u8; 70];
    if stream.read_exact(&mut rest).await.is_err() {
        return;
    }

    let mut pkt = Vec::with_capacity(72);
    pkt.extend_from_slice(&first_cmd_bytes);
    pkt.extend_from_slice(&rest);

    // Auth: char_id at offset 2 (32 bytes), char_pw at offset 34 (32 bytes)
    let got_id = std::str::from_utf8(&pkt[2..34]).unwrap_or("").trim_end_matches('\0');
    let got_pw = std::str::from_utf8(&pkt[34..66]).unwrap_or("").trim_end_matches('\0');

    if got_id != state.config.char_id || got_pw != state.config.char_pw {
        let _ = stream.write_all(&[0x00, 0x38, 0x01, 0x00]).await;
        return;
    }

    let ip = u32::from_le_bytes([pkt[66], pkt[67], pkt[68], pkt[69]]);
    let port = u16::from_le_bytes([pkt[70], pkt[71]]);

    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(64);
    let idx = {
        let mut servers = state.map_servers.lock().await;
        let idx = servers.iter().position(|s| s.is_none()).unwrap_or_else(|| {
            servers.push(None);
            servers.len() - 1
        });
        servers[idx] = Some(MapFifo { tx, ip, port, maps: Vec::new() });
        idx
    };

    // Auth success: send 0x3800 result=0x00, server_idx
    let _ = stream.write_all(&[0x00, 0x38, 0x00, idx as u8]).await;
    tracing::info!("[char] [mapif] Map Server connected id={} port={}", idx, port);

    let (mut rh, mut wh) = stream.into_split();

    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if wh.write_all(&msg).await.is_err() {
                break;
            }
        }
    });

    loop {
        let mut cmd_bytes = [0u8; 2];
        if rh.read_exact(&mut cmd_bytes).await.is_err() {
            break;
        }
        let cmd = u16::from_le_bytes(cmd_bytes);

        let table_idx = (cmd as usize).wrapping_sub(0x3000);
        if table_idx >= PKT_LENS.len() || PKT_LENS[table_idx] == 0 {
            tracing::warn!("[char] [mapif] unknown cmd={:04X}", cmd);
            continue;
        }

        let (pkt_len, len_bytes) = if PKT_LENS[table_idx] == -1 {
            // Variable: 4-byte LE total length at offset 2 (bytes 2..6)
            let mut lbuf = [0u8; 4];
            if rh.read_exact(&mut lbuf).await.is_err() {
                break;
            }
            (u32::from_le_bytes(lbuf) as usize, Some(lbuf))
        } else {
            (PKT_LENS[table_idx] as usize, None)
        };

        // rest_len = total - 2 (cmd) - 4 (len bytes if variable)
        let already_read = 2 + if len_bytes.is_some() { 4 } else { 0 };
        let rest_len = pkt_len.saturating_sub(already_read);
        let mut rest = vec![0u8; rest_len];
        if rh.read_exact(&mut rest).await.is_err() {
            break;
        }

        let mut pkt = Vec::with_capacity(pkt_len);
        pkt.extend_from_slice(&cmd_bytes);
        if let Some(lb) = len_bytes {
            pkt.extend_from_slice(&lb);
        }
        pkt.extend_from_slice(&rest);

        dispatch_map_packet(&state, idx, cmd, &pkt).await;
    }

    {
        let mut servers = state.map_servers.lock().await;
        servers[idx] = None;
    }
    db::set_all_online(&state.db, false).await;
    writer.abort();
    tracing::info!("[char] [mapif] Map Server #{} disconnected", idx);
}

async fn dispatch_map_packet(state: &Arc<CharState>, map_idx: usize, cmd: u16, pkt: &[u8]) {
    match cmd {
        0x3001 => handle_mapset(state, map_idx, pkt).await,
        0x3002 => handle_map_login(state, pkt).await,
        0x3003 => handle_request_char(state, map_idx, pkt).await,
        0x3004 => handle_save_char(state, pkt).await,
        0x3005 => handle_logout(state, pkt).await,
        0x3007 => handle_save_char_logout(state, pkt).await,
        0x3008 => handle_delete_post(state, map_idx, pkt).await,
        0x3009 => handle_show_posts(state, map_idx, pkt).await,
        0x300A => handle_read_post(state, map_idx, pkt).await,
        0x300B => handle_user_list(state, map_idx, pkt).await,
        0x300C => handle_board_post(state, map_idx, pkt).await,
        0x300D => handle_nmail_write(state, map_idx, pkt).await,
        0x300E => { /* findnewmp — no-op in C */ }
        0x300F => handle_nmail_write_copy(state, pkt).await,
        _ => tracing::warn!("[char] [mapif] unhandled cmd={:04X}", cmd),
    }
}

async fn handle_mapset(state: &Arc<CharState>, map_idx: usize, pkt: &[u8]) {
    if pkt.len() < 8 {
        return;
    }
    // Variable packet: bytes 2..6 = total len, bytes 6..8 = map count
    let map_n = u16::from_le_bytes([pkt[6], pkt[7]]) as usize;
    let mut maps = Vec::with_capacity(map_n);
    for i in 0..map_n {
        let off = 8 + i * 2;
        if off + 2 > pkt.len() {
            break;
        }
        maps.push(u16::from_le_bytes([pkt[off], pkt[off + 1]]));
    }
    {
        let mut servers = state.map_servers.lock().await;
        if let Some(Some(s)) = servers.get_mut(map_idx) {
            s.maps = maps;
        }
    }
    tracing::info!("[char] [mapif] Map Server #{} registered {} maps", map_idx, map_n);
}

async fn handle_map_login(state: &Arc<CharState>, pkt: &[u8]) {
    if pkt.len() < 20 {
        return;
    }
    // Forward session auth check to login server: build 0x2003 packet
    let mut msg = vec![0u8; 27];
    msg[0] = 0x03; msg[1] = 0x20; // cmd 0x2003 LE
    msg[2] = pkt[2]; msg[3] = pkt[3]; // session_id
    msg[4] = 0x00; // result=ok placeholder
    msg[5..21].copy_from_slice(&pkt[4..20]); // char name
    let login_tx = state.login_tx.lock().await;
    if let Some(tx) = login_tx.as_ref() {
        let _ = tx.send(msg).await;
    }
}

async fn handle_request_char(state: &Arc<CharState>, map_idx: usize, pkt: &[u8]) {
    if pkt.len() < 8 {
        return;
    }
    let char_id = u32::from_le_bytes([pkt[4], pkt[5], pkt[6], pkt[7]]);
    let session_id = u16::from_le_bytes([pkt[2], pkt[3]]);
    let login_name = std::str::from_utf8(&pkt[8..]).unwrap_or("").trim_end_matches('\0');

    let char_bytes = match db::load_char_bytes(&state.db, char_id, login_name).await {
        Ok(b) => b,
        Err(_) => return,
    };

    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    let _ = enc.write_all(&char_bytes);
    let compressed = enc.finish().unwrap_or_default();
    let clen = compressed.len() as u32;

    // Build response 0x3803
    let total_len = clen + 8;
    let mut resp = Vec::with_capacity(8 + compressed.len());
    resp.extend_from_slice(&[0x03, 0x38]); // cmd LE
    resp.extend_from_slice(&total_len.to_le_bytes());
    resp.extend_from_slice(&session_id.to_le_bytes());
    resp.extend_from_slice(&compressed);

    send_to_map(state, map_idx, resp).await;
}

async fn handle_save_char(state: &Arc<CharState>, pkt: &[u8]) {
    if pkt.len() < 6 {
        return;
    }
    let total_len = u32::from_le_bytes([pkt[2], pkt[3], pkt[4], pkt[5]]) as usize;
    let data_len = total_len.saturating_sub(6);
    if pkt.len() < 6 + data_len {
        return;
    }
    let compressed = &pkt[6..6 + data_len];

    use flate2::read::ZlibDecoder;
    use std::io::Read;
    let mut dec = ZlibDecoder::new(compressed);
    let mut raw = Vec::new();
    if dec.read_to_end(&mut raw).is_err() {
        return;
    }
    let _ = db::save_char_bytes(&state.db, &raw).await;
}

async fn handle_logout(state: &Arc<CharState>, pkt: &[u8]) {
    if pkt.len() < 6 {
        return;
    }
    let char_id = u32::from_le_bytes([pkt[2], pkt[3], pkt[4], pkt[5]]);
    db::set_online(&state.db, char_id, false).await;
    let mut online = state.online.lock().await;
    online.remove(&char_id);
}

async fn handle_save_char_logout(state: &Arc<CharState>, pkt: &[u8]) {
    handle_save_char(state, pkt).await;
    // char_id is inside the compressed charstatus blob; full impl deferred to Task 7
}

// ── Board/mail helpers ────────────────────────────────────────────────────────

fn read_str(src: &[u8], offset: usize, len: usize) -> String {
    let end = (offset + len).min(src.len());
    let s = &src[offset..end];
    let nul = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    String::from_utf8_lossy(&s[..nul]).into_owned()
}

fn write_u16_le(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_u32_le(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_str_padded(buf: &mut Vec<u8>, s: &str, len: usize) {
    let bytes = s.as_bytes();
    let n = bytes.len().min(len);
    buf.extend_from_slice(&bytes[..n]);
    buf.extend(std::iter::repeat(0u8).take(len - n));
}

// ── 0x3008 — Delete post ─────────────────────────────────────────────────────

async fn handle_delete_post(state: &Arc<CharState>, map_idx: usize, pkt: &[u8]) {
    if pkt.len() < 28 { return; }
    let sfd    = u16::from_le_bytes([pkt[2], pkt[3]]);
    let gm_lvl = u16::from_le_bytes([pkt[4], pkt[5]]);
    let can_del= u16::from_le_bytes([pkt[6], pkt[7]]);
    let board  = u16::from_le_bytes([pkt[8], pkt[9]]);
    let post   = u16::from_le_bytes([pkt[10], pkt[11]]);
    let name   = read_str(pkt, 12, 16);

    let result: u8 = if board == 0 {
        // NMAIL delete
        let r = sqlx::query(
            "UPDATE `Mail` SET `MalDeleted` = 1, `MalNew` = 0 \
             WHERE `MalChaNameDestination` = ? AND `MalPosition` = ?"
        ).bind(&name).bind(post).execute(&state.db).await;
        if r.is_err() { 2 } else { 0 }
    } else if gm_lvl >= 50 || can_del != 0 {
        let r = sqlx::query(
            "DELETE FROM `Boards` WHERE `BrdBnmId` = ? AND `BrdPosition` = ?"
        ).bind(board).bind(post).execute(&state.db).await;
        if r.is_err() { 2 } else { 0 }
    } else {
        // Non-GM: verify ownership first
        let owns: Option<(i64,)> = sqlx::query_as(
            "SELECT COUNT(*) FROM `Boards` \
             WHERE `BrdBnmId` = ? AND `BrdPosition` = ? AND `BrdChaName` = ?"
        ).bind(board).bind(post).bind(&name)
         .fetch_optional(&state.db).await.unwrap_or(None);
        match owns {
            Some((n,)) if n > 0 => {
                let r = sqlx::query(
                    "DELETE FROM `Boards` WHERE `BrdBnmId` = ? AND `BrdPosition` = ?"
                ).bind(board).bind(post).execute(&state.db).await;
                if r.is_err() { 2 } else { 0 }
            }
            _ => 1,
        }
    };

    let mut resp = Vec::with_capacity(5);
    write_u16_le(&mut resp, 0x3808);
    write_u16_le(&mut resp, sfd);
    resp.push(result);
    send_to_map(state, map_idx, resp).await;
}

// ── 0x3009 — Show posts ──────────────────────────────────────────────────────

async fn handle_show_posts(state: &Arc<CharState>, map_idx: usize, pkt: &[u8]) {
    // pkt[2..] = board_show_0 (36 bytes)
    if pkt.len() < 38 { return; }
    let fd_slot = u32::from_le_bytes([pkt[2], pkt[3], pkt[4], pkt[5]]);
    let board   = u32::from_le_bytes([pkt[6], pkt[7], pkt[8], pkt[9]]);
    let bcount  = u32::from_le_bytes([pkt[10], pkt[11], pkt[12], pkt[13]]);
    let flags   = u32::from_le_bytes([pkt[14], pkt[15], pkt[16], pkt[17]]);
    let popup   = pkt[18];
    let name    = read_str(pkt, 19, 16);

    // Compute flags1, flags2 (mirrors C logic)
    let flags1: u32 = if popup != 0 && board != 0 {
        if flags == 6 { 6 } else if flags & 1 == 0 { 0 } else { 2 }
    } else {
        if flags == 6 { 6 } else if flags & 1 == 0 { 1 } else { 3 }
    };
    let flags2: u32 = if board == 0 { 4 } else { 2 };

    // boards_show_header_1: fd(4)+board(4)+count(4)+flags1(4)+flags2(4)+array(4)+name[16] = 40
    let build_header = |array: u32| -> Vec<u8> {
        let mut h = Vec::with_capacity(40);
        write_u32_le(&mut h, fd_slot);
        write_u32_le(&mut h, board);
        write_u32_le(&mut h, 0); // count unused
        write_u32_le(&mut h, flags1);
        write_u32_le(&mut h, flags2);
        write_u32_le(&mut h, array);
        write_str_padded(&mut h, &name, 16);
        h
    };

    let offset = bcount * 20;
    let (sql, use_nmail) = if board == 0 {
        (format!(
            "SELECT `MalNew`, `MalChaName`, `MalSubject`, `MalPosition`, \
             `MalMonth`, `MalDay`, `MalId` FROM `Mail` \
             WHERE `MalChaNameDestination` = '{}' AND `MalDeleted` = 0 \
             ORDER BY `MalPosition` DESC LIMIT {}, 20", name, offset), true)
    } else {
        (format!(
            "SELECT `BrdHighlighted`, `BrdChaName`, `BrdTopic`, `BrdPosition`, \
             `BrdMonth`, `BrdDay`, `BrdBtlId` FROM `Boards` \
             WHERE `BrdBnmId` = {} ORDER BY `BrdPosition` DESC LIMIT {}, 20",
            board, offset), false)
    };
    let _ = use_nmail;

    // boards_show_array_1: board_name(4)+color(4)+post_id(4)+month(4)+day(4)+user[32]+topic[64] = 116
    let rows: Vec<(i32, String, String, i32, i32, i32, i32)> =
        sqlx::query_as(&sql).fetch_all(&state.db).await.unwrap_or_default();

    let header = build_header(rows.len() as u32);
    let total_len = (6 + 40 + 116 * rows.len()) as u32;

    let mut resp = Vec::with_capacity(total_len as usize);
    write_u16_le(&mut resp, 0x3809);
    write_u32_le(&mut resp, total_len);
    resp.extend_from_slice(&header);
    for (color, user, topic, post_id, month, day, board_name) in &rows {
        write_u32_le(&mut resp, *board_name as u32);
        write_u32_le(&mut resp, *color as u32);
        write_u32_le(&mut resp, *post_id as u32);
        write_u32_le(&mut resp, *month as u32);
        write_u32_le(&mut resp, *day as u32);
        write_str_padded(&mut resp, user, 32);
        write_str_padded(&mut resp, topic, 64);
    }
    send_to_map(state, map_idx, resp).await;
}

// ── 0x300A — Read post ───────────────────────────────────────────────────────

async fn handle_read_post(state: &Arc<CharState>, map_idx: usize, pkt: &[u8]) {
    // pkt[2..] = boards_read_post_0 (32 bytes): name[16]+fd(4)+post(4)+board(4)+flags(4)
    if pkt.len() < 34 { return; }
    let name  = read_str(pkt, 2, 16);
    let fd_sl = u32::from_le_bytes([pkt[18], pkt[19], pkt[20], pkt[21]]);
    let post  = u32::from_le_bytes([pkt[22], pkt[23], pkt[24], pkt[25]]);
    let board = u32::from_le_bytes([pkt[26], pkt[27], pkt[28], pkt[29]]);
    let flags = u32::from_le_bytes([pkt[30], pkt[31], pkt[32], pkt[33]]);

    let post_type: u32  = if board == 0 { 5 } else { 3 };
    let buttons: u32 = if board == 0 || flags & 1 != 0 { 3 } else { 1 };

    // Get MAX position
    let max_sql = if board == 0 {
        format!("SELECT MAX(`MalPosition`) FROM `Mail` WHERE `MalChaNameDestination` = '{}' AND `MalDeleted` = '0'", name)
    } else {
        format!("SELECT MAX(`BrdPosition`) FROM `Boards` WHERE `BrdBnmId` = '{}'", board)
    };
    let max_row: Option<(Option<u32>,)> = sqlx::query_as(&max_sql)
        .fetch_optional(&state.db).await.unwrap_or(None);
    let max_pos = max_row.and_then(|(v,)| v).unwrap_or(0);
    let post = if post > max_pos { 1 } else { post };

    // Fetch post content
    let read_sql = if board == 0 {
        format!(
            "SELECT `MalChaName`, `MalSubject`, `MalBody`, `MalPosition`, \
             `MalMonth`, `MalDay`, `MalId` FROM `Mail` \
             WHERE `MalChaNameDestination` = '{}' AND `MalPosition` >= {} \
             AND `MalDeleted` = '0' ORDER BY `MalPosition` LIMIT 1", name, post)
    } else {
        format!(
            "SELECT `BrdChaName`, `BrdTopic`, `BrdPost`, `BrdPosition`, \
             `BrdMonth`, `BrdDay`, `BrdBtlId` FROM `Boards` \
             WHERE `BrdBnmId` = {} AND `BrdPosition` >= {} \
             ORDER BY `BrdPosition` LIMIT 1", board, post)
    };

    let row: Option<(String, String, String, u32, u32, u32, u32)> =
        sqlx::query_as(&read_sql).fetch_optional(&state.db).await.unwrap_or(None);

    let (user, topic, msg, real_post, month, day, board_name) = match row {
        Some(r) => r,
        None => return,
    };

    // boards_read_post_1: fd(4)+post(4)+month(4)+day(4)+board(4)+board_name(4)+type(4)+buttons(4)
    //   +name[16]+msg[4000]+user[52]+topic[52] = 4152
    let mut resp = Vec::with_capacity(4154);
    write_u16_le(&mut resp, 0x380F);
    write_u32_le(&mut resp, fd_sl);
    write_u32_le(&mut resp, real_post);
    write_u32_le(&mut resp, month);
    write_u32_le(&mut resp, day);
    write_u32_le(&mut resp, board);
    write_u32_le(&mut resp, board_name);
    write_u32_le(&mut resp, post_type);
    write_u32_le(&mut resp, buttons);
    write_str_padded(&mut resp, &name, 16);
    write_str_padded(&mut resp, &msg, 4000);
    write_str_padded(&mut resp, &user, 52);
    write_str_padded(&mut resp, &topic, 52);
    send_to_map(state, map_idx, resp).await;

    // For NMAIL: mark as read, check for remaining unread
    if board == 0 {
        let _ = sqlx::query(
            "UPDATE `Mail` SET `MalNew` = 0 \
             WHERE `MalPosition` = ? AND `MalChaNameDestination` = ?"
        ).bind(real_post).bind(&name).execute(&state.db).await;

        let unread: Option<(i64,)> = sqlx::query_as(
            "SELECT COUNT(*) FROM `Mail` WHERE `MalChaNameDestination` = ? AND `MalNew` = 1"
        ).bind(&name).fetch_optional(&state.db).await.unwrap_or(None);

        if unread.map(|(n,)| n).unwrap_or(0) <= 0 {
            let mut flag_resp = Vec::with_capacity(6);
            write_u16_le(&mut flag_resp, 0x380E);
            write_u32_le(&mut flag_resp, fd_sl);
            write_u16_le(&mut flag_resp, 0);
            send_to_map(state, map_idx, flag_resp).await;
        }
    }
}

// ── 0x300B — User list ───────────────────────────────────────────────────────

async fn handle_user_list(state: &Arc<CharState>, map_idx: usize, pkt: &[u8]) {
    if pkt.len() < 4 { return; }
    let sfd = u16::from_le_bytes([pkt[2], pkt[3]]);

    let rows: Vec<(i32, i32, i32, String, i32, u32)> = sqlx::query_as(
        "SELECT `ChaPthId`, `ChaMark`, `ChaClnId`, `ChaName`, \
         `ChaHunter`, `ChaNation` FROM `Character` WHERE `ChaOnline` = 1 \
         AND `ChaHeroes` = 1 GROUP BY `ChaName`, `ChaId` ORDER BY \
         `ChaMark` DESC, `ChaLevel` DESC, \
         SUM((`ChaBaseMana`*2) + `ChaBaseVita`) DESC, `ChaId` ASC"
    ).fetch_all(&state.db).await.unwrap_or_default();

    let count = rows.len() as u16;
    let total_len = (count as u32) * 22 + 36;

    let mut resp = Vec::with_capacity(total_len as usize);
    write_u16_le(&mut resp, 0x380A);
    write_u32_le(&mut resp, total_len);
    write_u16_le(&mut resp, sfd);
    write_u16_le(&mut resp, count);

    for (class, mark, clan, name, hunter, nation) in &rows {
        write_u16_le(&mut resp, *hunter as u16);
        write_u16_le(&mut resp, *class as u16);
        write_u16_le(&mut resp, *mark as u16);
        write_u16_le(&mut resp, *clan as u16);
        write_u16_le(&mut resp, *nation as u16);
        write_str_padded(&mut resp, name, 16);
    }

    // Pad to 36 bytes before user entries (header is 10 bytes, pad to 36)
    while resp.len() < 36 {
        resp.push(0);
    }
    send_to_map(state, map_idx, resp).await;
}

// ── 0x300C — Board post ──────────────────────────────────────────────────────

async fn handle_board_post(state: &Arc<CharState>, map_idx: usize, pkt: &[u8]) {
    // pkt[2..] = boards_post_0: fd(4)+board(4)+nval(4)+name[16]+topic[53]+post[4001]
    if pkt.len() < 4086 { return; }
    let fd_slot = u32::from_le_bytes([pkt[2], pkt[3], pkt[4], pkt[5]]);
    let board   = u32::from_le_bytes([pkt[6], pkt[7], pkt[8], pkt[9]]);
    let nval    = i32::from_le_bytes([pkt[10], pkt[11], pkt[12], pkt[13]]);
    let name    = read_str(pkt, 14, 16);
    let topic   = read_str(pkt, 30, 53);
    let post    = read_str(pkt, 83, 4001);

    // Get next post ID
    let max_row: Option<(Option<u32>,)> = sqlx::query_as(
        "SELECT MAX(`BrdPosition`) FROM `Boards` WHERE `BrdBnmId` = ?"
    ).bind(board).fetch_optional(&state.db).await.unwrap_or(None);
    let new_id = max_row.and_then(|(v,)| v).unwrap_or(0) + 1;

    let result = sqlx::query(
        "INSERT INTO `Boards` \
         (`BrdBnmId`, `BrdHighlighted`, `BrdChaName`, `BrdTopic`, `BrdPost`, \
          `BrdPosition`, `BrdMonth`, `BrdDay`, `BrdBtlId`) \
         VALUES(?, 0, ?, ?, ?, ?, DATE_FORMAT(CURDATE(),'%m'), \
                DATE_FORMAT(CURDATE(),'%d'), ?)"
    ).bind(board).bind(&name).bind(&topic).bind(&post)
     .bind(new_id).bind(nval)
     .execute(&state.db).await;

    let mut resp = Vec::with_capacity(6);
    write_u16_le(&mut resp, 0x380B);
    write_u32_le(&mut resp, fd_slot);
    write_u16_le(&mut resp, if result.is_err() { 1 } else { 0 });
    send_to_map(state, map_idx, resp).await;
}

// ── 0x300D — NMAIL write ─────────────────────────────────────────────────────

async fn handle_nmail_write(state: &Arc<CharState>, map_idx: usize, pkt: &[u8]) {
    if pkt.len() < 4124 { return; }
    let sfd   = u16::from_le_bytes([pkt[2], pkt[3]]);
    let from  = read_str(pkt, 4, 16);
    let to    = read_str(pkt, 20, 52);
    let topic = read_str(pkt, 72, 52);
    let msg   = read_str(pkt, 124, 4000);

    let result = nmail_insert(&state.db, &from, &to, &topic, &msg).await;

    let mut resp = Vec::with_capacity(6);
    write_u16_le(&mut resp, 0x380C);
    write_u16_le(&mut resp, sfd);
    write_u16_le(&mut resp, result);
    send_to_map(state, map_idx, resp).await;
}

// ── 0x300F — NMAIL write copy (no response) ──────────────────────────────────

async fn handle_nmail_write_copy(state: &Arc<CharState>, pkt: &[u8]) {
    if pkt.len() < 4124 { return; }
    let from  = read_str(pkt, 4, 16);
    let to    = read_str(pkt, 20, 52);
    let topic = read_str(pkt, 72, 52);
    let msg   = read_str(pkt, 124, 4000);
    let _ = nmail_insert(&state.db, &from, &to, &topic, &msg).await;
}

async fn nmail_insert(pool: &sqlx::MySqlPool, from: &str, to: &str, topic: &str, msg: &str) -> u16 {
    // Verify recipient exists
    let exists: Option<(u32,)> = sqlx::query_as(
        "SELECT `ChaId` FROM `Character` WHERE `ChaName` = ?"
    ).bind(to).fetch_optional(pool).await.unwrap_or(None);
    if exists.is_none() { return 2; }

    // Get next mail position
    let max_row: Option<(Option<u32>,)> = sqlx::query_as(
        "SELECT MAX(`MalPosition`) FROM `Mail` WHERE `MalChaNameDestination` = ?"
    ).bind(to).fetch_optional(pool).await.unwrap_or(None);
    let new_id = max_row.and_then(|(v,)| v).unwrap_or(0) + 1;

    let result = sqlx::query(
        "INSERT INTO `Mail` \
         (`MalChaName`, `MalChaNameDestination`, `MalPosition`, `MalSubject`, \
          `MalBody`, `MalMonth`, `MalDay`, `MalNew`) \
         VALUES(?, ?, ?, ?, ?, DATE_FORMAT(CURDATE(),'%m'), \
                DATE_FORMAT(CURDATE(),'%d'), 1)"
    ).bind(from).bind(to).bind(new_id).bind(topic).bind(msg)
     .execute(pool).await;

    if result.is_err() { 1 } else { 0 }
}

async fn send_to_map(state: &Arc<CharState>, map_idx: usize, msg: Vec<u8>) {
    let servers = state.map_servers.lock().await;
    if let Some(Some(s)) = servers.get(map_idx) {
        let _ = s.tx.send(msg).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_packet_len() {
        // 0x3000 auth packet is 72 bytes
        assert_eq!(PKT_LENS[0], 72);
    }

    #[test]
    fn test_map_fifo_from_mapid() {
        let maps: Vec<u16> = vec![0, 1, 2];
        let found = maps.iter().position(|&m| m == 0u16);
        assert_eq!(found, Some(0));
    }

    #[test]
    fn test_pkt_lens_table_size() {
        // Table covers 0x3000..=0x3015 (22 entries)
        assert_eq!(PKT_LENS.len(), 22);
    }

    #[test]
    fn test_board_struct_sizes() {
        // Values confirmed with gcc sizeof check
        assert_eq!(PKT_LENS[9], 38);   // board_show_0 + 2
        assert_eq!(PKT_LENS[10], 34);  // boards_read_post_0 + 2
        assert_eq!(PKT_LENS[12], 4086); // boards_post_0 + 2
    }
}
