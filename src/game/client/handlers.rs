//! Client event and packet handler functions.
//!

//!
//! Functions ported:
//!   clif_quit, clif_stoptimers, clif_handle_disconnect,
//!   clif_handle_missingobject, clif_handle_menuinput, clif_handle_powerboards,
//!   clif_handle_boards, clif_handle_obstruction, clif_parsemenu,
//!   clif_getName, clif_Hacker, clif_accept2,
//!   clif_transfer, clif_transfer_test, clif_sendBoardQuestionaire,
//!   clif_changestatus.

#![allow(non_snake_case)]

use std::ffi::CStr;

use crate::database::get_pool;
use crate::database::map_db::BlockList;
use crate::game::block::map_delblock;
use crate::game::scripting::sl_resumemenu;
use crate::game::map_parse::packet::{rfifob, rfifop, wfifohead, wfifop, wfifoset};
use crate::game::map_server::{
    boards_delete, boards_post, boards_readpost, boards_showposts, hasCoref,
    map_changepostcolor, map_deliddb, map_getpostcolor, map_id2bl_ref, map_id2sd_pc,
    nmail_sendmessage, nmail_write,
};
use crate::game::map_parse::chat::clif_sendminitext;
use crate::game::map_parse::dialogs::{clif_parsebuy, clif_parseinput, clif_parsesell};
use crate::game::map_parse::groups::clif_leavegroup;
use crate::game::map_parse::movement::clif_charspecific;
use crate::game::map_parse::player_state::clif_sendxy;
use crate::game::map_parse::trading::{clif_exchange_close, clif_exchange_message};
use crate::game::map_parse::visual::{clif_lookgone, clif_object_look_specific};
use crate::game::mob::{BL_PC, MAX_MAGIC_TIMERS};
use crate::game::pc::{pc_stoptimer, MapSessionData};



use crate::game::pc::{
    pc_changeitem, pc_readglobalreg,
    pc_dropitemmap, pc_setpos,
    addtokillreg,
};
use crate::game::time_util::timer_remove;
use crate::game::scripting::sl_async_freeco;
use crate::game::map_char::intif_save_impl::sl_intif_savequit;
use crate::game::client::visual::clif_showboards;
use crate::database::board_db;
use crate::game::map_parse::combat::clif_sendaction;
use crate::database::item_db;
use crate::database::magic_db;
use crate::session::{session_exists, session_set_eof, SessionId};

use crate::game::block::AreaType;
use crate::game::block_grid;
use crate::game::map_parse::visual::clif_object_look_sub_inner;

/// Dispatch a Lua event with a single block_list argument.
#[allow(dead_code)]
unsafe fn sl_doscript_simple(root: *const i8, method: *const i8, bl: *mut crate::database::map_db::BlockList) -> i32 {
    crate::game::scripting::doscript_blargs(root, method, &[bl as *mut _])
}

/// Dispatch a Lua event with two block_list arguments.
#[allow(dead_code)]
unsafe fn sl_doscript_2(root: *const i8, method: *const i8, bl1: *mut crate::database::map_db::BlockList, bl2: *mut crate::database::map_db::BlockList) -> i32 {
    crate::game::scripting::doscript_blargs(root, method, &[bl1 as *mut _, bl2 as *mut _])
}


// ─── Map data read helper ─────────────────────────────────────────────────────

#[inline]
unsafe fn read_obj(m: i32, x: i32, y: i32) -> u16 {
    use crate::database::map_db::raw_map_ptr;
    let md = &*raw_map_ptr().add(m as usize);
    if md.obj.is_null() { return 0; }
    *md.obj.add(x as usize + y as usize * md.xs as usize)
}

// ─── Session buffer read helpers ─────────────────────────────────────────────

#[inline]
unsafe fn rbyte(fd: SessionId, pos: usize) -> u8 {
    rfifob(fd, pos)
}

#[inline]
unsafe fn rword_be(fd: SessionId, pos: usize) -> u16 {
    let p = rfifop(fd, pos);
    if p.is_null() { return 0; }
    u16::from_be_bytes([*p, *p.add(1)])
}

#[inline]
unsafe fn rlong_be(fd: SessionId, pos: usize) -> u32 {
    let p = rfifop(fd, pos);
    if p.is_null() { return 0; }
    u32::from_be_bytes([*p, *p.add(1), *p.add(2), *p.add(3)])
}

// ─── Functions ───────────────────────────────────────────────────────────────

/// Remove `sd` from the block grid and broadcast a look-gone packet to visible players.
////
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
pub unsafe fn clif_quit(sd: *mut MapSessionData) -> i32 {
    map_delblock(&raw mut (*sd).bl);
    clif_lookgone(&raw const (*sd).bl);
    0
}

/// Remove all active duration and aether timers for `sd`.
////
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
pub unsafe fn clif_stoptimers(sd: *mut MapSessionData) -> i32 {
    let sd = &mut *sd;
    for x in 0..MAX_MAGIC_TIMERS {
        if sd.status.dura_aether[x].dura_timer != 0 {
            timer_remove(sd.status.dura_aether[x].dura_timer as i32);
        }
        if sd.status.dura_aether[x].aether_timer != 0 {
            timer_remove(sd.status.dura_aether[x].aether_timer as i32);
        }
    }
    0
}

