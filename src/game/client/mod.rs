//! Client packet dispatcher — Rust replacement for `clif_parse` in `map_parse.c`.
//!
//! Packet framing (custom Nexon/TK protocol):
//!   [0]     = 0xAA magic
//!   [1..2]  = payload length (u16 big-endian)
//!   [3]     = opcode
//!   [4]     = encryption seed
//!   [5..]   = payload
//!
//! Total packet size = length_field + 3.
//!
//! All handler functions remain in C (`map_parse.c`) for now. They are called via
//! Client packet send functions. Remove stubs from this module
//! and add a Rust implementation in its place.

pub mod handlers;
pub mod visual;

use crate::game::player::entity::PlayerEntity;

#[inline]
unsafe fn clif_cancelafk(pe: &PlayerEntity) { visual::clif_cancelafk(pe); }
#[inline]
unsafe fn clif_sendprofile(pe: &PlayerEntity) { visual::clif_sendprofile(pe); }
#[inline]
unsafe fn clif_sendboard(pe: &PlayerEntity) { visual::clif_sendboard(pe); }

use crate::session::{
    session_exists, session_get_data, session_get_eof,
    session_set_eof, SessionId,
};
use crate::session::get_session_manager;
use crate::game::pc::MapSessionData;

// ─── Session buffer helpers ───────────────────────────────────────────────────

use crate::game::map_parse::packet::{rfifob, rfifop, rfiforest, rfifoskip, wfifohead, wfifop, wfifoset};

/// Read one byte from the receive FIFO at offset `pos` (big-endian context).
#[inline]
fn rbyte(fd: SessionId, pos: usize) -> u8 {
    rfifob(fd, pos)
}

/// Read a big-endian u16 from the receive FIFO at offset `pos`.
#[inline]
fn rword_be(fd: SessionId, pos: usize) -> u16 {
    if let Some(s) = get_session_manager().get_session(fd) {
        if let Ok(session) = s.try_lock() {
            let bytes = session.rdata_bytes();
            if pos + 1 < bytes.len() {
                return u16::from_be_bytes([bytes[pos], bytes[pos + 1]]);
            }
        }
    }
    0
}

/// Read a big-endian u32 from the receive FIFO at offset `pos`.
#[inline]
fn rlong_be(fd: SessionId, pos: usize) -> u32 {
    if let Some(s) = get_session_manager().get_session(fd) {
        if let Ok(session) = s.try_lock() {
            let bytes = session.rdata_bytes();
            if pos + 3 < bytes.len() {
                return u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
            }
        }
    }
    0
}

/// Raw pointer into recv buffer at `pos`.
#[inline]
unsafe fn rptr(fd: SessionId, pos: usize) -> *const i8 {
    rfifop(fd, pos) as *const i8
}


// Dispatcher wrappers — thin shims forwarding to their canonical modules.

use crate::network::crypt::{decrypt, encrypt};
use crate::game::scripting::pc_accessors::{
    sl_pc_time, sl_pc_set_time, sl_pc_chat_timer, sl_pc_set_chat_timer,
    sl_pc_attacked, sl_pc_set_attacked, sl_pc_attack_speed, sl_pc_loaded,
    sl_pc_paralyzed, sl_pc_sleep, sl_pc_status_gm_level,
    sl_pc_status_mute, sl_pc_inventory_id, sl_pc_bl_m, sl_map_spell,
};
use crate::database::item_db;
use crate::game::pc::pc_atkspeed;
use crate::game::time_util::timer_insert;

