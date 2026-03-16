/// Visual appearance and UI settings. Small, mostly client-sync.
#[derive(Debug, Clone, Default)]
pub struct PlayerAppearance {
    pub face: u16,
    pub hair: u16,
    pub face_color: u16,
    pub hair_color: u16,
    pub armor_color: u16,
    pub skin_color: u16,
    pub disguise: u16,
    pub disguise_color: u16,
    pub setting_flags: u16,
    pub heroes: u32,
    pub mini_map_toggle: u32,
    pub profile_vitastats: u8,
    pub profile_equiplist: u8,
    pub profile_legends: u8,
    pub profile_spells: u8,
    pub profile_inventory: u8,
    pub profile_bankitems: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_appearance() {
        let a = PlayerAppearance::default();
        assert_eq!(a.face, 0);
        assert_eq!(a.setting_flags, 0);
    }
}