/// Handle a clean disconnect: cancel exchange, run logout script, save, remove from world.
////
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
pub async unsafe fn clif_handle_disconnect(sd: *mut MapSessionData) -> i32 {
    if (*sd).exchange.target != 0 {
        let tsd = map_id2sd_pc((*sd).exchange.target)
            .map(|arc| &mut *arc.write() as *mut MapSessionData).unwrap_or(std::ptr::null_mut());
        clif_exchange_close(sd);
        if !tsd.is_null() && (*tsd).exchange.target == (*sd).bl.id {
            clif_exchange_message(tsd, c"Exchange cancelled.".as_ptr(), 4, 0);
            clif_exchange_close(tsd);
        }
    }

    pc_stoptimer(sd);
    sl_async_freeco(sd);
    clif_leavegroup(sd);
    clif_stoptimers(sd);
    sl_doscript_simple(c"logout".as_ptr(), std::ptr::null::<i8>(), &raw mut (*sd).bl);
    sl_intif_savequit(sd);

    // Capture fields before map_deliddb drops the Box.
    let id = (*sd).status.id;
    let name = CStr::from_ptr((*sd).status.name.as_ptr() as *const i8).to_string_lossy().into_owned();

    clif_quit(sd);
    map_deliddb((*sd).bl.id);

    if let Err(e) = sqlx::query("UPDATE `Character` SET `ChaOnline` = '0' WHERE `ChaId` = ?")
        .bind(id)
        .execute(get_pool())
        .await
    {
        tracing::error!("[handle_disconnect] ChaOnline update failed: {e}");
    }

    tracing::info!("[map] [handle_disconnect] name={name}");
    0
}

/// Send any missing objects that the client requested by ID.
////
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
pub unsafe fn clif_handle_missingobject(sd: *mut MapSessionData) -> i32 {
    let id = rlong_be((*sd).fd, 5);
    let bl = map_id2bl_ref(id);
    if !bl.is_null() {
        if (*bl).bl_type as i32 == BL_PC {
            clif_charspecific((*sd).status.id as i32, id as i32);
            clif_charspecific(id as i32, (*sd).status.id as i32);
        } else {
            clif_object_look_specific(sd, id);
        }
    }
    0
}

/// Dispatch a menu-input packet to the appropriate handler.
////
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
pub unsafe fn clif_handle_menuinput(sd: *mut MapSessionData) -> i32 {
    if hasCoref(sd) == 0 {
        return 0;
    }
    match rbyte((*sd).fd, 5) {
        0 => sl_async_freeco(sd),
        1 => { clif_parsemenu(sd); }
        2 => { clif_parsebuy(sd); }
        3 => { clif_parseinput(sd); }
        4 => { clif_parsesell(sd); }
        _ => sl_async_freeco(sd),
    }
    0
}

/// Handle a powerboard interaction: route the powerBoard Lua script with optional target.
////
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
pub unsafe fn clif_handle_powerboards(sd: *mut MapSessionData) -> i32 {
    let tsd = map_id2sd_pc(rlong_be((*sd).fd, 11))
        .map(|arc| &mut *arc.write() as *mut MapSessionData).unwrap_or(std::ptr::null_mut());
    if !tsd.is_null() {
        (*sd).pbColor = rbyte((*sd).fd, 15) as i32;
    } else {
        (*sd).pbColor = 0;
    }

    if !tsd.is_null() {
        sl_doscript_2(c"powerBoard".as_ptr(), std::ptr::null::<i8>(), &raw mut (*sd).bl, &raw mut (*tsd).bl);
    } else {
        sl_doscript_2(c"powerBoard".as_ptr(), std::ptr::null::<i8>(), &raw mut (*sd).bl, std::ptr::null_mut::<BlockList>());
    }
    0
}

/// Handle a boards/nmail packet: show boards, read/post/delete posts, send nmail.
////
/// Note: case 8 intentionally falls through to case 9 (matching C behavior).
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
pub async unsafe fn clif_handle_boards(sd: *mut MapSessionData) -> i32 {
    match rbyte((*sd).fd, 5) {
        1 => {
            (*sd).bcount = 0;
            (*sd).board_popup = 0;
            clif_showboards(sd);
        }
        2 => {
            if rbyte((*sd).fd, 8) == 127 {
                (*sd).bcount = 0;
            }
            boards_showposts(sd, rword_be((*sd).fd, 6) as i32);
        }
        3 => {
            boards_readpost(
                sd,
                rword_be((*sd).fd, 6) as i32,
                rword_be((*sd).fd, 8) as i32,
            );
        }
        4 => {
            boards_post(sd, rword_be((*sd).fd, 6) as i32);
        }
        5 => {
            boards_delete(sd, rword_be((*sd).fd, 6) as i32);
        }
        6 => {
            if (*sd).status.level >= 10 {
                nmail_write(sd).await;
            } else {
                clif_sendminitext(
                    sd,
                    c"You must be at least level 10 to view/send nmail.".as_ptr(),
                );
            }
        }
        7 => {
            if (*sd).status.gm_level != 0 {
                let board = rword_be((*sd).fd, 6) as i32;
                let post  = rword_be((*sd).fd, 8) as i32;
                let color = map_getpostcolor(board, post).await ^ 1;
                map_changepostcolor(board, post, color).await;
                nmail_sendmessage(sd, c"Post updated.".as_ptr(), 6, 0);
            }
        }
        8 => {
            // C fallthrough: case 8 runs the Lua write script, then falls into case 9.
            let board = rword_be((*sd).fd, 6) as i32;
            sl_doscript_simple(board_db::yname_ptr(board), c"write".as_ptr(), &raw mut (*sd).bl);
            (*sd).bcount = 0;
            boards_showposts(sd, 0);
        }
        9 => {
            (*sd).bcount = 0;
            boards_showposts(sd, 0);
        }
        _ => {}
    }
    0
}