#[inline]
unsafe fn clif_isignore(src: &PlayerEntity, dst: &PlayerEntity) -> i32 {
    crate::game::map_parse::chat::clif_isignore(src, dst)
}
#[allow(dead_code)]
#[inline] unsafe fn decrypt_fd(fd: SessionId) { decrypt(fd); }
#[inline] async unsafe fn clif_handle_disconnect(pe: &PlayerEntity) { crate::game::client::handlers::clif_handle_disconnect(pe).await; }
#[inline] unsafe fn clif_closeit(pe: &PlayerEntity) { crate::game::map_parse::dialogs::clif_closeit(pe); }
#[inline] async unsafe fn clif_mystaytus(pe: &PlayerEntity) { crate::game::map_parse::player_state::clif_mystaytus(pe).await; }
#[inline] unsafe fn clif_groupstatus(pe: &PlayerEntity) { crate::game::map_parse::groups::clif_groupstatus(pe); }
#[inline] unsafe fn clif_refresh(pe: &PlayerEntity) { crate::game::map_parse::player_state::clif_refresh(pe); }
#[inline] unsafe fn clif_changestatus(pe: &PlayerEntity, status: u8) { crate::game::client::handlers::clif_changestatus(pe, status as i32); }
#[inline] async unsafe fn clif_accept2(fd: SessionId, name: *const i8, val: u8) { crate::game::client::handlers::clif_accept2(fd, name as *mut i8, val as i32).await; }
#[inline] unsafe fn clif_parsemap(pe: &PlayerEntity) { crate::game::map_parse::movement::clif_parsemap(pe); }
#[inline] unsafe fn clif_parsewalk(pe: &PlayerEntity) { crate::game::map_parse::movement::clif_parsewalk(pe); }
#[inline] unsafe fn clif_parsewalkpong(pe: &PlayerEntity) { crate::game::map_parse::movement::clif_parsewalkpong(pe); }
#[inline] unsafe fn clif_handle_missingobject(pe: &PlayerEntity) { crate::game::client::handlers::clif_handle_missingobject(pe); }
#[inline] unsafe fn clif_parselookat(pe: &PlayerEntity) { crate::game::map_parse::movement::clif_parselookat(pe); }
#[inline] unsafe fn clif_parselookat_2(pe: &PlayerEntity) { crate::game::map_parse::movement::clif_parselookat_2(pe); }
#[inline] unsafe fn clif_open_sub(pe: &PlayerEntity) { crate::game::map_parse::items::clif_open_sub(pe); }
#[inline] async unsafe fn clif_handle_clickgetinfo(pe: &PlayerEntity) { crate::game::map_parse::dialogs::clif_handle_clickgetinfo(pe).await; }
#[inline] unsafe fn clif_parseviewchange(pe: &PlayerEntity) { crate::game::map_parse::movement::clif_parseviewchange(pe); }
#[inline] unsafe fn clif_parseside(pe: &PlayerEntity) { crate::game::map_parse::movement::clif_parseside(pe); }
#[inline] unsafe fn clif_parseemotion(pe: &PlayerEntity) { crate::game::map_parse::chat::clif_parseemotion(pe); }
#[inline] unsafe fn clif_parsesay(pe: &PlayerEntity) { crate::game::map_parse::chat::clif_parsesay(pe); }
#[inline] unsafe fn clif_parsewisp(pe: &PlayerEntity) { crate::game::map_parse::chat::clif_parsewisp(pe); }
#[inline] unsafe fn clif_parseignore(pe: &PlayerEntity) { crate::game::map_parse::chat::clif_parseignore(pe); }
#[inline] async unsafe fn clif_parsefriends(pe: &PlayerEntity, list: *const i8, len: i32) { crate::game::client::handlers::clif_parsefriends(pe, list, len).await; }
#[inline] unsafe fn clif_user_list(pe: &PlayerEntity) { crate::game::client::visual::clif_user_list(pe); }
#[inline] unsafe fn clif_addgroup(pe: &PlayerEntity) { crate::game::map_parse::groups::clif_addgroup(pe); }
#[inline] unsafe fn clif_parsegetitem(pe: &PlayerEntity) { crate::game::map_parse::items::clif_parsegetitem(pe); }
#[inline] unsafe fn clif_parsedropitem(pe: &PlayerEntity) { crate::game::client::handlers::clif_parsedropitem(pe); }
#[inline] unsafe fn clif_parseeatitem(pe: &PlayerEntity) { crate::game::map_parse::items::clif_parseeatitem(pe); }
#[inline] unsafe fn clif_parseuseitem(pe: &PlayerEntity) { crate::game::map_parse::items::clif_parseuseitem(pe); }
#[inline] unsafe fn clif_parseunequip(pe: &PlayerEntity) { crate::game::map_parse::items::clif_parseunequip(pe); }
#[inline] unsafe fn clif_parsewield(pe: &PlayerEntity) { crate::game::map_parse::items::clif_parsewield(pe); }
#[inline] unsafe fn clif_parsethrow(pe: &PlayerEntity) { crate::game::map_parse::items::clif_parsethrow(pe); }
#[inline] unsafe fn clif_throwconfirm(pe: &PlayerEntity) { crate::game::map_parse::items::clif_throwconfirm(pe); }
#[inline] unsafe fn clif_dropgold(pe: &PlayerEntity, amount: u32) { crate::game::map_parse::items::clif_dropgold(pe, amount); }
#[inline] unsafe fn clif_postitem(pe: &PlayerEntity) { crate::game::client::handlers::clif_postitem(pe); }
#[inline] unsafe fn clif_handitem(pe: &PlayerEntity) { crate::game::map_parse::trading::clif_handitem(pe); }
#[inline] unsafe fn clif_handgold(pe: &PlayerEntity) { crate::game::map_parse::trading::clif_handgold(pe); }
#[inline] unsafe fn clif_parsemagic(pe: &PlayerEntity) { crate::game::map_parse::combat::clif_parsemagic(&mut *pe.write()); }
#[inline] unsafe fn clif_parseattack(pe: &PlayerEntity) { crate::game::map_parse::combat::clif_parseattack(&mut *pe.write()); }
#[inline] unsafe fn clif_sendminitext(pe: &PlayerEntity, msg: *const i8) { crate::game::map_parse::chat::clif_sendminitext(pe, msg); }
#[inline] unsafe fn clif_parsenpcdialog(pe: &PlayerEntity) { crate::game::map_parse::dialogs::clif_parsenpcdialog(pe); }
#[inline] unsafe fn clif_handle_menuinput(pe: &PlayerEntity) { crate::game::client::handlers::clif_handle_menuinput(pe); }
#[inline] unsafe fn clif_paperpopupwrite_save(pe: &PlayerEntity) { crate::game::client::visual::clif_paperpopupwrite_save(pe); }
#[inline] unsafe fn clif_parsechangespell(pe: &PlayerEntity) { crate::game::map_parse::items::clif_parsechangespell(pe); }
#[inline] unsafe fn clif_parsechangepos(pe: &PlayerEntity) { crate::game::client::handlers::clif_parsechangepos(pe); }
#[inline] async unsafe fn pc_warp(pe: &PlayerEntity, m: i32, x: i32, y: i32) -> i32 {
    // Extract a stable raw pointer from the Box inside the RwLock. The Box does not
    // move for the lifetime of the PlayerEntity, so sd_ptr remains valid across the
    // await points inside pc_warp as long as the Arc<PlayerEntity> (held by `sd` in
    // clif_parse) stays alive.
    let sd_ptr: *mut MapSessionData = { &mut *pe.write() as *mut MapSessionData };
    crate::game::pc::pc_warp(sd_ptr, m, x, y).await
}
#[inline] unsafe fn clif_changeprofile(pe: &PlayerEntity) { crate::game::client::visual::clif_changeprofile(pe); }
#[inline] async unsafe fn clif_handle_boards(pe: &PlayerEntity) { crate::game::client::handlers::clif_handle_boards(pe).await; }
#[inline] unsafe fn clif_handle_powerboards(pe: &PlayerEntity) { crate::game::client::handlers::clif_handle_powerboards(pe); }
#[inline] unsafe fn clif_parseparcel(pe: &PlayerEntity) { crate::game::map_parse::groups::clif_parseparcel(pe); }
#[inline] async unsafe fn clif_parseranking(pe: &PlayerEntity, fd: SessionId) { crate::game::map_parse::events::clif_parseranking(pe, fd).await; }
#[allow(dead_code, non_snake_case)]
#[inline] async unsafe fn clif_sendRewardInfo(pe: &PlayerEntity, fd: SessionId) { crate::game::map_parse::events::clif_sendRewardInfo(pe, fd).await; }
#[allow(dead_code, non_snake_case)]
#[inline] async unsafe fn clif_getReward(pe: &PlayerEntity, fd: SessionId) { crate::game::map_parse::events::clif_getReward(pe, fd).await; }
#[inline] unsafe fn clif_sendtowns(pe: &PlayerEntity) { crate::game::map_parse::dialogs::clif_sendtowns(pe); }
#[inline] async unsafe fn clif_huntertoggle(pe: &PlayerEntity) { crate::game::map_parse::groups::clif_huntertoggle(pe).await; }
#[inline] async unsafe fn clif_sendhunternote(pe: &PlayerEntity) { crate::game::map_parse::groups::clif_sendhunternote(pe).await; }
#[inline] unsafe fn clif_sendminimap(pe: &PlayerEntity) { crate::game::map_parse::player_state::clif_sendminimap(pe); }
#[inline] unsafe fn clif_parse_exchange(pe: &PlayerEntity) { crate::game::map_parse::trading::clif_parse_exchange(pe); }
#[inline] unsafe fn send_meta(pe: &PlayerEntity) {
    let mut guard = pe.write();
    crate::network::crypt::send_meta(&mut *guard as *mut MapSessionData);
}
#[inline] unsafe fn send_metalist(pe: &PlayerEntity) {
    let mut guard = pe.write();
    crate::network::crypt::send_metalist(&mut *guard as *mut MapSessionData);
}
#[inline] unsafe fn createdb_start(pe: &PlayerEntity) { crate::game::client::handlers::createdb_start(pe); }

