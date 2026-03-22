//! Mob spatial helpers: warping, movement, collision checks, viewport broadcast.

#![allow(non_snake_case, dead_code)]

use super::entity::MobSpawnData;
use super::systems::mob_trap_look_inner;
use crate::common::constants::entity::item::FLOORITEM_START_NUM;
use crate::common::constants::entity::mob::{MOB_DEAD, MOB_START_NUM};
use crate::common::constants::entity::npc::NPC_START_NUM;
use crate::common::constants::entity::BL_MOB;
use crate::common::traits::LegacyEntity;
use crate::database::map_db::BLOCK_SIZE;
use crate::database::map_db::WarpList;
use crate::database::map_db::{get_map_ptr as ffi_get_map_ptr, map_is_loaded as ffi_map_is_loaded};
use crate::game::block::{map_addblock_id, map_delblock_id, map_moveblock_id, AreaType};
use crate::game::block_grid;
use crate::game::client::visual::clif_sendmob_side;
use crate::game::map_parse::movement::{
    clif_mob_move_inner, clif_object_canmove, clif_object_canmove_from,
};
use crate::game::map_parse::visual::{
    clif_cmoblook, clif_mob_look_close_func_inner, clif_mob_look_start_func_inner,
    clif_lookgone_by_id, clif_object_look2_mob, clif_object_look_mob,
};
use crate::game::map_server::{map_canmove, map_id2mob_ref};

// ─── Warp ────────────────────────────────────────────────────────────────────

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_warp(mob: *mut MobSpawnData, m: i32, x: i32, y: i32) -> i32 {
    if mob.is_null() {
        return 0;
    }
    if ((*mob).id) < MOB_START_NUM || ((*mob).id) >= NPC_START_NUM {
        return 0;
    }
    map_delblock_id((*mob).id, (*mob).m);
    clif_lookgone_by_id((*mob).id);
    (*mob).m = m as u16;
    (*mob).x = x as u16;
    (*mob).y = y as u16;
    (*mob).bl_type = BL_MOB as u8;
    if map_addblock_id((*mob).id, (*mob).bl_type, (*mob).m, (*mob).x, (*mob).y) != 0 {
        tracing::warn!("Error warping mob.");
    }
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
        if !(*mob).data.is_null() && (*(*mob).data).mobtype == 1 {
            for id in ids {
                if let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_cmoblook(&*mob, &sd_arc);
                }
            }
        } else {
            for id in ids {
                if let Some(pe) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_object_look2_mob(pe.fd, &*mob);
                }
            }
        }
    }
    0
}

// ─── Private movement helpers ────────────────────────────────────────────────

/// Shared warp-tile check used by all three move_mob variants.
unsafe fn warp_at(slot: *mut crate::database::map_db::MapData, dx: i32, dy: i32) -> bool {
    let bxs = (*slot).bxs as usize;
    let xs = (*slot).xs as usize;
    let ys = (*slot).ys as usize;
    let dx = dx as usize;
    let dy = dy as usize;
    if dx >= xs || dy >= ys {
        return false;
    }
    let idx = dx / BLOCK_SIZE + (dy / BLOCK_SIZE) * bxs;
    let warp: *mut WarpList = *(*slot).warp.add(idx);
    let mut w = warp;
    while !w.is_null() {
        if (*w).x == dx as i32 && (*w).y == dy as i32 {
            return true;
        }
        w = (*w).next;
    }
    false
}