/// Correct the player's position after a movement obstruction, then resync the client.
////
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
pub unsafe fn clif_handle_obstruction(sd: *mut MapSessionData) -> i32 {
    (*sd).canmove = 0;
    let xold = rword_be((*sd).fd, 5) as i32;
    let yold = rword_be((*sd).fd, 7) as i32;
    let mut nx = xold;
    let mut ny = yold;

    match rbyte((*sd).fd, 9) {
        0 => ny = yold - 1,
        1 => nx = xold + 1,
        2 => ny = yold + 1,
        3 => nx = xold - 1,
        _ => {}
    }

    (*sd).bl.x = nx as u16;
    (*sd).bl.y = ny as u16;
    clif_sendxy(sd);
    0
}

/// Resume a Lua NPC menu with the player's selection.
////
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
pub unsafe fn clif_parsemenu(sd: *mut MapSessionData) -> i32 {
    let selection = rword_be((*sd).fd, 10) as u32;
    sl_resumemenu(selection, sd);
    0
}

/// Post an item from inventory onto an adjacent board/prop when the client requests it.
////
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
pub unsafe fn clif_postitem(sd: *mut MapSessionData) -> i32 {
    use crate::game::map_parse::dialogs::clif_input;
    let slot = rbyte((*sd).fd, 5) as i32 - 1;
    let (mut x, mut y) = (0i32, 0i32);
    let bx = (*sd).bl.x as i32;
    let by = (*sd).bl.y as i32;
    match (*sd).status.side {
        0 => { x = bx;     y = by - 1; }
        1 => { x = bx + 1; y = by;     }
        2 => { x = bx;     y = by + 1; }
        3 => { x = bx - 1; y = by;     }
        _ => {}
    }
    if x < 0 || y < 0 { return 0; }
    let obj = read_obj((*sd).bl.m as i32, x as i32, y as i32) as i32;
    if obj == 1619 || obj == 1620 {
        if (*sd).status.inventory[slot as usize].amount > 1 {
            clif_input(sd, (*sd).last_click as i32, c"How many would you like to post?".as_ptr(), c"".as_ptr());
        }
    }
    (*sd).invslot = slot as u8;
    0
}

/// Swap two inventory slots from a client rearrange packet.
////
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
pub unsafe fn clif_parsechangepos(sd: *mut MapSessionData) -> i32 {
    use crate::game::map_parse::chat::clif_sendminitext;
    if rbyte((*sd).fd, 5) == 0 {
        pc_changeitem(
            sd,
            rbyte((*sd).fd, 6) as i32 - 1,
            rbyte((*sd).fd, 7) as i32 - 1,
        );
    } else {
        clif_sendminitext(sd, c"You are busy.".as_ptr());
    }
    0
}

/// Save the player's friend list (20 slots) from a client update packet.
////
/// # Safety
/// `sd` must be valid. `friend_list` is a raw byte buffer of length `len`.
pub async unsafe fn clif_parsefriends(
    sd: *mut MapSessionData,
    friend_list: *const i8,
    len: i32,
) -> i32 {
    if sd.is_null() || friend_list.is_null() || len <= 0 { return 0; }

    // Parse up to 20 null-terminated names separated by 0x0C control bytes.
    let bytes = std::slice::from_raw_parts(friend_list as *const u8, len as usize);
    let mut friends: [String; 20] = std::array::from_fn(|_| String::new());
    let mut count = 0usize;
    let mut i = 0usize;
    while i < bytes.len() && count < 20 {
        if bytes[i] == 0x0C {
            i += 1;
            let mut name = Vec::new();
            while i < bytes.len() && bytes[i] != 0x00 {
                name.push(bytes[i]);
                i += 1;
            }
            friends[count] = String::from_utf8_lossy(&name).into_owned();
            count += 1;
        }
        i += 1;
    }

    let id = (*sd).status.id;
    let pool = get_pool();
    // Upsert: ensure row exists
    let exists: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 FROM `Friends` WHERE `FndChaId` = ?"
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if !exists {
        if let Err(e) = sqlx::query("INSERT INTO `Friends` (`FndChaId`) VALUES (?)")
            .bind(id)
            .execute(pool)
            .await
        {
            tracing::error!("[parsefriends] id={id} insert: {e}");
        }
    }
    // Update all 20 slots in one query
    if let Err(e) = sqlx::query(
        "UPDATE `Friends` SET \
         `FndChaName1`=?,`FndChaName2`=?,`FndChaName3`=?,`FndChaName4`=?,\
         `FndChaName5`=?,`FndChaName6`=?,`FndChaName7`=?,`FndChaName8`=?,\
         `FndChaName9`=?,`FndChaName10`=?,`FndChaName11`=?,`FndChaName12`=?,\
         `FndChaName13`=?,`FndChaName14`=?,`FndChaName15`=?,`FndChaName16`=?,\
         `FndChaName17`=?,`FndChaName18`=?,`FndChaName19`=?,`FndChaName20`=? \
         WHERE `FndChaId` = ?"
    )
    .bind(&friends[0])  .bind(&friends[1])  .bind(&friends[2])  .bind(&friends[3])
    .bind(&friends[4])  .bind(&friends[5])  .bind(&friends[6])  .bind(&friends[7])
    .bind(&friends[8])  .bind(&friends[9])  .bind(&friends[10]) .bind(&friends[11])
    .bind(&friends[12]) .bind(&friends[13]) .bind(&friends[14]) .bind(&friends[15])
    .bind(&friends[16]) .bind(&friends[17]) .bind(&friends[18]) .bind(&friends[19])
    .bind(id)
    .execute(pool)
    .await
    {
        tracing::error!("[parsefriends] id={id}: {e}");
    }
    0
}

