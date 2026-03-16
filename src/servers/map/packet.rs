use std::io::Read;
use std::sync::Arc;

use flate2::read::ZlibDecoder;

use crate::database::board_db;
use crate::database::class_db;
use crate::database::map_db::{self, MAP_SLOTS};
use crate::game::map_char::intif_install_player;
use crate::game::map_server::nmail_sendmessage;
use crate::game::pc::{MapSessionData, FLAG_MAIL};
use crate::network::crypt::encrypt;
use crate::game::map_parse::packet::{wfifop, wfifohead, wfifoset};
use crate::session::{
    get_session_manager, session_exists, session_get_data,
    session_get_eof, session_set_eof, SessionId,
};
use super::{AuthEntry, MapState};

/// Packet length table for incoming 0x3800–0x3811 packets from char_server.
/// Index = cmd - 0x3800. -1 = variable (read 4-byte len at offset 2). 0 = unknown.
pub const PKT_LENS: &[i32] = &[
    4,   // 0x3800 accept
    -1,  // 0x3801 mapset (variable)
    38,  // 0x3802 authadd
    -1,  // 0x3803 charload (variable, zlib)
    6,   // 0x3804 checkonline
    -1,  // 0x3805 unused
    255, // 0x3806 unused
    -1,  // 0x3807 unused
    5,   // 0x3808 deletepostresponse
    -1,  // 0x3809 showpostresponse (variable)
    -1,  // 0x380A userlist (variable)
    6,   // 0x380B boardpostresponse
    6,   // 0x380C nmailwriteresponse
    8,   // 0x380D findmp
    6,   // 0x380E setmp
    4154, // 0x380F readpost (opcode(2) + boards_read_post_1(4152))
    255, // 0x3810 unused
    30,  // 0x3811
];

pub async fn dispatch(state: &Arc<MapState>, cmd: u16, pkt: &[u8]) {
    tracing::debug!("[map] [charif] dispatch cmd={:#06X} len={}", cmd, pkt.len());
    match cmd {
        0x3800 => handle_accept(state, pkt).await,
        0x3801 => { /* mapset — no-op, intif_parse_mapset is commented out */ }
        0x3802 => handle_authadd(state, pkt).await,
        0x3803 => handle_charload(state, pkt).await,
        0x3804 => handle_checkonline(pkt),
        0x3808 => handle_deletepost_response(pkt),
        0x3809 => handle_showposts_response(pkt),
        0x380A => handle_userlist_response(pkt),
        0x380B => handle_boardpost_response(pkt),
        0x380C => handle_nmailwrite_response(pkt),
        0x380D => { /* findmp — no action needed on map server */ }
        0x380E => handle_setmp(pkt),
        0x380F => handle_readpost_response(pkt),
        _ => tracing::warn!("[map] [charif] unhandled cmd={:04X}", cmd),
    }
}

// ─── Packet builder ──────────────────────────────────────────────────────────

/// Safe builder for client-bound 0xAA packets.  Assembles the payload into a
/// `Vec<u8>`, then flushes to the session FIFO in a single `unsafe` block.
struct ClientPacket {
    buf: Vec<u8>,
}

impl ClientPacket {
    /// Start a new 0xAA/0x31 board packet with the given sub-type byte at [5].
    fn board(sub5: u8) -> Self {
        // [0]=0xAA, [1..2]=len placeholder, [3]=0x31, [4]=3, [5]=sub5
        let buf = vec![0xAA, 0, 0, 0x31, 3, sub5];
        Self { buf }
    }

    fn put_u8(&mut self, v: u8) { self.buf.push(v); }
    fn put_u16_be(&mut self, v: u16) { self.buf.extend_from_slice(&v.to_be_bytes()); }

    fn put_str(&mut self, s: &str) {
        let b = s.as_bytes();
        debug_assert!(b.len() <= 255, "put_str: string too long ({} bytes)", b.len());
        self.buf.push(b.len() as u8);
        self.buf.extend_from_slice(b);
    }

