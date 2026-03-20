//! Board and N-Mail system — post/read/delete/send operations.

use crate::common::constants::world::{BOARD_CAN_DEL, BOARD_CAN_WRITE};
use crate::database::{blocking_run_async, boards as db_boards, get_pool};
use crate::game::client::handlers::clif_Hacker;
use crate::game::game_registry::map_readglobalgamereg;
use crate::game::map_parse::packet::{rfifob, rfifop, wfifohead, wfifop, wfifoset};
use crate::game::pc::{MapSessionData, FLAG_MAIL};
use crate::game::scripting::sl_exec;
use crate::network::crypt::encrypt;
use crate::servers::map::packet::ClientPacket;
use crate::session::session_exists;

use crate::database::board_db;
use crate::game::lua::dispatch::dispatch;

// ---------------------------------------------------------------------------
// nmail_sendmessage — sends a notification message packet to the player's fd.
//
// Packet layout (pre-encryption):
//   [0]     = 0xAA  (magic)
//   [1..2]  = SWAP16(len+5)  (big-endian total payload len)
//   [3]     = 0x31  (packet id)
//   [4]     = 0x03  (sub-id)
//   [5]     = other (byte)
//   [6]     = type  (byte)
//   [7]     = strlen(message)  (byte)
//   [8..]   = message (null-terminated)
//   [len+7] = 0x07  (terminator)
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn nmail_sendmessage(
    sd: *mut MapSessionData,
    message: *const i8,
    other: i32,
    r#type: i32,
) -> i32 {
    if is_player_active(sd) == 0 {
        return 0;
    }

    let fd = (*sd).fd;
    if !session_exists(fd) {
        return 0;
    }

    let msg_len = libc_strlen(message);

    wfifohead(fd, 65535 + 3);
    let p0 = wfifop(fd, 0);
    if p0.is_null() {
        return 0;
    }

    *p0 = 0xAA_u8;
    *wfifop(fd, 3) = 0x31_u8;
    *wfifop(fd, 4) = 0x03_u8;
    *wfifop(fd, 5) = other as u8;
    *wfifop(fd, 6) = r#type as u8;
    *wfifop(fd, 7) = msg_len as u8;
    std::ptr::copy_nonoverlapping(message as *const u8, wfifop(fd, 8), msg_len);
    *wfifop(fd, msg_len + 8) = 0x07_u8;
    let size_be = ((msg_len as u16) + 6).to_be();
    (wfifop(fd, 1) as *mut u16).write_unaligned(size_be);

    let enc_len = encrypt(fd) as usize;
    wfifoset(fd, enc_len);
    0
}

