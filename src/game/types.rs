//! Shared `#[repr(C)]` game types mirroring C structs from `map_server.h`.
//!
//! `Item`      → import from `crate::servers::char::charstatus::Item`
//! `GlobalReg` → import from `crate::database::map_db::GlobalReg`
//! `GfxViewer` → defined here (first use; shared by npc, mob, pc)

use std::ffi::c_char;

/// Mirrors `struct gfxViewer` from `map_server.h`. Must be 72 bytes.
///
/// Field layout (no padding — all `u16` first, then all single-byte fields):
/// - 10 × `u16` equipment/appearance slots   (20 bytes)
/// - 10 × `u8`  color overrides for slots    (10 bytes)
/// -  7 × `u8`  hair/face/skin/dye/color     ( 7 bytes)
/// -  1 × `c_char` toggle                    ( 1 byte)
/// - 34 × `c_char` name buffer               (34 bytes)
///                                      total: 72 bytes
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
    pub toggle:      c_char,
    pub name:        [c_char; 34],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gfx_viewer_size() {
        assert_eq!(
            std::mem::size_of::<GfxViewer>(),
            72,
            "GfxViewer size mismatch — check struct gfxViewer in map_server.h"
        );
    }
}