    fn put_str_u16_be(&mut self, s: &str) {
        let b = s.as_bytes();
        self.buf.extend_from_slice(&(b.len() as u16).to_be_bytes());
        self.buf.extend_from_slice(b);
    }

    /// Finalize length field and send to the player's session fd.
    /// [1..2] stores the payload length after [3] (i.e. buf.len() - 3),
    /// which encrypt() reads and adds 6 to compute total wire size.
    fn send(mut self, fd: SessionId) {
        let len = (self.buf.len() - 3) as u16;
        let len_be = len.to_be_bytes();
        self.buf[1] = len_be[0];
        self.buf[2] = len_be[1];

        unsafe {
            wfifohead(fd, self.buf.len() + 64);
            let p = wfifop(fd, 0);
            if p.is_null() { return; }
            std::ptr::copy_nonoverlapping(self.buf.as_ptr(), p, self.buf.len());
            let enc_len = encrypt(fd);
            if enc_len <= 0 {
                tracing::warn!("[map] [packet] encrypt failed fd={} rc={}", fd, enc_len);
                return;
            }
            wfifoset(fd, enc_len as usize);
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Read a null-terminated string from a fixed-size `[i8; N]` field.
fn read_str(src: &[u8], offset: usize, len: usize) -> String {
    let end = (offset + len).min(src.len());
    let s = &src[offset..end];
    let nul = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    String::from_utf8_lossy(&s[..nul]).into_owned()
}

fn read_u16_le(pkt: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([pkt[off], pkt[off + 1]])
}

fn read_u32_le(pkt: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([pkt[off], pkt[off + 1], pkt[off + 2], pkt[off + 3]])
}

/// Look up a player's MapSessionData by fd.  Returns None if invalid.
fn get_sd(fd: SessionId) -> Option<&'static mut MapSessionData> {
    if fd.raw() <= 0 || !session_exists(fd) { return None; }
    let ptr = session_get_data(fd);
    if ptr.is_null() { return None; }
    Some(unsafe { &mut *ptr })
}

/// Send a board notification message to a player.
fn send_message(fd: SessionId, msg: &std::ffi::CStr, r#type: i32) {
    if let Some(sd) = get_sd(fd) {
        unsafe { nmail_sendmessage(sd, msg.as_ptr(), 6, r#type); }
    }
}

pub async fn send_to_char(state: &Arc<MapState>, msg: Vec<u8>) {
    let ct = state.char_tx.lock().await;
    if let Some(tx) = ct.as_ref() {
        let _ = tx.send(msg).await;
    }
}

/// Expire auth tokens older than 30 seconds.
pub async fn expire_auth(state: &Arc<MapState>) {
    let now = std::time::Instant::now();
    let mut auth = state.auth_db.lock().await;
    auth.retain(|_, e| e.expires > now);
}

// ─── Core handlers (0x3800–0x3804) ───────────────────────────────────────────

/// 0x3800 — char_server accepted our registration.  Sends back 0x3001 with map list.
async fn handle_accept(state: &Arc<MapState>, pkt: &[u8]) {
    if pkt.len() < 4 { return; }
    if pkt[2] != 0 {
        tracing::warn!("[map] [charif] char_server rejected connection result={}", pkt[2]);
        return;
    }
    tracing::info!("[map] [charif] Connected to Char Server server_id={}", pkt[3]);

    let map_ids: Vec<u16> = unsafe {
        let map_ptr = map_db::raw_map_ptr();
        let map_n = map_db::map_n.load(std::sync::atomic::Ordering::Relaxed) as usize;
        if map_ptr.is_null() {
            vec![]
        } else {
            (0..MAP_SLOTS)
                .filter(|&i| !(*map_ptr.add(i)).tile.is_null())
                .take(map_n)
                .map(|i| i as u16)
                .collect()
        }
    };

    let map_count = map_ids.len() as u16;
    let total_len = 8u32 + map_count as u32 * 2;
    let mut resp = Vec::with_capacity(total_len as usize);
    resp.extend_from_slice(&0x3001u16.to_le_bytes());
    resp.extend_from_slice(&total_len.to_le_bytes());
    resp.extend_from_slice(&map_count.to_le_bytes());
    for id in &map_ids {
        resp.extend_from_slice(&id.to_le_bytes());
    }
    tracing::info!("[map] [charif] sending map list count={}", map_count);
    send_to_char(state, resp).await;
}

/// 0x3802 — char_server is routing a player to this map server.
async fn handle_authadd(state: &Arc<MapState>, pkt: &[u8]) {
    tracing::info!("[map] [charif] handle_authadd len={}", pkt.len());
    if pkt.len() < 38 { return; }
    let session_fd = read_u16_le(pkt, 2);
    let account_id = read_u32_le(pkt, 4);
    let char_name = read_str(pkt, 8, 16);
    let client_ip = read_u32_le(pkt, 34);

    {
        let mut auth = state.auth_db.lock().await;
        auth.insert(char_name.clone(), AuthEntry {
            char_name: char_name.clone(),
            account_id,
            client_ip,
            expires: std::time::Instant::now() + std::time::Duration::from_secs(30),
        });
    }

    let mut resp = vec![0u8; 20];
    resp[0] = 0x02; resp[1] = 0x30;
    resp[2] = pkt[2]; resp[3] = pkt[3];
    let nb = char_name.as_bytes();
    resp[4..4 + nb.len().min(16)].copy_from_slice(&nb[..nb.len().min(16)]);
    tracing::info!("[map] [charif] authadd name={} session_fd={}", char_name, session_fd);
    send_to_char(state, resp).await;
}

/// 0x3803 — char_server sent a zlib-compressed mmo_charstatus for a player session.
async fn handle_charload(_state: &Arc<MapState>, pkt: &[u8]) {
    if pkt.len() < 8 { return; }
    let session_fd = read_u16_le(pkt, 6);
    let compressed = &pkt[8..];

    let mut dec = ZlibDecoder::new(compressed);
    let mut raw = Vec::new();
    if dec.read_to_end(&mut raw).is_err() {
        tracing::warn!("[map] [charif] charload: zlib decompression failed");
        return;
    }
    tracing::info!("[map] [charif] charload session_fd={} bytes={}", session_fd, raw.len());

    let fd = session_fd as i32;
    let sid = SessionId::from_raw(fd);

    let player: crate::common::player::PlayerData = match bincode::deserialize(&raw) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("[map] [charif] charload: bincode deserialize failed: {}", e);
            session_set_eof(sid, 7);
            return;
        }
    };

    // Suppress write notifications during spawn so packets flush as a single batch.
    let manager = get_session_manager();
    if let Some(session_arc) = manager.get_session(sid) {
        session_arc.lock().await.suppress_notify = true;
    }

    let rc = intif_install_player(fd, player);
    tracing::info!("[map] [charif] intif_install_player returned rc={}", rc);

    if let Some(session_arc) = manager.get_session(sid) {
        let mut session = session_arc.lock().await;
        session.suppress_notify = false;
        session.write_notify.notify_one();
    }
}

/// 0x3804 — char_server is forcing a player offline (duplicate login detected).
fn handle_checkonline(pkt: &[u8]) {
    if pkt.len() < 6 { return; }
    let char_id = read_u32_le(pkt, 2);

    let manager = get_session_manager();
    for fd in manager.get_all_fds() {
        if !session_exists(fd) || session_get_eof(fd) != 0 { continue; }
        let Some(sd) = get_sd(fd) else { continue };
        if sd.player.identity.id == char_id {
            tracing::warn!("[map] [charif] checkonline: kicking char_id={} fd={}", char_id, fd);
            session_set_eof(fd, 1);
            return;
        }
    }
    tracing::debug!("[map] [charif] checkonline: char_id={} not found", char_id);
}

// ─── Board/mail response handlers (0x3808–0x380F) ────────────────────────────

/// 0x3808 — Delete post response.
/// [2..3]=fd (u16 LE), [4]=result (0=ok, 1=no permission, 2=error)
fn handle_deletepost_response(pkt: &[u8]) {
    if pkt.len() < 5 { tracing::warn!("[map] [packet] 0x3808 too short len={}", pkt.len()); return; }
    let fd = SessionId::from_raw(read_u16_le(pkt, 2) as i32);
    let (msg, r#type) = match pkt[4] {
        0 => (c"The message has been deleted.", 1),
        1 => (c"You can only delete your own messages.", 0),
        _ => (c"Something went wrong. Please try again later.", 0),
    };
    // other=7 tells the client to refresh the board after delete.
    if let Some(sd) = get_sd(fd) {
        unsafe { nmail_sendmessage(sd, msg.as_ptr(), 7, r#type); }
    }
}

/// 0x3809 — Show posts response (variable length).
///
/// Inter-server layout:
///   [6..45]  header: fd(4) + board(4) + count(4) + flags1(4) + flags2(4) + array_count(4) + name(16)
///   [46..]   entries (116 bytes each): btl_id(4) + color(4) + post_id(4) + month(4) + day(4) + user(32) + topic(64)
fn handle_showposts_response(pkt: &[u8]) {
    tracing::debug!("[map] [packet] 0x3809 showposts response len={} header_hex={:02X?}", pkt.len(), &pkt[..pkt.len().min(50)]);
    if pkt.len() < 46 { tracing::warn!("[map] [packet] 0x3809 too short len={}", pkt.len()); return; }

    let h = &pkt[6..];
    let fd          = SessionId::from_raw(read_u32_le(h, 0) as i32);
    let board       = read_u32_le(h, 4) as i32;
    let flags1      = read_u32_le(h, 12) as u8;
    let flags2      = read_u32_le(h, 16) as u8;
    let array_count = read_u32_le(h, 20) as usize;
    let player_name = read_str(pkt, 30, 16);

    tracing::debug!("[map] [packet] 0x3809 fd={} board={} flags1={} flags2={} count={} player={}", fd, board, flags1, flags2, array_count, player_name);

    let sd = match get_sd(fd) {
        Some(sd) => sd,
        None => { tracing::warn!("[map] [packet] 0x3809 invalid fd={}", fd); return; }
    };

    // Verify the response is for the right player (matches C: strcasecmp check).
    let sd_name = &sd.player.identity.name;
    if !sd_name.eq_ignore_ascii_case(&player_name) {
        tracing::warn!("[map] [packet] 0x3809 name mismatch sd={} pkt={}", sd_name, player_name);
        return;
    }

    // Board display name from local board_db (NOT the player name).
    let board_display_name = board_db::board_name(board);

    let mut pkt_out = ClientPacket::board(flags2);
    pkt_out.put_u8(flags1);
    pkt_out.put_u16_be(board as u16);
    pkt_out.put_str(&board_display_name);

    if array_count == 0 {
        pkt_out.put_u8(0);
    } else {
        pkt_out.put_u8(array_count as u8);

        for i in 0..array_count {
            let off = 46 + i * 116;
            if off + 116 > pkt.len() { break; }
            let e = &pkt[off..];

            let btl_id  = read_u32_le(e, 0) as i32;
            let color   = read_u32_le(e, 4) as u8;
            let post_id = read_u32_le(e, 8) as u16;
            let month   = read_u32_le(e, 12) as u8;
            let day     = read_u32_le(e, 16) as u8;
            let user    = read_str(pkt, off + 20, 32);
            let topic   = read_str(pkt, off + 52, 64);

            // Entry format (matches C intif_parse_showpostresponse):
            //   color(1) + post_id(u16 BE) + composed_user(len-prefixed) + month(1) + day(1) + topic(len-prefixed)
            pkt_out.put_u8(color);
            pkt_out.put_u16_be(post_id);

            // Compose user string: "{bn_name} {user}" for boards, just "{user}" for nmail.
            let composed_user = if board != 0 && btl_id != 0 {
                let bn = board_db::bn_name(btl_id);
                format!("{} {}", bn, user)
            } else {
                user
            };
            pkt_out.put_str(&composed_user);

            pkt_out.put_u8(month);
            pkt_out.put_u8(day);
            pkt_out.put_str(&topic);
        }
    }

    tracing::debug!("[map] [packet] 0x3809 sending client packet fd={} buf_len={} hex={:02X?}", fd, pkt_out.buf.len(), &pkt_out.buf[..pkt_out.buf.len().min(60)]);
    pkt_out.send(fd);

    // Advance pagination counter (used by boards_showposts for OFFSET).
    sd.bcount += 1;
}

/// 0x380A — User list response (variable length).
///
/// Inter-server layout:
///   [6..7]=fd (u16 LE), [8..9]=count (u16 LE),
///   [10..]=entries (22-byte stride): hunter(2)+class(2)+mark(2)+clan(2)+nation(2)+name(12)
///
/// Client packet: [3]=0x36, [5..6]=total(u16 BE), [7..8]=server(u16 BE), [9]=1,
///   then per entry: path_nation(1)+mark_icon(1)+hunter(1)+color(1)+name(len-prefixed)
fn handle_userlist_response(pkt: &[u8]) {
    if pkt.len() < 10 { tracing::warn!("[map] [packet] 0x380A too short len={}", pkt.len()); return; }
    let fd    = SessionId::from_raw(read_u16_le(pkt, 6) as i32);
    let count = read_u16_le(pkt, 8) as usize;

    let sd = match get_sd(fd) {
        Some(sd) => sd,
        None => { tracing::warn!("[map] [packet] 0x380A invalid fd={}", fd); return; }
    };
    let sd_clan = sd.player.social.clan as i32;

    // Client packet uses 0x36, not 0x31/3 board packet.
    let mut buf = vec![0xAAu8, 0, 0, 0x36, 0];
    // [5..6] = total user count (u16 BE)
    buf.extend_from_slice(&(count as u16).to_be_bytes());
    // [7..8] = server user count (u16 BE) — same as total for single-server
    buf.extend_from_slice(&(count as u16).to_be_bytes());
    // [9] = 1
    buf.push(1);

    for i in 0..count {
        let off = 10 + i * 22;
        if off + 22 > pkt.len() { break; }
        let e = &pkt[off..];

        let hunter  = read_u16_le(e, 0) as i32;
        let class   = read_u16_le(e, 2) as i32;
        let mark    = read_u16_le(e, 4) as i32;
        let clan    = read_u16_le(e, 6) as i32;
        let nation  = read_u16_le(e, 8) as i32;
        let name    = read_str(pkt, off + 10, 12);

        let path = if class > 4 { class_db::path(class) } else { class };
        let icon = class_db::icon(class);

        // path + 16*nation encodes the nation/class combo
        buf.push((path + 16 * nation) as u8);
        // 16*mark + icon encodes rank/class icon
        buf.push((16 * mark + icon) as u8);
        // hunter flag
        buf.push(hunter as u8);
        // color: white=143, same-clan=63, GM(path==5)=47
        let color = if class_db::path(class) == 5 {
            47
        } else if sd_clan != 0 && sd_clan == clan {
            63
        } else {
            143
        };
        buf.push(color);
        // name (len-prefixed)
        let name_bytes = name.as_bytes();
        buf.push(name_bytes.len() as u8);
        buf.extend_from_slice(name_bytes);
    }

    // Write length at [1..2] (BE) — counts bytes from [3] onward (buf.len() - 3).
    let len = (buf.len() - 3) as u16;
    buf[1] = (len >> 8) as u8;
    buf[2] = (len & 0xFF) as u8;

    // Send via encrypt
    unsafe {
        wfifohead(fd, buf.len() + 64);
        let p = wfifop(fd, 0);
        if p.is_null() { return; }
        std::ptr::copy_nonoverlapping(buf.as_ptr(), p, buf.len());
        let enc_len = encrypt(fd);
        if enc_len <= 0 {
            tracing::warn!("[map] [packet] 0x380A encrypt failed fd={}", fd);
            return;
        }
        wfifoset(fd, enc_len as usize);
    }
}

/// 0x380B — Board post response.
/// [2..3]=fd (u16 LE), [4..5]=result (u16 LE, 0=ok, 1=error)
fn handle_boardpost_response(pkt: &[u8]) {
    if pkt.len() < 6 { tracing::warn!("[map] [packet] 0x380B too short len={}", pkt.len()); return; }
    let fd = SessionId::from_raw(read_u16_le(pkt, 2) as i32);
    let result = read_u16_le(pkt, 4);
    let (msg, r#type) = match result {
        0 => (c"Your message has been posted.", 1),
        _ => (c"Something went wrong. Please try again later.", 0),
    };
    send_message(fd, msg, r#type);
}

/// 0x380C — Nmail write response.
/// [2..3]=fd (u16 LE), [4..5]=result (u16 LE, 0=ok, 1=error, 2=not found)
fn handle_nmailwrite_response(pkt: &[u8]) {
    if pkt.len() < 6 { tracing::warn!("[map] [packet] 0x380C too short len={}", pkt.len()); return; }
    let fd = SessionId::from_raw(read_u16_le(pkt, 2) as i32);
    let result = read_u16_le(pkt, 4);
    let (msg, r#type) = match result {
        0 => (c"Your message has been sent.", 1),
        2 => (c"User does not exist.", 0),
        _ => (c"Something went wrong. Please try again later.", 0),
    };
    send_message(fd, msg, r#type);
}

/// 0x380E — Set MP flag (clear new-mail indicator).
/// [2..3]=fd (u16 LE), [4..5]=value (u16 LE)
fn handle_setmp(pkt: &[u8]) {
    if pkt.len() < 6 { tracing::warn!("[map] [packet] 0x380E too short len={}", pkt.len()); return; }
    let fd = SessionId::from_raw(read_u16_le(pkt, 2) as i32);
    let value = read_u16_le(pkt, 4);
    if value == 0 {
        if let Some(sd) = get_sd(fd) {
            sd.flags &= !FLAG_MAIL;
        }
    }
}

/// 0x380F — Read post response (variable length).
///
/// Inter-server layout (4158 bytes):
///   [6..9]=fd, [10..13]=post, [14..17]=month, [18..21]=day,
///   [22..25]=board, [26..29]=btl_id, [30..33]=type, [34..37]=buttons,
///   [38..53]=name(16), [54..4053]=msg(4000), [4054..4105]=user(52), [4106..4157]=topic(52)
fn handle_readpost_response(pkt: &[u8]) {
    // Fixed-length packet: opcode(2) + boards_read_post_1(4152) = 4154.
    // Struct layout: fd(4)+post(4)+month(4)+day(4)+board(4)+board_name(4)+type(4)+buttons(4)
    //               +name[16]+msg[4000]+user[52]+topic[52]
    if pkt.len() < 4154 { tracing::warn!("[map] [packet] 0x380F too short len={}", pkt.len()); return; }

    let fd         = SessionId::from_raw(read_u32_le(pkt, 2) as i32);
    let post       = read_u32_le(pkt, 6) as u16;
    let month      = read_u32_le(pkt, 10) as u8;
    let day        = read_u32_le(pkt, 14) as u8;
    let board      = read_u32_le(pkt, 18);
    let _brd_name  = read_u32_le(pkt, 22);
    let post_type  = read_u32_le(pkt, 26) as u8;
    let buttons    = read_u32_le(pkt, 30) as u8;
    let name  = read_str(pkt, 34, 16);
    let msg   = read_str(pkt, 50, 4000);
    let user  = read_str(pkt, 4050, 52);
    let topic = read_str(pkt, 4102, 52);

    tracing::debug!("[map] [packet] 0x380F fd={} post={} board={} type={} buttons={} user={}", fd, post, board, post_type, buttons, user);

    let sd = match get_sd(fd) {
        Some(sd) => sd,
        None => { tracing::warn!("[map] [packet] 0x380F invalid fd={}", fd); return; }
    };

    // Verify player name matches.
    let sd_name = &sd.player.identity.name;
    if !sd_name.eq_ignore_ascii_case(&name) {
        tracing::warn!("[map] [packet] 0x380F name mismatch sd={} pkt={}", sd_name, name);
        return;
    }

    // Client packet format (from C intif_parse_readpost):
    //   [0]=0xAA, [1..2]=len(BE), [3]=0x31, [4]=unused, [5]=type, [6]=buttons
    //   [7]=nmail_flag (1 if board==0), [8..9]=post(u16 BE)
    //   user(len-prefixed), month(1), day(1), topic(len-prefixed), msg(u16-BE-prefixed)
    let mut pkt_out = ClientPacket::board(post_type);
    // Overwrite [4] — C code does NOT set [4]=3 for readpost.
    pkt_out.buf[4] = 0;
    pkt_out.put_u8(buttons);
    pkt_out.put_u8(if board == 0 { 1 } else { 0 });
    pkt_out.put_u16_be(post);
    pkt_out.put_str(&user);
    pkt_out.put_u8(month);
    pkt_out.put_u8(day);
    pkt_out.put_str(&topic);
    pkt_out.put_str_u16_be(&msg);

    tracing::debug!("[map] [packet] 0x380F sending client packet fd={} buf_len={}", fd, pkt_out.buf.len());
    pkt_out.send(fd);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkt_lens_accept() {
        assert_eq!(PKT_LENS[0], 4);
    }

    #[test]
    fn test_pkt_lens_authadd() {
        assert_eq!(PKT_LENS[2], 38);
    }

    #[test]
    fn test_pkt_lens_variable() {
        assert_eq!(PKT_LENS[1], -1);
    }

    #[test]
    fn test_parse_authadd_name() {
        let mut pkt = vec![0u8; 38];
        pkt[0] = 0x02; pkt[1] = 0x38;
        pkt[4..8].copy_from_slice(&42u32.to_le_bytes());
        pkt[8..14].copy_from_slice(b"Yuria\0");
        let account_id = read_u32_le(&pkt, 4);
        let name = read_str(&pkt, 8, 16);
        assert_eq!(account_id, 42);
        assert_eq!(name, "Yuria");
    }

    #[test]
    fn test_read_str_nul_terminated() {
        let src = b"hello\0extra";
        assert_eq!(read_str(src, 0, 11), "hello");
    }

    #[test]
    fn test_read_str_full() {
        let src = b"abcdefghijklmnop";
        assert_eq!(read_str(src, 0, 16), "abcdefghijklmnop");
    }

    #[test]
    fn test_client_packet_board_header() {
        let pkt = ClientPacket::board(0);
        assert_eq!(&pkt.buf[..6], &[0xAA, 0, 0, 0x31, 3, 0]);
    }

    #[test]
    fn test_client_packet_put_str() {
        let mut pkt = ClientPacket::board(0);
        pkt.put_str("hello");
        assert_eq!(pkt.buf[6], 5); // length byte
        assert_eq!(&pkt.buf[7..12], b"hello");
    }

    #[test]
    fn test_read_helpers() {
        let data = [0x34, 0x12, 0x78, 0x56];
        assert_eq!(read_u16_le(&data, 0), 0x1234);
        assert_eq!(read_u32_le(&data, 0), 0x56781234);
    }
}
