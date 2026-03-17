//! Map data structures and loader.
//!
//! Map and global registry data structures.
#![allow(non_upper_case_globals)]

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use rayon::prelude::*;
use sqlx::Row;

use crate::database::{blocking_run, get_pool};

pub const BLOCK_SIZE: usize = 8;
pub const MAX_MAPREG: usize = 500;
pub const MAP_SLOTS: usize = 65535;

pub use crate::common::types::GlobalReg;

/// Warp portal entry for map-to-map teleportation. 40 bytes on 64-bit.
#[repr(C)]
pub struct WarpList {
    pub x:    i32,
    pub y:    i32,
    pub tm:   i32,
    pub tx:   i32,
    pub ty:   i32,
    pub next: *mut WarpList,
    pub prev: *mut WarpList,
}
// SAFETY: WarpList contains raw pointers to C-managed warp chain nodes.
// All access is gated behind unsafe blocks; no Rust code aliases these pointers.
unsafe impl Send for WarpList {}
// SAFETY: same as Send — no interior mutability, no aliasing through Rust references.
unsafe impl Sync for WarpList {}

/// Per-map data: grid dimensions, tile flags, entity lists, and registry.
/// Pointer fields managed by Rust (tile/pass/obj/map/registry) or C (block/block_mob/warp).
#[repr(C)]
pub struct MapData {
    pub title: [i8; 64],
    pub mapfile: [i8; 1024],
    pub maprejectmsg: [i8; 64],
    pub block:     *mut *mut u8,     // legacy — spatial indexing uses block_grid
    pub block_mob: *mut *mut u8,     // legacy — spatial indexing uses block_grid
    pub warp:      *mut *mut WarpList,
    pub registry: *mut GlobalReg,
    pub max_sweep_count: i32,
    pub user: i32,
    pub registry_num: i32,
    pub id: i32,
    pub xs: u16,
    pub ys: u16,
    pub bxs: u16,
    pub bys: u16,
    pub port: u16,
    pub bgm: u16,
    pub bgmtype: u16,
    pub map: *mut u8,   // walkability byte per cell — zeroed
    pub tile: *mut u16, // tile id per cell — from .map file
    pub obj: *mut u16,  // obj id per cell — from .map file
    pub pass: *mut u16, // passability per cell — from .map file
    pub ip: u32,
    pub sweeptime: u32,
    pub pvp: u8,
    pub spell: u8,
    pub light: u8,
    pub weather: u8,
    pub cantalk: u8,
    pub show_ghosts: u8,
    pub region: u8,
    pub indoor: u8,
    pub warpout: u8,
    pub bind: u8,
    pub reqlvl: u32,
    pub reqvita: u32,
    pub reqmana: u32,
    pub lvlmax: u32,
    pub manamax: u32,
    pub vitamax: u32,
    pub reqmark: u8,
    pub reqpath: u8,
    pub summon: u8,
    pub can_use: u8,
    pub can_eat: u8,
    pub can_smoke: u8,
    pub can_mount: u8,
    pub can_group: u8,
    pub can_equip: u8,
}

/// Tile arrays parsed from a single .map file. Raw pointers are independently
/// heap-allocated — no aliases — so safe to move across threads.
pub struct ParsedTiles {
    pub xs: u16,
    pub ys: u16,
    pub bxs: u16,
    pub bys: u16,
    pub tile: *mut u16,
    pub pass: *mut u16,
    pub obj: *mut u16,
    pub map: *mut u8,
}
// Each pointer is a uniquely-owned allocation with no aliases.
unsafe impl Send for ParsedTiles {}

impl Drop for ParsedTiles {
    fn drop(&mut self) {
        unsafe fn free_slice<T>(ptr: *mut T, len: usize) {
            if !ptr.is_null() {
                drop(Vec::from_raw_parts(ptr, len, len));
            }
        }
        let cell_count = self.xs as usize * self.ys as usize;
        unsafe {
            free_slice(self.tile, cell_count);
            free_slice(self.pass, cell_count);
            free_slice(self.obj, cell_count);
            free_slice(self.map, cell_count);
        }
    }
}

