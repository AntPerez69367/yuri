//! Entity collection functions for block-grid spatial queries.
//!
//! Returns `Vec<u32>` of entity IDs matching type and alive filters.
//! Callers use `id_to_lua` to convert IDs to Lua userdata.

use crate::game::block::AreaType;
use crate::game::block_grid;
use crate::game::mob::{MOB_START_NUM, FLOORITEM_START_NUM, NPC_START_NUM};
use crate::game::pc::{BL_PC, BL_MOB, BL_NPC, BL_ITEM};

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Check if an entity ID's type matches the bl_type bitmask using ID ranges.
#[inline]
fn id_matches_bl_type(id: u32, bl_type: i32) -> bool {
    if id == 0 { return false; }
    let entity_type = if id < MOB_START_NUM {
        BL_PC
    } else if id >= NPC_START_NUM {
        BL_NPC
    } else if id >= FLOORITEM_START_NUM {
        BL_ITEM
    } else {
        BL_MOB
    };
    (entity_type & bl_type) != 0
}

// ─── Cell queries ────────────────────────────────────────────────────────────

/// Collect entity IDs of `bl_type` at cell (x, y) on map `m`.
pub fn get_objects_cell(m: i32, x: i32, y: i32, bl_type: i32) -> Vec<u32> {
    let mut result = Vec::new();
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let cell_ids = grid.ids_at_tile(x as u16, y as u16);
        for id in cell_ids {
            if id_matches_bl_type(id, bl_type) {
                result.push(id);
            }
        }
    }
    result
}

/// Same as `get_objects_cell` — trap enumeration is TODO.
pub fn get_objects_cell_with_traps(m: i32, x: i32, y: i32, bl_type: i32) -> Vec<u32> {
    get_objects_cell(m, x, y, bl_type)
}

/// Like `get_objects_cell` but skips dead mobs and stealthed / dead PCs.
pub fn get_alive_objects_cell(m: i32, x: i32, y: i32, bl_type: i32) -> Vec<u32> {
    let mut result = Vec::new();
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let cell_ids = grid.ids_at_tile(x as u16, y as u16);
        for id in cell_ids {
            if id_matches_bl_type(id, bl_type) && crate::game::block::is_alive_id(id) {
                result.push(id);
            }
        }
    }
    result
}

// ─── Map-wide query ──────────────────────────────────────────────────────────

/// Collect entity IDs of `bl_type` across the entire map `m`.
pub fn get_objects_in_map(m: i32, bl_type: i32) -> Vec<u32> {
    let mut result = Vec::new();
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let slot = unsafe { &*crate::database::map_db::raw_map_ptr().add(m as usize) };
        let ids = block_grid::ids_in_area(grid, 0, 0, AreaType::SameMap, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if id_matches_bl_type(id, bl_type) {
                result.push(id);
            }
        }
    }
    result
}

// ─── Area queries (centred on entity position) ──────────────────────────────

/// Collect entity IDs of `bl_type` within AREA range of position (m, x, y).
pub fn get_objects_area(m: u16, x: u16, y: u16, bl_type: i32) -> Vec<u32> {
    let mut result = Vec::new();
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let slot = unsafe { &*crate::database::map_db::raw_map_ptr().add(m as usize) };
        let ids = block_grid::ids_in_area(grid, x as i32, y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if id_matches_bl_type(id, bl_type) {
                result.push(id);
            }
        }
    }
    result
}

/// Like `get_objects_area` but skips dead mobs and stealthed / dead PCs.
pub fn get_alive_objects_area(m: u16, x: u16, y: u16, bl_type: i32) -> Vec<u32> {
    let mut result = Vec::new();
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let slot = unsafe { &*crate::database::map_db::raw_map_ptr().add(m as usize) };
        let ids = block_grid::ids_in_area(grid, x as i32, y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if id_matches_bl_type(id, bl_type) && crate::game::block::is_alive_id(id) {
                result.push(id);
            }
        }
    }
    result
}

/// Collect entity IDs of `bl_type` across the whole map that position (m, x, y) is on.
pub fn get_objects_same_map(m: u16, bl_type: i32) -> Vec<u32> {
    let mut result = Vec::new();
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let slot = unsafe { &*crate::database::map_db::raw_map_ptr().add(m as usize) };
        let ids = block_grid::ids_in_area(grid, 0, 0, AreaType::SameMap, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if id_matches_bl_type(id, bl_type) {
                result.push(id);
            }
        }
    }
    result
}

/// Like `get_objects_same_map` but skips dead mobs and stealthed / dead PCs.
pub fn get_alive_objects_same_map(m: u16, bl_type: i32) -> Vec<u32> {
    let mut result = Vec::new();
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let slot = unsafe { &*crate::database::map_db::raw_map_ptr().add(m as usize) };
        let ids = block_grid::ids_in_area(grid, 0, 0, AreaType::SameMap, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if id_matches_bl_type(id, bl_type) && crate::game::block::is_alive_id(id) {
                result.push(id);
            }
        }
    }
    result
}
