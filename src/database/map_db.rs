//! Map data structures and loader.
//!
//! `MapData` mirrors `struct map_data` from `map_server.h` exactly.
//! `GlobalReg` mirrors `struct global_reg` from `mmo.h` exactly.

use std::os::raw::{c_char, c_int, c_uchar, c_uint, c_ushort};

use anyhow::{Context, Result};
use rayon::prelude::*;
use sqlx::Row;

use crate::database::{blocking_run, get_pool};

pub const BLOCK_SIZE: usize = 8;
pub const MAX_MAPREG: usize = 500;
pub const MAP_SLOTS: usize = 65535;

/// Mirrors `struct global_reg` from `mmo.h`.
#[repr(C)]
pub struct GlobalReg {
    pub str: [c_char; 64],
    pub val: c_int,
}

/// Mirrors `struct block_list` from `map_server.h`. 48 bytes on 64-bit.
/// Intrusive doubly-linked list header embedded as first field in every entity
/// struct (mob, pc, npc, flooritem). `bl_type` selects which grid chain is used.
#[repr(C)]
pub struct BlockList {
    pub next:          *mut BlockList,
    pub prev:          *mut BlockList,
    pub id:            c_uint,
    pub bx:            c_uint,
    pub by:            c_uint,
    pub graphic_id:    c_uint,
    pub graphic_color: c_uint,
    pub m:             c_ushort,
    pub x:             c_ushort,
    pub y:             c_ushort,
    pub bl_type:       c_uchar,
    pub subtype:       c_uchar,
}
// SAFETY: BlockList contains raw pointers to C-managed intrusive list nodes.
// All access is gated behind unsafe blocks; no Rust code aliases these pointers.
unsafe impl Send for BlockList {}
// SAFETY: same as Send — no interior mutability, no aliasing through Rust references.
unsafe impl Sync for BlockList {}

/// Mirrors `struct warp_list` from `map_server.h`. 40 bytes on 64-bit.
#[repr(C)]
pub struct WarpList {
    pub x:    c_int,
    pub y:    c_int,
    pub tm:   c_int,
    pub tx:   c_int,
    pub ty:   c_int,
    pub next: *mut WarpList,
    pub prev: *mut WarpList,
}
// SAFETY: WarpList contains raw pointers to C-managed warp chain nodes.
// All access is gated behind unsafe blocks; no Rust code aliases these pointers.
unsafe impl Send for WarpList {}
// SAFETY: same as Send — no interior mutability, no aliasing through Rust references.
unsafe impl Sync for WarpList {}

/// Mirrors `struct map_data` from `map_server.h`.
/// Pointer fields managed by Rust (tile/pass/obj/map/registry) or C (block/block_mob/warp).
#[repr(C)]
pub struct MapData {
    pub title: [c_char; 64],
    pub mapfile: [c_char; 1024],
    pub maprejectmsg: [c_char; 64],
    pub block:     *mut *mut BlockList,
    pub block_mob: *mut *mut BlockList,
    pub warp:      *mut *mut WarpList,
    pub registry: *mut GlobalReg,
    pub max_sweep_count: c_int,
    pub user: c_int,
    pub registry_num: c_int,
    pub id: c_int,
    pub xs: c_ushort,
    pub ys: c_ushort,
    pub bxs: c_ushort,
    pub bys: c_ushort,
    pub port: c_ushort,
    pub bgm: c_ushort,
    pub bgmtype: c_ushort,
    pub map: *mut c_uchar,   // walkability byte per cell — zeroed
    pub tile: *mut c_ushort, // tile id per cell — from .map file
    pub obj: *mut c_ushort,  // obj id per cell — from .map file
    pub pass: *mut c_ushort, // passability per cell — from .map file
    pub ip: c_uint,
    pub sweeptime: c_uint,
    pub pvp: c_uchar,
    pub spell: c_uchar,
    pub light: c_uchar,
    pub weather: c_uchar,
    pub cantalk: c_uchar,
    pub show_ghosts: c_uchar,
    pub region: c_uchar,
    pub indoor: c_uchar,
    pub warpout: c_uchar,
    pub bind: c_uchar,
    pub reqlvl: c_uint,
    pub reqvita: c_uint,
    pub reqmana: c_uint,
    pub lvlmax: c_uint,
    pub manamax: c_uint,
    pub vitamax: c_uint,
    pub reqmark: c_uchar,
    pub reqpath: c_uchar,
    pub summon: c_uchar,
    pub can_use: c_uchar,
    pub can_eat: c_uchar,
    pub can_smoke: c_uchar,
    pub can_mount: c_uchar,
    pub can_group: c_uchar,
    pub can_equip: c_uchar,
}

/// Tile arrays parsed from a single .map file. Raw pointers are independently
/// heap-allocated — no aliases — so safe to move across threads.
struct ParsedTiles {
    xs: c_ushort,
    ys: c_ushort,
    bxs: c_ushort,
    bys: c_ushort,
    tile: *mut c_ushort,
    pass: *mut c_ushort,
    obj: *mut c_ushort,
    map: *mut c_uchar,
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
fn copy_str_to_fixed<const N: usize>(dest: &mut [c_char; N], src: &str) {
    let bytes = src.as_bytes();
    let copy_len = bytes.len().min(N - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, dest.as_mut_ptr(), copy_len);
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

    let bxs = ((xs as usize + BLOCK_SIZE - 1) / BLOCK_SIZE) as c_ushort;
    let bys = ((ys as usize + BLOCK_SIZE - 1) / BLOCK_SIZE) as c_ushort;

    let tile = alloc_zeroed_slice::<c_ushort>(cell_count);
    let pass = alloc_zeroed_slice::<c_ushort>(cell_count);
    let obj = alloc_zeroed_slice::<c_ushort>(cell_count);
    let map = alloc_zeroed_slice::<c_uchar>(cell_count);

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
        map,
    })
}