/// Allocate a zeroed heap slice and return a raw pointer (caller owns memory).
fn alloc_zeroed_slice<T: Default + Clone>(len: usize) -> *mut T {
    let mut v: Vec<T> = vec![Default::default(); len];
    let ptr = v.as_mut_ptr();
    std::mem::forget(v);
    ptr
}

/// Allocate a zeroed array of GlobalReg (no Default/Clone needed — uses raw alloc).
fn alloc_zeroed_registry(len: usize) -> *mut GlobalReg {
    let layout = std::alloc::Layout::array::<GlobalReg>(len).unwrap();
    let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
    if ptr.is_null() {
        std::alloc::handle_alloc_error(layout);
    }
    ptr as *mut GlobalReg
}

/// Copy a Rust &str into a fixed C char array, null-terminating.
fn copy_str_to_fixed<const N: usize>(dest: &mut [i8; N], src: &str) {
    let bytes = src.as_bytes();
    let copy_len = bytes.len().min(N - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const i8, dest.as_mut_ptr(), copy_len);
        *dest.as_mut_ptr().add(copy_len) = 0;
    }
}

/// Parse a `.map` binary file and return allocated tile arrays.
/// File format: [xs: u16 BE][ys: u16 BE] then xs*ys × (tile u16 BE, pass u16 BE, obj u16 BE).
/// Reads the entire file in one syscall, then parses from the in-memory buffer.
pub fn parse_map_file(path: &str) -> Result<ParsedTiles> {
    let data = std::fs::read(path).with_context(|| format!("map file not found: {path}"))?;

    if data.len() < 4 {
        anyhow::bail!("map file too short: {path}");
    }

    let xs = u16::from_be_bytes([data[0], data[1]]);
    let ys = u16::from_be_bytes([data[2], data[3]]);

    let cell_count = xs as usize * ys as usize;
    let expected = 4 + cell_count * 6;
    if data.len() < expected {
        anyhow::bail!(
            "map file truncated: {path} (got {} bytes, need {expected})",
            data.len()
        );
    }

    let bxs = ((xs as usize + BLOCK_SIZE - 1) / BLOCK_SIZE) as u16;
    let bys = ((ys as usize + BLOCK_SIZE - 1) / BLOCK_SIZE) as u16;

    let tile = alloc_zeroed_slice::<u16>(cell_count);
    let pass = alloc_zeroed_slice::<u16>(cell_count);
    let obj = alloc_zeroed_slice::<u16>(cell_count);
    let map_cells = alloc_zeroed_slice::<u8>(cell_count);

    let mut pos = 4usize;
    for i in 0..cell_count {
        unsafe {
            *tile.add(i) = u16::from_be_bytes([data[pos], data[pos + 1]]);
            *pass.add(i) = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
            *obj.add(i) = u16::from_be_bytes([data[pos + 4], data[pos + 5]]);
        }
        pos += 6;
    }

    Ok(ParsedTiles {
        xs,
        ys,
        bxs,
        bys,
        tile,
        pass,
        obj,
        map: map_cells,
    })
}

/// Write a slice of registry rows into a slot's pre-allocated registry array.
fn apply_registry(slot: &mut MapData, rows: &[(String, u32)]) {
    slot.registry_num = rows.len().min(MAX_MAPREG) as i32;
    for (i, (identifier, value)) in rows.iter().take(MAX_MAPREG).enumerate() {
        let reg = unsafe { &mut *slot.registry.add(i) };
        let bytes = identifier.as_bytes();
        let copy_len = bytes.len().min(63);
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr() as *const i8,
                reg.str.as_mut_ptr(),
                copy_len,
            );
            *reg.str.as_mut_ptr().add(copy_len) = 0;
        }
        reg.val = *value as i32;
    }
}