/// Return the AccountId for the given character ID, or 0 if not found.
pub async unsafe fn clif_isregistered(id: u32) -> i32 {
    sqlx::query_scalar::<_, u32>(
        "SELECT `AccountId` FROM `Accounts` WHERE \
         `AccountCharId1`=? OR `AccountCharId2`=? OR `AccountCharId3`=? OR \
         `AccountCharId4`=? OR `AccountCharId5`=? OR `AccountCharId6`=?"
    )
    .bind(id).bind(id).bind(id).bind(id).bind(id).bind(id)
    .fetch_optional(get_pool())
    .await
    .ok()
    .flatten()
    .unwrap_or(0) as i32
}

/// Return a heap-allocated C string with the account email for character `id`, or NULL.
/// The caller does not free the pointer (leaked to match original C behaviour).
pub async unsafe fn clif_getaccountemail(id: u32) -> *const i8 {
    let acct_id = clif_isregistered(id).await;
    if acct_id == 0 { return std::ptr::null(); }
    let email: Option<String> = sqlx::query_scalar(
        "SELECT `AccountEmail` FROM `Accounts` WHERE `AccountId` = ?"
    )
    .bind(acct_id as u32)
    .fetch_optional(get_pool())
    .await
    .unwrap_or(None);
    match email {
        Some(s) => match std::ffi::CString::new(s) {
            Ok(cs) => cs.into_raw() as *const i8, // leaked intentionally
            Err(_) => std::ptr::null(),
        },
        None => std::ptr::null(),
    }
}

/// Busy-wait for `milliseconds` milliseconds.
pub unsafe fn clif_delay(milliseconds: i32) {
    let dur = std::time::Duration::from_millis(milliseconds as u64);
    let start = std::time::Instant::now();
    while start.elapsed() < dur {}
}

/// Send a heartbeat packet (opcode 0x3B) to the player with id `id`.
pub unsafe fn clif_sendheartbeat(id: i32, _none: i32) -> i32 {
    let sd = map_id2sd_pc(id as u32).map(|arc| &mut *arc.write() as *mut MapSessionData).unwrap_or(std::ptr::null_mut());
    if sd.is_null() { return 1; }
    if !session_exists((*sd).fd) {
        return 0;
    }
    let fd = (*sd).fd;
    // Payload length = 7 (bytes [3..9]); total_size = 7 + 6 = 13
    wfifohead(fd, 13);
    let w = |off: usize| wfifop(fd, off);
    *w(0) = 0xAA;
    *w(1) = 0x00; *w(2) = 0x07; // length = 7
    *w(3) = 0x3B;
    *w(4) = 0x00;
    *w(5) = 0x5F;
    *w(6) = 0x0A;
    use crate::network::crypt::encrypt;
    let n = encrypt(fd) as usize;
    wfifoset(fd, n);
    0
}

/// `foreach_in_cell` callback: run the floor NPC's "click2" script.
pub unsafe fn clif_runfloor_sub_inner(bl: *mut crate::database::map_db::BlockList, sd: *mut MapSessionData) -> i32 {
    use crate::game::pc::FLOOR;
    if bl.is_null() || sd.is_null() { return 0; }
    use crate::game::npc::NpcData;
    if (*bl).subtype as i32 != FLOOR as i32 { return 0; }
    let nd = bl as *mut NpcData;
    sl_async_freeco(sd);
    sl_doscript_2((*nd).name.as_ptr(), c"click2".as_ptr(), &raw mut (*sd).bl, &raw mut (*nd).bl);
    0
}

/// Propagate a kill-registry entry to all group members on the same map.
////
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
pub unsafe fn clif_addtokillreg(sd: *mut MapSessionData, mob: i32) -> i32 {
    use crate::game::pc::{groups, MAX_GROUP_MEMBERS};
    if sd.is_null() { return 0; }
    let grp = groups();
    for x in 0..(*sd).group_count as usize {
        let member_id = grp[(*sd).groupid as usize * MAX_GROUP_MEMBERS + x];
        let tsd = map_id2sd_pc(member_id).map(|arc| &mut *arc.write() as *mut MapSessionData).unwrap_or(std::ptr::null_mut());
        if tsd.is_null() { continue; }
        if (*tsd).bl.m == (*sd).bl.m {
            addtokillreg(tsd, mob);
        }
    }
    0
}

/// Handle a client item-drop request.
////
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialized [`MapSessionData`].
pub unsafe fn clif_parsedropitem(sd: *mut MapSessionData) -> i32 {
    if pc_readglobalreg(sd, c"goldbardupe".as_ptr()) != 0 { return 0; }
    if (*sd).status.gm_level == 0 {
        if (*sd).status.state == 3 {
            clif_sendminitext(sd, c"You cannot do that while riding a mount.".as_ptr());
            return 0;
        }
        if (*sd).status.state == 1 {
            clif_sendminitext(sd, c"Spirits can't do that.".as_ptr());
            return 0;
        }
    }
    (*sd).fakeDrop = 0;
    let id = rbyte((*sd).fd, 5) as i32 - 1;
    let all = rbyte((*sd).fd, 6) as i32;
    if id as usize >= (*sd).status.maxinv as usize { return 0; }
    if (*sd).status.inventory[id as usize].id != 0 {
        if item_db::search((*sd).status.inventory[id as usize].id as u32).droppable != 0 {
            clif_sendminitext(sd, c"You can't drop this item.".as_ptr());
            return 0;
        }
    }
    clif_sendaction(&mut (*sd).bl, 5, 20, 0);
    (*sd).invslot = id as u8;
    let drop_item = item_db::search((*sd).status.inventory[id as usize].id as u32);
    sl_doscript_simple(drop_item.yname.as_ptr(), c"on_drop".as_ptr(), &raw mut (*sd).bl);
    for x in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[x].id > 0 && (*sd).status.dura_aether[x].duration > 0 {
            sl_doscript_simple(magic_db::search((*sd).status.dura_aether[x].id as i32).yname.as_ptr(), c"on_drop_while_cast".as_ptr(), &raw mut (*sd).bl);
        }
    }
    for x in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[x].id > 0 && (*sd).status.dura_aether[x].aether > 0 {
            sl_doscript_simple(magic_db::search((*sd).status.dura_aether[x].id as i32).yname.as_ptr(), c"on_drop_while_aether".as_ptr(), &raw mut (*sd).bl);
        }
    }
    if (*sd).fakeDrop != 0 { return 0; }
    pc_dropitemmap(sd, id, all);
    0
}

