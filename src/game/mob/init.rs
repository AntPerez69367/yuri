//! Mob spawn-table loading, respawn, and one-time spawn helpers.

#![allow(non_snake_case, dead_code)]

use std::sync::atomic::Ordering;

use super::entity::{
    mob_get_free_id, mob_get_new_id, sl_doscript_simple, MobSpawnData, MAX_NORMAL_ID, MOB_ID,
    MOB_SPAWN_MAX,
};
use super::spatial::mob_warp;
use super::systems::mob_respawn_getstats;
use crate::common::constants::entity::mob::{MOB_ALIVE, MOB_DEAD, MOB_START_NUM};
use crate::common::constants::entity::BL_MOB;
use crate::common::traits::LegacyEntity;
use crate::common::types::Point;
use crate::database::map_db::{get_map_ptr as ffi_get_map_ptr, map_is_loaded as ffi_map_is_loaded};
use crate::database::mob_db;
use crate::database::get_pool;
use crate::game::block::{map_addblock_id, map_delblock_id, map_moveblock_id, AreaType};
use crate::game::block_grid;
use crate::game::map_parse::visual::{
    clif_cmoblook, clif_mob_look_close_func_inner, clif_mob_look_start_func_inner,
    clif_object_look_mob,
};
use crate::game::map_server::{map_deliddb, map_id2mob_ref};
use crate::game::time_util::gettick;

// ---- Spawn table loader -----------------------------------------------------

async fn mobspawn_fetch(serverid_val: i32) -> Result<Vec<sqlx::mysql::MySqlRow>, sqlx::Error> {
    let pool = get_pool();
    let query = format!(
        "SELECT `SpnMapId`, `SpnX`, `SpnY`, `SpnMobId`, \
         `SpnLastDeath`, `SpnId`, `SpnStartTime`, `SpnEndTime`, \
         `SpnMobIdReplace` FROM `Spawns{}` ORDER BY `SpnId`",
        serverid_val
    );
    sqlx::query(&query).fetch_all(pool).await
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn mobspawn_read() -> i32 {
    let serverid_val = crate::config::config().server_id;
    let result = mobspawn_fetch(serverid_val).await;

    let rows = match result {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("[mob] spawn read error: {}", e);
            return 0;
        }
    };

    let mut mstr = 0i32;
    for row in &rows {
        use sqlx::Row;
        // All Spawns columns are int(10) unsigned -> read as u32, cast to dest type
        let startm: u16 = row.try_get::<u32, _>(0).unwrap_or(0) as u16;
        let startx: u16 = row.try_get::<u32, _>(1).unwrap_or(0) as u16;
        let starty: u16 = row.try_get::<u32, _>(2).unwrap_or(0) as u16;
        let mobid: u32 = row.try_get::<u32, _>(3).unwrap_or(0);
        let last_death: u32 = row.try_get::<u32, _>(4).unwrap_or(0);
        let spn_id: u32 = row.try_get::<u32, _>(5).unwrap_or(0);
        let start: i8 = row.try_get::<u32, _>(6).unwrap_or(25) as i8;
        let end: i8 = row.try_get::<u32, _>(7).unwrap_or(25) as i8;
        let replace: u32 = row.try_get::<u32, _>(8).unwrap_or(0);

        let (db, checkspawn, new_box_option) = match crate::game::map_server::map_id2mob_ref(spn_id)
        {
            Some(existing_arc) => {
                {
                    let existing = existing_arc.read();
                    map_delblock_id(existing.id, existing.m);
                }
                map_deliddb(spn_id);
                // After deliddb the Arc is removed from global map; create fresh box
                let mut new_mob_box: Box<MobSpawnData> = Box::new_zeroed().assume_init();
                let p: *mut MobSpawnData = new_mob_box.as_mut() as *mut MobSpawnData;
                (p, false, Some(new_mob_box))
            }
            None => {
                let mut new_mob_box: Box<MobSpawnData> = Box::new_zeroed().assume_init();
                let p: *mut MobSpawnData = new_mob_box.as_mut() as *mut MobSpawnData;
                (p, true, Some(new_mob_box))
            }
        };

        if (*db).exp == 0 {
            (*db).exp = mob_db::experience(mobid);
        }

        (*db).id = spn_id;
        (*db).bl_type = BL_MOB as u8;
        (*db).startm = startm;
        (*db).startx = startx;
        (*db).starty = starty;
        (*db).mobid = mobid;
        (*db).start = start;
        (*db).end = end;
        (*db).replace = replace;
        (*db).last_death = last_death;
        (*db).onetime = 0;

        if (*db).id < MOB_START_NUM {
            let new_id = mob_get_new_id();
            MAX_NORMAL_ID.store(new_id, Ordering::Relaxed);
            (*db).m = startm;
            (*db).x = startx;
            (*db).y = starty;
            (*db).id = new_id;
            mob_respawn_getstats(db);
        }

        if checkspawn {
            (*db).state = MOB_DEAD;
        }

        if ffi_map_is_loaded((*db).m) {
            let map_slot = ffi_get_map_ptr((*db).m);
            let xs = (*map_slot).xs;
            let ys = (*map_slot).ys;
            if (*db).x >= xs {
                (*db).x = xs - 1;
            }
            if (*db).y >= ys {
                (*db).y = ys - 1;
            }
        }

        // Insert into MOB_MAP first -- this moves the Box data into Arc<RwLock>,
        // freeing the Box. After this, `db` is dangling and must not be used.
        let mob_id = (*db).id;
        if let Some(b) = new_box_option {
            crate::game::map_server::map_addiddb_mob(mob_id, b);
        }
        // Read fields from the live Arc<MobEntity> (not the freed Box).
        {
            let mob_arc = map_id2mob_ref(mob_id).expect("mob just inserted");
            let guard = mob_arc.read();
            map_addblock_id(guard.id, guard.bl_type, guard.m, guard.x, guard.y);
        }
        mstr += 1;
    }

    MOB_SPAWN_MAX.store(MOB_ID.load(Ordering::Relaxed), Ordering::Relaxed);
    libc::srand(gettick());
    println!("[mob] [spawn] read done count={}", mstr);
    0
}