/// Load MapRegistry rows for one map into its pre-allocated registry array.
/// Used by map_loadregistry (single-map reload triggered by GM command).
pub fn load_registry(slot: &mut MapData, map_id: u32) -> Result<()> {
    if slot.registry.is_null() {
        anyhow::bail!("map_id={map_id} registry not initialized (map not loaded)");
    }

    #[derive(sqlx::FromRow)]
    struct RegRow {
        mrg_identifier: String,
        mrg_value: u32,
    }

    let rows: Vec<RegRow> = blocking_run(
        sqlx::query_as(
            "SELECT MrgIdentifier AS mrg_identifier, MrgValue AS mrg_value \
                        FROM MapRegistry WHERE MrgMapId = ? LIMIT ?",
        )
        .bind(map_id)
        .bind(MAX_MAPREG as u32)
        .fetch_all(get_pool()),
    )?;

    let pairs: Vec<(String, u32)> = rows
        .into_iter()
        .map(|r| (r.mrg_identifier, r.mrg_value))
        .collect();
    apply_registry(slot, &pairs);
    Ok(())
}

/// Bulk-load all MapRegistry rows for a set of map IDs in one query.
/// Returns a HashMap from map_id → Vec<(identifier, value)>.
fn load_all_registries(
    map_ids: &[u32],
) -> Result<std::collections::HashMap<u32, Vec<(String, u32)>>> {
    if map_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    // Build "WHERE MrgMapId IN (?,?,?...)" with one placeholder per id.
    let placeholders = map_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT MrgMapId AS mrg_map_id, MrgIdentifier AS mrg_identifier, MrgValue AS mrg_value \
         FROM MapRegistry WHERE MrgMapId IN ({placeholders})"
    );

    let mut query = sqlx::query(&sql);
    for id in map_ids {
        query = query.bind(id);
    }

    let rows = blocking_run(query.fetch_all(get_pool()))?;

    let mut registry: std::collections::HashMap<u32, Vec<(String, u32)>> =
        std::collections::HashMap::new();
    for row in rows {
        let map_id: u32 = row.try_get("mrg_map_id")?;
        let identifier: String = row.try_get("mrg_identifier")?;
        let value: u32 = row.try_get("mrg_value")?;
        registry.entry(map_id).or_default().push((identifier, value));
    }
    Ok(registry)
}

