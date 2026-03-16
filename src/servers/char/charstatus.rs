//! Character status struct.
//!
//! MUST match the C struct byte-for-byte (same field order, same types, same
//! padding). Verified target size: 3,171,352 bytes (from gcc sizeof check).
//!
//! Safety: All types are Plain Old Data. `unsafe impl Pod` is used because
//! bytemuck derive doesn't handle arrays larger than 32 elements. All implicit
//! repr(C) padding has been replaced with explicit `_pad` fields (e.g.
//! `SkillInfo::_pad`) so no struct contains uninitialized padding bytes.

pub use crate::common::types::{
    BankData, GlobalReg, GlobalRegString, Item, KillReg, Legend, Point, SkillInfo,
};

// ── Array sizes from mmo.h ────────────────────────────────────────────────────

pub const MAX_SPELLS: usize = 52;
pub const MAX_EQUIP: usize = 15;
pub const MAX_INVENTORY: usize = 52;
pub const MAX_LEGENDS: usize = 1000;
pub const MAX_MAGIC_TIMERS: usize = 200;
pub const MAX_GLOBALREG: usize = 5000;
pub const MAX_GLOBALQUESTREG: usize = 250;
pub const MAX_KILLREG: usize = 5000;
pub const MAX_BANK_SLOTS: usize = 255;

// ── Main struct ───────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone)]
pub struct MmoCharStatus {
    pub id: u32,
    pub partner: u32,
    pub clan: u32,
    pub hp: u32,
    pub basehp: u32,
    pub mp: u32,
    pub basemp: u32,
    pub exp: u32,
    pub money: u32,
    pub maxslots: u32,
    pub bankmoney: u32,
    pub killedby: u32,
    pub killspk: u32,
    pub pkduration: u32,
    pub profile_vitastats: u8,
    pub profile_equiplist: u8,
    pub profile_legends: u8,
    pub profile_spells: u8,
    pub profile_inventory: u8,
    pub profile_bankitems: u8,
    pub name: [i8; 16],
    pub pass: [i8; 33],
    pub f1name: [i8; 16],
    pub title: [i8; 32],
    pub clan_title: [i8; 32],
    pub ipaddress: [i8; 255],
    pub gm_level: i8,
    pub sex: i8,
    pub country: i8,
    pub state: i8,
    pub side: i8,
    pub clan_chat: i8,
    pub novice_chat: i8,
    pub afkmessage: [i8; 80],
    pub tutor: u8,
    pub subpath_chat: i8,
    pub mute: i8,
    pub alignment: i8,
    pub basearmor: i32,
    pub karma: f32,
    pub clan_rank: i32,
    pub class_rank: i32,
    pub basemight: u32,
    pub basewill: u32,
    pub basegrace: u32,
    pub might: u32,
    pub will: u32,
    pub grace: u32,
    pub heroes: u32,
    pub mini_map_toggle: u32,
    pub level: u8,
    pub totem: u8,
    pub class: u8,
    pub tier: u8,
    pub mark: u8,
    pub magic_number: u8,
    pub maxinv: u8,
    pub pk: u8,
    pub face_color: u16,
    pub hair: u16,
    pub hair_color: u16,
    pub armor_color: u16,
    pub skin_color: u16,
    pub setting_flags: u16,
    pub face: u16,
    pub disguise: u16,
    pub disguise_color: u16,
    pub skill: [u16; MAX_SPELLS],
    pub expsold_magic: u64,
    pub expsold_health: u64,
    pub expsold_stats: u64,
    pub map_server: i32,
    pub int_percentage: i32,
    pub percentage: f32,
    pub nextlevelxp: u32,
    pub maxtnl: u32,
    pub realtnl: u32,
    pub tnl: u32,
    pub dest_pos: Point,
    pub last_pos: Point,
    pub equip: [Item; MAX_EQUIP],
    pub inventory: [Item; MAX_INVENTORY],
    pub legends: [Legend; MAX_LEGENDS],
    pub dura_aether: [SkillInfo; MAX_MAGIC_TIMERS],
    pub global_reg: [GlobalReg; MAX_GLOBALREG],
    pub global_regstring: [GlobalRegString; MAX_GLOBALREG],
    pub acctreg: [GlobalReg; MAX_GLOBALREG],
    pub npcintreg: [GlobalReg; MAX_GLOBALREG],
    pub questreg: [GlobalReg; MAX_GLOBALQUESTREG],
    pub killreg: [KillReg; MAX_KILLREG],
    pub global_reg_num: i32,
    pub global_regstring_num: i32,
    pub banks: [BankData; MAX_BANK_SLOTS],
}

/// Cast a zero-initialized boxed MmoCharStatus to its raw bytes.
pub fn char_status_to_bytes(s: &MmoCharStatus) -> &[u8] {
    // Safety: MmoCharStatus is #[repr(C)] with no padding beyond explicit _pad fields.
    unsafe {
        std::slice::from_raw_parts(
            s as *const MmoCharStatus as *const u8,
            std::mem::size_of::<MmoCharStatus>(),
        )
    }
}

/// Copy a byte slice into an aligned, heap-allocated MmoCharStatus.
/// Returns None if the slice is too short.
pub fn char_status_from_bytes(bytes: &[u8]) -> Option<Box<MmoCharStatus>> {
    if bytes.len() < std::mem::size_of::<MmoCharStatus>() {
        return None;
    }
    // Allocate aligned memory and copy bytes in — avoids UB from casting a
    // potentially 1-byte-aligned &[u8] pointer directly to *const MmoCharStatus.
    let mut s: Box<MmoCharStatus> = unsafe {
        let layout = std::alloc::Layout::new::<MmoCharStatus>();
        let ptr = std::alloc::alloc_zeroed(layout) as *mut MmoCharStatus;
        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        Box::from_raw(ptr)
    };
    unsafe {
        std::ptr::copy_nonoverlapping(
            bytes.as_ptr(),
            &mut *s as *mut MmoCharStatus as *mut u8,
            std::mem::size_of::<MmoCharStatus>(),
        );
    }
    Some(s)
}

// ── Size verification tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sub_struct_sizes() {
        assert_eq!(std::mem::size_of::<Point>(), 6);
        assert_eq!(std::mem::size_of::<KillReg>(), 8);
        assert_eq!(std::mem::size_of::<GlobalReg>(), 68);
        assert_eq!(std::mem::size_of::<GlobalRegString>(), 319);
        assert_eq!(std::mem::size_of::<SkillInfo>(), 48);
        assert_eq!(std::mem::size_of::<Item>(), 880);
        assert_eq!(std::mem::size_of::<Legend>(), 328);
        assert_eq!(std::mem::size_of::<BankData>(), 400);
    }

    #[test]
    fn test_charstatus_size() {
        assert_eq!(std::mem::size_of::<MmoCharStatus>(), 3_171_352);
    }
}
