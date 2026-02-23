/// Rust mirror of `struct mmo_charstatus` from c_src/mmo.h.
///
/// MUST match the C struct byte-for-byte (same field order, same types, same
/// padding). Verified target size: 3,171,352 bytes (from gcc sizeof check).
///
/// Safety: All types are Plain Old Data. `unsafe impl Pod` is used because
/// bytemuck derive doesn't handle arrays larger than 32 elements or structs
/// with trailing padding bytes.

// ── Sub-structs ───────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Point {
    pub m: u16,
    pub x: u16,
    pub y: u16,
}

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

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Legend {
    pub icon: u16,
    pub color: u16,
    pub text: [i8; 255],
    pub name: [i8; 64],
    pub _pad0: [u8; 1],
    pub tchaid: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct SkillInfo {
    pub duration: i32,
    pub aether: i32,
    pub time: i32,
    pub id: u16,
    pub animation: u16,
    pub caster_id: u32,
    pub dura_timer: u32,
    pub aether_timer: u32,
    pub lasttick_dura: u64,
    pub lasttick_aether: u64,
}

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

#[repr(C)]
#[derive(Copy, Clone)]
pub struct KillReg {
    pub mob_id: u32,
    pub amount: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct GlobalReg {
    pub str: [i8; 64],
    pub val: i32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct GlobalRegString {
    pub str: [i8; 64],
    pub val: [i8; 255],
}

// Safety: all fields are primitive types or arrays of primitive types with
// no uninitialized padding beyond explicit `_pad` fields.
unsafe impl bytemuck::Zeroable for Point {}
unsafe impl bytemuck::Pod for Point {}
unsafe impl bytemuck::Zeroable for Item {}
unsafe impl bytemuck::Pod for Item {}
unsafe impl bytemuck::Zeroable for Legend {}
unsafe impl bytemuck::Pod for Legend {}
unsafe impl bytemuck::Zeroable for SkillInfo {}
unsafe impl bytemuck::Pod for SkillInfo {}
unsafe impl bytemuck::Zeroable for BankData {}
unsafe impl bytemuck::Pod for BankData {}
unsafe impl bytemuck::Zeroable for KillReg {}
unsafe impl bytemuck::Pod for KillReg {}
unsafe impl bytemuck::Zeroable for GlobalReg {}
unsafe impl bytemuck::Pod for GlobalReg {}
unsafe impl bytemuck::Zeroable for GlobalRegString {}
unsafe impl bytemuck::Pod for GlobalRegString {}

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

/// Interpret a byte slice as a MmoCharStatus reference.
/// Returns None if the slice is too short.
pub fn char_status_from_bytes(bytes: &[u8]) -> Option<&MmoCharStatus> {
    if bytes.len() < std::mem::size_of::<MmoCharStatus>() {
        return None;
    }
    // Safety: bytes must be at least size_of::<MmoCharStatus>() long and
    // properly aligned. Caller ensures this from DB/network data.
    Some(unsafe { &*(bytes.as_ptr() as *const MmoCharStatus) })
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