/// Query the Maps table and populate map slots. Called once at startup.
/// Returns the number of maps loaded, or an error.
pub fn load_maps(
    maps_dir: &str,
    server_id: i32,
    slots: &mut [MapData; MAP_SLOTS],
) -> Result<usize> {
    // Types match DB schema (all INT UNSIGNED except MapReqLvl which is INT).
    // All int(10) unsigned → u32; MapReqLvl int(10) → i32.
    #[derive(sqlx::FromRow)]
    struct MapRow {
        map_id: u32,
        map_name: String,
        map_bgm: u32,
        map_bgm_type: u32,
        map_pv_p: u32,
        map_spells: u32,
        map_light: u32,
        map_weather: u32,
        map_sweep_time: u32,
        map_chat: u32,
        map_ghosts: u32,
        map_region: u32,
        map_indoor: u32,
        map_warpout: u32,
        map_bind: u32,
        map_file: String,
        map_req_lvl: i32,
        map_req_path: u32,
        map_req_mark: u32,
        map_can_summon: u32,
        map_req_vita: u32,
        map_req_mana: u32,
        map_lvl_max: u32,
        map_vita_max: u32,
        map_mana_max: u32,
        map_reject_msg: String,
        map_can_use: u32,
        map_can_eat: u32,
        map_can_smoke: u32,
        map_can_mount: u32,
        map_can_group: u32,
        map_can_equip: u32,
    }

    let rows: Vec<MapRow> = blocking_run(
        sqlx::query_as(
            "SELECT MapId AS map_id, MapName AS map_name, MapBGM AS map_bgm,
             MapBGMType AS map_bgm_type, MapPvP AS map_pv_p, MapSpells AS map_spells,
             MapLight AS map_light, MapWeather AS map_weather, MapSweepTime AS map_sweep_time,
             MapChat AS map_chat, MapGhosts AS map_ghosts, MapRegion AS map_region,
             MapIndoor AS map_indoor, MapWarpout AS map_warpout, MapBind AS map_bind,
             MapFile AS map_file, MapReqLvl AS map_req_lvl, MapReqPath AS map_req_path,
             MapReqMark AS map_req_mark, MapCanSummon AS map_can_summon,
             MapReqVita AS map_req_vita, MapReqMana AS map_req_mana, MapLvlMax AS map_lvl_max,
             MapVitaMax AS map_vita_max, MapManaMax AS map_mana_max,
             MapRejectMsg AS map_reject_msg, MapCanUse AS map_can_use, MapCanEat AS map_can_eat,
             MapCanSmoke AS map_can_smoke, MapCanMount AS map_can_mount,
             MapCanGroup AS map_can_group, MapCanEquip AS map_can_equip
             FROM Maps WHERE MapServer = ? ORDER BY MapId",
        )
        .bind(server_id)
        .fetch_all(get_pool()),
    )?;

    // Phase 1: parse all .map files in parallel across rayon's thread pool.
    let parsed: Vec<(u32, Result<ParsedTiles>)> = rows
        .par_iter()
        .map(|row| {
            let path = format!("{}{}", maps_dir, row.map_file);
            (row.map_id, parse_map_file(&path))
        })
        .collect();

    // Phase 2: bulk-load all registry rows in one query.
    let map_ids: Vec<u32> = rows.iter().map(|r| r.map_id).collect();
    let mut registries = load_all_registries(&map_ids)?;

    // Phase 3: apply parsed tiles + scalar fields + registry to slots sequentially.
    let mut loaded = 0usize;
    for (row, (_, tiles_result)) in rows.iter().zip(parsed.into_iter()) {
        let id = row.map_id as usize;
        if id >= MAP_SLOTS {
            tracing::warn!("[map] map_id={id} >= MAP_SLOTS={MAP_SLOTS}, skipping");
            continue;
        }
        let mut tiles = tiles_result.with_context(|| format!("loading map id={}", row.map_id))?;
        let slot = &mut slots[id];

        copy_str_to_fixed(&mut slot.title, &row.map_name);
        copy_str_to_fixed(&mut slot.mapfile, &row.map_file);
        copy_str_to_fixed(&mut slot.maprejectmsg, &row.map_reject_msg);
        slot.id = row.map_id as i32;
        slot.bgm = row.map_bgm as u16;
        slot.bgmtype = row.map_bgm_type as u16;
        slot.pvp = row.map_pv_p as u8;
        slot.spell = row.map_spells as u8;
        slot.light = row.map_light as u8;
        slot.weather = row.map_weather as u8;
        slot.sweeptime = row.map_sweep_time;
        slot.cantalk = row.map_chat as u8;
        slot.show_ghosts = row.map_ghosts as u8;
        slot.region = row.map_region as u8;
        slot.indoor = row.map_indoor as u8;
        slot.warpout = row.map_warpout as u8;
        slot.bind = row.map_bind as u8;
        slot.reqlvl = row.map_req_lvl as u32;
        slot.reqpath = row.map_req_path as u8;
        slot.reqmark = row.map_req_mark as u8;
        slot.summon = row.map_can_summon as u8;
        slot.reqvita = row.map_req_vita;
        slot.reqmana = row.map_req_mana;
        slot.lvlmax = row.map_lvl_max;
        slot.vitamax = row.map_vita_max;
        slot.manamax = row.map_mana_max;
        slot.can_use = row.map_can_use as u8;
        slot.can_eat = row.map_can_eat as u8;
        slot.can_smoke = row.map_can_smoke as u8;
        slot.can_mount = row.map_can_mount as u8;
        slot.can_group = row.map_can_group as u8;
        slot.can_equip = row.map_can_equip as u8;

        slot.xs = tiles.xs;
        slot.ys = tiles.ys;
        slot.bxs = tiles.bxs;
        slot.bys = tiles.bys;
        // Transfer ownership of tile arrays to the slot; null out tiles so
        // ParsedTiles::drop does not double-free the transferred pointers.
        slot.tile = std::mem::replace(&mut tiles.tile, std::ptr::null_mut());
        slot.pass = std::mem::replace(&mut tiles.pass, std::ptr::null_mut());
        slot.obj = std::mem::replace(&mut tiles.obj, std::ptr::null_mut());
        slot.map = std::mem::replace(&mut tiles.map, std::ptr::null_mut());
        slot.registry = alloc_zeroed_registry(MAX_MAPREG);

        if let Some(regs) = registries.remove(&row.map_id) {
            apply_registry(slot, &regs);
        }
        loaded += 1;
    }

    Ok(loaded)
}

