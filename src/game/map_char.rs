//! Map-char inter-server communication.
//!
//! Contains `intif_install_player` — the login landing function that installs a
//! freshly-received `PlayerData` into a live session and fires the full
//! player-login sequence.

#![allow(non_snake_case, dead_code, unused_variables)]

use std::ptr;

use crate::common::constants::entity::player::{
    OPT_FLAG_WALKTHROUGH, SFLAG_FULLSTATS, SFLAG_HPMP, SFLAG_XPMONEY,
};
use crate::common::player::PlayerData;
use crate::common::traits::LegacyEntity;
use crate::database::map_db::raw_map_ptr;
use crate::database::{assert_send, blocking_run_async, get_pool};
use crate::game::block::{map_is_loaded, AreaType};
use crate::game::block_grid;
use crate::game::client::visual::broadcast_update_state;
use crate::game::lua::dispatch::dispatch;
use crate::game::map_parse::player_state::{
    clif_getchararea, clif_mystatus, clif_refresh, clif_retrieveprofile, clif_sendack,
    clif_sendid, clif_sendmapinfo, clif_sendstatus, clif_sendtime, clif_sendxy,
};
use crate::game::map_parse::visual::{
    clif_mob_look_close_func_inner, clif_mob_look_start_func_inner, clif_object_look_by_id,
    clif_spawn,
};
use crate::game::map_server::{map_addiddb_player, map_id2sd_pc, mmo_setonline, MAP_FD};
use crate::game::pc::{
    pc_calcstat, pc_checklevel_pe, pc_loadequip, pc_loaditem, pc_loadmagic, pc_magic_startup_pe,
    pc_requestmp, pc_setpos, pc_starttimer, MapSessionData,
};
use crate::network::crypt::populate_table;
use crate::session::{get_session_manager, session_get_client_ip, SessionId};

// ---------------------------------------------------------------------------
// intif_install_player — replaces intif_mmo_tosd (bincode era)
// ---------------------------------------------------------------------------