// ─── Constants needed by handler functions ─────────────────────────────────

#[allow(dead_code)]
const SAMEAREA: i32 = 6;
const LOOK_GET:  i32 = 0;

// ─── Board questionnaire struct ─────────────────────────────────────────────

#[repr(C)]
pub struct BoardQuestionaire {
    pub header:     [u8; 255],
    pub question:   [u8; 255],
    pub input_lines: u32,
}

// ─── WFIFO write helpers ────────────────────────────────────────────────────

/// Write big-endian u16 at `pos` in the send-FIFO.
#[allow(dead_code)]
#[inline]
unsafe fn wbe16(fd: SessionId, pos: usize, val: u16) {
    let p = wfifop(fd, pos) as *mut u16;
    if !p.is_null() { p.write_unaligned(val.to_be()); }
}

/// Write big-endian u32 at `pos` in the send-FIFO.
#[allow(dead_code)]
#[inline]
unsafe fn wbe32(fd: SessionId, pos: usize, val: u32) {
    let p = wfifop(fd, pos) as *mut u32;
    if !p.is_null() { p.write_unaligned(val.to_be()); }
}

/// Copy null-terminated string bytes starting at `pos`.
#[allow(dead_code)]
#[inline]
unsafe fn wfifo_strcpy(fd: SessionId, pos: usize, s: &[u8]) {
    let p = wfifop(fd, pos);
    if !p.is_null() {
        std::ptr::copy_nonoverlapping(s.as_ptr(), p, s.len());
        *p.add(s.len()) = 0;
    }
}

// ─── Static buffer for clif_getName ────────────────────────────────────────

/// Reusable 16-byte name buffer.
static NAME_BUF: std::sync::Mutex<[u8; 16]> = std::sync::Mutex::new([0u8; 16]);

// ─── New ported functions ───────────────────────────────────────────────────

/// Look up a character name by ChaId; returns pointer into a static 16-byte buffer.
pub async unsafe fn clif_getName(id: u32) -> *mut i8 {
    let name: String = sqlx::query_scalar::<_, String>(
        "SELECT `ChaName` FROM `Character` WHERE `ChaId` = ?"
    )
    .bind(id)
    .fetch_optional(get_pool())
    .await
    .ok()
    .flatten()
    .unwrap_or_default();
    let mut buf = NAME_BUF.lock().unwrap_or_else(|e| e.into_inner());
    buf.fill(0);
    let bytes = name.as_bytes();
    let n = bytes.len().min(15);
    buf[..n].copy_from_slice(&bytes[..n]);
    buf.as_mut_ptr() as *mut i8
}

/// Log a possible hacking event and broadcast to GMs.
pub unsafe fn clif_Hacker(name: *mut i8, reason: *const i8) -> i32 {
    let name_s = if name.is_null() { "[?]" }
        else { CStr::from_ptr(name).to_str().unwrap_or("[?]") };
    let reason_s = if reason.is_null() { "" }
        else { CStr::from_ptr(reason).to_str().unwrap_or("") };
    tracing::warn!("{} possibly hacking{}", name_s, reason_s);
    let msg = std::ffi::CString::new(
        format!("{} possibly hacking: {}", name_s, reason_s)
    ).unwrap_or_default();
    crate::game::map_parse::chat::clif_broadcasttogm(msg.as_ptr(), -1);
    0
}

/// Accept a character-load request; look up ChaId by name, then call intif_load.
pub async unsafe fn clif_accept2(
    fd: SessionId, name: *mut i8, name_len: i32,
) -> i32 {
    if name_len <= 0 || name_len > 16 {
        session_set_eof(fd, 11);
        return 0;
    }
    if crate::core::should_shutdown() {
        session_set_eof(fd, 1);
        return 0;
    }
    let mut n = [0u8; 16];
    std::ptr::copy_nonoverlapping(name as *const u8, n.as_mut_ptr(), name_len as usize);
    let name_str = CStr::from_ptr(n.as_ptr() as *const i8)
        .to_str().unwrap_or("").to_owned();
    let id: u32 = sqlx::query_scalar::<_, u32>(
        "SELECT `ChaId` FROM `Character` WHERE `ChaName` = ?"
    )
    .bind(name_str)
    .fetch_optional(get_pool())
    .await
    .ok()
    .flatten()
    .unwrap_or(0);
    crate::game::map_char::intif_load(fd.raw(), id, n.as_ptr() as *const i8);
    0
}