/// Reload map metadata and tile data in-place. Used by map_reload().
/// A map is considered "already loaded" if its registry pointer is non-null.
pub fn reload_maps(
    maps_dir: &str,
    server_id: i32,
    slots: &mut [MapData; MAP_SLOTS],
) -> Result<usize> {
    #[derive(sqlx::FromRow)]
    struct MapRow {
        map_id: u32,
        map_name: String,
        map_bgm: u32,
        map_bgm_type: u32,
        map_pv_p: u32,
        map_spells: u32,
        map_light: u32,
        map_weather: u32,
        map_sweep_time: u32,
        map_chat: u32,
        map_ghosts: u32,
        map_region: u32,
        map_indoor: u32,
        map_warpout: u32,
        map_bind: u32,
        map_file: String,
        map_req_lvl: i32,
        map_req_path: u32,
        map_req_mark: u32,
        map_can_summon: u32,
        map_req_vita: u32,
        map_req_mana: u32,
        map_lvl_max: u32,
        map_vita_max: u32,
        map_mana_max: u32,
        map_reject_msg: String,
        map_can_use: u32,
        map_can_eat: u32,
        map_can_smoke: u32,
        map_can_mount: u32,
        map_can_group: u32,
        map_can_equip: u32,
    }

    let rows: Vec<MapRow> = blocking_run(
        sqlx::query_as(
            "SELECT MapId AS map_id, MapName AS map_name, MapBGM AS map_bgm,
             MapBGMType AS map_bgm_type, MapPvP AS map_pv_p, MapSpells AS map_spells,
             MapLight AS map_light, MapWeather AS map_weather, MapSweepTime AS map_sweep_time,
             MapChat AS map_chat, MapGhosts AS map_ghosts, MapRegion AS map_region,
             MapIndoor AS map_indoor, MapWarpout AS map_warpout, MapBind AS map_bind,
             MapFile AS map_file, MapReqLvl AS map_req_lvl, MapReqPath AS map_req_path,
             MapReqMark AS map_req_mark, MapCanSummon AS map_can_summon,
             MapReqVita AS map_req_vita, MapReqMana AS map_req_mana, MapLvlMax AS map_lvl_max,
             MapVitaMax AS map_vita_max, MapManaMax AS map_mana_max,
             MapRejectMsg AS map_reject_msg, MapCanUse AS map_can_use, MapCanEat AS map_can_eat,
             MapCanSmoke AS map_can_smoke, MapCanMount AS map_can_mount,
             MapCanGroup AS map_can_group, MapCanEquip AS map_can_equip
             FROM Maps WHERE MapServer = ? ORDER BY MapId",
        )
        .bind(server_id)
        .fetch_all(get_pool()),
    )?;

    for row in &rows {
        let id = row.map_id as usize;
        if id >= MAP_SLOTS {
            tracing::warn!("[map] map_id={id} >= MAP_SLOTS={MAP_SLOTS}, skipping");
            continue;
        }
        let slot = &mut slots[id];

        // Parse the map file first — on failure, leave the slot untouched.
        let path = format!("{}{}", maps_dir, row.map_file);
        let mut tiles =
            parse_map_file(&path).with_context(|| format!("reloading map id={}", row.map_id))?;

        // Parse succeeded — now free the old tile arrays and registry.
        if !slot.registry.is_null() {
            let old_cells = slot.xs as usize * slot.ys as usize;
            unsafe {
                drop(Vec::from_raw_parts(slot.tile, old_cells, old_cells));
                drop(Vec::from_raw_parts(slot.pass, old_cells, old_cells));
                drop(Vec::from_raw_parts(slot.obj, old_cells, old_cells));
                drop(Vec::from_raw_parts(slot.map, old_cells, old_cells));
                let reg_layout = std::alloc::Layout::array::<GlobalReg>(MAX_MAPREG).unwrap();
                std::alloc::dealloc(slot.registry as *mut u8, reg_layout);
            }
        }

        copy_str_to_fixed(&mut slot.title, &row.map_name);
        copy_str_to_fixed(&mut slot.mapfile, &row.map_file);
        copy_str_to_fixed(&mut slot.maprejectmsg, &row.map_reject_msg);
        slot.id = row.map_id as i32;
        slot.bgm = row.map_bgm as u16;
        slot.bgmtype = row.map_bgm_type as u16;
        slot.pvp = row.map_pv_p as u8;
        slot.spell = row.map_spells as u8;
        slot.light = row.map_light as u8;
        slot.weather = row.map_weather as u8;
        slot.sweeptime = row.map_sweep_time;
        slot.cantalk = row.map_chat as u8;
        slot.show_ghosts = row.map_ghosts as u8;
        slot.region = row.map_region as u8;
        slot.indoor = row.map_indoor as u8;
        slot.warpout = row.map_warpout as u8;
        slot.bind = row.map_bind as u8;
        slot.reqlvl = row.map_req_lvl as u32;
        slot.reqpath = row.map_req_path as u8;
        slot.reqmark = row.map_req_mark as u8;
        slot.summon = row.map_can_summon as u8;
        slot.reqvita = row.map_req_vita;
        slot.reqmana = row.map_req_mana;
        slot.lvlmax = row.map_lvl_max;
        slot.vitamax = row.map_vita_max;
        slot.manamax = row.map_mana_max;
        slot.can_use = row.map_can_use as u8;
        slot.can_eat = row.map_can_eat as u8;
        slot.can_smoke = row.map_can_smoke as u8;
        slot.can_mount = row.map_can_mount as u8;
        slot.can_group = row.map_can_group as u8;
        slot.can_equip = row.map_can_equip as u8;

        slot.xs = tiles.xs;
        slot.ys = tiles.ys;
        slot.bxs = tiles.bxs;
        slot.bys = tiles.bys;
        // Transfer ownership of tile arrays to the slot; null out tiles so
        // ParsedTiles::drop does not double-free the transferred pointers.
        slot.tile = std::mem::replace(&mut tiles.tile, std::ptr::null_mut());
        slot.pass = std::mem::replace(&mut tiles.pass, std::ptr::null_mut());
        slot.obj = std::mem::replace(&mut tiles.obj, std::ptr::null_mut());
        slot.map = std::mem::replace(&mut tiles.map, std::ptr::null_mut());
        slot.registry = alloc_zeroed_registry(MAX_MAPREG);

        load_registry(slot, row.map_id)?;
    }

    Ok(rows.len())
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn global_reg_layout() {
        // struct global_reg { char str[64]; int val; } = 68 bytes
        assert_eq!(std::mem::size_of::<GlobalReg>(), 68);
        assert_eq!(std::mem::offset_of!(GlobalReg, val), 64);
    }

    #[test]
    fn map_data_size() {
        let size = std::mem::size_of::<MapData>();
        println!("MapData size = {size}");
        assert_eq!(size, 1304, "MapData size mismatch");
    }

    #[test]
    fn warp_list_layout() {
        assert_eq!(std::mem::size_of::<WarpList>(), 40);
        assert_eq!(std::mem::offset_of!(WarpList, next), 24);
    }
}