/// Compute viewport delta strip for a one-step move in `direction`.
/// Returns `(x0, y0, x1, y1, dx, dy, nothingnew)`.
unsafe fn viewport_delta(
    mob: *const MobSpawnData,
    slot: *mut crate::database::map_db::MapData,
) -> (i32, i32, i32, i32, i32, i32, bool) {
    let backx = (*mob).x as i32;
    let backy = (*mob).y as i32;
    let xs = (*slot).xs as i32;
    let ys = (*slot).ys as i32;
    let (mut x0, mut y0) = (backx, backy);
    let (mut x1, mut y1) = (0, 0);
    let mut dx = backx;
    let mut dy = backy;
    let mut nothingnew = false;

    match (*mob).side {
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
                    nothingnew = true;
                }
                if y0 == 7 {
                    y1 += 7;
                    y0 = 0;
                }
                if x0 + 19 + 9 >= xs {
                    x1 += 9 - ((x0 + 19 + 9) - xs);
                }
                if x0 <= 8 {
                    x1 += x0;
                    x0 = 0;
                }
            }
        }
        1 => {
            // Right
            if backx < xs {
                x0 += 10;
                y0 -= 8;
                if y0 < 0 {
                    y0 = 0;
                }
                dx = backx + 1;
                y1 = 17;
                x1 = 1;
                if x0 > xs - 9 {
                    nothingnew = true;
                }
                if x0 == xs - 9 {
                    x1 += 9;
                }
                if y0 + 17 + 8 >= ys {
                    y1 += 8 - ((y0 + 17 + 8) - ys);
                }
                if y0 <= 7 {
                    y1 += y0;
                    y0 = 0;
                }
            }
        }
        2 => {
            // Down
            if backy < ys {
                x0 -= 9;
                if x0 < 0 {
                    x0 = 0;
                }
                y0 += 9;
                dy = backy + 1;
                y1 = 1;
                x1 = 19;
                if y0 + 8 > ys {
                    nothingnew = true;
                }
                if y0 + 8 == ys {
                    y1 += 8;
                }
                if x0 + 19 + 9 >= xs {
                    x1 += 9 - ((x0 + 19 + 9) - xs);
                }
                if x0 <= 8 {
                    x1 += x0;
                    x0 = 0;
                }
            }
        }
        3 => {
            // Left
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
                    nothingnew = true;
                }
                if x0 == 8 {
                    x0 = 0;
                    x1 += 8;
                }
                if y0 + 17 + 8 >= ys {
                    y1 += 8 - ((y0 + 17 + 8) - ys);
                }
                if y0 <= 7 {
                    y1 += y0;
                    y0 = 0;
                }
            }
        }
        _ => {}
    }
    (x0, y0, x1, y1, dx, dy, nothingnew)
}

/// Shared post-move broadcast used by move_mob variants.
unsafe fn broadcast_move(
    mob: *mut MobSpawnData,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    nothingnew: bool,
) {
    let m = (*mob).m as usize;
    let mut subt = [0i32; 1];
    if let Some(grid) = block_grid::get_grid(m) {
        if !nothingnew {
            let rect_ids = grid.ids_in_rect(x0, y0, x0 + x1 - 1, y0 + y1 - 1);
            if !(*mob).data.is_null() && (*(*mob).data).mobtype == 1 {
                for id in &rect_ids {
                    if let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(*id) {
                        clif_cmoblook(&*mob, &sd_arc);
                    }
                }
            } else {
                for id in &rect_ids {
                    if let Some(pe) = crate::game::map_server::map_id2sd_pc(*id) {
                        clif_mob_look_start_func_inner(pe.fd, &mut pe.net.write().look);
                    }
                }
                for id in &rect_ids {
                    if let Some(pe) = crate::game::map_server::map_id2sd_pc(*id) {
                        clif_object_look_mob(pe.fd, &mut pe.net.write().look, &*mob);
                    }
                }
                for id in &rect_ids {
                    if let Some(pe) = crate::game::map_server::map_id2sd_pc(*id) {
                        clif_mob_look_close_func_inner(pe.fd, &mut pe.net.write().look);
                    }
                }
            }
        }
        // NPC trap check at mob's current cell
        {
            let cell_ids = grid.ids_at_tile((*mob).x, (*mob).y);
            let def_ptr = subt.as_mut_ptr();
            for id in cell_ids {
                if let Some(npc_arc) = crate::game::map_server::map_id2npc_ref(id) {
                    mob_trap_look_inner(
                        &mut *npc_arc.write() as *mut crate::game::npc::NpcData,
                        mob,
                        0,
                        def_ptr,
                    );
                }
            }
        }
        // Send mob move to nearby PCs
        {
            let slot = &*ffi_get_map_ptr((*mob).m);
            let ids = block_grid::ids_in_area(
                grid,
                (*mob).x as i32,
                (*mob).y as i32,
                AreaType::Area,
                slot.xs as i32,
                slot.ys as i32,
            );
            for id in ids {
                if let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_mob_move_inner(&sd_arc, mob);
                }
            }
        }
    }
}

