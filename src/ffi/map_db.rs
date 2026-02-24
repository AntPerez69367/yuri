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
            std::alloc::alloc_zeroed(layout) as *mut MapData
        };

        match db::load_maps(dir, server_id, unsafe { &mut *(raw as *mut [MapData; MAP_SLOTS]) }) {
            Ok(count) => {
                unsafe {
                    map = raw;
                    map_n = count as c_int;
                }
                println!("[map] Map data file reading finished. {count} maps loaded!");
                0
            }
            Err(e) => {
                eprintln!("[map] rust_map_init failed: {e}");
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
            Err(e) => { eprintln!("[map] rust_map_reload failed: {e}"); -1 }
        }
    })
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
            Err(e) => { eprintln!("[map] rust_map_loadregistry({map_id}) failed: {e}"); -1 }
        }
    })
}