// ─── Public API exports ────────────────────────────────────────────────────

/// Newtype wrapping the heap-allocated MapData array pointer.
/// `unsafe impl Send + Sync`: set once at startup before any concurrent access;
/// thereafter read-only from the game thread.
struct MapPtr(*mut MapData);
unsafe impl Send for MapPtr {}
unsafe impl Sync for MapPtr {}

static MAP_PTR: OnceLock<MapPtr> = OnceLock::new();
/// Count of loaded map slots. Written once in `map_init` during startup.
pub static map_n: AtomicI32 = AtomicI32::new(0);

/// Returns the raw base pointer to the map array, or null if not yet initialized.
/// Safe to call; dereferencing the returned pointer requires `unsafe`.
pub fn raw_map_ptr() -> *mut MapData {
    MAP_PTR.get().map(|m| m.0).unwrap_or(std::ptr::null_mut())
}

/// Safe read-only access to map data by index.
/// Returns `None` if the map array isn't initialized or `m` is out of bounds.
pub fn map_data(m: usize) -> Option<&'static MapData> {
    let ptr = MAP_PTR.get()?.0;
    if ptr.is_null() || m >= MAP_SLOTS { return None; }
    Some(unsafe { &*ptr.add(m) })
}

/// Safe mutable access to map data by index.
/// Returns `None` if the map array isn't initialized or `m` is out of bounds.
pub fn map_data_mut(m: usize) -> Option<&'static mut MapData> {
    let ptr = MAP_PTR.get()?.0;
    if ptr.is_null() || m >= MAP_SLOTS { return None; }
    Some(unsafe { &mut *ptr.add(m) })
}

