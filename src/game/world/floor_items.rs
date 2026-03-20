//! Floor item ID pool — bitmap tracking and world insert/remove.

use std::sync::{Mutex, OnceLock};

use crate::game::entity_store::{item_map, map_addiddb_item, map_id2fl_ref};
use crate::game::scripting::types::floor::FloorItemData;

/// Upper bound on simultaneously active floor items.
const MAX_FLOORITEM: usize = 100_000_000;

/// Bitmap tracking which floor item slots are in use (1 = occupied, 0 = free).
/// Grown on demand by `map_additem`; cleared by `map_clritem`.
/// Mutex is only for the `Sync` bound required by `OnceLock`; the game loop is single-threaded
/// and never actually contends.
static OBJECT_SLOTS: OnceLock<Mutex<Vec<u8>>> = OnceLock::new();

#[inline]
fn object_slots() -> std::sync::MutexGuard<'static, Vec<u8>> {
    OBJECT_SLOTS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Free all floor item ID slots.
pub fn map_clritem() {
    object_slots().clear();
}

/// Remove a floor item from the world by its ID.
///
/// Unlinks from the block grid, then removes the Arc from `ITEM_MAP`.
/// The block-grid unlink must come first: `map_delblock` needs a valid pointer.
/// With Arc<RwLock<T>>, removing from the map just drops one reference count —
/// the data stays alive until all Arc holders are dropped.
///
/// # Safety
/// `id` must be a valid floor item ID currently registered in `ITEM_MAP`.
/// Must be called on the game thread (single-threaded game loop).
pub unsafe fn map_delitem(id: u32) {
    use crate::game::block::map_delblock_id;

    let Some(arc) = map_id2fl_ref(id) else { return };
    let m = arc.read().m;

    map_delblock_id(id, m);

    item_map().remove(&id);

    let idx = id.wrapping_sub(crate::game::mob::FLOORITEM_START_NUM) as usize;
    let mut slots = object_slots();
    if idx < slots.len() {
        slots[idx] = 0;
    }
}

/// Assign an ID to a new floor item and insert it into the world.
///
/// Scans the bitmap for the first free slot, grows the bitmap if necessary,
/// assigns the item's ID, takes ownership of the `Box<FloorItemData>` via `Box::from_raw`,
/// registers it in `ITEM_MAP`, and links it into the block grid.
///
/// # Safety
/// - `fl` must be a valid non-null pointer to a `FloorItemData`,
///   allocated via `Box` (i.e., `Box::into_raw`), with `m`/`x`/`y` already set.
/// - Caller must not use `fl` after this call — ownership transfers to `ITEM_MAP`.
/// - Must be called on the game thread (single-threaded game loop).
pub unsafe fn map_additem(fl: *mut FloorItemData) {
    let mut slots = object_slots();

    let i = slots.iter().position(|&b| b == 0).unwrap_or(slots.len());

    if i >= MAX_FLOORITEM {
        tracing::error!("map_additem: floor item capacity exceeded ({MAX_FLOORITEM})");
        unsafe {
            drop(Box::from_raw(fl));
        }
        return;
    }

    if i >= slots.len() {
        let new_n = i + 256;
        slots.resize(new_n, 0);
    }

    slots[i] = 1;
    drop(slots);

    let id = (i as u32).wrapping_add(crate::game::mob::FLOORITEM_START_NUM);
    (*fl).id = id;
    (*fl).bl_type = crate::game::mob::BL_ITEM as u8;
    let item_box = Box::from_raw(fl);
    map_addiddb_item(id, item_box);
    let arc_clone = map_id2fl_ref(id).expect("just inserted");
    let fi = arc_clone.read();
    crate::game::block::map_addblock_id(fi.id, fi.bl_type, fi.m, fi.x, fi.y);
}
