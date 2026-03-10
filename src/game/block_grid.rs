use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::database::map_db::BLOCK_SIZE;
use crate::game::map_server::{map_id2entity, GameEntity};
use crate::game::mob::{BL_ITEM, BL_MOB, BL_NPC, MOB_DEAD};
use crate::game::pc::{OPT_FLAG_STEALTH, PC_DIE};

/// Single-threaded grid storage. Uses `UnsafeCell` to avoid `static mut`
/// deprecation while keeping the same zero-overhead access pattern.
/// SAFETY: All access is single-threaded (map server main loop).
struct GridStorage(UnsafeCell<HashMap<usize, BlockGrid>>);
unsafe impl Sync for GridStorage {}

static GRIDS: OnceLock<GridStorage> = OnceLock::new();

/// Initialize the grid storage. Call once at startup.
pub fn init_grids() {
    GRIDS.get_or_init(|| GridStorage(UnsafeCell::new(HashMap::new())));
}

/// Get the grid for map slot `m`, if it exists.
pub fn get_grid(m: usize) -> Option<&'static BlockGrid> {
    // SAFETY: single-threaded access
    unsafe { (*GRIDS.get()?.0.get()).get(&m) }
}

/// Get a mutable reference to the grid for map slot `m`.
pub fn get_grid_mut(m: usize) -> Option<&'static mut BlockGrid> {
    // SAFETY: single-threaded access
    unsafe { (*GRIDS.get()?.0.get()).get_mut(&m) }
}

/// Create a grid for map slot `m` with the given dimensions.
pub fn create_grid(m: usize, xs: u16, ys: u16) {
    if let Some(storage) = GRIDS.get() {
        // SAFETY: single-threaded access
        unsafe { (*storage.0.get()).insert(m, BlockGrid::new(xs, ys)); }
    }
}

/// Safe spatial index: a flat grid of cells, each containing a Vec of entity IDs.
/// Each entity's cell and exact tile position are tracked in `positions` so removal
/// is position-independent (matching the old linked list behavior).
pub struct BlockGrid {
    cells: Vec<Vec<u32>>,
    /// Maps entity ID → (cell_index, exact_x, exact_y) for position-independent removal
    /// and exact-tile queries.
    positions: HashMap<u32, (usize, u16, u16)>,
    bxs: usize,
    bys: usize,
    pub user_count: i32,
}

// BL_PC constant for user_count tracking.
const BL_PC: u8 = 0x01;