/// Allocate the 65535-slot map array, load all maps from DB + files, set globals.
pub unsafe fn map_init(maps_dir: &str, server_id: i32) -> i32 {
    let raw = unsafe {
        let layout = std::alloc::Layout::new::<[MapData; MAP_SLOTS]>();
        let ptr = std::alloc::alloc_zeroed(layout);
        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        ptr as *mut MapData
    };

    match load_maps(maps_dir, server_id, unsafe { &mut *(raw as *mut [MapData; MAP_SLOTS]) }) {
        Ok(count) => {
            if MAP_PTR.set(MapPtr(raw)).is_err() {
                tracing::warn!("[map] map_init called more than once — ignoring duplicate init");
                unsafe {
                    let layout = std::alloc::Layout::new::<[MapData; MAP_SLOTS]>();
                    std::alloc::dealloc(raw as *mut u8, layout);
                }
                return 0;
            }
            map_n.store(count as i32, Ordering::Relaxed);
            tracing::info!("[map] map data loaded count={count}");
            0
        }
        Err(e) => {
            tracing::error!("[map] map_init failed: {e:#}");
            unsafe {
                let layout = std::alloc::Layout::new::<[MapData; MAP_SLOTS]>();
                std::alloc::dealloc(raw as *mut u8, layout);
            }
            -1
        }
    }
}

/// Reload map metadata + registry in-place.
pub unsafe fn map_reload(maps_dir: &str, server_id: i32) -> i32 {
    if raw_map_ptr().is_null() { return -1; }
    let slots = unsafe { &mut *(raw_map_ptr() as *mut [MapData; MAP_SLOTS]) };
    match reload_maps(maps_dir, server_id, slots) {
        Ok(_) => 0,
        Err(e) => { tracing::error!("[map] map_reload failed: {e:#}"); -1 }
    }
}

/// Returns a raw pointer to the MapData slot for `id`, or null if out of range.
/// Safe to call; dereferencing the returned pointer requires `unsafe`.
pub fn get_map_ptr(id: u16) -> *mut MapData {
    let base = raw_map_ptr();
    if base.is_null() || id as usize >= MAP_SLOTS {
        std::ptr::null_mut()
    } else {
        unsafe { base.add(id as usize) }
    }
}

/// Returns the warp list head at the block containing `(dx, dy)` on map `m`.
pub unsafe fn map_get_warp(m: u16, dx: u16, dy: u16) -> *mut WarpList {
    let md_ptr = get_map_ptr(m);
    if md_ptr.is_null() { return std::ptr::null_mut(); }
    let md = &*md_ptr;
    if md.xs == 0 || md.ys == 0 { return std::ptr::null_mut(); }
    if dx >= md.xs || dy >= md.ys { return std::ptr::null_mut(); }
    if md.warp.is_null() { return std::ptr::null_mut(); }
    let block_size = BLOCK_SIZE;
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

/// Reload the MapRegistry for a single map.
pub fn map_loadregistry(map_id: i32) -> i32 {
    if raw_map_ptr().is_null() { return -1; }
    let id = map_id as usize;
    if id >= MAP_SLOTS { return -1; }
    let slot = unsafe { &mut *raw_map_ptr().add(id) };
    match load_registry(slot, map_id as u32) {
        Ok(_) => 0,
        Err(e) => { tracing::error!("[map] map_loadregistry map_id={map_id} failed: {e:#}"); -1 }
    }
}
