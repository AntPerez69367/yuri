//! NPC spatial operations — warping and movement.

#![allow(non_snake_case)]

use super::entity::NpcData;
use crate::common::traits::LegacyEntity;
use crate::common::constants::entity::npc::NPC_START_NUM;
use crate::common::constants::entity::BL_NPC;
use crate::database::map_db::{get_map_ptr, map_is_loaded};
use crate::game::block::{map_addblock_id, map_delblock_id, map_moveblock_id};
use crate::game::block::AreaType;
use crate::game::block_grid;
use crate::game::map_parse::movement::{clif_object_canmove, clif_object_canmove_from};
use crate::game::map_parse::movement::clif_npc_move_inner;
use crate::game::map_parse::visual::clif_lookgone_by_id;
use crate::game::map_parse::visual::{
    clif_cnpclook, clif_mob_look_close_func_inner, clif_mob_look_start_func_inner,
    clif_object_look2_npc, clif_object_look_npc,
};
use crate::game::map_server::map_canmove;

/// Teleports an NPC to map `m` at coordinates `(x, y)`.
///
/// Removes the NPC from its current grid cell, signals surrounding players
/// that it has gone, updates the NPC's position fields, re-inserts it into
/// the new cell, then broadcasts its new appearance to nearby players.
///
///
/// # Safety
///
/// `nd` must be a valid, aligned, non-null pointer to an `NpcData` that is
/// currently registered in the map block-grid.  Caller must hold the
/// server-wide lock.
pub unsafe fn npc_warp(nd: *mut NpcData, m: i32, x: i32, y: i32) -> i32 {
    if nd.is_null() {
        return 0;
    }
    let nd = &mut *nd;
    if nd.id < NPC_START_NUM {
        return 0;
    }

    map_delblock_id(nd.id, nd.m);
    clif_lookgone_by_id(nd.id);
    nd.m = m as u16;
    nd.x = x as u16;
    nd.y = y as u16;
    nd.bl_type = BL_NPC as u8;

    if map_addblock_id(nd.id, nd.bl_type, nd.m, nd.x, nd.y) != 0 {
        tracing::error!("Error warping npcchar.");
    }

    if let Some(grid) = block_grid::get_grid(m as usize) {
        let slot = &*crate::database::map_db::raw_map_ptr().add(m as usize);
        let ids =
            block_grid::ids_in_area(grid, x, y, AreaType::Area, slot.xs as i32, slot.ys as i32);
        if nd.npctype == 1 {
            for id in ids {
                if let Some(arc) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_cnpclook(&*nd, &arc);
                }
            }
        } else {
            for id in ids {
                if let Some(pe) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_object_look2_npc(pe.fd, nd);
                }
            }
        }
    }
    0
}

// ---------------------------------------------------------------------------
// npc_move_sub — callback for map_foreachincell during NPC movement
// ---------------------------------------------------------------------------

/// Callback for `map_foreachincell` during NPC movement — checks for blocking entities.
///
/// Sets `nd.canmove = 1` if the cell is occupied by a non-passable entity.
///
/// - `BL_NPC`: skip if `bl.subtype != 0` (non-zero subtype NPCs don't block).
/// - `BL_MOB`: skip if the mob is in `MOB_DEAD` state.
/// - `BL_PC`:  skip if dead/invisible/GM.
///
/// # Safety
///
/// Must only be called by `map_foreachincell`. `ap` must contain exactly one
/// argument: a `*mut NpcData`.
pub unsafe fn npc_move_sub_id(entity_id: u32, nd: *mut NpcData) -> i32 {
    if nd.is_null() {
        return 0;
    }
    if (*nd).canmove == 1 {
        return 0;
    }

    if let Some(arc) = crate::game::map_server::map_id2npc_ref(entity_id) {
        if arc.read().subtype != 0 {
            return 0;
        }
    } else if let Some(arc) = crate::game::map_server::map_id2mob_ref(entity_id) {
        if arc.read().state == crate::game::mob::MOB_DEAD {
            return 0;
        }
    } else if let Some(arc) = crate::game::map_server::map_id2sd_pc(entity_id) {
        let sd = arc.read();
        let npc_m = (*nd).m;
        let show_ghosts: u8 = if map_is_loaded(npc_m) {
            (*get_map_ptr(npc_m)).show_ghosts
        } else {
            0
        };
        let state = sd.player.combat.state;
        if (show_ghosts != 0 && state == crate::game::pc::PC_DIE as i8)
            || state == -1
            || sd.player.identity.gm_level >= 50
        {
            return 0;
        }
    } else {
        return 0;
    }

    (*nd).canmove = 1;
    0
}