unsafe fn check_mob_collision(moving_mob: *mut MobSpawnData, m: i32, x: i32, y: i32) {
    if (*moving_mob).canmove == 1 {
        return;
    }
    if x < 0 || y < 0 {
        return;
    }
    let self_id = (*moving_mob).id;
    if let Some(grid) = crate::game::block_grid::get_grid(m as usize) {
        for id in grid.ids_at_tile(x as u16, y as u16) {
            if id == self_id {
                continue;
            }
            if let Some(mob_arc) = crate::game::map_server::map_id2mob_ref(id) {
                let mob = mob_arc.read();
                if mob.x as i32 == x && mob.y as i32 == y && mob.state != MOB_DEAD {
                    (*moving_mob).canmove = 1;
                    return;
                }
            }
        }
    }
}

/// PC-collision check -- sets `moving_mob.canmove = 1` if a physical, non-GM, non-dead player occupies `(x, y)`.
unsafe fn check_pc_collision(moving_mob: *mut MobSpawnData, m: i32, x: i32, y: i32) {
    use crate::game::pc::PC_DIE;
    if (*moving_mob).canmove == 1 {
        return;
    }
    if x < 0 || y < 0 {
        return;
    }
    let slot = ffi_get_map_ptr(m as u16);
    if slot.is_null() {
        return;
    }
    let show_ghosts = (*slot).show_ghosts;
    if let Some(grid) = crate::game::block_grid::get_grid(m as usize) {
        for id in grid.ids_at_tile(x as u16, y as u16) {
            if let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                let pos = sd_arc.position();
                if pos.x as i32 == x && pos.y as i32 == y {
                    let sd = sd_arc.read();
                    let state = sd.player.combat.state;
                    let gm_lvl = sd.player.identity.gm_level;
                    let passable =
                        (show_ghosts != 0 && state == PC_DIE as i8) || state == -1 || gm_lvl >= 50;
                    if !passable {
                        (*moving_mob).canmove = 1;
                        return;
                    }
                }
            }
        }
    }
}

