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
use crate::game::pc::MapSessionData;
// map_delblock removed — now using map_delblock_id directly
use crate::config::Point;
use crate::game::map_parse::chat::clif_sendminitext;
use crate::game::map_parse::dialogs::{clif_parsebuy, clif_parseinput, clif_parsesell};
use crate::game::map_parse::groups::clif_leavegroup;
use crate::game::map_parse::movement::clif_charspecific;
use crate::game::map_parse::packet::{rfifob, rfifop, wfifohead, wfifop, wfifoset};
use crate::game::map_parse::player_state::clif_sendxy;
use crate::game::map_parse::trading::{clif_exchange_close, clif_exchange_message};
use crate::game::map_parse::visual::{clif_lookgone_by_id, clif_object_look_specific};
use crate::game::map_server::{
    boards_delete, boards_post, boards_readpost, boards_showposts, entity_position, hasCoref,
    map_changepostcolor, map_deliddb, map_getpostcolor, map_id2sd_pc, nmail_sendmessage,
    nmail_write,
};
use crate::game::mob::{BL_PC, MAX_MAGIC_TIMERS};
use crate::game::pc::pc_stoptimer;
use crate::game::player::entity::PlayerEntity;
use crate::game::player::prelude::*;
use crate::game::scripting::{carray_to_str, sl_resumemenu};

use crate::database::board_db;
use crate::database::item_db;
use crate::database::magic_db;
use crate::game::client::visual::clif_showboards;
use crate::game::map_char::intif_save_impl::sl_intif_savequit;
use crate::game::map_parse::combat::clif_sendaction_pc;
use crate::game::pc::{addtokillreg, pc_changeitem, pc_dropitemmap, pc_readglobalreg, pc_setpos};
use crate::game::scripting::sl_async_freeco;
use crate::game::time_util::timer_remove;
use crate::session::{session_exists, session_set_eof, SessionId};

use crate::game::block::AreaType;
use crate::game::block_grid;
use crate::game::lua::dispatch::dispatch;
use crate::game::map_parse::visual::{
    clif_mob_look_close_func_inner, clif_mob_look_start_func_inner, clif_object_look_by_id,
};

/// Dispatch a Lua event with a single entity ID argument.
fn sl_doscript_simple(root: &str, method: Option<&str>, id: u32) -> bool {
    dispatch(root, method, &[id])
}

/// Dispatch a Lua event with two entity ID arguments.
fn sl_doscript_2(root: &str, method: Option<&str>, id1: u32, id2: u32) -> bool {
    dispatch(root, method, &[id1, id2])
}

// ─── Map data read helper ─────────────────────────────────────────────────────

#[inline]
unsafe fn read_obj(m: i32, x: i32, y: i32) -> u16 {
    use crate::database::map_db::raw_map_ptr;
    let md = &*raw_map_ptr().add(m as usize);
    if md.obj.is_null() {
        return 0;
    }
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
    if p.is_null() {
        return 0;
    }
    u16::from_be_bytes([*p, *p.add(1)])
}

#[inline]
unsafe fn rlong_be(fd: SessionId, pos: usize) -> u32 {
    let p = rfifop(fd, pos);
    if p.is_null() {
        return 0;
    }
    u32::from_be_bytes([*p, *p.add(1), *p.add(2), *p.add(3)])
}

// ─── Functions ───────────────────────────────────────────────────────────────

/// Remove `pe` from the block grid and broadcast a look-gone packet to visible players.
pub fn clif_quit(pe: &PlayerEntity) -> i32 {
    let (id, m) = {
        let r = pe.read();
        (r.id, r.m)
    };
    crate::game::block::map_delblock_id(id, m);
    unsafe { clif_lookgone_by_id(id) };
    0
}

/// Remove all active duration and aether timers for `pe`.
pub fn clif_stoptimers(pe: &PlayerEntity) -> i32 {
    for x in 0..MAX_MAGIC_TIMERS {
        let dura_timer = pe.read().player.spells.dura_aether[x].dura_timer;
        if dura_timer != 0 {
            timer_remove(dura_timer as i32);
        }
        let aether_timer = pe.read().player.spells.dura_aether[x].aether_timer;
        if aether_timer != 0 {
            timer_remove(aether_timer as i32);
        }
    }
    0
}