/// Installs a `PlayerData` (deserialized from bincode by the caller) into a
/// fresh MapSessionData and fires the full player-login sequence.
pub fn intif_install_player(fd: i32, player: PlayerData) -> i32 {
    let sid = SessionId::from_raw(fd);
    if fd == MAP_FD.load(std::sync::atomic::Ordering::Relaxed) {
        return 0;
    }

    // ── Phase 1: Build MapSessionData in a Box ──────────────────────────────
    // SAFETY: Box::new_zeroed heap-allocates (~3MB) without a stack copy.
    // assume_init is valid for numeric/pointer fields. The ptr::write
    // immediately overwrites the zeroed PlayerData (String/Vec/HashMap)
    // before anything can read or drop those fields.
    let mut sd_box: Box<MapSessionData> = unsafe { Box::new_zeroed().assume_init() };
    unsafe { ptr::write(&mut sd_box.player, player) };

    sd_box.fd = sid;
    sd_box.id = sd_box.player.identity.id;

    // Encryption hash from character name.
    {
        let name = sd_box.player.identity.name.as_bytes();
        let n = name.len().min(15);
        let mut buf = [0u8; 16];
        buf[..n].copy_from_slice(&name[..n]);
        populate_table(&buf[..n], &mut sd_box.EncHash);
    }

    // Visual / display defaults.
    sd_box.disguise = sd_box.player.appearance.disguise;
    sd_box.disguise_color = sd_box.player.appearance.disguise_color;
    sd_box.viewx = 8;
    sd_box.viewy = 7;

    // Copy IP address into legacy fixed-size array.
    {
        let ip = sd_box.player.identity.ipaddress.as_bytes();
        let n = ip.len().min(254);
        for (dst, &src) in sd_box.ipaddress[..n].iter_mut().zip(ip) {
            *dst = src as i8;
        }
        sd_box.ipaddress[n] = 0;
    }

    // Query DB for stored map position.
    let char_id = sd_box.player.identity.id;
    let pos_opt: Option<(u32, u32, u32)> = blocking_run_async(async move {
        let pool = get_pool();
        sqlx::query_as::<_, (u32, u32, u32)>(
            "SELECT `ChaMapId`, `ChaX`, `ChaY` FROM `Character` WHERE `ChaId` = ?",
        )
        .bind(char_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
    });

    if let Some((map_id, cx, cy)) = pos_opt {
        sd_box.player.identity.last_pos.m = map_id as u16;
        sd_box.player.identity.last_pos.x = cx as u16;
        sd_box.player.identity.last_pos.y = cy as u16;
    }

    if sd_box.player.identity.gm_level != 0 {
        sd_box.optFlags |= OPT_FLAG_WALKTHROUGH;
    }

    if !map_is_loaded(sd_box.player.identity.last_pos.m as i32) {
        sd_box.player.identity.last_pos.m = 0;
        sd_box.player.identity.last_pos.x = 8;
        sd_box.player.identity.last_pos.y = 7;
    }

    // Legacy pc_* functions still take *mut MapSessionData.
    let last_pos = sd_box.player.identity.last_pos;
    let sd_ptr: *mut MapSessionData = &mut *sd_box;
    unsafe {
        pc_setpos(sd_ptr, last_pos.m as i32, last_pos.x as i32, last_pos.y as i32);
        pc_loadmagic(sd_ptr);
        pc_starttimer(sd_ptr);
        pc_requestmp(sd_ptr);
    }

    // ── Phase 2: Insert into PLAYER_MAP — use Arc from here on ──────────────
    let sd_id = sd_box.id;
    let sd_fd = sd_box.fd;
    tracing::info!("[map] [login] fd={:?} step=addiddb", sid);
    map_addiddb_player(sd_id, sd_fd, sd_box);
    let arc = map_id2sd_pc(sd_id).expect("player just inserted");

    // Store Arc in session so encrypt/decrypt can find the EncHash.
    if let Some(session_arc) = get_session_manager().get_session(sd_fd) {
        if let Ok(mut session) = session_arc.try_lock() {
            session.session_data = Some(arc.clone());
        }
    }

    // ── Phase 3: Login packet sequence (unsafe — raw packet writes) ────────
    let fd = arc.fd;
    unsafe {
        tracing::info!("[map] [login] fd={} step=sendack", fd);
        clif_sendack(&arc);
        tracing::info!("[map] [login] fd={} step=sendtime", fd);
        clif_sendtime(&arc);
        tracing::info!("[map] [login] fd={} step=sendid", fd);
        clif_sendid(&arc);
        tracing::info!("[map] [login] fd={} step=sendmapinfo", fd);
        clif_sendmapinfo(&arc);
        tracing::info!("[map] [login] fd={} step=sendstatus", fd);
        clif_sendstatus(&arc, SFLAG_FULLSTATS | SFLAG_HPMP | SFLAG_XPMONEY);
        tracing::info!("[map] [login] fd={} step=mystaytus_1", fd);
        clif_mystatus(&arc);
        tracing::info!("[map] [login] fd={} step=spawn", fd);
        clif_spawn(&arc);
        tracing::info!("[map] [login] fd={} step=refresh", fd);
        clif_refresh(&arc);
        tracing::info!("[map] [login] fd={} step=sendxy", fd);
        clif_sendxy(&arc);
        tracing::info!("[map] [login] fd={} step=getchararea", fd);
        clif_getchararea(&arc);

        tracing::info!("[map] [login] fd={} step=mob_look_start", fd);
        {
            let (m, x, y, player_id) = {
                let sd = arc.read();
                (sd.m, sd.x, sd.y, sd.player.identity.id)
            };
            let mut net = arc.net.write();
            clif_mob_look_start_func_inner(arc.fd, &mut net.look);
            if let Some(grid) = block_grid::get_grid(m as usize) {
                let slot = &*raw_map_ptr().add(m as usize);
                let ids = block_grid::ids_in_area(
                    grid,
                    x as i32,
                    y as i32,
                    AreaType::SameArea,
                    slot.xs as i32,
                    slot.ys as i32,
                );
                for id in ids {
                    clif_object_look_by_id(arc.fd, &mut net.look, player_id, id);
                }
            }
            clif_mob_look_close_func_inner(arc.fd, &mut net.look);
        }

        tracing::info!("[map] [login] fd={} step=loaditem", fd);
        pc_loaditem(&arc);
        tracing::info!("[map] [login] fd={} step=loadequip", fd);
        pc_loadequip(&arc);
    }

    tracing::info!("[map] [login] fd={} step=magic_startup", fd);
    pc_magic_startup_pe(&arc);

    let (player_id, player_name) = {
        let sd = arc.read();
        (sd.player.identity.id, sd.player.identity.name.clone())
    };
    let fire_login_hook = unsafe {
        blocking_run_async(assert_send(mmo_setonline(player_id, 1)))
    };
    if fire_login_hook {
        let raw_ip = session_get_client_ip(fd);
        let addr = format!(
            "{}.{}.{}.{}",
            raw_ip & 0xff,
            (raw_ip >> 8) & 0xff,
            (raw_ip >> 16) & 0xff,
            (raw_ip >> 24) & 0xff
        );
        println!("[map] [login] name={} addr={}", player_name, addr);
        tracing::info!("[map] [login] fd={} step=lua_login_start", fd);
        dispatch("login", None, &[arc.id]);
        tracing::info!("[map] [login] fd={} step=lua_login_done", fd);
    }

    unsafe {
        tracing::info!("[map] [login] fd={} step=calcstat", fd);
        pc_calcstat(&arc);
        pc_checklevel_pe(&arc);
        tracing::info!("[map] [login] fd={} step=mystaytus_2", fd);
        clif_mystatus(&arc);

        tracing::info!("[map] [login] fd={} step=updatestate", fd);
        broadcast_update_state(&arc);

        tracing::info!("[map] [login] fd={} step=retrieveprofile", fd);
        clif_retrieveprofile(&arc);
    }
    tracing::info!("[map] [login] fd={} step=done", fd);
    0
}

// ---------------------------------------------------------------------------

pub mod intif_save_impl {
    use crate::common::traits::LegacyEntity;
    use crate::game::block::map_is_loaded;
    use crate::game::pc::MapSessionData;
    use crate::game::player::entity::PlayerEntity;

    /// # Safety
    ///
    /// Caller must ensure all pointer arguments are valid and non-null.
    pub unsafe fn sl_intif_save(sd: *mut MapSessionData) -> i32 {
        if sd.is_null() {
            return -1;
        }

        // Sync runtime shadow fields into player before save.
        (*sd).player.identity.last_pos.m = (*sd).m;
        (*sd).player.identity.last_pos.x = (*sd).x;
        (*sd).player.identity.last_pos.y = (*sd).y;
        (*sd).player.appearance.disguise = (*sd).disguise;
        (*sd).player.appearance.disguise_color = (*sd).disguise_color;

        let player = (*sd).player.clone();
        crate::database::blocking_run_async(async move {
            let pool = crate::database::get_pool();
            if let Err(e) = crate::servers::char::db::save_player(pool, &player).await {
                tracing::error!(
                    "[map] [save] save_player failed for id={}: {}",
                    player.identity.id,
                    e
                );
            }
        });
        0
    }

    pub fn sl_intif_savequit(pe: &PlayerEntity) -> i32 {
        let sd = &mut *pe.write() as *mut MapSessionData;
        unsafe {
            if !map_is_loaded((*sd).player.identity.dest_pos.m as i32) {
                if (*sd).player.identity.dest_pos.m == 0 {
                    (*sd).player.identity.dest_pos.m = (*sd).m;
                    (*sd).player.identity.dest_pos.x = (*sd).x;
                    (*sd).player.identity.dest_pos.y = (*sd).y;
                }
                (*sd).player.identity.last_pos = (*sd).player.identity.dest_pos;
            } else {
                (*sd).player.identity.last_pos.m = (*sd).m;
                (*sd).player.identity.last_pos.x = (*sd).x;
                (*sd).player.identity.last_pos.y = (*sd).y;
            }

            (*sd).player.appearance.disguise = (*sd).disguise;
            (*sd).player.appearance.disguise_color = (*sd).disguise_color;

            let player = (*sd).player.clone();
            let char_id = player.identity.id;
            crate::database::blocking_run_async(async move {
                let pool = crate::database::get_pool();
                if let Err(e) = crate::servers::char::db::save_player(pool, &player).await {
                    tracing::error!("[map] [save] save_player failed for id={}: {}", char_id, e);
                }
                crate::servers::char::db::set_online(pool, char_id, false).await;
            });

            // Remove from online tracking
            if let Some(world) = crate::world::get_world() {
                world.online.remove(&char_id);
            }
        } // end unsafe
        0
    }
}
