//! FFI bridge for map data loading.
//!
//! Rust owns map and map_n as statics, exported to C via #[no_mangle].
//! map_server.c must NOT define these — they're provided by libyuri.a.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};

use crate::database::map_db::{self as db, MapData, MAP_SLOTS};

// Rust owns these globals. Exported to C so map_server.c can read map[id].* unchanged.
#[no_mangle]
pub static mut map: *mut MapData = std::ptr::null_mut();
#[no_mangle]
pub static mut map_n: c_int = 0;

/// Allocate the 65535-slot map array, load all maps from DB + files, set C globals.
/// Replaces map_read() in do_init(). Returns 0 on success, -1 on error.
#[no_mangle]
pub unsafe extern "C" fn rust_map_init(maps_dir: *const c_char, server_id: c_int) -> c_int {
    ffi_catch!(-1, {
        let dir = match unsafe { CStr::from_ptr(maps_dir) }.to_str() {
            Ok(s) => s,
            Err(_) => return -1,
        };

        // Allocate zeroed 65535-slot array on the heap, leak it (lives for process lifetime).
        let raw = unsafe {
            let layout = std::alloc::Layout::new::<[MapData; MAP_SLOTS]>();
            let ptr = std::alloc::alloc_zeroed(layout);
            if ptr.is_null() {
                std::alloc::handle_alloc_error(layout);
            }
            ptr as *mut MapData
        };

        match db::load_maps(dir, server_id, unsafe { &mut *(raw as *mut [MapData; MAP_SLOTS]) }) {
            Ok(count) => {
                unsafe {
                    map = raw;
                    map_n = count as c_int;
                }
                tracing::info!("[map] map data loaded count={count}");
                0
            }
            Err(e) => {
                tracing::error!("[map] rust_map_init failed: {e:#}");
                // free the allocation since we won't use it
                unsafe {
                    let layout = std::alloc::Layout::new::<[MapData; MAP_SLOTS]>();
                    std::alloc::dealloc(raw as *mut u8, layout);
                }
                -1
            }
        }
    })
}

/// Reload map metadata + registry in-place (tile arrays reallocated, block grid preserved).
/// Replaces map_reload() body. Returns 0 on success, -1 on error.
/// NOTE: C wrapper still calls map_foreachinarea(sl_updatepeople,...) after this returns.
#[no_mangle]
pub unsafe extern "C" fn rust_map_reload(maps_dir: *const c_char, server_id: c_int) -> c_int {
    ffi_catch!(-1, {
        if unsafe { map.is_null() } { return -1; }
        let dir = match unsafe { CStr::from_ptr(maps_dir) }.to_str() {
            Ok(s) => s,
            Err(_) => return -1,
        };
        let slots = unsafe { &mut *(map as *mut [MapData; MAP_SLOTS]) };
        match db::reload_maps(dir, server_id, slots) {
            Ok(_) => 0,
            Err(e) => { tracing::error!("[map] rust_map_reload failed: {e:#}"); -1 }
        }
    })
}

/// Returns a raw pointer to the MapData slot for `id`, or null if out of range.
pub unsafe fn get_map_ptr(id: u16) -> *mut MapData {
    if map.is_null() || id as usize >= MAP_SLOTS {
        std::ptr::null_mut()
    } else {
        map.add(id as usize)
    }
}

/// Returns the warp list head at the block containing `(dx, dy)` on map `m`.
/// Returns null if the map is not loaded or coords are out of range.
pub unsafe fn map_get_warp(m: u16, dx: u16, dy: u16) -> *mut crate::database::map_db::WarpList {
    let md_ptr = get_map_ptr(m);
    if md_ptr.is_null() { return std::ptr::null_mut(); }
    let md = &*md_ptr;
    if md.xs == 0 || md.ys == 0 { return std::ptr::null_mut(); }
    if dx >= md.xs || dy >= md.ys { return std::ptr::null_mut(); }
    if md.warp.is_null() { return std::ptr::null_mut(); }
    let block_size = crate::database::map_db::BLOCK_SIZE;
    let bx = dx as usize / block_size;
    let by = dy as usize / block_size;
    if bx >= md.bxs as usize || by >= md.bys as usize { return std::ptr::null_mut(); }
    let idx = bx + by * md.bxs as usize;
    md.warp.add(idx).read()
}

/// Returns true if the map slot for `id` is loaded (xs > 0).
pub unsafe fn map_is_loaded(id: u16) -> bool {
    let ptr = get_map_ptr(id);
    !ptr.is_null() && (*ptr).xs > 0
}
/// Reload the MapRegistry for a single map. Called from map_loadregistry() C shim.
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub unsafe extern "C" fn rust_map_loadregistry(map_id: c_int) -> c_int {
    ffi_catch!(-1, {
        if unsafe { map.is_null() } { return -1; }
        let id = map_id as usize;
        if id >= MAP_SLOTS { return -1; }
        let slot = unsafe { &mut *map.add(id) };
        match db::load_registry(slot, map_id as u32) {
            Ok(_) => 0,
            Err(e) => { tracing::error!("[map] rust_map_loadregistry map_id={map_id} failed: {e:#}"); -1 }
        }
    })
}