/// Handle a clean disconnect: cancel exchange, run logout script, save, remove from world.
pub async fn clif_handle_disconnect(pe: &PlayerEntity) -> i32 {
    let exchange_target = pe.read().exchange.target;
    if exchange_target != 0 {
        let tpe = map_id2sd_pc(exchange_target);
        unsafe { clif_exchange_close(pe) };
        if let Some(ref tpe) = tpe {
            let target_exchange_target = tpe.read().exchange.target;
            if target_exchange_target == pe.id {
                unsafe {
                    clif_exchange_message(tpe, c"Exchange cancelled.".as_ptr(), 4, 0);
                    clif_exchange_close(tpe);
                }
            }
        }
    }

    unsafe {
        pc_stoptimer(&mut *pe.write() as *mut MapSessionData);
        let sd_ptr = &mut *pe.write() as *mut MapSessionData;
        sl_async_freeco(sd_ptr);
        clif_leavegroup(pe);
    }
    clif_stoptimers(pe);
    sl_doscript_simple("logout", None, pe.id);
    sl_intif_savequit(pe);

    // Capture fields before map_deliddb drops the Box.
    let id = pe.read().player.identity.id;
    let name = pe.read().player.identity.name.clone();

    clif_quit(pe);
    map_deliddb(pe.id);

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
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_handle_missingobject(pe: &PlayerEntity) -> i32 {
    let id = rlong_be(pe.fd, 5);
    let identity_id = pe.read().player.identity.id;
    if let Some((_pos, bl_type)) = entity_position(id) {
        if bl_type as i32 == BL_PC {
            clif_charspecific(identity_id as i32, id as i32);
            clif_charspecific(id as i32, identity_id as i32);
        } else {
            clif_object_look_specific(&mut *pe.write() as *mut MapSessionData, id);
        }
    }
    0
}

/// Dispatch a menu-input packet to the appropriate handler.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_handle_menuinput(pe: &PlayerEntity) -> i32 {
    if hasCoref(&mut *pe.write() as *mut MapSessionData) == 0 {
        return 0;
    }
    match rbyte(pe.fd, 5) {
        0 => {
            let sd_ptr = &mut *pe.write() as *mut MapSessionData;
            sl_async_freeco(sd_ptr);
        }
        1 => {
            clif_parsemenu(pe);
        }
        2 => {
            clif_parsebuy(pe);
        }
        3 => {
            clif_parseinput(pe);
        }
        4 => {
            clif_parsesell(pe);
        }
        _ => {
            let sd_ptr = &mut *pe.write() as *mut MapSessionData;
            sl_async_freeco(sd_ptr);
        }
    }
    0
}

/// Handle a powerboard interaction: route the powerBoard Lua script with optional target.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_handle_powerboards(pe: &PlayerEntity) -> i32 {
    let target_id = rlong_be(pe.fd, 11);
    let tpe = map_id2sd_pc(target_id);
    if tpe.is_some() {
        pe.write().pbColor = rbyte(pe.fd, 15) as i32;
    } else {
        pe.write().pbColor = 0;
    }

    if let Some(ref tpe) = tpe {
        sl_doscript_2("powerBoard", None, pe.id, tpe.id);
    } else {
        sl_doscript_2("powerBoard", None, pe.id, 0);
    }
    0
}

/// Handle a boards/nmail packet: show boards, read/post/delete posts, send nmail.
///
/// Note: case 8 intentionally falls through to case 9 (matching C behavior).
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn clif_handle_boards(pe: &PlayerEntity) -> i32 {
    match rbyte(pe.fd, 5) {
        1 => {
            pe.write().bcount = 0;
            pe.write().board_popup = 0;
            clif_showboards(pe);
        }
        2 => {
            if rbyte(pe.fd, 8) == 127 {
                pe.write().bcount = 0;
            }
            let sd_ptr = &mut *pe.write() as *mut MapSessionData;
            boards_showposts(sd_ptr, rword_be(pe.fd, 6) as i32);
        }
        3 => {
            let sd_ptr = &mut *pe.write() as *mut MapSessionData;
            boards_readpost(sd_ptr, rword_be(pe.fd, 6) as i32, rword_be(pe.fd, 8) as i32);
        }
        4 => {
            let sd_ptr = &mut *pe.write() as *mut MapSessionData;
            boards_post(sd_ptr, rword_be(pe.fd, 6) as i32);
        }
        5 => {
            let sd_ptr = &mut *pe.write() as *mut MapSessionData;
            boards_delete(sd_ptr, rword_be(pe.fd, 6) as i32);
        }
        6 => {
            if pe.read().player.progression.level >= 10 {
                let sd_ptr = &mut *pe.write() as *mut MapSessionData;
                nmail_write(sd_ptr).await;
            } else {
                clif_sendminitext(
                    pe,
                    c"You must be at least level 10 to view/send nmail.".as_ptr(),
                );
            }
        }
        7 => {
            if pe.read().player.identity.gm_level != 0 {
                let board = rword_be(pe.fd, 6) as i32;
                let post = rword_be(pe.fd, 8) as i32;
                let color = map_getpostcolor(board, post).await ^ 1;
                map_changepostcolor(board, post, color).await;
                nmail_sendmessage(
                    &mut *pe.write() as *mut MapSessionData,
                    c"Post updated.".as_ptr(),
                    6,
                    0,
                );
            }
        }
        8 => {
            // C fallthrough: case 8 runs the Lua write script, then falls into case 9.
            let board = rword_be(pe.fd, 6) as i32;
            sl_doscript_simple(
                carray_to_str(&board_db::search(board).yname),
                Some("write"),
                pe.id,
            );
            pe.write().bcount = 0;
            let sd_ptr = &mut *pe.write() as *mut MapSessionData;
            boards_showposts(sd_ptr, 0);
        }
        9 => {
            pe.write().bcount = 0;
            let sd_ptr = &mut *pe.write() as *mut MapSessionData;
            boards_showposts(sd_ptr, 0);
        }
        _ => {}
    }
    0
}