/// Send a server-transfer packet to redirect the client to another map server.
pub unsafe fn clif_transfer(
    sd: *mut MapSessionData, serverid: i32,
    _m: i32, _x: i32, _y: i32,
) -> i32 {
    if !session_exists((*sd).fd) {
        return 0;
    }
    let fd = (*sd).fd;
    let dest_port: u16 = match serverid {
        0 => 2001,
        1 => 2002,
        _ => 2003,
    };
    let xk = crate::config::config().xor_key.as_bytes();
    let xk_len = xk.len().min(9);
    let name_bytes = CStr::from_ptr((*sd).status.name.as_ptr() as *const i8).to_bytes();
    let name_len = name_bytes.len();

    use crate::network::crypt::encrypt;
    wfifohead(fd, 255);
    let w = |off: usize| wfifop(fd, off);
    *w(0) = 0xAA;
    *w(3) = 0x03;
    // SWAP32(map_ip) — network-order IP → host-order, then LE write = network bytes on wire
    let map_ip: u32 = crate::config::config().map_ip
        .parse::<std::net::Ipv4Addr>()
        .map(|a| u32::from_le_bytes(a.octets()))
        .unwrap_or(0);
    (w(4) as *mut u32).write_unaligned(map_ip.swap_bytes());
    (w(8) as *mut u16).write_unaligned(dest_port.to_be());
    *w(10) = 0x16;
    (w(11) as *mut u16).write_unaligned(9u16.to_be());
    std::ptr::copy_nonoverlapping(xk.as_ptr(), w(13), xk_len);
    *w(13 + xk_len) = 0;
    let mut len: usize = 11;
    *w(len + 11) = name_len as u8;
    std::ptr::copy_nonoverlapping(name_bytes.as_ptr(), w(len + 12), name_len);
    len += name_len + 1;
    len += 4;
    *w(10) = len as u8;
    (w(1) as *mut u16).write_unaligned((len as u16 + 8).to_be());
    wfifoset(fd, encrypt(fd) as usize);
    0
}

/// Send a test server-transfer packet (hardcoded IP 192.88.99.100, port 2001).
pub unsafe fn clif_transfer_test(
    sd: *mut MapSessionData, _m: i32, _x: i32, _y: i32,
) -> i32 {
    if !session_exists((*sd).fd) {
        return 0;
    }
    let fd = (*sd).fd;
    // inet_addr("192.88.99.100") on LE x86 — bytes stored in network order
    // SWAP32 of that = host-order; WFIFOL writes LE → wire bytes are network-order
    let test_ip_net: u32 = u32::from_ne_bytes([192, 88, 99, 100]);
    let xk = crate::config::config().xor_key.as_bytes();
    let xk_len = xk.len().min(9);
    const FAKE_NAME: &[u8] = b"FAKEUSERNAME";
    let name_len = FAKE_NAME.len();

    use crate::network::crypt::encrypt;
    wfifohead(fd, 255);
    let w = |off: usize| wfifop(fd, off);
    *w(0) = 0xAA;
    *w(3) = 0x03;
    (w(4) as *mut u32).write_unaligned(test_ip_net.swap_bytes());
    (w(8) as *mut u16).write_unaligned(2001u16.to_be());
    *w(10) = 0x16;
    (w(11) as *mut u16).write_unaligned(9u16.to_be());
    std::ptr::copy_nonoverlapping(xk.as_ptr(), w(13), xk_len);
    *w(13 + xk_len) = 0;
    let mut len: usize = 11;
    *w(len + 11) = name_len as u8;
    std::ptr::copy_nonoverlapping(FAKE_NAME.as_ptr(), w(len + 12), name_len);
    len += name_len + 1;
    len += 4;
    *w(10) = len as u8;
    (w(1) as *mut u16).write_unaligned((len as u16 + 8).to_be());
    wfifoset(fd, encrypt(fd) as usize);
    0
}

/// Send the board questionnaire dialog to `sd`.
pub unsafe fn clif_sendBoardQuestionaire(
    sd: *mut MapSessionData,
    q: *const BoardQuestionaire,
    count: i32,
) -> i32 {
    if !session_exists((*sd).fd) {
        return 0;
    }
    let fd = (*sd).fd;
    use crate::network::crypt::encrypt;
    wfifohead(fd, 65535);
    let w = |off: usize| wfifop(fd, off);
    *w(0) = 0xAA;
    *w(3) = 0x31;
    *w(5) = 0x09;
    *w(6) = count as u8;
    let mut len: usize = 7;
    for i in 0..count as usize {
        let item = &*q.add(i);
        let hlen = CStr::from_ptr(item.header.as_ptr() as *const i8).to_bytes().len();
        *w(len) = hlen as u8;
        len += 1;
        std::ptr::copy_nonoverlapping(item.header.as_ptr(), w(len), hlen);
        len += hlen;
        *w(len) = 1;
        *w(len + 1) = 2;
        len += 2;
        *w(len) = item.input_lines as u8;
        len += 1;
        let qlen = CStr::from_ptr(item.question.as_ptr() as *const i8).to_bytes().len();
        *w(len) = qlen as u8;
        len += 1;
        std::ptr::copy_nonoverlapping(item.question.as_ptr(), w(len), qlen);
        len += qlen;
        *w(len) = 1;
        len += 1;
    }
    *w(len) = 0;
    *w(len + 1) = 0x6B;
    len += 2;
    (w(1) as *mut u16).write_unaligned(((len + 3) as u16).to_be());
    wfifoset(fd, encrypt(fd) as usize);
    0
}