// ---------------------------------------------------------------------------
// npc_move — move an NPC one step in its facing direction
// ---------------------------------------------------------------------------

/// Moves an NPC one step in its facing direction.
///
/// Computes the new candidate cell based on `nd.side` (direction 0-3 = up/right/down/left),
/// checks for warps and blocking entities, then calls `map_moveblock` if the move is valid.
/// Broadcasts visibility updates to nearby players.
///
///
/// # Safety
///
/// `nd` must be a valid, aligned, non-null pointer to a live `NpcData`.
/// Caller must hold the server-wide lock.
pub unsafe fn npc_move(nd: *mut NpcData) -> i32 {
    if nd.is_null() {
        return 0;
    }
    let nd = &mut *nd;

    let m = nd.m as i32;
    let backx = nd.x as i32;
    let backy = nd.y as i32;
    let mut dx = backx;
    let mut dy = backy;
    let direction = nd.side as i32;
    let mut x0 = backx;
    let mut y0 = backy;
    let mut x1: i32 = 0;
    let mut y1: i32 = 0;
    let mut nothingnew: i32 = 0;

    let md = crate::database::map_db::get_map_ptr(nd.m);
    if md.is_null() {
        return 0;
    }
    let map_xs = (*md).xs as i32;
    let map_ys = (*md).ys as i32;

    match direction {
        0 => {
            // UP
            if backy > 0 {
                dy = backy - 1;
                x0 -= 9;
                if x0 < 0 {
                    x0 = 0;
                }
                y0 -= 9;
                y1 = 1;
                x1 = 19;
                if y0 < 7 {
                    nothingnew = 1;
                }
                if y0 == 7 {
                    y1 += 7;
                    y0 = 0;
                }
                if x0 + 19 + 9 >= map_xs {
                    x1 += 9 - (x0 + 19 + 9 - map_xs);
                }
                if x0 <= 8 {
                    x1 += x0;
                    x0 = 0;
                }
            }
        }
        1 => {
            // RIGHT
            if backx < map_xs {
                x0 += 10;
                y0 -= 8;
                if y0 < 0 {
                    y0 = 0;
                }
                dx = backx + 1;
                y1 = 17;
                x1 = 1;
                if x0 > map_xs - 9 {
                    nothingnew = 1;
                }
                if x0 == map_xs - 9 {
                    x1 += 9;
                }
                if y0 + 17 + 8 >= map_ys {
                    y1 += 8 - (y0 + 17 + 8 - map_ys);
                }
                if y0 <= 7 {
                    y1 += y0;
                    y0 = 0;
                }
            }
        }
        2 => {
            // DOWN
            if backy < map_ys {
                x0 -= 9;
                if x0 < 0 {
                    x0 = 0;
                }
                y0 += 9;
                dy = backy + 1;
                y1 = 1;
                x1 = 19;
                if y0 + 8 > map_ys {
                    nothingnew = 1;
                }
                if y0 + 8 == map_ys {
                    y1 += 8;
                }
                if x0 + 19 + 9 >= map_xs {
                    x1 += 9 - (x0 + 19 + 9 - map_xs);
                }
                if x0 <= 8 {
                    x1 += x0;
                    x0 = 0;
                }
            }
        }
        3 => {
            // LEFT
            if backx > 0 {
                x0 -= 10;
                y0 -= 8;
                if y0 < 0 {
                    y0 = 0;
                }
                y1 = 17;
                x1 = 1;
                dx = backx - 1;
                if x0 < 8 {
                    nothingnew = 1;
                }
                if x0 == 8 {
                    x0 = 0;
                    x1 += 8;
                }
                if y0 + 17 + 8 >= map_ys {
                    y1 += 8 - (y0 + 17 + 8 - map_ys);
                }
                if y0 <= 7 {
                    y1 += y0;
                    y0 = 0;
                }
            }
        }
        _ => {
            return 0;
        }
    }

    if dx >= map_xs {
        dx = map_xs - 1;
    }
    if dy >= map_ys {
        dy = map_ys - 1;
    }

    // Check warp at destination block
    let mut war = crate::database::map_db::map_get_warp(nd.m, dx as u16, dy as u16);
    while !war.is_null() {
        if (*war).x == dx && (*war).y == dy {
            return 0;
        }
        war = (*war).next;
    }

    // Check for blockers in destination cell
    nd.canmove = 0;
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let cell_ids = grid.ids_at_tile(dx as u16, dy as u16);
        for id in cell_ids {
            npc_move_sub_id(id, nd as *mut NpcData);
        }
    }

    if clif_object_canmove(m, dx, dy, direction) != 0 {
        nd.canmove = 0;
        return 0;
    }
    if clif_object_canmove_from(m, backx, backy, direction) != 0 {
        nd.canmove = 0;
        return 0;
    }
    if map_canmove(m, dx, dy) == 1 || nd.canmove == 1 {
        nd.canmove = 0;
        return 0;
    }

    if x0 > map_xs - 1 {
        x0 = map_xs - 1;
    }
    if y0 > map_ys - 1 {
        y0 = map_ys - 1;
    }
    if x0 < 0 {
        x0 = 0;
    }
    if y0 < 0 {
        y0 = 0;
    }
    if dx >= map_xs {
        dx = backx;
    }
    if dy >= map_ys {
        dy = backy;
    }
    if dx < 0 {
        dx = backx;
    }
    if dy < 0 {
        dy = backy;
    }

    if dx != backx || dy != backy {
        nd.prev_x = backx as u16;
        nd.prev_y = backy as u16;
        let old_x = nd.x;
        let old_y = nd.y;
        map_moveblock_id(nd.id, nd.m, old_x, old_y, dx as u16, dy as u16);
        nd.x = dx as u16;
        nd.y = dy as u16;

        if nothingnew == 0 {
            let nm = nd.m as i32;
            if let Some(grid) = block_grid::get_grid(nm as usize) {
                let rect_ids = grid.ids_in_rect(x0, y0, x0 + x1 - 1, y0 + y1 - 1);
                if nd.npctype == 1 {
                    for &id in &rect_ids {
                        if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                            clif_cnpclook(&*nd, &pc_arc);
                        }
                    }
                } else {
                    for &id in &rect_ids {
                        if let Some(pe) = crate::game::map_server::map_id2sd_pc(id) {
                            clif_mob_look_start_func_inner(pe.fd, &mut pe.net.write().look);
                        }
                    }
                    for &id in &rect_ids {
                        if let Some(pe) = crate::game::map_server::map_id2sd_pc(id) {
                            clif_object_look_npc(pe.fd, &mut pe.net.write().look, nd);
                        }
                    }
                    for &id in &rect_ids {
                        if let Some(pe) = crate::game::map_server::map_id2sd_pc(id) {
                            clif_mob_look_close_func_inner(pe.fd, &mut pe.net.write().look);
                        }
                    }
                }
            }
        }

        let nd_ptr = nd as *mut NpcData;
        if let Some(grid) = block_grid::get_grid(m as usize) {
            let slot = &*crate::database::map_db::raw_map_ptr().add(m as usize);
            let ids = block_grid::ids_in_area(
                grid,
                nd.x as i32,
                nd.y as i32,
                AreaType::Area,
                slot.xs as i32,
                slot.ys as i32,
            );
            for id in ids {
                if crate::game::map_server::map_id2sd_pc(id).is_some() {
                    clif_npc_move_inner(nd_ptr);
                }
            }
        }
        return 1;
    }
    0
}