// ─── Send-type constants (from map_parse.h) ───────────────────────────────────

use crate::common::constants::network::{
    ALL_CLIENT, SAMESRV, SAMEMAP, SAMEMAP_WOS,
    AREA, AREA_WOS, SAMEAREA, SAMEAREA_WOS,
    CORNER, SELF,
};

use crate::common::constants::entity::BL_PC_U8;

// ─── clif_send / clif_sendtogm ────────────────────────────────────────────────

/// Send `buf[0..len]` to clients matching `send_type`, applying ignore-list filtering.
///
/// # Safety
///
/// - `buf` must point to at least `len` readable bytes.
/// - When `send_type == SELF`, `src_id` must identify a valid player entity.
pub unsafe fn clif_send(
    buf: *const u8,
    len: i32,
    src_id: u32,
    m: u16,
    x: u16,
    y: u16,
    bl_type: u8,
    send_type: i32,
) -> i32 {
    // Source player arc (non-null only when src is BL_PC). Kept alive for full function scope.
    let src_arc = if bl_type == BL_PC_U8 { crate::game::map_server::map_id2sd_pc(src_id) } else { None };

    match send_type {
        ALL_CLIENT | SAMESRV => {
            for i_fd in get_session_manager().get_all_fds() {
                let sd_opt = session_get_data(i_fd);
                if sd_opt.is_none() {
                    continue;
                }
                // Ignore-list: skip if this is a whisper (opcode 0x0D) that src ignores.
                if let Some(src_pe) = src_arc.as_deref() {
                    let dst_pe = sd_opt.as_deref().unwrap();
                    if !buf.is_null()
                        && *buf.add(3) == 0x0D
                        && clif_isignore(src_pe, dst_pe) == 0
                    {
                        continue;
                    }
                }
                send_to_fd(i_fd, buf, len);
            }
        }
        SAMEMAP => {
            for i_fd in get_session_manager().get_all_fds() {
                let sd_opt = session_get_data(i_fd);
                let Some(dst_arc) = sd_opt else { continue; };
                if dst_arc.read().m != m {
                    continue;
                }
                if let Some(src_pe) = src_arc.as_deref() {
                    let dst_pe: &PlayerEntity = &*dst_arc;
                    if !buf.is_null()
                        && *buf.add(3) == 0x0D
                        && clif_isignore(src_pe, dst_pe) == 0
                    {
                        continue;
                    }
                }
                send_to_fd(i_fd, buf, len);
            }
        }
        SAMEMAP_WOS => {
            for i_fd in get_session_manager().get_all_fds() {
                let sd_opt = session_get_data(i_fd);
                let Some(dst_arc) = sd_opt else { continue; };
                if dst_arc.read().m != m {
                    continue;
                }
                // Skip sending to source itself when it is a player.
                if src_arc.is_some() && dst_arc.id == src_id {
                    continue;
                }
                if let Some(src_pe) = src_arc.as_deref() {
                    let dst_pe: &PlayerEntity = &*dst_arc;
                    if !buf.is_null()
                        && *buf.add(3) == 0x0D
                        && clif_isignore(src_pe, dst_pe) == 0
                    {
                        continue;
                    }
                }
                send_to_fd(i_fd, buf, len);
            }
        }
        AREA | AREA_WOS => {
            clif_send_area(
                m as i32,
                x as i32,
                y as i32,
                AREA,
                send_type,
                buf,
                len,
                src_id,
                bl_type,
            );
        }
        SAMEAREA | SAMEAREA_WOS => {
            clif_send_area(
                m as i32,
                x as i32,
                y as i32,
                SAMEAREA,
                send_type,
                buf,
                len,
                src_id,
                bl_type,
            );
        }
        CORNER => {
            clif_send_area(
                m as i32,
                x as i32,
                y as i32,
                CORNER,
                send_type,
                buf,
                len,
                src_id,
                bl_type,
            );
        }
        SELF => {
            if let Some(arc) = crate::game::map_server::map_id2sd_pc(src_id) {
                send_to_fd(arc.fd, buf, len);
            }
        }
        _ => {}
    }
    0
}