/// Handle client setting-toggle request (whisper, group, shout, etc.).
pub unsafe fn clif_changestatus(sd: *mut MapSessionData, type_: i32) -> i32 {
    use crate::game::pc::{
        FLAG_WHISPER, FLAG_GROUP, FLAG_SHOUT, FLAG_ADVICE, FLAG_MAGIC,
        FLAG_WEATHER, FLAG_REALM, FLAG_EXCHANGE, FLAG_FASTMOVE, FLAG_SOUND,
        FLAG_HELM, FLAG_NECKLACE,
    };
    use crate::game::map_parse::{
        movement::clif_sendchararea,
        player_state::{clif_getchararea, clif_sendmapinfo, clif_sendstatus},
        visual::{clif_mob_look_close, clif_mob_look_start, clif_spawn},
        groups::clif_findmount,
        chat::clif_sendminitext,
    };
    use crate::game::client::handlers::clif_quit;
    use crate::game::pc::pc_setglobalreg;

    let sflag = (*sd).status.setting_flags;

    match type_ {
        0x00 => {
            if rbyte((*sd).fd, 7) == 1 {
                match (*sd).status.state {
                    0 => {
                        clif_findmount(sd);
                        if (*sd).status.state == 0 {
                            clif_sendminitext(sd, c"Good try, but there is nothing here that you can ride.".as_ptr());
                        }
                    }
                    1 => { clif_sendminitext(sd, c"Spirits can't do that.".as_ptr()); }
                    2 => { clif_sendminitext(sd, c"Good try, but there is nothing here that you can ride.".as_ptr()); }
                    3 => {
                        sl_doscript_simple(c"onDismount".as_ptr(), std::ptr::null(), &raw mut (*sd).bl);
                    }
                    4 => { clif_sendminitext(sd, c"You cannot do that while transformed.".as_ptr()); }
                    _ => {}
                }
            }
        }
        0x01 => {
            (*sd).status.setting_flags ^= FLAG_WHISPER as u16;
            if (*sd).status.setting_flags & FLAG_WHISPER as u16 != 0 {
                clif_sendminitext(sd, c"Listen to whisper:ON".as_ptr());
            } else {
                clif_sendminitext(sd, c"Listen to whisper:OFF".as_ptr());
            }
            clif_sendstatus(sd, 0);
        }
        0x02 => {
            (*sd).status.setting_flags ^= FLAG_GROUP as u16;
            if (*sd).status.setting_flags & FLAG_GROUP as u16 != 0 {
                clif_sendminitext(sd, c"Join a group     :ON".as_ptr());
            } else {
                if (*sd).group_count > 0 { clif_leavegroup(sd); }
                clif_sendminitext(sd, c"Join a group     :OFF".as_ptr());
            }
            clif_sendstatus(sd, 0);
        }
        0x03 => {
            (*sd).status.setting_flags ^= FLAG_SHOUT as u16;
            if (*sd).status.setting_flags & FLAG_SHOUT as u16 != 0 {
                clif_sendminitext(sd, c"Listen to shout  :ON".as_ptr());
            } else {
                clif_sendminitext(sd, c"Listen to shout  :OFF".as_ptr());
            }
            clif_sendstatus(sd, 0);
        }
        0x04 => {
            (*sd).status.setting_flags ^= FLAG_ADVICE as u16;
            if (*sd).status.setting_flags & FLAG_ADVICE as u16 != 0 {
                clif_sendminitext(sd, c"Listen to advice :ON".as_ptr());
            } else {
                clif_sendminitext(sd, c"Listen to advice :OFF".as_ptr());
            }
            clif_sendstatus(sd, 0);
        }
        0x05 => {
            (*sd).status.setting_flags ^= FLAG_MAGIC as u16;
            if (*sd).status.setting_flags & FLAG_MAGIC as u16 != 0 {
                clif_sendminitext(sd, c"Believe in magic :ON".as_ptr());
            } else {
                clif_sendminitext(sd, c"Believe in magic :OFF".as_ptr());
            }
            clif_sendstatus(sd, 0);
        }
        0x06 => {
            (*sd).status.setting_flags ^= FLAG_WEATHER as u16;
            if (*sd).status.setting_flags & FLAG_WEATHER as u16 != 0 {
                clif_sendminitext(sd, c"Weather change   :ON".as_ptr());
            } else {
                clif_sendminitext(sd, c"Weather change   :OFF".as_ptr());
            }
            crate::game::client::visual::clif_sendweather(sd);
            clif_sendstatus(sd, 0);
        }
        0x07 => {
            let oldm = (*sd).bl.m as i32;
            let oldx = (*sd).bl.x as i32;
            let oldy = (*sd).bl.y as i32;
            (*sd).status.setting_flags ^= FLAG_REALM as u16;
            clif_quit(sd);
            clif_sendmapinfo(sd);
            pc_setpos(sd, oldm, oldx, oldy);
            clif_sendmapinfo(sd);
            clif_spawn(sd);
            clif_mob_look_start(sd);
            if let Some(grid) = block_grid::get_grid((*sd).bl.m as usize) {
                let slot = &*crate::database::map_db::raw_map_ptr().add((*sd).bl.m as usize);
                let ids = block_grid::ids_in_area(grid, (*sd).bl.x as i32, (*sd).bl.y as i32, AreaType::SameArea, slot.xs as i32, slot.ys as i32);
                for id in ids {
                    let bl_ptr = crate::game::map_server::map_id2bl_ref(id);
                    if !bl_ptr.is_null() {
                        clif_object_look_sub_inner(bl_ptr, LOOK_GET, sd as *mut BlockList);
                    }
                }
            }
            clif_mob_look_close(sd);
            crate::game::client::visual::clif_destroyold(sd);
            clif_sendchararea(sd);
            clif_getchararea(sd);
            if (*sd).status.setting_flags & FLAG_REALM as u16 != 0 {
                clif_sendminitext(sd, c"Realm-centered   :ON".as_ptr());
            } else {
                clif_sendminitext(sd, c"Realm-centered   :OFF".as_ptr());
            }
            clif_sendstatus(sd, 0);
        }
        0x08 => {
            (*sd).status.setting_flags ^= FLAG_EXCHANGE as u16;
            if (*sd).status.setting_flags & FLAG_EXCHANGE as u16 != 0 {
                clif_sendminitext(sd, c"Exchange         :ON".as_ptr());
            } else {
                clif_sendminitext(sd, c"Exchange         :OFF".as_ptr());
            }
            clif_sendstatus(sd, 0);
        }
        0x09 => {
            (*sd).status.setting_flags ^= FLAG_FASTMOVE as u16;
            if (*sd).status.setting_flags & FLAG_FASTMOVE as u16 != 0 {
                clif_sendminitext(sd, c"Fast Move        :ON".as_ptr());
            } else {
                clif_sendminitext(sd, c"Fast Move        :OFF".as_ptr());
            }
            clif_sendstatus(sd, 0);
        }
        10 => {
            (*sd).status.clan_chat = ((*sd).status.clan_chat + 1) % 2;
            if (*sd).status.clan_chat != 0 {
                clif_sendminitext(sd, c"Clan whisper     :ON".as_ptr());
            } else {
                clif_sendminitext(sd, c"Clan whisper     :OFF".as_ptr());
            }
        }
        13 => {
            if rbyte((*sd).fd, 4) == 3 { return 0; }
            (*sd).status.setting_flags ^= FLAG_SOUND as u16;
            if (*sd).status.setting_flags & FLAG_SOUND as u16 != 0 {
                clif_sendminitext(sd, c"Hear sounds      :ON".as_ptr());
            } else {
                clif_sendminitext(sd, c"Hear sounds      :OFF".as_ptr());
            }
            clif_sendstatus(sd, 0);
        }
        14 => {
            (*sd).status.setting_flags ^= FLAG_HELM as u16;
            if (*sd).status.setting_flags & FLAG_HELM as u16 != 0 {
                clif_sendminitext(sd, c"Show Helmet      :ON".as_ptr());
                pc_setglobalreg(sd, c"show_helmet".as_ptr(), 1);
            } else {
                clif_sendminitext(sd, c"Show Helmet      :OFF".as_ptr());
                pc_setglobalreg(sd, c"show_helmet".as_ptr(), 0);
            }
            clif_sendstatus(sd, 0);
            clif_sendchararea(sd);
            clif_getchararea(sd);
        }
        15 => {
            (*sd).status.setting_flags ^= FLAG_NECKLACE as u16;
            if (*sd).status.setting_flags & FLAG_NECKLACE as u16 != 0 {
                clif_sendminitext(sd, c"Show Necklace      :ON".as_ptr());
                pc_setglobalreg(sd, c"show_necklace".as_ptr(), 1);
            } else {
                clif_sendminitext(sd, c"Show Necklace      :OFF".as_ptr());
                pc_setglobalreg(sd, c"show_necklace".as_ptr(), 0);
            }
            clif_sendstatus(sd, 0);
            clif_sendchararea(sd);
            clif_getchararea(sd);
        }
        _ => {}
    }
    let _ = sflag; // suppress unused warning if no match uses it
    0
}