/// Correct the player's position after a movement obstruction, then resync the client.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_handle_obstruction(pe: &PlayerEntity) -> i32 {
    pe.write().canmove = 0;
    let xold = rword_be(pe.fd, 5) as i32;
    let yold = rword_be(pe.fd, 7) as i32;
    let mut nx = xold;
    let mut ny = yold;

    match rbyte(pe.fd, 9) {
        0 => ny = yold - 1,
        1 => nx = xold + 1,
        2 => ny = yold + 1,
        3 => nx = xold - 1,
        _ => {}
    }

    pe.write().x = nx as u16;
    pe.write().y = ny as u16;
    let (m, x, y) = {
        let r = pe.read();
        (r.m, r.x, r.y)
    };
    pe.set_position(Point { m, x, y });
    clif_sendxy(pe);
    0
}

/// Resume a Lua NPC menu with the player's selection.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parsemenu(pe: &PlayerEntity) -> i32 {
    let selection = rword_be(pe.fd, 10) as u32;
    let sd_ptr = &mut *pe.write() as *mut MapSessionData;
    sl_resumemenu(selection, sd_ptr);
    0
}

/// Post an item from inventory onto an adjacent board/prop when the client requests it.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_postitem(pe: &PlayerEntity) -> i32 {
    use crate::game::map_parse::dialogs::clif_input;
    let slot = rbyte(pe.fd, 5) as i32 - 1;
    let (mut x, mut y) = (0i32, 0i32);
    let (bx, by, side, m, last_click) = {
        let r = pe.read();
        (
            r.x as i32,
            r.y as i32,
            r.player.combat.side,
            r.m,
            r.last_click,
        )
    };
    match side {
        0 => {
            x = bx;
            y = by - 1;
        }
        1 => {
            x = bx + 1;
            y = by;
        }
        2 => {
            x = bx;
            y = by + 1;
        }
        3 => {
            x = bx - 1;
            y = by;
        }
        _ => {}
    }
    if x < 0 || y < 0 {
        return 0;
    }
    let obj = read_obj(m as i32, x, y) as i32;
    if (obj == 1619 || obj == 1620)
        && pe.read().player.inventory.inventory[slot as usize].amount > 1
    {
        clif_input(
            pe,
            last_click as i32,
            c"How many would you like to post?".as_ptr(),
            c"".as_ptr(),
        );
    }
    pe.write().invslot = slot as u8;
    0
}

/// Swap two inventory slots from a client rearrange packet.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parsechangepos(pe: &PlayerEntity) -> i32 {
    use crate::game::map_parse::chat::clif_sendminitext;
    if rbyte(pe.fd, 5) == 0 {
        pc_changeitem(pe, rbyte(pe.fd, 6) as i32 - 1, rbyte(pe.fd, 7) as i32 - 1);
    } else {
        clif_sendminitext(pe, c"You are busy.".as_ptr());
    }
    0
}

/// Save the player's friend list (20 slots) from a client update packet.
///
/// # Safety
/// `friend_list` is a raw byte buffer of length `len`.
pub async unsafe fn clif_parsefriends(pe: &PlayerEntity, friend_list: *const i8, len: i32) -> i32 {
    if friend_list.is_null() || len <= 0 {
        return 0;
    }

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

    let id = pe.read().player.identity.id;
    let pool = get_pool();
    // Upsert: ensure row exists
    let exists: bool =
        sqlx::query_scalar("SELECT COUNT(*) > 0 FROM `Friends` WHERE `FndChaId` = ?")
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
         WHERE `FndChaId` = ?",
    )
    .bind(&friends[0])
    .bind(&friends[1])
    .bind(&friends[2])
    .bind(&friends[3])
    .bind(&friends[4])
    .bind(&friends[5])
    .bind(&friends[6])
    .bind(&friends[7])
    .bind(&friends[8])
    .bind(&friends[9])
    .bind(&friends[10])
    .bind(&friends[11])
    .bind(&friends[12])
    .bind(&friends[13])
    .bind(&friends[14])
    .bind(&friends[15])
    .bind(&friends[16])
    .bind(&friends[17])
    .bind(&friends[18])
    .bind(&friends[19])
    .bind(id)
    .execute(pool)
    .await
    {
        tracing::error!("[parsefriends] id={id}: {e}");
    }
    0
}

