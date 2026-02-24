//! Map data structures and loader.
//!
//! `MapData` mirrors `struct map_data` from `map_server.h` exactly.
//! `GlobalReg` mirrors `struct global_reg` from `mmo.h` exactly.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uchar, c_uint, c_ushort};
use std::ptr::null_mut;

use anyhow::{Context, Result};
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

/// Mirrors `struct map_data` from `map_server.h`.
/// Pointer fields managed by Rust (tile/pass/obj/map/registry) or C (block/block_mob/warp).
#[repr(C)]
pub struct MapData {
    pub title: [c_char; 64],
    pub mapfile: [c_char; 1024],
    pub maprejectmsg: [c_char; 64],
    pub block: *mut *mut u8,       // struct block_list** — C-managed, opaque to Rust
    pub block_mob: *mut *mut u8,   // struct block_list** — C-managed, opaque to Rust
    pub warp: *mut *mut u8,        // struct warp_list**  — C-managed, opaque to Rust
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
    pub map: *mut c_uchar,         // walkability byte per cell — zeroed
    pub tile: *mut c_ushort,       // tile id per cell — from .map file
    pub obj: *mut c_ushort,        // obj id per cell — from .map file
    pub pass: *mut c_ushort,       // passability per cell — from .map file
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

/// Allocate a zeroed heap slice and return a raw pointer (caller owns memory).
fn alloc_zeroed_slice<T: Default + Clone>(len: usize) -> *mut T {
    let mut v: Vec<T> = vec![Default::default(); len];
    let ptr = v.as_mut_ptr();
    std::mem::forget(v);
    ptr
}

/// Allocate a zeroed array of GlobalReg (no Default/Clone needed — uses raw alloc).
fn alloc_zeroed_registry(len: usize) -> *mut GlobalReg {
    unsafe {
        let layout = std::alloc::Layout::array::<GlobalReg>(len).unwrap();
        std::alloc::alloc_zeroed(layout) as *mut GlobalReg
    }
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

/// Parse a `.map` binary file into a MapData's tile/pass/obj arrays.
/// File format: [xs: u16 BE][ys: u16 BE] then xs*ys × (tile u16 BE, pass u16 BE, obj u16 BE).
/// Allocates tile, pass, obj, map (walkability, zeroed) arrays via Box::into_raw.
/// Returns Err if the file cannot be opened or is too short.
pub fn parse_map_file(slot: &mut MapData, path: &str) -> Result<()> {
    use std::io::Read;
    let mut fp = std::fs::File::open(path)
        .with_context(|| format!("map file not found: {path}"))?;

    let mut buf2 = [0u8; 2];
    fp.read_exact(&mut buf2).context("reading xs")?;
    slot.xs = u16::from_be_bytes(buf2);
    fp.read_exact(&mut buf2).context("reading ys")?;
    slot.ys = u16::from_be_bytes(buf2);

    let cell_count = slot.xs as usize * slot.ys as usize;
    slot.bxs = ((slot.xs as usize + BLOCK_SIZE - 1) / BLOCK_SIZE) as c_ushort;
    slot.bys = ((slot.ys as usize + BLOCK_SIZE - 1) / BLOCK_SIZE) as c_ushort;

    let tile = alloc_zeroed_slice::<c_ushort>(cell_count);
    let pass = alloc_zeroed_slice::<c_ushort>(cell_count);
    let obj  = alloc_zeroed_slice::<c_ushort>(cell_count);
    let map  = alloc_zeroed_slice::<c_uchar>(cell_count);

    for i in 0..cell_count {
        fp.read_exact(&mut buf2).context("reading tile")?;
        unsafe { *tile.add(i) = u16::from_be_bytes(buf2); }
        fp.read_exact(&mut buf2).context("reading pass")?;
        unsafe { *pass.add(i) = u16::from_be_bytes(buf2); }
        fp.read_exact(&mut buf2).context("reading obj")?;
        unsafe { *obj.add(i)  = u16::from_be_bytes(buf2); }
    }

    slot.tile = tile;
    slot.pass = pass;
    slot.obj  = obj;
    slot.map  = map;
    Ok(())
}

/// Load MapRegistry rows for one map into its pre-allocated registry array.
/// Matches C's map_loadregistry(id): SELECT MrgIdentifier, MrgValue WHERE MrgMapId = id LIMIT 500.
pub fn load_registry(slot: &mut MapData, map_id: u32) -> Result<()> {
    #[derive(sqlx::FromRow)]
    struct RegRow { mrg_identifier: String, mrg_value: i32 }

    let rows: Vec<RegRow> = blocking_run(
        sqlx::query_as("SELECT MrgIdentifier AS mrg_identifier, MrgValue AS mrg_value \
                        FROM MapRegistry WHERE MrgMapId = ? LIMIT ?")
            .bind(map_id)
            .bind(MAX_MAPREG as u32)
            .fetch_all(get_pool())
    )?;

    slot.registry_num = rows.len() as c_int;
    for (i, row) in rows.iter().enumerate() {
        let reg = unsafe { &mut *slot.registry.add(i) };
        let bytes = row.mrg_identifier.as_bytes();
        let copy_len = bytes.len().min(63);
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr() as *const c_char,
                reg.str.as_mut_ptr(),
                copy_len,
            );
            *reg.str.as_mut_ptr().add(copy_len) = 0;
        }
        reg.val = row.mrg_value;
    }
    Ok(())
}