/// Send `buf[0..len]` to clients matching `send_type`, without ignore-list filtering.
///
/// # Safety
///
/// - `buf` must point to at least `len` readable bytes.
/// - When `send_type == SELF`, `src_id` must identify a valid player entity.
pub unsafe fn clif_sendtogm(
    buf: *const u8,
    len: i32,
    src_id: u32,
    m: u16,
    x: u16,
    y: u16,
    bl_type: u8,
    send_type: i32,
) -> i32 {
    match send_type {
        ALL_CLIENT | SAMESRV => {
            for i_fd in get_session_manager().get_all_fds() {
                let sd_opt = session_get_data(i_fd);
                if sd_opt.is_none() {
                    continue;
                }
                send_to_fd(i_fd, buf, len);
            }
        }
        SAMEMAP => {
            for i_fd in get_session_manager().get_all_fds() {
                let sd_opt = session_get_data(i_fd);
                let Some(dst_arc) = sd_opt else { continue; };
                if dst_arc.read().m != m {
                    continue;
                }
                send_to_fd(i_fd, buf, len);
            }
        }
        SAMEMAP_WOS => {
            for i_fd in get_session_manager().get_all_fds() {
                let sd_opt = session_get_data(i_fd);
                let Some(dst_arc) = sd_opt else { continue; };
                if dst_arc.read().m != m {
                    continue;
                }
                // Skip sending to source itself when it is a player.
                if bl_type == BL_PC_U8 && dst_arc.id == src_id {
                    continue;
                }
                send_to_fd(i_fd, buf, len);
            }
        }
        AREA | AREA_WOS => {
            clif_send_area(
                m as i32,
                x as i32,
                y as i32,
                AREA,
                send_type,
                buf,
                len,
                src_id,
                bl_type,
            );
        }
        SAMEAREA | SAMEAREA_WOS => {
            clif_send_area(
                m as i32,
                x as i32,
                y as i32,
                SAMEAREA,
                send_type,
                buf,
                len,
                src_id,
                bl_type,
            );
        }
        CORNER => {
            clif_send_area(
                m as i32,
                x as i32,
                y as i32,
                CORNER,
                send_type,
                buf,
                len,
                src_id,
                bl_type,
            );
        }
        SELF => {
            if let Some(arc) = crate::game::map_server::map_id2sd_pc(src_id) {
                send_to_fd(arc.fd, buf, len);
            }
        }
        _ => {}
    }
    0
}

/// Write `buf[0..len]` into fd's send buffer and commit.
///
/// Encrypt and commit `buf` to the send FIFO.
///
/// # Safety
///
/// - `fd` must be a live session registered with the session manager.
/// - `buf` must point to at least `len` readable bytes.
#[inline]
unsafe fn send_to_fd(fd: SessionId, buf: *const u8, len: i32) {
    wfifohead(fd, (len as usize) + 3);
    let wptr = wfifop(fd, 0);
    if !wptr.is_null() {
        std::ptr::copy_nonoverlapping(buf, wptr, len as usize);
    }
    wfifoset(fd, encrypt(fd) as usize);
}

// ─── clif_send_area / clif_send_sub ─────────────────────────────────────────

