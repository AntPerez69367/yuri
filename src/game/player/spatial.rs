#![allow(non_snake_case, dead_code, unused_variables)]

use std::sync::atomic::Ordering;
use crate::common::player::spells::MAX_SPELLS;
use crate::common::traits::{Spatial};
use crate::config::Point;
use crate::database::{self, map_db, magic_db};
use crate::game::block::AreaType;
use crate::game::block_grid;
use crate::game::client::handlers::{clif_quit, clif_transfer};
use crate::game::client::visual::broadcast_update_state;
use crate::game::map_parse::combat::{
    clif_send_duration, clif_sendanimation_inner,
};
use crate::game::map_parse::player_state::{
    clif_sendstatus, clif_refresh, clif_sendtime,
};
use crate::game::map_parse::visual::clif_spawn;
use crate::game::map_server;
use crate::game::mob::{
    MAX_MAGIC_TIMERS, MAX_THREATCOUNT,
    MOB_START_NUM, MOB_SPAWN_START, MOB_SPAWN_MAX,
    MOB_ONETIME_START, MOB_ONETIME_MAX,
};
use crate::game::player::PlayerEntity;
use crate::game::scripting;
use crate::session::session_exists;
use crate::common::constants::entity::BL_PC;
use crate::common::constants::entity::player::{PC_ALIVE, SFLAG_HPMP};
use crate::common::constants::world::MAX_MAP_PER_SERVER;
use super::types::MapSessionData;
use super::systems::{pc_calcstat, sl_doscript_simple_pc, sl_doscript_2_pc};

// ─── Position / warp / magic / state / combat functions ───────────────────────
//


impl Spatial for PlayerEntity {
    #[inline]
    fn id(&self) -> u32 {
        self.id
    }

    #[inline]
    fn position(&self) -> Point {
        Point::from_u64(self.pos_atomic.load(Ordering::Relaxed))
    }

    #[inline]
    fn set_position(&self, p: Point) {
        self.pos_atomic.store(p.to_u64(), Ordering::Relaxed);
    }

    #[inline]
    fn map_id(&self) -> u16 {
        self.position().m
    }
}

/// Sets the player's block-list
/// position without sending any client packets.
///
/// Guards against attempting to set position on a mob object (bl.id >= MOB_START_NUM).
/// Sets bl.m, bl.x, bl.y, and bl.type.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_setpos(
    sd: *mut MapSessionData,
    m: i32,
    x: i32,
    y: i32,
) -> i32 {

    if (*sd).id >= MOB_START_NUM { return 0; }
    (*sd).m  = m as u16;
    (*sd).x  = x as u16;
    (*sd).y  = y as u16;
    (*sd).bl_type = BL_PC as u8;
    if let Some(pc_arc) = map_server::map_id2sd_pc((*sd).id) {
        pc_arc.set_position(Point { m: m as u16, x: x as u16, y: y as u16 });
    }
    0
}