/// Return the AccountId for the given character ID, or 0 if not found.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn clif_isregistered(id: u32) -> i32 {
    sqlx::query_scalar::<_, u32>(
        "SELECT `AccountId` FROM `Accounts` WHERE \
         `AccountCharId1`=? OR `AccountCharId2`=? OR `AccountCharId3`=? OR \
         `AccountCharId4`=? OR `AccountCharId5`=? OR `AccountCharId6`=?",
    )
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .fetch_optional(get_pool())
    .await
    .ok()
    .flatten()
    .unwrap_or(0) as i32
}

/// Return a heap-allocated C string with the account email for character `id`, or NULL.
/// The caller does not free the pointer (leaked to match original C behaviour).
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn clif_getaccountemail(id: u32) -> *const i8 {
    let acct_id = clif_isregistered(id).await;
    if acct_id == 0 {
        return std::ptr::null();
    }
    let email: Option<String> =
        sqlx::query_scalar("SELECT `AccountEmail` FROM `Accounts` WHERE `AccountId` = ?")
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
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_delay(milliseconds: i32) {
    let dur = std::time::Duration::from_millis(milliseconds as u64);
    let start = std::time::Instant::now();
    while start.elapsed() < dur {}
}

/// Send a heartbeat packet (opcode 0x3B) to the player with id `id`.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendheartbeat(id: i32, _none: i32) -> i32 {
    let pe = match map_id2sd_pc(id as u32) {
        Some(arc) => arc,
        None => return 1,
    };
    if !session_exists(pe.fd) {
        return 0;
    }
    let fd = pe.fd;
    // Payload length = 7 (bytes [3..9]); total_size = 7 + 6 = 13
    wfifohead(fd, 13);
    let w = |off: usize| wfifop(fd, off);
    *w(0) = 0xAA;
    *w(1) = 0x00;
    *w(2) = 0x07; // length = 7
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
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_runfloor_sub_inner(entity_id: u32, pe: &PlayerEntity) -> i32 {
    use crate::game::pc::FLOOR;
    let Some(arc) = crate::game::map_server::map_id2npc_ref(entity_id) else {
        return 0;
    };
    let (npc_name, npc_id) = {
        let nd = arc.read();
        if nd.subtype as i32 != FLOOR as i32 {
            return 0;
        }
        (carray_to_str(&nd.name).to_owned(), nd.id)
    };
    let sd_ptr = &mut *pe.write() as *mut MapSessionData;
    sl_async_freeco(sd_ptr);
    sl_doscript_2(&npc_name, Some("click2"), pe.id, npc_id);
    0
}

/// Propagate a kill-registry entry to all group members on the same map.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_addtokillreg(pe: &PlayerEntity, mob: i32) -> i32 {
    use crate::common::constants::world::MAX_GROUP_MEMBERS;
    use crate::game::pc::groups;
    let grp = groups();
    let (group_count, groupid, pe_m) = {
        let r = pe.read();
        (r.group_count as usize, r.groupid as usize, r.m)
    };
    for x in 0..group_count {
        let member_id = grp[groupid * MAX_GROUP_MEMBERS + x];
        if let Some(tpe) = map_id2sd_pc(member_id) {
            if tpe.read().m == pe_m {
                addtokillreg(&mut *tpe.write() as *mut MapSessionData, mob);
            }
        }
    }
    0
}