impl BlockGrid {
    /// Create a new grid for a map with dimensions `xs * ys` tiles.
    pub fn new(xs: u16, ys: u16) -> Self {
        let bxs = (xs as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
        let bys = (ys as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
        let cell_count = bxs * bys;
        Self {
            cells: vec![Vec::new(); cell_count],
            positions: HashMap::new(),
            bxs,
            bys,
            user_count: 0,
        }
    }

    /// Cell index for a tile coordinate.
    #[inline]
    fn cell_index(&self, x: u16, y: u16) -> usize {
        let bx = x as usize / BLOCK_SIZE;
        let by = y as usize / BLOCK_SIZE;
        bx + by * self.bxs
    }

    /// Insert entity into the grid cell for (x, y).
    pub fn add(&mut self, id: u32, x: u16, y: u16, bl_type: u8) {
        let idx = self.cell_index(x, y);
        if idx >= self.cells.len() { return; }
        self.cells[idx].push(id);
        self.positions.insert(id, (idx, x, y));
        if bl_type == BL_PC { self.user_count += 1; }
    }

    /// Remove entity from the grid using tracked position. Ignores x/y — uses
    /// the positions map to find the actual cell (position-independent removal).
    pub fn remove(&mut self, id: u32, _x: u16, _y: u16, bl_type: u8) -> bool {
        if let Some((idx, _, _)) = self.positions.remove(&id) {
            if idx < self.cells.len() {
                let cell = &mut self.cells[idx];
                if let Some(pos) = cell.iter().position(|&eid| eid == id) {
                    cell.swap_remove(pos);
                }
            }
            if bl_type == BL_PC { self.user_count -= 1; }
            true
        } else {
            false
        }
    }

    /// Move entity from old position to new position. Uses tracked position
    /// for removal (position-independent), not old_x/old_y.
    pub fn move_entity(&mut self, id: u32, _old_x: u16, _old_y: u16, new_x: u16, new_y: u16) {
        let new_idx = self.cell_index(new_x, new_y);
        // Remove from actual current cell (tracked, not coordinate-based)
        if let Some(&(old_idx, _, _)) = self.positions.get(&id) {
            if old_idx == new_idx {
                // Same cell — just update stored position, no cell change needed.
                self.positions.insert(id, (old_idx, new_x, new_y));
                return;
            }
            if old_idx < self.cells.len() {
                let cell = &mut self.cells[old_idx];
                if let Some(pos) = cell.iter().position(|&eid| eid == id) {
                    cell.swap_remove(pos);
                }
            }
        }
        // Add to new cell
        if new_idx < self.cells.len() {
            self.cells[new_idx].push(id);
            self.positions.insert(id, (new_idx, new_x, new_y));
        }
    }

    /// Collect all entity IDs in the rectangular tile region [x0..x1] x [y0..y1].
    /// Coordinates are clamped to grid bounds.
    pub fn ids_in_rect(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<u32> {
        let x0 = x0.max(0) as usize;
        let y0 = y0.max(0) as usize;
        let x1 = (x1.max(0) as usize).min(self.bxs * BLOCK_SIZE - 1);
        let y1 = (y1.max(0) as usize).min(self.bys * BLOCK_SIZE - 1);

        let bx0 = x0 / BLOCK_SIZE;
        let by0 = y0 / BLOCK_SIZE;
        let bx1 = x1 / BLOCK_SIZE;
        let by1 = y1 / BLOCK_SIZE;

        let mut result = Vec::new();
        for by in by0..=by1.min(self.bys - 1) {
            for bx in bx0..=bx1.min(self.bxs - 1) {
                let pos = bx + by * self.bxs;
                if pos < self.cells.len() {
                    result.extend_from_slice(&self.cells[pos]);
                }
            }
        }
        result
    }

    /// Collect all entity IDs in the 8x8 block containing tile (x, y).
    /// For exact-tile queries, use `ids_at_tile` instead.
    pub fn ids_in_cell(&self, x: u16, y: u16) -> Vec<u32> {
        let idx = self.cell_index(x, y);
        if idx < self.cells.len() {
            self.cells[idx].clone()
        } else {
            Vec::new()
        }
    }

    /// Collect all entity IDs at **exactly** tile (x, y).
    /// Unlike `ids_in_cell` which returns all entities in the 8x8 block,
    /// this filters by the stored exact position in the `positions` map.
    pub fn ids_at_tile(&self, x: u16, y: u16) -> Vec<u32> {
        let idx = self.cell_index(x, y);
        if idx >= self.cells.len() {
            return Vec::new();
        }
        self.cells[idx]
            .iter()
            .copied()
            .filter(|&id| {
                if let Some(&(_, px, py)) = self.positions.get(&id) {
                    px == x && py == y
                } else {
                    false
                }
            })
            .collect()
    }

    /// Number of grid columns.
    pub fn bxs(&self) -> usize { self.bxs }

    /// Number of grid rows.
    pub fn bys(&self) -> usize { self.bys }

    /// Total number of cells.
    pub fn cell_count(&self) -> usize { self.cells.len() }

    /// Iterate all IDs across all cells (for full-map scans like mob respawn).
    pub fn all_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.cells.iter().flat_map(|cell| cell.iter().copied())
    }
}

/// Viewport half-widths matching the original constants.
const NX: i32 = 18; // AREAX_SIZE
const NY: i32 = 16; // AREAY_SIZE

/// Compute entity IDs in the viewport area around (x, y) on a map of size (map_xs, map_ys).
pub fn ids_in_area(
    grid: &BlockGrid,
    x: i32,
    y: i32,
    area: AreaType,
    map_xs: i32,
    map_ys: i32,
) -> Vec<u32> {
    match area {
        AreaType::Area => {
            grid.ids_in_rect(x - NX - 1, y - NY - 1, x + NX + 1, y + NY + 1)
        }
        AreaType::SameArea => {
            let mut sx = x - NX;
            let mut sy = y - NY;
            let mut ex = x + NX;
            let mut ey = y + NY;
            if sx < 0 { ex -= sx; sx = 0; }
            if sy < 0 { ey -= sy; sy = 0; }
            if ex >= map_xs { sx -= ex - map_xs + 1; ex = map_xs - 1; }
            if ey >= map_ys { sy -= ey - map_ys + 1; ey = map_ys - 1; }
            sx = sx.max(0);
            sy = sy.max(0);
            grid.ids_in_rect(sx, sy, ex, ey)
        }
        AreaType::SameMap => {
            grid.ids_in_rect(0, 0, map_xs - 1, map_ys - 1)
        }
        AreaType::Corner => {
            // Corner sends 4 strips. For the safe grid, this is called per-strip
            // by the caller (clif_parsewalk). Return empty — callers use ids_in_rect directly.
            Vec::new()
        }
    }
}

/// Re-export AreaType for callers migrating from block.rs.
pub use crate::game::block::AreaType;

/// Return the `BL_*` type constant for a `GameEntity`, or 0 if unknown.
fn entity_bl_type(ent: &GameEntity) -> i32 {
    match ent {
        GameEntity::Player(_) => crate::game::mob::BL_PC,
        GameEntity::Mob(_) => BL_MOB,
        GameEntity::Npc(_) => BL_NPC,
        GameEntity::Item(_) => BL_ITEM,
    }
}

/// Return `true` if the entity is alive and visible.
///
/// - Mobs: `state != MOB_DEAD`.
/// - PCs: not dead and not stealthed.
/// - NPCs / Items: always alive.
fn entity_is_alive(ent: &GameEntity) -> bool {
    match ent {
        GameEntity::Mob(mob) => mob.state != MOB_DEAD,
        GameEntity::Player(sd) => {
            let dead = sd.status.state == PC_DIE as i8;
            let stealth = (sd.optFlags & OPT_FLAG_STEALTH) != 0;
            !dead && !stealth
        }
        GameEntity::Npc(_) | GameEntity::Item(_) => true,
    }
}

/// Filter entity IDs by `bl_type` bitmask, returning only those whose type
/// matches the mask and that are alive (mobs not dead, PCs not dead/stealthed).
pub fn filter_by_type(ids: &[u32], bl_type_mask: i32) -> Vec<u32> {
    ids.iter()
        .copied()
        .filter(|&id| {
            if let Some(ent) = map_id2entity(id) {
                let ty = entity_bl_type(&ent);
                (bl_type_mask & ty) != 0 && entity_is_alive(&ent)
            } else {
                false
            }
        })
        .collect()
}

/// Get the first entity ID at exact tile `(x, y)` on map `m` that matches `bl_type`.
///
/// Returns `None` if no matching live entity is found in that cell.
pub fn first_in_cell(m: usize, x: u16, y: u16, bl_type: i32) -> Option<u32> {
    let grid = get_grid(m)?;
    let ids = grid.ids_at_tile(x, y);
    ids.into_iter().find(|&id| {
        if let Some(ent) = map_id2entity(id) {
            let ty = entity_bl_type(&ent);
            (bl_type & ty) != 0 && entity_is_alive(&ent)
        } else {
            false
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_ids_in_cell() {
        let mut grid = BlockGrid::new(16, 16);
        grid.add(100, 5, 5, 0x02); // mob
        grid.add(200, 5, 5, 0x01); // pc
        grid.add(300, 5, 8, 0x02); // mob at different block row (y=8 → by=1)

        let ids = grid.ids_in_cell(5, 5);
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&100));
        assert!(ids.contains(&200));

        // Different cell (y=8 is in block row 1, y=5 is in block row 0)
        let ids2 = grid.ids_in_cell(5, 8);
        assert_eq!(ids2.len(), 1);
        assert!(ids2.contains(&300));
    }

    #[test]
    fn test_remove() {
        let mut grid = BlockGrid::new(16, 16);
        grid.add(100, 3, 3, 0x02);
        grid.add(200, 3, 3, 0x01);

        assert!(grid.remove(100, 3, 3, 0x02));
        let ids = grid.ids_in_cell(3, 3);
        assert_eq!(ids, vec![200]);

        // Remove non-existent returns false
        assert!(!grid.remove(999, 3, 3, 0x02));
    }

    #[test]
    fn test_move_entity() {
        let mut grid = BlockGrid::new(32, 32);
        grid.add(100, 2, 2, 0x02);

        // Move to different cell
        grid.move_entity(100, 2, 2, 20, 20);
        assert!(grid.ids_in_cell(2, 2).is_empty());
        assert_eq!(grid.ids_in_cell(20, 20), vec![100]);
    }

    #[test]
    fn test_move_same_cell_is_noop() {
        let mut grid = BlockGrid::new(16, 16);
        grid.add(100, 1, 1, 0x02);
        grid.add(200, 2, 2, 0x02);

        // Move within same 8x8 cell — no grid change
        grid.move_entity(100, 1, 1, 3, 3);
        let ids = grid.ids_in_cell(1, 1);
        assert!(ids.contains(&100)); // still in same cell
    }

    #[test]
    fn test_ids_in_rect() {
        let mut grid = BlockGrid::new(32, 32);
        grid.add(10, 5, 5, 0x01);
        grid.add(20, 15, 15, 0x02);
        grid.add(30, 25, 25, 0x04);

        // Rect covering first two cells only
        let ids = grid.ids_in_rect(0, 0, 20, 20);
        assert!(ids.contains(&10));
        assert!(ids.contains(&20));
        assert!(!ids.contains(&30));
    }

    #[test]
    fn test_user_count() {
        let mut grid = BlockGrid::new(16, 16);
        assert_eq!(grid.user_count, 0);

        grid.add(1, 5, 5, 0x01); // BL_PC
        assert_eq!(grid.user_count, 1);

        grid.add(2, 5, 5, 0x02); // BL_MOB
        assert_eq!(grid.user_count, 1); // unchanged

        grid.remove(1, 5, 5, 0x01);
        assert_eq!(grid.user_count, 0);
    }

    #[test]
    fn test_all_ids() {
        let mut grid = BlockGrid::new(16, 16);
        grid.add(1, 0, 0, 0x01);
        grid.add(2, 8, 8, 0x02);
        grid.add(3, 0, 0, 0x04);

        let mut all: Vec<u32> = grid.all_ids().collect();
        all.sort();
        assert_eq!(all, vec![1, 2, 3]);
    }

    #[test]
    fn test_empty_grid() {
        let grid = BlockGrid::new(16, 16);
        assert!(grid.ids_in_cell(5, 5).is_empty());
        assert!(grid.ids_in_rect(0, 0, 15, 15).is_empty());
        assert_eq!(grid.all_ids().count(), 0);
    }

    #[test]
    fn test_remove_nonexistent_no_panic() {
        let mut grid = BlockGrid::new(16, 16);
        assert!(!grid.remove(999, 5, 5, 0x01));
    }

    #[test]
    fn test_ids_in_area_samemap() {
        let mut grid = BlockGrid::new(32, 32);
        grid.add(1, 0, 0, 0x01);
        grid.add(2, 31, 31, 0x02);

        let ids = ids_in_area(&grid, 16, 16, AreaType::SameMap, 32, 32);
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
    }

    #[test]
    fn test_ids_at_tile_exact_match() {
        let mut grid = BlockGrid::new(16, 16);
        // Add 3 entities in the same 8x8 block but different exact tiles
        grid.add(100, 2, 3, 0x02);
        grid.add(200, 4, 5, 0x01);
        grid.add(300, 2, 3, 0x04); // same exact tile as 100

        // ids_at_tile returns only entities at exact (2,3)
        let exact = grid.ids_at_tile(2, 3);
        assert_eq!(exact.len(), 2);
        assert!(exact.contains(&100));
        assert!(exact.contains(&300));
        assert!(!exact.contains(&200)); // different tile, same block

        // ids_in_cell returns ALL entities in the block
        let block = grid.ids_in_cell(2, 3);
        assert_eq!(block.len(), 3); // includes 200

        // Empty tile in same block
        assert!(grid.ids_at_tile(0, 0).is_empty());
    }

    #[test]
    fn test_ids_in_area_area() {
        let mut grid = BlockGrid::new(64, 64);
        grid.add(1, 30, 30, 0x01); // within ±19 of (30,30)
        grid.add(2, 60, 60, 0x02); // far away

        let ids = ids_in_area(&grid, 30, 30, AreaType::Area, 64, 64);
        assert!(ids.contains(&1));
        assert!(!ids.contains(&2));
    }
}