/// Full warp sequence.
///
/// If the target map is not loaded on this server, queries the `Maps` table for
/// the destination map server and calls `clif_transfer`. Otherwise, fires
/// pre-warp Lua hooks, calls `clif_quit` / `pc_setpos` / `clif_spawn` /
/// `clif_refresh`, then fires post-warp Lua hooks.
async fn lookup_map_server(map_id: i32) -> Option<u32> {
    sqlx::query_scalar::<_, Option<u32>>(
        "SELECT `MapServer` FROM `Maps` WHERE `MapId` = ?"
    )
    .bind(map_id)
    .fetch_optional(database::get_pool())
    .await
    .ok()
    .flatten()
    .flatten()
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn pc_warp(
    sd: *mut MapSessionData,
    mut m: i32,
    mut x: i32,
    mut y: i32,
) -> i32 {

    if sd.is_null() { return 0; }

    let oldmap = (*sd).m as i32;

    if m < 0 { m = 0; }
    if m >= MAX_MAP_PER_SERVER { m = MAX_MAP_PER_SERVER - 1; }

    // If the target map is not loaded on this server, hand off to the right server.
    if !map_db::map_is_loaded(m as u16) {
        if !session_exists((*sd).fd) {
            return 0;
        }

        let destsrv = lookup_map_server(m).await;

        let destsrv = match destsrv {
            Some(srv) => srv as i32,
            None => return 0,
        };

        if !(0..=255).contains(&x) { x = 1; }
        if !(0..=255).contains(&y) { y = 1; }

        (*sd).player.identity.dest_pos.m = m as u16;
        (*sd).player.identity.dest_pos.x = x as u16;
        (*sd).player.identity.dest_pos.y = y as u16;

        if let Some(pe) = map_server::map_id2sd_pc((*sd).id) {
            clif_transfer(&pe, destsrv, m, x, y);
        }
        return 0;
    }

    // Map is loaded locally — clamp coordinates to map bounds.
    let map_ptr = map_db::get_map_ptr(m as u16);
    if map_ptr.is_null() { return 0; }
    let xs = (*map_ptr).xs as i32;
    let ys = (*map_ptr).ys as i32;
    let can_mount = (*map_ptr).can_mount;

    if x == -1 {
        x = (xs / 2) + if xs % 2 != 0 { 1 } else { 0 };
        y = (ys / 2) + if ys % 2 != 0 { 1 } else { 0 };
    }

    if x < 0 { x = 0; }
    if y < 0 { y = 0; }
    if x >= xs { x = xs - 1; }
    if y >= ys { y = ys - 1; }

    // Fire map-leave hooks when changing maps.
    if m != oldmap {
        sl_doscript_simple_pc("mapLeave", None, (*sd).id);
        if can_mount == 0 {
            sl_doscript_simple_pc("onDismount", None, (*sd).id);
        }
    }

    // Fire passive_before_warp for each known spell.
    for i in 0..MAX_SPELLS {
        sl_doscript_simple_pc(scripting::carray_to_str(&magic_db::search((&(*sd).player.spells.skills)[i] as i32).yname), Some("passive_before_warp"), (*sd).id);
    }

    // Fire before_warp_while_cast for each active aether timer.
    for i in 0..MAX_MAGIC_TIMERS {
        sl_doscript_simple_pc(scripting::carray_to_str(&magic_db::search((&(*sd).player.spells.dura_aether)[i].id as i32).yname), Some("before_warp_while_cast"), (*sd).id);
    }

    // Perform the actual move.
    if let Some(pe) = map_server::map_id2sd_pc((*sd).id) { clif_quit(&pe); }
    pc_setpos(sd, m, x, y);
    if let Some(pe) = map_server::map_id2sd_pc((*sd).id) { clif_sendtime(&pe); }
    clif_spawn(sd);
    if let Some(pe) = map_server::map_id2sd_pc((*sd).id) { clif_refresh(&pe); }

    // Fire map-enter hooks when changing maps.
    if m != oldmap {
        sl_doscript_simple_pc("mapEnter", None, (*sd).id);
    }

    // Fire passive_on_warp for each known spell.
    for i in 0..MAX_SPELLS {
        sl_doscript_simple_pc(scripting::carray_to_str(&magic_db::search((&(*sd).player.spells.skills)[i] as i32).yname), Some("passive_on_warp"), (*sd).id);
    }

    // Fire on_warp_while_cast for each active aether timer.
    for i in 0..MAX_MAGIC_TIMERS {
        sl_doscript_simple_pc(scripting::carray_to_str(&magic_db::search((&(*sd).player.spells.dura_aether)[i].id as i32).yname), Some("on_warp_while_cast"), (*sd).id);
    }

    0
}

/// Fires the `onDeathPlayer` Lua hook.
///
/// The actual stat/state changes are handled by `pc_diescript`; this function
/// just fires the hook so scripts can respond immediately.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_die(sd: *mut MapSessionData) -> i32 {
    sl_doscript_simple_pc("onDeathPlayer", None, (*sd).id);
    0
}