/// Handle a client item-drop request.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_parsedropitem(pe: &PlayerEntity) -> i32 {
    if pc_readglobalreg(
        &mut *pe.write() as *mut MapSessionData,
        c"goldbardupe".as_ptr(),
    ) != 0
    {
        return 0;
    }
    let (gm_level, state) = {
        let r = pe.read();
        (r.player.identity.gm_level, r.player.combat.state)
    };
    if gm_level == 0 {
        if state == 3 {
            clif_sendminitext(pe, c"You cannot do that while riding a mount.".as_ptr());
            return 0;
        }
        if state == 1 {
            clif_sendminitext(pe, c"Spirits can't do that.".as_ptr());
            return 0;
        }
    }
    pe.write().fakeDrop = 0;
    let id = rbyte(pe.fd, 5) as i32 - 1;
    let all = rbyte(pe.fd, 6) as i32;
    if id as usize >= pe.read().player.inventory.max_inv as usize {
        return 0;
    }
    {
        let inv_id = pe.read().player.inventory.inventory[id as usize].id;
        if inv_id != 0 && item_db::search(inv_id as u32).droppable != 0 {
            clif_sendminitext(pe, c"You can't drop this item.".as_ptr());
            return 0;
        }
    }
    clif_sendaction_pc(&mut pe.write(), 5, 20, 0);
    pe.write().invslot = id as u8;
    let drop_item_name = {
        let inv_id = pe.read().player.inventory.inventory[id as usize].id;
        carray_to_str(&item_db::search(inv_id as u32).yname).to_owned()
    };
    sl_doscript_simple(&drop_item_name, Some("on_drop"), pe.id);
    for x in 0..MAX_MAGIC_TIMERS {
        let (spell_id, duration) = {
            let r = pe.read();
            (
                r.player.spells.dura_aether[x].id,
                r.player.spells.dura_aether[x].duration,
            )
        };
        if spell_id > 0 && duration > 0 {
            let spell_name = carray_to_str(&magic_db::search(spell_id as i32).yname).to_owned();
            sl_doscript_simple(&spell_name, Some("on_drop_while_cast"), pe.id);
        }
    }
    for x in 0..MAX_MAGIC_TIMERS {
        let (spell_id, aether) = {
            let r = pe.read();
            (
                r.player.spells.dura_aether[x].id,
                r.player.spells.dura_aether[x].aether,
            )
        };
        if spell_id > 0 && aether > 0 {
            let spell_name = carray_to_str(&magic_db::search(spell_id as i32).yname).to_owned();
            sl_doscript_simple(&spell_name, Some("on_drop_while_aether"), pe.id);
        }
    }
    if pe.read().fakeDrop != 0 {
        return 0;
    }
    pc_dropitemmap(pe, id, all);
    0
}

// ─── Constants needed by handler functions ─────────────────────────────────

// ─── Board questionnaire struct ─────────────────────────────────────────────

#[repr(C)]
pub struct BoardQuestionaire {
    pub header: [u8; 255],
    pub question: [u8; 255],
    pub input_lines: u32,
}

// ─── WFIFO write helpers ────────────────────────────────────────────────────

/// Write big-endian u16 at `pos` in the send-FIFO.
#[allow(dead_code)]
#[inline]
unsafe fn wbe16(fd: SessionId, pos: usize, val: u16) {
    let p = wfifop(fd, pos) as *mut u16;
    if !p.is_null() {
        p.write_unaligned(val.to_be());
    }
}

/// Write big-endian u32 at `pos` in the send-FIFO.
#[allow(dead_code)]
#[inline]
unsafe fn wbe32(fd: SessionId, pos: usize, val: u32) {
    let p = wfifop(fd, pos) as *mut u32;
    if !p.is_null() {
        p.write_unaligned(val.to_be());
    }
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
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_getName(id: u32) -> *mut i8 {
    let name = if let Some(pe) = crate::game::map_server::map_id2sd_pc(id) {
        pe.name.clone()
    } else {
        String::new()
    };
    let mut buf = NAME_BUF.lock().unwrap_or_else(|e| e.into_inner());
    buf.fill(0);
    let bytes = name.as_bytes();
    let n = bytes.len().min(15);
    buf[..n].copy_from_slice(&bytes[..n]);
    buf.as_mut_ptr() as *mut i8
}

/// Log a possible hacking event and broadcast to GMs.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_Hacker(name: *mut i8, reason: *const i8) -> i32 {
    let name_s = if name.is_null() {
        "[?]"
    } else {
        CStr::from_ptr(name).to_str().unwrap_or("[?]")
    };
    let reason_s = if reason.is_null() {
        ""
    } else {
        CStr::from_ptr(reason).to_str().unwrap_or("")
    };
    tracing::warn!("{} possibly hacking{}", name_s, reason_s);
    let msg = std::ffi::CString::new(format!("{} possibly hacking: {}", name_s, reason_s))
        .unwrap_or_default();
    crate::game::map_parse::chat::clif_broadcasttogm(msg.as_ptr(), -1);
    0
}

/// Accept a character-load request; look up auth token, load player, install session.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn clif_accept2(fd: SessionId, name: *mut i8, name_len: i32) -> i32 {
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
        .to_str()
        .unwrap_or("")
        .to_owned();

    let world = match crate::world::get_world() {
        Some(w) => w,
        None => {
            tracing::error!("[map] [accept2] WorldState not available");
            session_set_eof(fd, 11);
            return 0;
        }
    };

    let normalized = name_str.to_lowercase();
    let entry = match world.auth_db.remove(&normalized) {
        Some((_, e)) => e,
        None => {
            tracing::warn!("[map] [accept2] auth_db miss: name={}", name_str);
            session_set_eof(fd, 11);
            return 0;
        }
    };

    let char_id = entry.char_id;
    tracing::info!(
        "[map] [accept2] auth_db hit: name={} char_id={}",
        name_str,
        char_id
    );

    // Load player directly from DB — no inter-server roundtrip.
    let player = match crate::servers::char::db::load_player(&world.db, char_id, &name_str).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("[map] [accept2] load_player failed: {}", e);
            session_set_eof(fd, 7);
            return 0;
        }
    };

    // Install player session (runs on LocalSet — safe for Lua)
    crate::game::map_char::intif_install_player(fd.raw(), player);
    world.online.insert(char_id);
    0
}