/// Channel registry: (global_reg name, channel byte value).
///
/// When a 0x0D packet carries one of these channel byte values, the player must
/// have the corresponding global_reg set ≥ 1 to receive it.  The channel byte is
/// temporarily zeroed out while writing to the session buffer, then restored.
const CHANNEL_REGS: &[(&str, u8)] = &[
    ("chann_en", 10),
    ("chann_es", 11),
    ("chann_fr", 12),
    ("chann_cn", 13),
    ("chann_pt", 14),
    ("chann_id", 15),
];

/// Decide whether the packet in `buf` should be sent to the target player `dst`,
/// given that the source is `src_arc` and the send type is `send_type`.
///
/// Returns `true` if the packet should be delivered.
///
/// # Safety
/// - `buf` must point to at least `len` readable bytes.
#[inline]
unsafe fn should_send_to(
    dst: &PlayerEntity,
    src_arc: &Option<std::sync::Arc<PlayerEntity>>,
    src_id: u32,
    _bl_type: u8,
    send_type: i32,
    buf: *const u8,
    len: i32,
) -> bool {
    use crate::game::pc::{OPT_FLAG_STEALTH, OPT_FLAG_GHOSTS};
    use crate::database::map_db::raw_map_ptr;
    use crate::database::map_db::MAP_SLOTS;

    if let Some(src_pe) = src_arc.as_deref() {
        let src_sd = src_pe.read();
        let dst_sd = dst.read();

        // ── Stealth filter ────────────────────────────────────────────────────────
        // If source is stealthed, only send to GMs or to the source themselves.
        if (src_sd.optFlags & OPT_FLAG_STEALTH) != 0
            && dst_sd.player.identity.gm_level == 0
            && dst_sd.player.identity.id != src_sd.player.identity.id
        {
            return false;
        }

        // ── Ghost filter ──────────────────────────────────────────────────────
        // If the map shows ghosts and the source is a ghost (state==1), only
        // send to other ghosts or to players that opted into ghost visibility.
        let m_idx = src_sd.m as usize;
        if !raw_map_ptr().is_null() && m_idx < MAP_SLOTS {
            let map_slot = &*raw_map_ptr().add(m_idx);
            if map_slot.show_ghosts != 0
                && src_sd.player.combat.state == 1
                && src_sd.id != dst_sd.id
                && dst_sd.player.combat.state != 1
                && (dst_sd.optFlags & OPT_FLAG_GHOSTS) == 0
            {
                return false;
            }
        }

        // ── Ignore-list filter (whisper-like packets, opcode 0x0D) ───────────────
        drop(src_sd);
        drop(dst_sd);
        if len >= 4 && !buf.is_null() && *buf.add(3) == 0x0D {
            if clif_isignore(src_pe, dst) == 0 {
                return false;
            }
        }
    }

    // ── WOS (without self) filter ─────────────────────────────────────────────
    match send_type {
        AREA_WOS | SAMEAREA_WOS => {
            if dst.id == src_id {
                return false;
            }
        }
        _ => {}
    }

    // ── Session liveness ──────────────────────────────────────────────────────
    if !session_exists(dst.fd) {
        return false;
    }

    true
}

/// Send `buf[0..len]` to all players in the spatial area defined by `area` around
/// `(x, y)` on map `m`, applying the filtering logic from `should_send_to`.
///
/// For channel packets (opcode 0x0D with byte 5 >= 10), the channel byte is
/// temporarily set to 0 before writing to each player's session buffer and
/// restored afterwards (mirrors the C behaviour).
///
/// # Safety
/// - `buf` must be a valid, writable pointer to at least `len` bytes.
///   The function temporarily mutates `buf[5]` for channel packets and restores it.
/// - The `map` global and block grid must be initialized.
unsafe fn send_to_area(
    m: i32,
    x: i32,
    y: i32,
    area: crate::game::block::AreaType,
    buf: *mut u8,
    len: i32,
    src_id: u32,
    bl_type: u8,
    send_type: i32,
) {
    use crate::game::block_grid;

    if buf.is_null() || len <= 0 {
        return;
    }

    // Determine if this is a channel packet: opcode 0x0D (byte 3) and channel byte (byte 5) >= 10.
    let is_channel_pkt = len >= 6 && *buf.add(3) == 0x0D && *buf.add(5) >= 10;

    let Some(grid) = block_grid::get_grid(m as usize) else { return; };
    let slot = &*crate::database::map_db::raw_map_ptr().add(m as usize);
    let ids = block_grid::ids_in_area(grid, x, y, area, slot.xs as i32, slot.ys as i32);

    // Source player arc kept alive for the duration of the area send.
    let src_arc = if bl_type == BL_PC_U8 { crate::game::map_server::map_id2sd_pc(src_id) } else { None };

    let mut _send_count = 0i32;
    for id in ids {
        let Some(dst_arc) = crate::game::map_server::map_id2sd_pc(id) else { continue; };
        let dst_pe: &PlayerEntity = &*dst_arc;

        if !should_send_to(dst_pe, &src_arc, src_id, bl_type, send_type, buf as *const u8, len) {
            continue;
        }

        let fd = dst_pe.fd;
        if len >= 4 && *buf.add(3) == 0x1A {
            _send_count += 1;
            tracing::debug!("[attack] send_to_area: sending 0x1A action to fd={} (player id={})", fd, dst_pe.id);
        }

        if is_channel_pkt {
            // Channel packet: check if the player has the matching channel reg.
            let ch_byte = *buf.add(5);
            let mut matched = false;
            for &(reg_name, ch_val) in CHANNEL_REGS {
                if ch_byte == ch_val {
                    // Check if player has this channel enabled (global_reg >= 1).
                    let reg_cstr = std::ffi::CString::new(reg_name).unwrap_or_default();
                    let v = {
                        let mut guard = dst_pe.write();
                        let sd_ptr: *mut MapSessionData = &mut *guard as *mut MapSessionData;
                        crate::game::pc::pc_readglobalreg(sd_ptr, reg_cstr.as_ptr() as *const i8)
                    };
                    if v >= 1 {
                        // Temporarily zero out channel byte, write, restore.
                        *buf.add(5) = 0;
                        wfifohead(fd, (len as usize) + 3);
                        let wptr = wfifop(fd, 0);
                        if !wptr.is_null() {
                            std::ptr::copy_nonoverlapping(buf, wptr, len as usize);
                        }
                        wfifoset(fd, encrypt(fd) as usize);
                        *buf.add(5) = ch_byte;
                        matched = true;
                    }
                    break;
                }
            }
            // If channel byte doesn't match any known channel, send normally.
            if !matched && !CHANNEL_REGS.iter().any(|&(_, v)| v == ch_byte) {
                wfifohead(fd, (len as usize) + 3);
                let wptr = wfifop(fd, 0);
                if !wptr.is_null() {
                    std::ptr::copy_nonoverlapping(buf, wptr, len as usize);
                }
                wfifoset(fd, encrypt(fd) as usize);
            }
        } else {
            // Normal packet: write directly.
            wfifohead(fd, (len as usize) + 3);
            let wptr = wfifop(fd, 0);
            if !wptr.is_null() {
                std::ptr::copy_nonoverlapping(buf, wptr, len as usize);
            }
            wfifoset(fd, encrypt(fd) as usize);
        }

    }
}