/// Full death processing.
///
/// - Clears `deathflag`, sets state to dead, zeroes HP.
/// - Clears all non-dispel-immune aether timers and fires their `uncast` hooks.
/// - Removes the dead player from all mob threat tables.
/// - Resets combat state (enchanted, flank, backstab, dmgshield).
/// - Recalculates stats and broadcasts updated state.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_diescript(sd: *mut MapSessionData) -> i32 {


    if sd.is_null() { return 0; }

    let attacker_id = (*sd).attacker;

    (*sd).deathflag = 0;

    // Set the killer if the attacker entity still exists.
    if attacker_id > 0 && map_server::entity_position(attacker_id).is_some() {
        (*sd).player.social.killed_by = attacker_id;
    }
    (*sd).player.combat.state = 1; // PC_DIE
    (*sd).player.combat.hp    = 0;

    // Clear active aether timers that are not dispel-immune.
    for i in 0..MAX_MAGIC_TIMERS {
        let id = (&(*sd).player.spells.dura_aether)[i].id;
        if id == 0 { continue; }

        if magic_db::search(id as i32).dispell as i32 > 0 { continue; }

        (&mut (*sd).player.spells.dura_aether)[i].duration = 0;
        {
            let caster_id = (&(*sd).player.spells.dura_aether)[i].caster_id;
            let caster_pe = if caster_id > 0 {
                map_server::map_id2sd_pc(caster_id)
            } else {
                None
            };
            if let Some(ref cpe) = caster_pe {
                let mut caster_sd = cpe.write();
                clif_send_duration(
                    &mut *sd,
                    (&(*sd).player.spells.dura_aether)[i].id as i32,
                    0,
                    &mut *caster_sd as *mut MapSessionData,
                );
            } else {
                clif_send_duration(
                    &mut *sd,
                    (&(*sd).player.spells.dura_aether)[i].id as i32,
                    0,
                    std::ptr::null_mut(),
                );
            }
        }
        (&mut (*sd).player.spells.dura_aether)[i].caster_id = 0;

        {
            let anim = (&(*sd).player.spells.dura_aether)[i].animation as i32;
            if let Some(grid) = block_grid::get_grid((*sd).m as usize) {
                let slot = &*map_db::get_map_ptr((*sd).m);
                let ids = block_grid::ids_in_area(grid, (*sd).x as i32, (*sd).y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
                for id in ids {
                    if let Some(tsd_arc) = map_server::map_id2sd_pc(id) {
                        let tsd_guard = tsd_arc.read();
                        clif_sendanimation_inner(tsd_guard.fd, tsd_guard.player.appearance.setting_flags, anim, (*sd).id, -1);
                    }
                }
            }
        }
        (&mut (*sd).player.spells.dura_aether)[i].animation = 0;

        if (&(*sd).player.spells.dura_aether)[i].aether == 0 {
            (&mut (*sd).player.spells.dura_aether)[i].id = 0;
        }

        // Fire uncast hook.
        let caster_id = (&(*sd).player.spells.dura_aether)[i].caster_id;
        if caster_id != (*sd).id && caster_id > 0 && map_server::entity_position(caster_id).is_some() {
            sl_doscript_2_pc(scripting::carray_to_str(&magic_db::search(id as i32).yname), Some("uncast"), (*sd).id, caster_id);
        } else {
            sl_doscript_simple_pc(scripting::carray_to_str(&magic_db::search(id as i32).yname), Some("uncast"), (*sd).id);
        }
    }

    // Remove dead player from all spawn-mob threat tables.
    let spawn_start = MOB_SPAWN_START.load(Ordering::Relaxed);
    let spawn_max   = MOB_SPAWN_MAX.load(Ordering::Relaxed);
    if spawn_start != spawn_max {
        let mut x = spawn_start;
        while x < spawn_max {
            if let Some(tmob_arc) = map_server::map_id2mob_ref(x) {
                let mut tmob = tmob_arc.write();
                for i in 0..MAX_THREATCOUNT {
                    if tmob.threat[i].user == (*sd).id {
                        tmob.threat[i].user   = 0;
                        tmob.threat[i].amount = 0;
                    }
                }
            }
            x += 1;
        }
    }

    // Remove dead player from all one-time mob threat tables.
    let onetime_start = MOB_ONETIME_START.load(Ordering::Relaxed);
    let onetime_max   = MOB_ONETIME_MAX.load(Ordering::Relaxed);
    if onetime_start != onetime_max {
        let mut x = onetime_start;
        while x < onetime_max {
            if let Some(tmob_arc) = map_server::map_id2mob_ref(x) {
                let mut tmob = tmob_arc.write();
                for i in 0..MAX_THREATCOUNT {
                    if tmob.threat[i].user == (*sd).id {
                        tmob.threat[i].user   = 0;
                        tmob.threat[i].amount = 0;
                    }
                }
            }
            x += 1;
        }
    }

    // Reset combat modifiers.
    (*sd).enchanted  = 1.0_f32;
    (*sd).flank      = 0;
    (*sd).backstab   = 0;
    (*sd).dmgshield  = 0.0_f32;

    if let Some(pe) = map_server::map_id2sd_pc((*sd).id) {
        pc_calcstat(&pe);
        broadcast_update_state(&pe);
    }

    0
}

/// Sync bridge for Lua/FFI callers that cannot `.await`.
/// SAFETY: MapSessionData: Send; blocking_run_async joins before returning.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_warp_sync(sd: *mut MapSessionData, m: i32, x: i32, y: i32) -> i32 {
    let sd_usize = sd as usize;
    database::blocking_run_async(database::assert_send(async move {
        let sd = sd_usize as *mut MapSessionData;
        pc_warp(sd, m, x, y).await
    }))
}

/// Resurrects the player in-place.
///
/// Sets state to alive, restores 100 HP, sends an HP/MP status update, and
/// warps the player to their current position (which re-spawns them for other
/// clients on the same map).
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_res(sd: *mut MapSessionData) -> i32 {
    (*sd).player.combat.state = PC_ALIVE as i8;
    (*sd).player.combat.hp    = 100;
    if let Some(pe) = map_server::map_id2sd_pc((*sd).id) { clif_sendstatus(&pe, SFLAG_HPMP); }
    pc_warp_sync(sd, (*sd).m as i32, (*sd).x as i32, (*sd).y as i32);
    0
}

// ─── Kill-registry helpers ────────────────────────────────────────────────────

/// Increment the kill-count for `mob` in `sd`'s kill registry, or add a new
/// entry if the mob is not yet present.
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
pub unsafe fn addtokillreg(sd: *mut MapSessionData, mob: i32) -> i32 {
    if sd.is_null() { return 0; }
    (*sd).player.registries.add_kill(mob as u32);
    0
}