// Stubs -- no active callers
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mobspawn2_read() -> i32 {
    0
}
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mobspeech_read() -> i32 {
    0
}

// ---- Respawn functions ------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_respawn_nousers(mob: *mut MobSpawnData) -> i32 {
    if (*mob).m != (*mob).startm {
        mob_warp(
            mob,
            (*mob).startm as i32,
            (*mob).startx as i32,
            (*mob).starty as i32,
        );
    } else {
        let old_x = (*mob).x;
        let old_y = (*mob).y;
        map_moveblock_id(
            (*mob).id,
            (*mob).m,
            old_x,
            old_y,
            (*mob).startx,
            (*mob).starty,
        );
        (*mob).x = (*mob).startx;
        (*mob).y = (*mob).starty;
    }
    (*mob).state = MOB_ALIVE;
    mob_respawn_getstats(mob);
    sl_doscript_simple("on_spawn", None, (*mob).id);
    if !(*mob).data.is_null() {
        let yname = crate::game::scripting::carray_to_str(&(*(*mob).data).yname);
        sl_doscript_simple(yname, Some("on_spawn"), (*mob).id);
    }
    0
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_respawn(mob: *mut MobSpawnData) -> i32 {
    if (*mob).m != (*mob).startm {
        mob_warp(
            mob,
            (*mob).startm as i32,
            (*mob).startx as i32,
            (*mob).starty as i32,
        );
    } else {
        let old_x = (*mob).x;
        let old_y = (*mob).y;
        map_moveblock_id(
            (*mob).id,
            (*mob).m,
            old_x,
            old_y,
            (*mob).startx,
            (*mob).starty,
        );
        (*mob).x = (*mob).startx;
        (*mob).y = (*mob).starty;
    }
    (*mob).state = MOB_ALIVE;
    mob_respawn_getstats(mob);
    if !(*mob).data.is_null() {
        let d = &*(*mob).data;
        if let Some(grid) = block_grid::get_grid((*mob).m as usize) {
            let slot = &*ffi_get_map_ptr((*mob).m);
            let ids = block_grid::ids_in_area(
                grid,
                (*mob).x as i32,
                (*mob).y as i32,
                AreaType::Area,
                slot.xs as i32,
                slot.ys as i32,
            );
            if d.mobtype == 1 {
                for id in ids {
                    if let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                        clif_cmoblook(&*mob, &sd_arc);
                    }
                }
            } else {
                for id in &ids {
                    if let Some(pe) = crate::game::map_server::map_id2sd_pc(*id) {
                        clif_mob_look_start_func_inner(pe.fd, &mut pe.net.write().look);
                    }
                }
                for id in &ids {
                    if let Some(pe) = crate::game::map_server::map_id2sd_pc(*id) {
                        clif_object_look_mob(pe.fd, &mut pe.net.write().look, &*mob);
                    }
                }
                for id in &ids {
                    if let Some(pe) = crate::game::map_server::map_id2sd_pc(*id) {
                        clif_mob_look_close_func_inner(pe.fd, &mut pe.net.write().look);
                    }
                }
            }
        }
    }
    sl_doscript_simple("on_spawn", None, (*mob).id);
    if !(*mob).data.is_null() {
        let yname = crate::game::scripting::carray_to_str(&(*(*mob).data).yname);
        sl_doscript_simple(yname, Some("on_spawn"), (*mob).id);
    }
    0
}