////
/// Wrapper around `send_to_area`
/// same signature as the old C function. `area_type` selects the spatial search
/// shape (AREA=4, SAMEAREA=6, CORNER=8); `send_type` is the send-type constant
/// (AREA_WOS=5, SAMEAREA_WOS=7, etc.) passed to the per-player filter.
///
/// # Safety
/// - `buf` must be a valid, writable pointer to at least `len` bytes.
pub unsafe fn clif_send_area(
    m: i32,
    x: i32,
    y: i32,
    area_type: i32,
    send_type: i32,
    buf: *const u8,
    len: i32,
    src_id: u32,
    bl_type: u8,
) {
    use crate::game::block::AreaType;

    let area = match area_type {
        AREA      => AreaType::Area,
        SAMEAREA  => AreaType::SameArea,
        CORNER    => AreaType::Corner,
        _         => AreaType::Area,
    };

    // Cast away const: send_to_area temporarily mutates buf[5] for channel packets
    // and restores it before returning. The caller's buffer is logically unchanged.
    send_to_area(m, x, y, area, buf as *mut u8, len, src_id, bl_type, send_type);
}

// ─── Dual-login check ─────────────────────────────────────────────────────────

/// Returns `true` if a duplicate session was detected (both connections closed).
///
/// Uses the session manager's fd map directly — no fixed-size buffer needed.
unsafe fn check_dual_login(fd: SessionId, pe: &PlayerEntity) -> bool {
    let my_id = pe.read().player.identity.id;
    let mut login_count = 0i32;
    for i_fd in get_session_manager().get_all_fds() {
        let tsd_opt = session_get_data(i_fd);
        let Some(t_arc) = tsd_opt else { continue; };
        if t_arc.read().player.identity.id == my_id {
            login_count += 1;
        }
        if login_count >= 2 {
            tracing::warn!("[map] dual login char_id={} fd={} dup_fd={}", my_id, fd, i_fd);
            session_set_eof(fd, 1);
            session_set_eof(i_fd, 1);
            return true;
        }
    }
    false
}

// ─── Main dispatcher ──────────────────────────────────────────────────────────

