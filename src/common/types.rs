//! Canonical shared `#[repr(C)]` types that mirror C structs from `mmo.h` / `map_server.h`.
//!
//! All types here are used across multiple modules (char server, game logic, scripting).
//! Other modules should re-export from here rather than define their own copies.
//!
//! Layout invariant: every struct must exactly match its C counterpart byte-for-byte.

use serde::{Deserialize, Serialize};

// ── Point ─────────────────────────────────────────────────────────────────────

/// Map/world position (map index + tile coordinates).  6 bytes, no padding.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub m: u16,
    pub x: u16,
    pub y: u16,
}

impl Point {
    pub fn new(m: u16, x: u16, y: u16) -> Self {
        Self { m, x, y }
    }
}

unsafe impl bytemuck::Zeroable for Point {}
unsafe impl bytemuck::Pod for Point {}

// ── Item ──────────────────────────────────────────────────────────────────────

/// Live item instance (bound to a player, floor, or mob).  880 bytes.
///
/// Matches `struct item` from `mmo.h`.  The two explicit padding fields
/// (`_pad0`, `_pad1`) replace implicit compiler padding so the struct is
/// `Pod`-safe (no uninitialized bytes).
#[repr(C)]
#[derive(Copy, Clone)]
pub struct Item {
    pub id: u32,
    pub owner: u32,
    pub custom: u32,
    pub time: u32,
    pub dura: i32,
    pub amount: i32,
    pub pos: u8,
    pub _pad0: [u8; 3],
    pub custom_look: u32,
    pub custom_icon: u32,
    pub custom_look_color: u32,
    pub custom_icon_color: u32,
    pub protected: u32,
    pub traps_table: [u32; 100],
    pub buytext: [u8; 64],
    pub note: [i8; 300],
    pub repair: i8,
    pub real_name: [i8; 64],
    pub _pad1: [u8; 3],
}

unsafe impl bytemuck::Zeroable for Item {}
unsafe impl bytemuck::Pod for Item {}

// ── BankData ──────────────────────────────────────────────────────────────────

/// Bank slot item.  400 bytes.  Matches `struct bank_data` from `mmo.h`.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct BankData {
    pub item_id: u32,
    pub amount: u32,
    pub owner: u32,
    pub time: u32,
    pub custom_icon: u32,
    pub custom_look: u32,
    pub real_name: [i8; 64],
    pub custom_look_color: u32,
    pub custom_icon_color: u32,
    pub protected: u32,
    pub note: [i8; 300],
}

unsafe impl bytemuck::Zeroable for BankData {}
unsafe impl bytemuck::Pod for BankData {}

// ── GlobalReg ─────────────────────────────────────────────────────────────────

/// Player/NPC/mob registry entry (string key + integer value).  68 bytes.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct GlobalReg {
    pub str: [i8; 64],
    pub val: i32,
}

unsafe impl bytemuck::Zeroable for GlobalReg {}
unsafe impl bytemuck::Pod for GlobalReg {}

// ── GfxViewer ─────────────────────────────────────────────────────────────────

/// Visual appearance snapshot shared by mob, npc, and player entities.  72 bytes.
///
/// Field layout (no padding — all `u16` first, then all single-byte fields):
/// - 10 × `u16` equipment/appearance slots   (20 bytes)
/// - 10 × `u8`  color overrides for slots    (10 bytes)
/// -  7 × `u8`  hair/face/skin/dye/color     ( 7 bytes)
/// -  1 × `i8`  toggle                       ( 1 byte)
/// - 34 × `i8`  name buffer                  (34 bytes)
/// - ----------------------------------- total: 72 bytes
#[repr(C)]
#[derive(Copy, Clone)]
pub struct GfxViewer {
    pub weapon:      u16,
    pub armor:       u16,
    pub helm:        u16,
    pub face_acc:    u16,
    pub crown:       u16,
    pub shield:      u16,
    pub necklace:    u16,
    pub mantle:      u16,
    pub boots:       u16,
    pub face_acc_t:  u16,
    pub cweapon:     u8,
    pub carmor:      u8,
    pub chelm:       u8,
    pub cface_acc:   u8,
    pub ccrown:      u8,
    pub cshield:     u8,
    pub cnecklace:   u8,
    pub cmantle:     u8,
    pub cboots:      u8,
    pub cface_acc_t: u8,
    pub hair:        u8,
    pub chair:       u8,
    pub face:        u8,
    pub cface:       u8,
    pub cskin:       u8,
    pub dye:         u8,
    pub title_color: u8,
    pub toggle:      i8,
    pub name:        [i8; 34],
}

// ── Size assertions ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_size()      { assert_eq!(std::mem::size_of::<Point>(),     6); }
    #[test]
    fn item_size()       { assert_eq!(std::mem::size_of::<Item>(),    880); }
    #[test]
    fn bank_data_size()  { assert_eq!(std::mem::size_of::<BankData>(), 400); }
    #[test]
    fn global_reg_size() { assert_eq!(std::mem::size_of::<GlobalReg>(), 68); }
    #[test]
    fn gfx_viewer_size() { assert_eq!(std::mem::size_of::<GfxViewer>(), 72); }
}
