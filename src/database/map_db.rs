//! Map data structures and loader.
//!
//! `MapData` mirrors `struct map_data` from `map_server.h` exactly.
//! `GlobalReg` mirrors `struct global_reg` from `mmo.h` exactly.

use std::os::raw::{c_char, c_int, c_uchar, c_uint, c_ushort};

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