/// Rust replacement for C `clif_parse(int fd)`.
/// Registered as the default parse callback at map_server startup.
pub async fn clif_parse(fd: SessionId) -> i32 {
    unsafe {
        if !session_exists(fd) {
            return 0;
        }

        let sd = session_get_data(fd);

        // EOF → disconnect and clean up
        if session_get_eof(fd) != 0 {
            tracing::info!("[map] [parse] fd={} eof reason={} sd_none={}", fd, session_get_eof(fd), sd.is_none());
            if let Some(pe) = sd.as_deref() {
                clif_handle_disconnect(pe).await;
                clif_closeit(pe);
            }
            visual::clif_print_disconnect(fd);
            session_set_eof(fd, 1);
            return 0;
        }

        // Validate packet header: must start with 0xAA
        let avail = rfiforest(fd) as usize;
        if avail > 0 && rbyte(fd, 0) != 0xAA {
            tracing::warn!("[map] [parse] fd={} bad header byte0={:#04X} avail={}", fd, rbyte(fd, 0), avail);
            session_set_eof(fd, 13);
            return 0;
        }
        if avail < 3 { return 0; }

        let pkt_len = rword_be(fd, 1) as usize + 3;
        if avail < pkt_len { return 0; }

        // Pre-login: only opcode 0x10 (character accept) is allowed
        if sd.is_none() {
            let op = rbyte(fd, 3);
            if op == 0x10 {
                tracing::debug!("[map] [parse] fd={} pre-login accept op=0x10", fd);
                clif_accept2(fd, rptr(fd, 16), rbyte(fd, 15)).await;
            } else {
                tracing::debug!("[map] [parse] fd={} pre-login op={:#04X} dropped (sd not set)", fd, op);
            }
            rfifoskip(fd, pkt_len);
            return 0;
        }

        let pe: &PlayerEntity = sd.as_deref().unwrap();

        // Dual-login check
        if check_dual_login(fd, pe) {
            rfifoskip(fd, pkt_len);
            return 0;
        }

        decrypt(fd);

        tracing::debug!("[map] [parse] fd={} op={:#04X} pkt_len={}", fd, rbyte(fd, 3), pkt_len);
        match rbyte(fd, 3) {
            0x05 => {
                clif_parsemap(pe);
            }
            0x06 => {
                clif_cancelafk(pe);
                clif_parsewalk(pe);
            }
            0x07 => {
                clif_cancelafk(pe);
                { let _t = sl_pc_time(&mut *pe.write()); sl_pc_set_time(&mut *pe.write(), _t + 1); }
                if sl_pc_time(&mut *pe.write()) < 4 {
                    clif_parsegetitem(pe);
                }
            }
            0x08 => {
                clif_cancelafk(pe);
                clif_parsedropitem(pe);
            }
            0x09 => {
                clif_cancelafk(pe);
                clif_parselookat_2(pe);
            }
            0x0A => {
                clif_cancelafk(pe);
                clif_parselookat(pe);
            }
            0x0B => {
                clif_cancelafk(pe);
                clif_closeit(pe);
            }
            0x0C => {
                clif_handle_missingobject(pe);
            }
            0x0D => {
                clif_parseignore(pe);
            }
            0x0E => {
                clif_cancelafk(pe);
                if sl_pc_status_gm_level(&mut *pe.write()) != 0 {
                    clif_parsesay(pe);
                } else {
                    { let _t = sl_pc_chat_timer(&mut *pe.write()); sl_pc_set_chat_timer(&mut *pe.write(), _t + 1); }
                    if sl_pc_chat_timer(&mut *pe.write()) < 2 && sl_pc_status_mute(&mut *pe.write()) == 0 {
                        clif_parsesay(pe);
                    }
                }
            }
            0x0F => {
                clif_cancelafk(pe);
                { let _t = sl_pc_time(&mut *pe.write()); sl_pc_set_time(&mut *pe.write(), _t + 1); }
                if sl_pc_paralyzed(&mut *pe.write()) == 0 && sl_pc_sleep(&mut *pe.write()) == 1 {
                    if sl_pc_time(&mut *pe.write()) < 4 {
                        if sl_map_spell(sl_pc_bl_m(&mut *pe.write())) != 0 || sl_pc_status_gm_level(&mut *pe.write()) != 0 {
                            clif_parsemagic(pe);
                        } else {
                            clif_sendminitext(
                                pe,
                                b"That doesn't work here.\0".as_ptr() as *const i8,
                            );
                        }
                    }
                }
            }
            0x11 => {
                clif_cancelafk(pe);
                clif_parseside(pe);
            }
            0x12 => {
                clif_cancelafk(pe);
                clif_parsewield(pe);
            }
            0x13 => {
                clif_cancelafk(pe);
                { let _t = sl_pc_time(&mut *pe.write()); sl_pc_set_time(&mut *pe.write(), _t + 1); }
                let attacked_val = sl_pc_attacked(&mut *pe.write());
                let spd_val = sl_pc_attack_speed(&mut *pe.write());
                let pe_id = pe.id;
                tracing::debug!("[attack] id={} attacked={} spd={}", pe_id, attacked_val, spd_val);
                if attacked_val != 1 && spd_val > 0 {
                    sl_pc_set_attacked(&mut *pe.write(), 1);
                    let delay = ((spd_val * 1000) / 60) as u32;
                    tracing::debug!("[attack] id={} delay={}ms — entering clif_parseattack", pe_id, delay);
                    timer_insert(
                        delay, delay, Some(pc_atkspeed), pe_id as i32, 0,
                    );
                    clif_parseattack(pe);
                } else {
                    tracing::warn!("[attack] id={} BLOCKED: attacked={} spd={}", pe_id, attacked_val, spd_val);
                }
            }
            0x17 => {
                clif_cancelafk(pe);
                let pos = rbyte(fd, 6) as i32;
                let confirm = rbyte(fd, 5);
                if item_db::search(sl_pc_inventory_id(&mut *pe.write(), pos - 1)).thrownconfirm == 1 {
                    if confirm == 1 { clif_parsethrow(pe); } else { clif_throwconfirm(pe); }
                } else {
                    clif_parsethrow(pe);
                }
            }
            0x18 => {
                clif_cancelafk(pe);
                clif_user_list(pe);
            }
            0x19 => {
                clif_cancelafk(pe);
                clif_parsewisp(pe);
            }
            0x1A => {
                clif_cancelafk(pe);
                clif_parseeatitem(pe);
            }
            0x1B => {
                if sl_pc_loaded(&mut *pe.write()) != 0 {
                    clif_changestatus(pe, rbyte(fd, 6));
                }
            }
            0x1C => {
                clif_cancelafk(pe);
                clif_parseuseitem(pe);
            }
            0x1D => {
                clif_cancelafk(pe);
                { let _t = sl_pc_time(&mut *pe.write()); sl_pc_set_time(&mut *pe.write(), _t + 1); }
                if sl_pc_time(&mut *pe.write()) < 4 {
                    clif_parseemotion(pe);
                }
            }
            0x1E => {
                clif_cancelafk(pe);
                { let _t = sl_pc_time(&mut *pe.write()); sl_pc_set_time(&mut *pe.write(), _t + 1); }
                if sl_pc_time(&mut *pe.write()) < 4 {
                    clif_parsewield(pe);
                }
            }
            0x1F => {
                clif_cancelafk(pe);
                if sl_pc_time(&mut *pe.write()) < 4 {
                    clif_parseunequip(pe);
                }
            }
            0x20 => {
                clif_cancelafk(pe);
                clif_open_sub(pe);
            }
            0x23 => {
                clif_paperpopupwrite_save(pe);
            }
            0x24 => {
                clif_cancelafk(pe);
                clif_dropgold(pe, rlong_be(fd, 5));
            }
            0x27 => {
                clif_cancelafk(pe);
                // Quest tab — no-op
            }
            0x29 => {
                clif_cancelafk(pe);
                clif_handitem(pe);
            }
            0x2A => {
                clif_cancelafk(pe);
                clif_handgold(pe);
            }
            0x2D => {
                clif_cancelafk(pe);
                if rbyte(fd, 5) == 0 { clif_mystaytus(pe).await; } else { clif_groupstatus(pe); }
            }
            0x2E => {
                clif_cancelafk(pe);
                clif_addgroup(pe);
            }
            0x30 => {
                clif_cancelafk(pe);
                if rbyte(fd, 5) == 1 { clif_parsechangespell(pe); } else { clif_parsechangepos(pe); }
            }
            0x32 => {
                clif_cancelafk(pe);
                clif_parsewalk(pe);
            }
            // 0x34 falls through to 0x38 in C — both fire
            0x34 => {
                clif_cancelafk(pe);
                clif_postitem(pe);
                clif_cancelafk(pe);
                clif_refresh(pe);
            }
            0x38 => {
                clif_cancelafk(pe);
                clif_refresh(pe);
            }
            0x39 => {
                clif_cancelafk(pe);
                clif_handle_menuinput(pe);
            }
            0x3A => {
                clif_cancelafk(pe);
                clif_parsenpcdialog(pe);
            }
            0x3B => {
                clif_cancelafk(pe);
                clif_handle_boards(pe).await;
            }
            0x3F => {
                pc_warp(pe, rword_be(fd, 5) as i32, rword_be(fd, 7) as i32, rword_be(fd, 9) as i32).await;
            }
            0x41 => {
                clif_cancelafk(pe);
                clif_parseparcel(pe);
            }
            0x42 => { /* Client crash debug — no-op */ }
            0x43 => {
                clif_cancelafk(pe);
                clif_handle_clickgetinfo(pe).await;
            }
            0x4A => {
                clif_cancelafk(pe);
                clif_parse_exchange(pe);
            }
            0x4C => {
                clif_cancelafk(pe);
                clif_handle_powerboards(pe);
            }
            0x4F => {
                clif_cancelafk(pe);
                clif_changeprofile(pe);
            }
            0x60 => { /* PING — no-op */ }
            0x66 => {
                clif_cancelafk(pe);
                clif_sendtowns(pe);
            }
            0x69 => { /* Obstruction — no-op */ }
            0x6B => {
                clif_cancelafk(pe);
                createdb_start(pe);
            }
            0x73 => {
                if rbyte(fd, 5) == 0x04 {
                    clif_sendprofile(pe);
                } else if rbyte(fd, 5) == 0x00 {
                    clif_sendboard(pe);
                }
            }
            0x75 => {
                clif_parsewalkpong(pe);
            }
            0x77 => {
                clif_cancelafk(pe);
                let friends_len = rword_be(fd, 1) as i32 - 5;
                clif_parsefriends(pe, rptr(fd, 5), friends_len).await;
            }
            0x7B => match rbyte(fd, 5) {
                0 => send_meta(pe),
                1 => send_metalist(pe),
                _ => {}
            },
            0x7C => {
                clif_cancelafk(pe);
                clif_sendminimap(pe);
            }
            0x7D => {
                clif_cancelafk(pe);
                match rbyte(fd, 5) {
                    5 => clif_sendRewardInfo(pe, fd).await,
                    6 => clif_getReward(pe, fd).await,
                    _ => clif_parseranking(pe, fd).await,
                }
            }
            0x82 => {
                clif_cancelafk(pe);
                clif_parseviewchange(pe);
            }
            0x83 => { /* Screenshots — no-op */ }
            0x84 => {
                clif_cancelafk(pe);
                clif_huntertoggle(pe).await;
            }
            0x85 => {
                clif_sendhunternote(pe).await;
                clif_cancelafk(pe);
            }
            op => {
                tracing::warn!("[map] [client] unknown packet op={:#04X}", op);
            }
        }

        rfifoskip(fd, pkt_len);
        0
    }
}