// ─── Public movement functions ───────────────────────────────────────────────

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn move_mob(mob: *mut MobSpawnData) -> i32 {
    let m = (*mob).m as i32;
    let backx = (*mob).x as i32;
    let backy = (*mob).y as i32;
    let slot = ffi_get_map_ptr((*mob).m);
    if slot.is_null() {
        return 0;
    }

    let (x0, y0, x1, y1, mut dx, mut dy, nothingnew) = viewport_delta(mob, slot);

    let xs = (*slot).xs as i32;
    let ys = (*slot).ys as i32;

    if dx >= xs {
        dx = xs - 1;
    }
    if dy >= ys {
        dy = ys - 1;
    }

    if warp_at(slot, dx, dy) {
        return 0;
    }

    check_mob_collision(mob, m, dx, dy);
    check_pc_collision(mob, m, dx, dy);
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let cell_ids = grid.ids_at_tile(dx as u16, dy as u16);
        for id in cell_ids {
            mob_move_inner_id(id, mob);
        }
    }

    if clif_object_canmove(m, dx, dy, (*mob).side) != 0 {
        (*mob).canmove = 0;
        return 0;
    }
    if clif_object_canmove_from(m, backx, backy, (*mob).side) != 0 {
        (*mob).canmove = 0;
        return 0;
    }
    if map_canmove(m, dx, dy) == 1 || (*mob).canmove == 1 {
        (*mob).canmove = 0;
        return 0;
    }

    // clamp after collision checks
    let dx = if dx >= xs || dx < 0 { backx } else { dx };
    let dy = if dy >= ys || dy < 0 { backy } else { dy };

    if dx != backx || dy != backy {
        (*mob).prev_x = backx as u16;
        (*mob).prev_y = backy as u16;
        let old_x = (*mob).x;
        let old_y = (*mob).y;
        map_moveblock_id((*mob).id, (*mob).m, old_x, old_y, dx as u16, dy as u16);
        (*mob).x = dx as u16;
        (*mob).y = dy as u16;
        broadcast_move(mob, x0, y0, x1, y1, nothingnew);
        return 1;
    }
    0
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn move_mob_ignore_object(mob: *mut MobSpawnData) -> i32 {
    let m = (*mob).m as i32;
    let backx = (*mob).x as i32;
    let backy = (*mob).y as i32;
    let slot = ffi_get_map_ptr((*mob).m);
    if slot.is_null() {
        return 0;
    }

    let (x0, y0, x1, y1, mut dx, mut dy, nothingnew) = viewport_delta(mob, slot);
    let xs = (*slot).xs as i32;
    let ys = (*slot).ys as i32;
    if dx >= xs {
        dx = xs - 1;
    }
    if dy >= ys {
        dy = ys - 1;
    }
    if warp_at(slot, dx, dy) {
        return 0;
    }

    // No collision callbacks -- ignore objects
    if clif_object_canmove(m, dx, dy, (*mob).side) != 0 {
        (*mob).canmove = 0;
        return 0;
    }
    if clif_object_canmove_from(m, backx, backy, (*mob).side) != 0 {
        (*mob).canmove = 0;
        return 0;
    }

    let dx = if dx >= xs || dx < 0 { backx } else { dx };
    let dy = if dy >= ys || dy < 0 { backy } else { dy };

    if dx != backx || dy != backy {
        (*mob).prev_x = backx as u16;
        (*mob).prev_y = backy as u16;
        let old_x = (*mob).x;
        let old_y = (*mob).y;
        map_moveblock_id((*mob).id, (*mob).m, old_x, old_y, dx as u16, dy as u16);
        (*mob).x = dx as u16;
        (*mob).y = dy as u16;
        broadcast_move(mob, x0, y0, x1, y1, nothingnew);
        return 1;
    }
    0
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn moveghost_mob(mob: *mut MobSpawnData) -> i32 {
    let m = (*mob).m as i32;
    let backx = (*mob).x as i32;
    let backy = (*mob).y as i32;
    let slot = ffi_get_map_ptr((*mob).m);
    if slot.is_null() {
        return 0;
    }

    let (x0, y0, x1, y1, mut dx, mut dy, nothingnew) = viewport_delta(mob, slot);
    let xs = (*slot).xs as i32;
    let ys = (*slot).ys as i32;
    if dx >= xs {
        dx = xs - 1;
    }
    if dy >= ys {
        dy = ys - 1;
    }
    if warp_at(slot, dx, dy) {
        return 0;
    }

    check_mob_collision(mob, m, dx, dy);
    check_pc_collision(mob, m, dx, dy);
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let cell_ids = grid.ids_at_tile(dx as u16, dy as u16);
        for id in cell_ids {
            mob_move_inner_id(id, mob);
        }
    }

    // Collision checks only apply when mob has no target
    if (*mob).target == 0 {
        if clif_object_canmove(m, dx, dy, (*mob).side) != 0 {
            (*mob).canmove = 0;
            return 0;
        }
        if clif_object_canmove_from(m, backx, backy, (*mob).side) != 0 {
            (*mob).canmove = 0;
            return 0;
        }
        if map_canmove(m, dx, dy) == 1 || (*mob).canmove == 1 {
            (*mob).canmove = 0;
            return 0;
        }
    }

    let dx = if dx >= xs || dx < 0 { backx } else { dx };
    let dy = if dy >= ys || dy < 0 { backy } else { dy };

    if dx != backx || dy != backy {
        (*mob).prev_x = backx as u16;
        (*mob).prev_y = backy as u16;
        let old_x = (*mob).x;
        let old_y = (*mob).y;
        map_moveblock_id((*mob).id, (*mob).m, old_x, old_y, dx as u16, dy as u16);
        (*mob).x = dx as u16;
        (*mob).y = dy as u16;
        broadcast_move(mob, x0, y0, x1, y1, nothingnew);
        return 1;
    }
    0
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_move2(mob: *mut MobSpawnData, x: i32, y: i32, side: i32) -> i32 {
    if (*mob).canmove != 0 {
        return 1;
    }
    if x < 0 || y < 0 {
        return 0;
    }
    let m = (*mob).m as i32;
    (*mob).side = side;
    check_mob_collision(mob, m, x, y);
    check_pc_collision(mob, m, x, y);
    let cm = (*mob).canmove;
    if map_canmove(m, x, y) == 0 && cm == 0 {
        (*mob).prev_x = (*mob).x;
        (*mob).prev_y = (*mob).y;
        map_moveblock_id((*mob).id, (*mob).m, (*mob).x, (*mob).y, x as u16, y as u16);
        (*mob).x = x as u16;
        (*mob).y = y as u16;
        if let Some(grid) = block_grid::get_grid(m as usize) {
            let slot = &*ffi_get_map_ptr((*mob).m);
            let ids = block_grid::ids_in_area(
                grid,
                (*mob).x as i32,
                (*mob).y as i32,
                AreaType::Area,
                slot.xs as i32,
                slot.ys as i32,
            );
            for id in ids {
                if let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_mob_move_inner(&sd_arc, mob);
                }
            }
        }
        (*mob).canmove = 1;
    } else {
        (*mob).canmove = 0;
        return 0;
    }
    1
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn move_mob_intent(mob: *mut MobSpawnData, target_x: i32, target_y: i32) -> i32 {
    (*mob).canmove = 0;
    let mx = (*mob).x as i32;
    let my = (*mob).y as i32;
    let px = target_x;
    let py = target_y;
    let ax = (mx - px).abs();
    let ay = (my - py).abs();
    let side = (*mob).side;
    if (ax == 0 && ay == 1) || (ax == 1 && ay == 0) {
        if mx < px {
            (*mob).side = 1;
        }
        if mx > px {
            (*mob).side = 3;
        }
        if my < py {
            (*mob).side = 2;
        }
        if my > py {
            (*mob).side = 0;
        }
        if side != (*mob).side {
            clif_sendmob_side(mob);
        }
        return 1;
    }
    0
}

// ─── Entity collision inner ──────────────────────────────────────────────────

/// Typed inner: check whether an entity blocks mob movement.
/// Sets `mob->canmove = 1` if the entity occupies the cell and is not a valid ghost/GM.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_move_inner_id(entity_id: u32, mob: *mut MobSpawnData) -> i32 {
    use crate::game::pc::PC_DIE;
    if mob.is_null() {
        return 0;
    }
    if (*mob).canmove == 1 {
        return 0;
    }

    if let Some(arc) = crate::game::map_server::map_id2npc_ref(entity_id) {
        if arc.read().subtype != 0 {
            return 0;
        }
    } else if let Some(arc) = map_id2mob_ref(entity_id) {
        let m2 = arc.read();
        if m2.state == MOB_DEAD {
            return 0;
        }
    } else if let Some(arc) = crate::game::map_server::map_id2sd_pc(entity_id) {
        let sd = arc.read();
        let show_ghosts = if ffi_map_is_loaded((*mob).m) {
            (*ffi_get_map_ptr((*mob).m)).show_ghosts
        } else {
            0
        };
        if (show_ghosts != 0 && sd.player.combat.state == PC_DIE as i8)
            || sd.player.combat.state == -1
            || sd.player.identity.gm_level >= 50
        {
            return 0;
        }
    } else {
        return 0;
    }
    (*mob).canmove = 1;
    0
}