/// Write a slice of registry rows into a slot's pre-allocated registry array.
fn apply_registry(slot: &mut MapData, rows: &[(String, u32)]) {
    slot.registry_num = rows.len().min(MAX_MAPREG) as c_int;
    for (i, (identifier, value)) in rows.iter().take(MAX_MAPREG).enumerate() {
        let reg = unsafe { &mut *slot.registry.add(i) };
        let bytes = identifier.as_bytes();
        let copy_len = bytes.len().min(63);
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr() as *const c_char,
                reg.str.as_mut_ptr(),
                copy_len,
            );
            *reg.str.as_mut_ptr().add(copy_len) = 0;
        }
        reg.val = *value as c_int;
    }
}

/// Load MapRegistry rows for one map into its pre-allocated registry array.
/// Used by rust_map_loadregistry (single-map reload triggered by GM command).
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

    let mut map: std::collections::HashMap<u32, Vec<(String, u32)>> =
        std::collections::HashMap::new();
    for row in rows {
        let map_id: u32 = row.try_get("mrg_map_id")?;
        let identifier: String = row.try_get("mrg_identifier")?;
        let value: u32 = row.try_get("mrg_value")?;
        map.entry(map_id).or_default().push((identifier, value));
    }
    Ok(map)
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
        slot.id = row.map_id as c_int;
        slot.bgm = row.map_bgm as c_ushort;
        slot.bgmtype = row.map_bgm_type as c_ushort;
        slot.pvp = row.map_pv_p as c_uchar;
        slot.spell = row.map_spells as c_uchar;
        slot.light = row.map_light as c_uchar;
        slot.weather = row.map_weather as c_uchar;
        slot.sweeptime = row.map_sweep_time;
        slot.cantalk = row.map_chat as c_uchar;
        slot.show_ghosts = row.map_ghosts as c_uchar;
        slot.region = row.map_region as c_uchar;
        slot.indoor = row.map_indoor as c_uchar;
        slot.warpout = row.map_warpout as c_uchar;
        slot.bind = row.map_bind as c_uchar;
        slot.reqlvl = row.map_req_lvl as c_uint;
        slot.reqpath = row.map_req_path as c_uchar;
        slot.reqmark = row.map_req_mark as c_uchar;
        slot.summon = row.map_can_summon as c_uchar;
        slot.reqvita = row.map_req_vita;
        slot.reqmana = row.map_req_mana;
        slot.lvlmax = row.map_lvl_max;
        slot.vitamax = row.map_vita_max;
        slot.manamax = row.map_mana_max;
        slot.can_use = row.map_can_use as c_uchar;
        slot.can_eat = row.map_can_eat as c_uchar;
        slot.can_smoke = row.map_can_smoke as c_uchar;
        slot.can_mount = row.map_can_mount as c_uchar;
        slot.can_group = row.map_can_group as c_uchar;
        slot.can_equip = row.map_can_equip as c_uchar;

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

/// Reload map metadata and tile data in-place. Used by rust_map_reload().
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
        slot.id = row.map_id as c_int;
        slot.bgm = row.map_bgm as c_ushort;
        slot.bgmtype = row.map_bgm_type as c_ushort;
        slot.pvp = row.map_pv_p as c_uchar;
        slot.spell = row.map_spells as c_uchar;
        slot.light = row.map_light as c_uchar;
        slot.weather = row.map_weather as c_uchar;
        slot.sweeptime = row.map_sweep_time;
        slot.cantalk = row.map_chat as c_uchar;
        slot.show_ghosts = row.map_ghosts as c_uchar;
        slot.region = row.map_region as c_uchar;
        slot.indoor = row.map_indoor as c_uchar;
        slot.warpout = row.map_warpout as c_uchar;
        slot.bind = row.map_bind as c_uchar;
        slot.reqlvl = row.map_req_lvl as c_uint;
        slot.reqpath = row.map_req_path as c_uchar;
        slot.reqmark = row.map_req_mark as c_uchar;
        slot.summon = row.map_can_summon as c_uchar;
        slot.reqvita = row.map_req_vita;
        slot.reqmana = row.map_req_mana;
        slot.lvlmax = row.map_lvl_max;
        slot.vitamax = row.map_vita_max;
        slot.manamax = row.map_mana_max;
        slot.can_use = row.map_can_use as c_uchar;
        slot.can_eat = row.map_can_eat as c_uchar;
        slot.can_smoke = row.map_can_smoke as c_uchar;
        slot.can_mount = row.map_can_mount as c_uchar;
        slot.can_group = row.map_can_group as c_uchar;
        slot.can_equip = row.map_can_equip as c_uchar;

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
    fn block_list_layout() {
        assert_eq!(std::mem::size_of::<BlockList>(), 48);
        assert_eq!(std::mem::offset_of!(BlockList, m),       36);
        assert_eq!(std::mem::offset_of!(BlockList, bl_type), 42);
    }

    #[test]
    fn warp_list_layout() {
        assert_eq!(std::mem::size_of::<WarpList>(), 40);
        assert_eq!(std::mem::offset_of!(WarpList, next), 24);
    }
}