// ---- One-time spawns --------------------------------------------------------

/// Spawn configuration for one-time mob spawns.
#[derive(Clone, Copy)]
pub struct SpawnConfig {
    pub times: i32,
    pub start: i32,
    pub end: i32,
    pub replace: u32,
    pub owner: u32,
}

/// Spawn `cfg.times` one-time instances of mob `id` at `pos`.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mobspawn_onetime(id: u32, pos: Point, cfg: SpawnConfig) -> Vec<u32> {
    let (m, x, y) = (pos.m, pos.x, pos.y);
    let SpawnConfig {
        times,
        start,
        end,
        replace,
        owner,
    } = cfg;
    const MAX_ONETIME_SPAWNS: i32 = 1024;
    if times <= 0 || times > MAX_ONETIME_SPAWNS {
        return Vec::new();
    }
    let mut spawnedmobs: Vec<u32> = Vec::with_capacity(times as usize);
    for _z in 0..times {
        let mut mob_box: Box<MobSpawnData> = Box::new_zeroed().assume_init();
        let db: *mut MobSpawnData = mob_box.as_mut() as *mut MobSpawnData;

        if (*db).exp == 0 {
            (*db).exp = mob_db::experience(id);
        }
        (*db).startm = m;
        (*db).startx = x;
        (*db).starty = y;
        (*db).mobid = id;
        (*db).start = start as i8;
        (*db).end = end as i8;
        (*db).replace = replace;
        (*db).state = MOB_DEAD;
        (*db).bl_type = BL_MOB as u8;
        (*db).m = m;
        (*db).x = x;
        (*db).y = y;
        (*db).owner = owner;
        (*db).onetime = 1;
        (*db).spawncheck = 0;

        let new_id = mob_get_free_id();
        if new_id == 0 {
            tracing::warn!("[mob] mobspawn_onetime: no free onetime ID, skipping spawn");
            // mob_box is dropped here automatically, no manual free needed
            continue;
        }
        (*db).id = new_id;

        spawnedmobs.push(new_id);
        // Insert into MOB_MAP first -- this moves the Box data into Arc<RwLock>,
        // freeing the Box. After this, `db` is dangling and must not be used.
        crate::game::map_server::map_addiddb_mob(new_id, mob_box);
        // Read fields from the live Arc<MobEntity>, then use raw ptr for
        // mob_respawn/mob_respawn_nousers which still take *mut MobSpawnData.
        let mob_arc = map_id2mob_ref(new_id).expect("mob just inserted");
        let (id, bl_type, m, x, y) = {
            let guard = mob_arc.read();
            (guard.id, guard.bl_type, guard.m, guard.x, guard.y)
        };
        map_addblock_id(id, bl_type, m, x, y);

        // SAFETY: single-threaded game loop, Arc keeps allocation alive.
        // mob_respawn/mob_respawn_nousers call Lua -- no guard held.
        let db = mob_arc.legacy.data_ptr();
        let has_users = ffi_map_is_loaded(m) && crate::game::block::map_user_count(m as usize) > 0;
        if has_users {
            mob_respawn(db);
        } else {
            mob_respawn_nousers(db);
        }
    }
    spawnedmobs
}