// ─── createdb_start ─────────────────────────────────────────────────────────
//
// Opcode 0x6B — item creation system.
// Reads ingredient items from the session buffer, builds a Lua `creationItems`
// table, and dispatches `itemCreation(pc)` script.
pub unsafe fn createdb_start(sd: *mut MapSessionData) -> i32 {
    use crate::game::scripting::sl_state;
    use crate::database::map_db::BlockList;

    if sd.is_null() { return 0; }
    let fd = (*sd).fd;

    // RFIFOB(fd, 5) — number of ingredient slots in this packet.
    let item_c = rfifob(fd, 5) as usize;
    let item_c = item_c.min(10);

    let mut items   = [0u32; 10];
    let mut amounts = [1u32; 10];
    let mut len = 6usize;

    for x in 0..item_c {
        // RFIFOB(fd, len) - 1 = inventory slot index.
        let raw = rfifob(fd, len) as usize;
        if raw == 0 { len += 1; continue; }
        let curitem = raw - 1;
        let maxinv = (*sd).status.maxinv as usize;
        if curitem < maxinv {
            items[x] = (*sd).status.inventory[curitem].id;
        }
        if item_db::search(items[x]).stack_amount > 1 {
            amounts[x] = rfifob(fd, len + 1) as u32;
            len += 2;
        } else {
            amounts[x] = 1;
            len += 1;
        }
    }

    (*sd).creation_works      = 0;
    (*sd).creation_item       = 0;
    (*sd).creation_itemamount = 0;

    // Build creationItems Lua table: [id1, amt1, id2, amt2, ...]
    let lua = sl_state();
    let _ = (|| -> mlua::Result<()> {
        let tbl = lua.create_table()?;
        for j in 0..item_c {
            tbl.raw_seti(j * 2 + 1, items[j])?;
            tbl.raw_seti(j * 2 + 2, amounts[j])?;
        }
        lua.globals().set("creationItems", tbl)?;
        Ok(())
    })();

    sl_async_freeco(sd);

    let bl_ptr = &mut (*sd).bl as *mut BlockList;
    crate::game::scripting::doscript_blargs(
        c"itemCreation".as_ptr(), std::ptr::null(), &[bl_ptr],
    );
    0
}

// ─── Sync wrappers for FFI callers that cannot .await ──────────────────────────
//
// These are thin blocking wrappers for C FFI / Lua accessor call sites
// (pc_accessors.rs) that cannot be made async. They use blocking_run_async
// because they execute inside the async LocalSet.
//
// NOTE: clif_getName_sync was removed — player_state.rs and visual.rs now
// import clif_getName directly and call it via blocking_run_async locally.

use crate::database::blocking_run_async;

/// Sync wrapper for [`clif_isregistered`] — for use in FFI / non-async call sites.
pub unsafe fn clif_isregistered_sync(id: u32) -> i32 {
    blocking_run_async(clif_isregistered(id))
}

/// Sync wrapper for [`clif_getaccountemail`] — for use in FFI / non-async call sites.
pub unsafe fn clif_getaccountemail_sync(id: u32) -> *const i8 {
    // Transmit the raw pointer through usize (which is Send) to satisfy blocking_run_async.
    let addr: usize = blocking_run_async(async move {
        clif_getaccountemail(id).await as usize
    });
    addr as *const i8
}