// ---------------------------------------------------------------------------
// boards_delete
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn boards_delete(sd: *mut MapSessionData, board: i32) -> i32 {
    if sd.is_null() {
        return 0;
    }

    let post = {
        let p = rfifop((*sd).fd, 8) as *const u16;
        if p.is_null() {
            return 0;
        }
        u16::from_be(p.read_unaligned()) as i32
    };

    let name = (*sd).player.identity.name.clone();
    let gm_level = (*sd).player.identity.gm_level as u16;
    let can_delete = (*sd).board_candel as u16;

    let result = blocking_run_async(async move {
        db_boards::delete_post(
            get_pool(),
            board as u16,
            post as u16,
            &name,
            gm_level,
            can_delete,
        )
        .await
    });

    let (msg, r#type) = match result {
        0 => (c"The message has been deleted.", 1),
        1 => (c"You can only delete your own messages.", 0),
        _ => (c"Something went wrong. Please try again later.", 0),
    };
    nmail_sendmessage(sd, msg.as_ptr(), 7, r#type);
    0
}

// ---------------------------------------------------------------------------
// boards_showposts
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn boards_showposts(sd: *mut MapSessionData, board: i32) -> i32 {
    if sd.is_null() {
        return 0;
    }

    (*sd).board_canwrite = 0;
    (*sd).board_candel = 0;
    (*sd).boardnameval = 0;

    if board == 0 {
        (*sd).board_canwrite = 1;
        (*sd).board_candel = 1;
    } else {
        (*sd).board = board;
        let bd = &*board_db::search(board);
        if bd.script != 0 {
            let yname = crate::game::scripting::carray_to_str(&bd.yname);
            sl_doscript_simple(yname, Some("check"), (*sd).id);
        } else {
            (*sd).board_canwrite = 1;
        }
        if (*sd).player.identity.gm_level == 99 {
            (*sd).board_canwrite = 1;
            (*sd).board_candel = 1;
        }
    }

    let mut flags: i32 = 0;
    if (*sd).board_canwrite != 0 {
        if (*sd).board_canwrite == 6 {
            flags = 6;
        } else {
            flags |= BOARD_CAN_WRITE;
        }
    }
    if (*sd).board_candel != 0 {
        flags |= BOARD_CAN_DEL;
    }

    let fd = (*sd).fd;
    let name = (*sd).player.identity.name.clone();
    let bcount = (*sd).bcount as u32;
    let popup = (*sd).board_popup as u8;

    let rows = blocking_run_async(async move {
        db_boards::list_posts(get_pool(), board as u32, bcount * 20, &name).await
    });

    tracing::debug!(
        "[map] [boards] showposts: board={} flags={} rows={}",
        board,
        flags,
        rows.len()
    );

    let flags1: u8 = if popup != 0 && board != 0 {
        if flags == 6 {
            6
        } else if flags & BOARD_CAN_WRITE == 0 {
            0
        } else {
            2
        }
    } else {
        if flags == 6 {
            6
        } else if flags & BOARD_CAN_WRITE == 0 {
            1
        } else {
            3
        }
    };
    let flags2: u8 = if board == 0 { 4 } else { 2 };

    let board_display_name = board_db::board_name(board);

    let mut pkt_out = ClientPacket::board(flags2);
    pkt_out.put_u8(flags1);
    pkt_out.put_u16_be(board as u16);
    pkt_out.put_str(&board_display_name);

    if rows.is_empty() {
        pkt_out.put_u8(0);
    } else {
        pkt_out.put_u8(rows.len() as u8);

        for row in &rows {
            pkt_out.put_u8(row.color as u8);
            pkt_out.put_u16_be(row.post_id as u16);

            let composed_user = if board != 0 && row.board_name != 0 {
                let bn = board_db::bn_name(row.board_name as i32);
                format!("{} {}", bn, row.user)
            } else {
                row.user.clone()
            };
            pkt_out.put_str(&composed_user);

            pkt_out.put_u8(row.month as u8);
            pkt_out.put_u8(row.day as u8);
            pkt_out.put_str(&row.topic);
        }
    }

    tracing::debug!(
        "[map] [boards] showposts: sending client packet fd={} buf_len={}",
        fd,
        pkt_out.buf.len()
    );
    pkt_out.send(fd);

    (*sd).bcount += 1;
    0
}

// ---------------------------------------------------------------------------
// boards_readpost
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn boards_readpost(sd: *mut MapSessionData, board: i32, post: i32) -> i32 {
    if board != 0 {
        (*sd).board = board;
        let bd = &*board_db::search(board);
        if bd.script != 0 {
            let yname = crate::game::scripting::carray_to_str(&bd.yname);
            sl_doscript_simple(yname, Some("check"), (*sd).id);
        } else {
            (*sd).board_canwrite = 1;
        }
        if (*sd).player.identity.gm_level == 99 {
            (*sd).board_canwrite = 1;
            (*sd).board_candel = 1;
        }
    }

    let mut flags: i32 = 0;
    if (*sd).board_canwrite != 0 {
        flags |= BOARD_CAN_WRITE;
    }
    if (*sd).board_candel != 0 {
        flags |= BOARD_CAN_DEL;
    }

    let fd = (*sd).fd;
    let name = (*sd).player.identity.name.clone();

    let content = blocking_run_async(async move {
        db_boards::read_post(get_pool(), board as u32, post as u32, &name).await
    });

    let Some(content) = content else {
        return 0;
    };

    if board == 0 {
        let name2 = (*sd).player.identity.name.clone();
        let post_id = content.post_id;
        let clear_flag = blocking_run_async(async move {
            db_boards::mark_mail_read(get_pool(), post_id, &name2).await
        });
        if clear_flag {
            (*sd).flags &= !FLAG_MAIL;
        }
    }

    let post_type: u8 = if board == 0 { 5 } else { 3 };
    let buttons: u8 = if board == 0 || (flags & BOARD_CAN_WRITE) != 0 {
        3
    } else {
        1
    };

    let mut pkt_out = ClientPacket::board(post_type);
    pkt_out.buf[4] = 0;
    pkt_out.put_u8(buttons);
    pkt_out.put_u8(if board == 0 { 1 } else { 0 });
    pkt_out.put_u16_be(content.post_id as u16);
    pkt_out.put_str(&content.user);
    pkt_out.put_u8(content.month as u8);
    pkt_out.put_u8(content.day as u8);
    pkt_out.put_str(&content.topic);
    pkt_out.put_str_u16_be(&content.body);

    tracing::debug!(
        "[map] [boards] readpost: sending client packet fd={} buf_len={}",
        fd,
        pkt_out.buf.len()
    );
    pkt_out.send(fd);
    0
}

// ---------------------------------------------------------------------------
// boards_post
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn boards_post(sd: *mut MapSessionData, board: i32) -> i32 {
    if sd.is_null() {
        return 0;
    }

    let fd = (*sd).fd;

    let topiclen = rfifob(fd, 8) as usize;
    if topiclen > 52 {
        let mut name_buf = [0u8; 16];
        let name_bytes = (*sd).player.identity.name.as_bytes();
        let n = name_bytes.len().min(15);
        name_buf[..n].copy_from_slice(&name_bytes[..n]);
        clif_Hacker(
            name_buf.as_mut_ptr() as *mut i8,
            c"Board hacking: TOPIC HACK".as_ptr(),
        );
        return 0;
    }

    let postlen = {
        let p = rfifop(fd, topiclen + 9) as *const u16;
        if p.is_null() {
            return 0;
        }
        u16::from_be(p.read_unaligned()) as usize
    };
    if postlen > 4000 {
        let mut name_buf = [0u8; 16];
        let name_bytes = (*sd).player.identity.name.as_bytes();
        let n = name_bytes.len().min(15);
        name_buf[..n].copy_from_slice(&name_bytes[..n]);
        clif_Hacker(
            name_buf.as_mut_ptr() as *mut i8,
            c"Board hacking: POST(BODY) HACK".as_ptr(),
        );
        return 0;
    }

    if topiclen == 0 {
        nmail_sendmessage(sd, c"Post must contain subject.".as_ptr(), 6, 0);
        return 0;
    }
    if postlen == 0 {
        nmail_sendmessage(sd, c"Post must contain a body.".as_ptr(), 6, 0);
        return 0;
    }

    let mut topic_buf = [0u8; 53];
    let mut post_buf = [0u8; 4001];
    std::ptr::copy_nonoverlapping(rfifop(fd, 9), topic_buf.as_mut_ptr(), topiclen);
    std::ptr::copy_nonoverlapping(rfifop(fd, topiclen + 11), post_buf.as_mut_ptr(), postlen);

    let name = (*sd).player.identity.name.clone();
    let mut nval = (*sd).boardnameval as i32;
    if (*sd).player.identity.gm_level != 0 {
        nval = 1;
    }

    let topic_str = std::str::from_utf8(&topic_buf[..topiclen])
        .unwrap_or("")
        .to_owned();
    let post_str = std::str::from_utf8(&post_buf[..postlen])
        .unwrap_or("")
        .to_owned();

    let result = blocking_run_async(async move {
        db_boards::create_board_post(get_pool(), board as u32, nval, &name, &topic_str, &post_str)
            .await
    });

    let (msg, r#type) = match result {
        0 => (c"Your message has been posted.", 1),
        _ => (c"Something went wrong. Please try again later.", 0),
    };
    nmail_sendmessage(sd, msg.as_ptr(), 6, r#type);
    0
}

// ---------------------------------------------------------------------------
// nmail_read — noop stub (original SQL was removed long ago).
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn nmail_read(_sd: *mut MapSessionData, _post: i32) -> i32 {
    0
}

// ---------------------------------------------------------------------------
// nmail_luascript
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn nmail_luascript(sd: *mut MapSessionData, to: i32, topic: i32, msg: i32) -> i32 {
    let fd = (*sd).fd;
    let mut message = [0i8; 4000];

    std::ptr::copy_nonoverlapping(
        rfifop(fd, (to + topic + 12) as usize) as *const i8,
        message.as_mut_ptr(),
        msg as usize,
    );

    let cha_name = (*sd).player.identity.name.clone();
    let body = std::ffi::CStr::from_ptr(message.as_ptr())
        .to_str()
        .unwrap_or("")
        .to_owned();

    let ok = sqlx::query(
            "INSERT INTO `Mail` (`MalChaName`, `MalChaNameDestination`, `MalBody`) VALUES (?, 'Lua', ?)"
        )
        .bind(cha_name)
        .bind(body)
        .execute(get_pool())
        .await
        .is_ok();
    if !ok {
        return 0;
    }

    sl_exec(sd, message.as_mut_ptr());
    0
}

// ---------------------------------------------------------------------------
// nmail_poemscript
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn nmail_poemscript(
    sd: *mut MapSessionData,
    topic: *const i8,
    message: *const i8,
) -> i32 {
    use chrono::Datelike as _;

    let now = chrono::Local::now();
    let month = now.month0() as i32;
    let day = now.day() as i32;

    let char_id = (*sd).player.identity.id as i32;

    let already_submitted = sqlx::query_scalar::<_, Option<u32>>(
        "SELECT `BrdId` FROM `Boards` WHERE `BrdBnmId` = '19' AND `BrdChaId` = ? LIMIT 1",
    )
    .bind(char_id)
    .fetch_optional(get_pool())
    .await
    .ok()
    .flatten()
    .is_some();

    if already_submitted {
        nmail_sendmessage(sd, c"You have already submitted a poem.".as_ptr(), 6, 1);
        return 0;
    }

    let topic_str = std::ffi::CStr::from_ptr(topic)
        .to_str()
        .unwrap_or("")
        .to_owned();
    let message_str = std::ffi::CStr::from_ptr(message)
        .to_str()
        .unwrap_or("")
        .to_owned();

    let boardpos: u32 = sqlx::query_scalar::<_, Option<u32>>(
        "SELECT MAX(`BrdPosition`) FROM `Boards` WHERE `BrdBnmId` = '19'",
    )
    .fetch_optional(get_pool())
    .await
    .ok()
    .flatten()
    .flatten()
    .unwrap_or(0);

    let ok = sqlx::query(
            "INSERT INTO `Boards` (`BrdBnmId`, `BrdChaName`, `BrdChaId`, `BrdTopic`, `BrdPost`, `BrdMonth`, `BrdDay`, `BrdPosition`) VALUES ('19', 'Anonymous', ?, ?, ?, ?, ?, ?)"
        )
        .bind(char_id)
        .bind(topic_str)
        .bind(message_str)
        .bind(month)
        .bind(day)
        .bind(boardpos.saturating_add(1))
        .execute(get_pool())
        .await
        .is_ok();
    if !ok {
        return 1;
    }

    nmail_sendmessage(sd, c"Poem submitted.".as_ptr(), 6, 1);
    0
}

// ---------------------------------------------------------------------------
// nmail_sendmailcopy
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn nmail_sendmailcopy(
    sd: *mut MapSessionData,
    to_user: *const i8,
    topic: *const i8,
    message: *const i8,
) -> i32 {
    if libc_strlen(to_user) > 16 || libc_strlen(topic) > 52 || libc_strlen(message) > 4000 {
        return 0;
    }

    let from = (*sd).player.identity.name.clone();
    let to_str = std::ffi::CStr::from_ptr(to_user)
        .to_string_lossy()
        .into_owned();
    let topic_str = std::ffi::CStr::from_ptr(topic)
        .to_string_lossy()
        .into_owned();
    let msg_str = std::ffi::CStr::from_ptr(message)
        .to_string_lossy()
        .into_owned();

    blocking_run_async(async move {
        let _ = db_boards::nmail_insert(get_pool(), &from, &to_str, &topic_str, &msg_str).await;
    });
    0
}

// ---------------------------------------------------------------------------
// nmail_write
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn nmail_write(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() {
        return 0;
    }
    let fd = (*sd).fd;

    let tolen = rfifob(fd, 8) as usize;
    if tolen > 52 {
        let mut name_buf = [0u8; 16];
        let name_bytes = (*sd).player.identity.name.as_bytes();
        let n = name_bytes.len().min(15);
        name_buf[..n].copy_from_slice(&name_bytes[..n]);
        clif_Hacker(name_buf.as_mut_ptr() as *mut i8, c"NMAIL: To User".as_ptr());
        return 0;
    }
    let topiclen = rfifob(fd, tolen + 9) as usize;
    if topiclen > 52 {
        let mut name_buf = [0u8; 16];
        let name_bytes = (*sd).player.identity.name.as_bytes();
        let n = name_bytes.len().min(15);
        name_buf[..n].copy_from_slice(&name_bytes[..n]);
        clif_Hacker(name_buf.as_mut_ptr() as *mut i8, c"NMAIL: Topic".as_ptr());
        return 0;
    }
    let messagelen = {
        let p = rfifop(fd, tolen + topiclen + 10) as *const u16;
        if p.is_null() {
            return 0;
        }
        u16::from_be(p.read_unaligned()) as usize
    };
    if messagelen > 4000 {
        let mut name_buf = [0u8; 16];
        let name_bytes = (*sd).player.identity.name.as_bytes();
        let n = name_bytes.len().min(15);
        name_buf[..n].copy_from_slice(&name_bytes[..n]);
        clif_Hacker(name_buf.as_mut_ptr() as *mut i8, c"NMAIL: Message".as_ptr());
        return 0;
    }

    let mut to_user = [0i8; 52];
    let mut topic = [0i8; 52];
    let mut message = [0i8; 4000];

    std::ptr::copy_nonoverlapping(rfifop(fd, 9) as *const i8, to_user.as_mut_ptr(), tolen);
    std::ptr::copy_nonoverlapping(
        rfifop(fd, tolen + 10) as *const i8,
        topic.as_mut_ptr(),
        topiclen,
    );
    std::ptr::copy_nonoverlapping(
        rfifop(fd, topiclen + tolen + 12) as *const i8,
        message.as_mut_ptr(),
        messagelen,
    );
    let send_copy = rfifob(fd, topiclen + tolen + 12 + messagelen) as i32;

    let to_user_cstr = std::ffi::CStr::from_ptr(to_user.as_ptr());
    let to_user_lower = to_user_cstr.to_string_lossy().to_ascii_lowercase();

    if to_user_lower == "lua" {
        std::ptr::copy_nonoverlapping(
            message.as_ptr(),
            (*sd).mail.as_mut_ptr(),
            messagelen.min((*sd).mail.len()),
        );
        (*sd).luaexec = 0;
        sl_doscript_simple("canRunLuaMail", None, (*sd).id);
        if (*sd).player.identity.gm_level == 99 || (*sd).luaexec != 0 {
            nmail_luascript(sd, tolen as i32, topiclen as i32, messagelen as i32).await;
            nmail_sendmessage(sd, c"LUA script ran!".as_ptr(), 6, 1);
            return 0;
        }
    }

    if to_user_lower == "poems" || to_user_lower == "poem" {
        if map_readglobalgamereg("poemAccept") == 0 {
            nmail_sendmessage(
                sd,
                c"Currently not accepting poem submissions.".as_ptr(),
                6,
                0,
            );
            return 0;
        }

        std::ptr::copy_nonoverlapping(
            message.as_ptr(),
            (*sd).mail.as_mut_ptr(),
            messagelen.min((*sd).mail.len()),
        );

        if topiclen == 0 {
            nmail_sendmessage(sd, c"Mail must contain a subject.".as_ptr(), 6, 0);
            return 0;
        }
        if messagelen == 0 {
            nmail_sendmessage(sd, c"Mail must contain a body.".as_ptr(), 6, 0);
            return 0;
        }

        nmail_poemscript(sd, topic.as_ptr(), message.as_ptr()).await;
        return 0;
    }

    // Standard mail
    if topiclen == 0 {
        nmail_sendmessage(sd, c"Mail must contain a subject.".as_ptr(), 6, 0);
        return 0;
    }
    if messagelen == 0 {
        nmail_sendmessage(sd, c"Mail must contain a body.".as_ptr(), 6, 0);
        return 0;
    }

    nmail_sendmail(sd, to_user.as_ptr(), topic.as_ptr(), message.as_ptr());

    if send_copy != 0 {
        let to_str = std::ffi::CStr::from_ptr(to_user.as_ptr()).to_string_lossy();
        let tp_str = std::ffi::CStr::from_ptr(topic.as_ptr()).to_string_lossy();
        let mut a_topic = format!("[To {}] {}", to_str, tp_str);
        a_topic.truncate(51);
        let a_topic_c = std::ffi::CString::new(a_topic).unwrap_or_default();
        let self_name_c =
            std::ffi::CString::new((*sd).player.identity.name.as_str()).unwrap_or_default();
        nmail_sendmailcopy(
            sd,
            self_name_c.as_ptr(),
            a_topic_c.as_ptr(),
            message.as_ptr(),
        );
    }

    0
}

// ---------------------------------------------------------------------------
// nmail_sendmail
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn nmail_sendmail(
    sd: *mut MapSessionData,
    to_user: *const i8,
    topic: *const i8,
    message: *const i8,
) -> i32 {
    if libc_strlen(to_user) > 16 || libc_strlen(topic) > 52 || libc_strlen(message) > 4000 {
        return 0;
    }

    let from = (*sd).player.identity.name.clone();
    let to_str = std::ffi::CStr::from_ptr(to_user)
        .to_string_lossy()
        .into_owned();
    let topic_str = std::ffi::CStr::from_ptr(topic)
        .to_string_lossy()
        .into_owned();
    let msg_str = std::ffi::CStr::from_ptr(message)
        .to_string_lossy()
        .into_owned();

    let result = blocking_run_async(async move {
        db_boards::nmail_insert(get_pool(), &from, &to_str, &topic_str, &msg_str).await
    });

    let (msg, r#type) = match result {
        0 => (c"Your message has been sent.", 1),
        2 => (c"User does not exist.", 0),
        _ => (c"Something went wrong. Please try again later.", 0),
    };
    nmail_sendmessage(sd, msg.as_ptr(), 6, r#type);
    0
}

// ---------------------------------------------------------------------------
// map_changepostcolor / map_getpostcolor
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn map_changepostcolor(board: i32, post: i32, color: i32) -> i32 {
    sqlx::query(
        "UPDATE `Boards` SET `BrdHighlighted` = ? WHERE `BrdBnmId` = ? AND `BrdPosition` = ?",
    )
    .bind(color)
    .bind(board)
    .bind(post)
    .execute(get_pool())
    .await
    .ok();
    0
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn map_getpostcolor(board: i32, post: i32) -> i32 {
    sqlx::query_scalar::<_, Option<i32>>(
        "SELECT `BrdHighlighted` FROM `Boards` WHERE `BrdBnmId` = ? AND `BrdPosition` = ?",
    )
    .bind(board)
    .bind(post)
    .fetch_optional(get_pool())
    .await
    .ok()
    .flatten()
    .flatten()
    .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

#[inline]
unsafe fn libc_strlen(s: *const i8) -> usize {
    if s.is_null() {
        return 0;
    }
    std::ffi::CStr::from_ptr(s).to_bytes().len()
}

/// Returns 1 if `sd` is non-null and has an active session.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn is_player_active(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() {
        return 0;
    }
    let fd = (*sd).fd;
    if fd.raw() == 0 {
        return 0;
    }
    if !session_exists(fd) {
        tracing::warn!(
            "[map] is_player_active: player exists but session does not ({})",
            (*sd).player.identity.name
        );
        return 0;
    }
    1
}

/// Dispatch a Lua event with a single entity ID argument.
fn sl_doscript_simple(root: &str, method: Option<&str>, id: u32) -> bool {
    dispatch(root, method, &[id])
}