/// Query the Maps table and populate map slots. Called once at startup.
/// Returns the number of maps loaded, or an error.
pub fn load_maps(maps_dir: &str, server_id: i32, slots: &mut [MapData; MAP_SLOTS]) -> Result<usize> {
    #[derive(sqlx::FromRow)]
    struct MapRow {
        map_id: u32, map_name: String, map_bgm: u16, map_bgm_type: u16,
        map_pv_p: u8, map_spells: u8, map_light: u8, map_weather: u8,
        map_sweep_time: u32, map_chat: u8, map_ghosts: u8, map_region: u8,
        map_indoor: u8, map_warpout: u8, map_bind: u8, map_file: String,
        map_req_lvl: u32, map_req_path: u8, map_req_mark: u8,
        map_can_summon: u8, map_req_vita: u32, map_req_mana: u32,
        map_lvl_max: u32, map_vita_max: u32, map_mana_max: u32,
        map_reject_msg: String, map_can_use: u8, map_can_eat: u8,
        map_can_smoke: u8, map_can_mount: u8, map_can_group: u8, map_can_equip: u8,
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
             FROM Maps WHERE MapServer = ? ORDER BY MapId"
        )
        .bind(server_id)
        .fetch_all(get_pool())
    )?;

    for row in &rows {
        let id = row.map_id as usize;
        if id >= MAP_SLOTS {
            eprintln!("[map] map_id {id} >= MAP_SLOTS {MAP_SLOTS}, skipping");
            continue;
        }
        let slot = &mut slots[id];

        copy_str_to_fixed(&mut slot.title,         &row.map_name);
        copy_str_to_fixed(&mut slot.mapfile,        &row.map_file);
        copy_str_to_fixed(&mut slot.maprejectmsg,   &row.map_reject_msg);
        slot.bgm        = row.map_bgm;
        slot.bgmtype    = row.map_bgm_type;
        slot.pvp        = row.map_pv_p;
        slot.spell      = row.map_spells;
        slot.light      = row.map_light;
        slot.weather    = row.map_weather;
        slot.sweeptime  = row.map_sweep_time;
        slot.cantalk    = row.map_chat;
        slot.show_ghosts = row.map_ghosts;
        slot.region     = row.map_region;
        slot.indoor     = row.map_indoor;
        slot.warpout    = row.map_warpout;
        slot.bind       = row.map_bind;
        slot.reqlvl     = row.map_req_lvl;
        slot.reqpath    = row.map_req_path;
        slot.reqmark    = row.map_req_mark;
        slot.summon     = row.map_can_summon;
        slot.reqvita    = row.map_req_vita;
        slot.reqmana    = row.map_req_mana;
        slot.lvlmax     = row.map_lvl_max;
        slot.vitamax    = row.map_vita_max;
        slot.manamax    = row.map_mana_max;
        slot.can_use    = row.map_can_use;
        slot.can_eat    = row.map_can_eat;
        slot.can_smoke  = row.map_can_smoke;
        slot.can_mount  = row.map_can_mount;
        slot.can_group  = row.map_can_group;
        slot.can_equip  = row.map_can_equip;

        slot.registry = alloc_zeroed_registry(MAX_MAPREG);

        let path = format!("{}{}", maps_dir, row.map_file);
        parse_map_file(slot, &path)
            .with_context(|| format!("loading map id={}", row.map_id))?;

        load_registry(slot, row.map_id)?;
    }

    Ok(rows.len())
}

/// Reload map metadata and tile data in-place. Used by rust_map_reload().
/// A map is considered "already loaded" if its registry pointer is non-null.
pub fn reload_maps(maps_dir: &str, server_id: i32, slots: &mut [MapData; MAP_SLOTS]) -> Result<usize> {
    todo!("implement reload_maps — see load_maps for pattern; add free-old-tiles path")
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
}