/// Send a server-transfer packet to redirect the client to another map server.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_transfer(pe: &PlayerEntity, serverid: i32, _m: i32, _x: i32, _y: i32) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }
    let fd = pe.fd;
    let dest_port: u16 = match serverid {
        0 => 2001,
        1 => 2002,
        _ => 2003,
    };
    let xk = crate::config::config().xor_key.as_bytes();
    let xk_len = xk.len().min(9);
    let name_string = pe.read().player.identity.name.clone();
    let name_bytes = name_string.as_bytes();
    let name_len = name_bytes.len();

    use crate::network::crypt::encrypt;
    wfifohead(fd, 255);
    let w = |off: usize| wfifop(fd, off);
    *w(0) = 0xAA;
    *w(3) = 0x03;
    // SWAP32(map_ip) — network-order IP → host-order, then LE write = network bytes on wire
    let map_ip: u32 = crate::config::config()
        .map_ip
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
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_transfer_test(pe: &PlayerEntity, _m: i32, _x: i32, _y: i32) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }
    let fd = pe.fd;
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

/// Send the board questionnaire dialog to `pe`.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_sendBoardQuestionaire(
    pe: &PlayerEntity,
    q: *const BoardQuestionaire,
    count: i32,
) -> i32 {
    if !session_exists(pe.fd) {
        return 0;
    }
    let fd = pe.fd;
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
        let hlen = CStr::from_ptr(item.header.as_ptr() as *const i8)
            .to_bytes()
            .len();
        *w(len) = hlen as u8;
        len += 1;
        std::ptr::copy_nonoverlapping(item.header.as_ptr(), w(len), hlen);
        len += hlen;
        *w(len) = 1;
        *w(len + 1) = 2;
        len += 2;
        *w(len) = item.input_lines as u8;
        len += 1;
        let qlen = CStr::from_ptr(item.question.as_ptr() as *const i8)
            .to_bytes()
            .len();
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
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_changestatus(pe: &PlayerEntity, type_: i32) -> i32 {
    use crate::game::client::handlers::clif_quit;
    use crate::game::map_parse::{
        chat::clif_sendminitext,
        groups::clif_findmount,
        movement::clif_sendchararea,
        player_state::{clif_getchararea, clif_sendmapinfo, clif_sendstatus},
        visual::clif_spawn,
    };
    use crate::game::pc::pc_setglobalreg;
    use crate::game::pc::{
        FLAG_ADVICE, FLAG_EXCHANGE, FLAG_FASTMOVE, FLAG_GROUP, FLAG_HELM, FLAG_MAGIC,
        FLAG_NECKLACE, FLAG_REALM, FLAG_SHOUT, FLAG_SOUND, FLAG_WEATHER, FLAG_WHISPER,
    };

    match type_ {
        0x00 => {
            if rbyte(pe.fd, 7) == 1 {
                match pe.read().player.combat.state {
                    0 => {
                        clif_findmount(pe);
                        if pe.read().player.combat.state == 0 {
                            clif_sendminitext(
                                pe,
                                c"Good try, but there is nothing here that you can ride.".as_ptr(),
                            );
                        }
                    }
                    1 => {
                        clif_sendminitext(pe, c"Spirits can't do that.".as_ptr());
                    }
                    2 => {
                        clif_sendminitext(
                            pe,
                            c"Good try, but there is nothing here that you can ride.".as_ptr(),
                        );
                    }
                    3 => {
                        sl_doscript_simple("onDismount", None, pe.id);
                    }
                    4 => {
                        clif_sendminitext(pe, c"You cannot do that while transformed.".as_ptr());
                    }
                    _ => {}
                }
            }
        }
        0x01 => {
            pe.write().player.appearance.setting_flags ^= FLAG_WHISPER;
            if pe.read().player.appearance.setting_flags & FLAG_WHISPER != 0 {
                clif_sendminitext(pe, c"Listen to whisper:ON".as_ptr());
            } else {
                clif_sendminitext(pe, c"Listen to whisper:OFF".as_ptr());
            }
            clif_sendstatus(pe, 0);
        }
        0x02 => {
            pe.write().player.appearance.setting_flags ^= FLAG_GROUP;
            if pe.read().player.appearance.setting_flags & FLAG_GROUP != 0 {
                clif_sendminitext(pe, c"Join a group     :ON".as_ptr());
            } else {
                if pe.read().group_count > 0 {
                    clif_leavegroup(pe);
                }
                clif_sendminitext(pe, c"Join a group     :OFF".as_ptr());
            }
            clif_sendstatus(pe, 0);
        }
        0x03 => {
            pe.write().player.appearance.setting_flags ^= FLAG_SHOUT;
            if pe.read().player.appearance.setting_flags & FLAG_SHOUT != 0 {
                clif_sendminitext(pe, c"Listen to shout  :ON".as_ptr());
            } else {
                clif_sendminitext(pe, c"Listen to shout  :OFF".as_ptr());
            }
            clif_sendstatus(pe, 0);
        }
        0x04 => {
            pe.write().player.appearance.setting_flags ^= FLAG_ADVICE;
            if pe.read().player.appearance.setting_flags & FLAG_ADVICE != 0 {
                clif_sendminitext(pe, c"Listen to advice :ON".as_ptr());
            } else {
                clif_sendminitext(pe, c"Listen to advice :OFF".as_ptr());
            }
            clif_sendstatus(pe, 0);
        }
        0x05 => {
            pe.write().player.appearance.setting_flags ^= FLAG_MAGIC;
            if pe.read().player.appearance.setting_flags & FLAG_MAGIC != 0 {
                clif_sendminitext(pe, c"Believe in magic :ON".as_ptr());
            } else {
                clif_sendminitext(pe, c"Believe in magic :OFF".as_ptr());
            }
            clif_sendstatus(pe, 0);
        }
        0x06 => {
            pe.write().player.appearance.setting_flags ^= FLAG_WEATHER;
            if pe.read().player.appearance.setting_flags & FLAG_WEATHER != 0 {
                clif_sendminitext(pe, c"Weather change   :ON".as_ptr());
            } else {
                clif_sendminitext(pe, c"Weather change   :OFF".as_ptr());
            }
            crate::game::client::visual::clif_sendweather(pe);
            clif_sendstatus(pe, 0);
        }
        0x07 => {
            let (oldm, oldx, oldy) = {
                let r = pe.read();
                (r.m as i32, r.x as i32, r.y as i32)
            };
            pe.write().player.appearance.setting_flags ^= FLAG_REALM;
            clif_quit(pe);
            clif_sendmapinfo(pe);
            pc_setpos(&mut *pe.write() as *mut MapSessionData, oldm, oldx, oldy);
            clif_sendmapinfo(pe);
            clif_spawn(pe);
            {
                let mut net = pe.net.write();
                clif_mob_look_start_func_inner(pe.fd, &mut net.look);
                if let Some(grid) = block_grid::get_grid(pe.read().m as usize) {
                    let m = pe.read().m;
                    let slot = &*crate::database::map_db::raw_map_ptr().add(m as usize);
                    let (x, y, identity_id) = {
                        let r = pe.read();
                        (r.x as i32, r.y as i32, r.player.identity.id)
                    };
                    let ids = block_grid::ids_in_area(
                        grid,
                        x,
                        y,
                        AreaType::SameArea,
                        slot.xs as i32,
                        slot.ys as i32,
                    );
                    for id in ids {
                        clif_object_look_by_id(pe.fd, &mut net.look, identity_id, id);
                    }
                }
                clif_mob_look_close_func_inner(pe.fd, &mut net.look);
            }
            crate::game::client::visual::clif_destroyold(pe);
            clif_sendchararea(pe);
            clif_getchararea(pe);
            if pe.read().player.appearance.setting_flags & FLAG_REALM != 0 {
                clif_sendminitext(pe, c"Realm-centered   :ON".as_ptr());
            } else {
                clif_sendminitext(pe, c"Realm-centered   :OFF".as_ptr());
            }
            clif_sendstatus(pe, 0);
        }
        0x08 => {
            pe.write().player.appearance.setting_flags ^= FLAG_EXCHANGE;
            if pe.read().player.appearance.setting_flags & FLAG_EXCHANGE != 0 {
                clif_sendminitext(pe, c"Exchange         :ON".as_ptr());
            } else {
                clif_sendminitext(pe, c"Exchange         :OFF".as_ptr());
            }
            clif_sendstatus(pe, 0);
        }
        0x09 => {
            pe.write().player.appearance.setting_flags ^= FLAG_FASTMOVE;
            if pe.read().player.appearance.setting_flags & FLAG_FASTMOVE != 0 {
                clif_sendminitext(pe, c"Fast Move        :ON".as_ptr());
            } else {
                clif_sendminitext(pe, c"Fast Move        :OFF".as_ptr());
            }
            clif_sendstatus(pe, 0);
        }
        10 => {
            pe.write().player.social.clan_chat = (pe.read().player.social.clan_chat + 1) % 2;
            if pe.read().player.social.clan_chat != 0 {
                clif_sendminitext(pe, c"Clan whisper     :ON".as_ptr());
            } else {
                clif_sendminitext(pe, c"Clan whisper     :OFF".as_ptr());
            }
        }
        13 => {
            if rbyte(pe.fd, 4) == 3 {
                return 0;
            }
            pe.write().player.appearance.setting_flags ^= FLAG_SOUND;
            if pe.read().player.appearance.setting_flags & FLAG_SOUND != 0 {
                clif_sendminitext(pe, c"Hear sounds      :ON".as_ptr());
            } else {
                clif_sendminitext(pe, c"Hear sounds      :OFF".as_ptr());
            }
            clif_sendstatus(pe, 0);
        }
        14 => {
            pe.write().player.appearance.setting_flags ^= FLAG_HELM;
            if pe.read().player.appearance.setting_flags & FLAG_HELM != 0 {
                clif_sendminitext(pe, c"Show Helmet      :ON".as_ptr());
                pc_setglobalreg(
                    &mut *pe.write() as *mut MapSessionData,
                    c"show_helmet".as_ptr(),
                    1,
                );
            } else {
                clif_sendminitext(pe, c"Show Helmet      :OFF".as_ptr());
                pc_setglobalreg(
                    &mut *pe.write() as *mut MapSessionData,
                    c"show_helmet".as_ptr(),
                    0,
                );
            }
            clif_sendstatus(pe, 0);
            clif_sendchararea(pe);
            clif_getchararea(pe);
        }
        15 => {
            pe.write().player.appearance.setting_flags ^= FLAG_NECKLACE;
            if pe.read().player.appearance.setting_flags & FLAG_NECKLACE != 0 {
                clif_sendminitext(pe, c"Show Necklace      :ON".as_ptr());
                pc_setglobalreg(
                    &mut *pe.write() as *mut MapSessionData,
                    c"show_necklace".as_ptr(),
                    1,
                );
            } else {
                clif_sendminitext(pe, c"Show Necklace      :OFF".as_ptr());
                pc_setglobalreg(
                    &mut *pe.write() as *mut MapSessionData,
                    c"show_necklace".as_ptr(),
                    0,
                );
            }
            clif_sendstatus(pe, 0);
            clif_sendchararea(pe);
            clif_getchararea(pe);
        }
        _ => {}
    }
    0
}

// ─── createdb_start ─────────────────────────────────────────────────────────
//
// Opcode 0x6B — item creation system.
// Reads ingredient items from the session buffer, builds a Lua `creationItems`
// table, and dispatches `itemCreation(pc)` script.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn createdb_start(pe: &PlayerEntity) -> i32 {
    use crate::game::scripting::sl_state;

    let fd = pe.fd;

    // RFIFOB(fd, 5) — number of ingredient slots in this packet.
    let item_c = rfifob(fd, 5) as usize;
    let item_c = item_c.min(10);

    let mut items = [0u32; 10];
    let mut amounts = [1u32; 10];
    let mut len = 6usize;

    for x in 0..item_c {
        // RFIFOB(fd, len) - 1 = inventory slot index.
        let raw = rfifob(fd, len) as usize;
        if raw == 0 {
            len += 1;
            continue;
        }
        let curitem = raw - 1;
        let maxinv = pe.read().player.inventory.max_inv as usize;
        if curitem < maxinv {
            items[x] = pe.read().player.inventory.inventory[curitem].id;
        }
        if item_db::search(items[x]).stack_amount > 1 {
            amounts[x] = rfifob(fd, len + 1) as u32;
            len += 2;
        } else {
            amounts[x] = 1;
            len += 1;
        }
    }

    {
        let mut w = pe.write();
        w.creation_works = 0;
        w.creation_item = 0;
        w.creation_itemamount = 0;
    }

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

    {
        let sd_ptr = &mut *pe.write() as *mut MapSessionData;
        sl_async_freeco(sd_ptr);
    }

    dispatch("itemCreation", None, &[pe.id]);
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
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_isregistered_sync(id: u32) -> i32 {
    blocking_run_async(clif_isregistered(id))
}

/// Sync wrapper for [`clif_getaccountemail`] — for use in FFI / non-async call sites.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn clif_getaccountemail_sync(id: u32) -> *const i8 {
    // Transmit the raw pointer through usize (which is Send) to satisfy blocking_run_async.
    let addr: usize = blocking_run_async(async move { clif_getaccountemail(id).await as usize });
    addr as *const i8
}