// ─── Scripting helper ────────────────────────────────────────────────────────

/// Return 1 if the mob can step forward in its current direction, 0 if blocked.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn sl_mob_checkmove(mob: *mut MobSpawnData) -> i32 {
    if mob.is_null() {
        return 0;
    }
    let m = (*mob).m as i32;
    let mut dx = (*mob).x as i32;
    let mut dy = (*mob).y as i32;
    let direction = (*mob).side;
    match direction {
        0 => dy -= 1,
        1 => dx += 1,
        2 => dy += 1,
        3 => dx -= 1,
        _ => {}
    }
    let slot = ffi_get_map_ptr((*mob).m);
    if slot.is_null() {
        return 0;
    }
    dx = dx.max(0).min((*slot).xs as i32 - 1);
    dy = dy.max(0).min((*slot).ys as i32 - 1);
    if warp_at(slot, dx, dy) {
        return 0;
    }
    (*mob).canmove = 0;
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let cell_ids = grid.ids_at_tile(dx as u16, dy as u16);
        for id in cell_ids {
            // Skip floor items -- they don't block movement
            if (FLOORITEM_START_NUM..NPC_START_NUM).contains(&id) {
                continue;
            }
            mob_move_inner_id(id, mob);
        }
    }
    if clif_object_canmove(m, dx, dy, direction) != 0 {
        return 0;
    }
    if clif_object_canmove_from(m, (*mob).x as i32, (*mob).y as i32, direction) != 0 {
        return 0;
    }
    if map_canmove(m, dx, dy) == 1 || (*mob).canmove == 1 {
        return 0;
    }
    1
}
