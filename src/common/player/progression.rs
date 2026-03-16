/// Level, class, experience, and advancement state.
#[derive(Debug, Clone)]
pub struct PlayerProgression {
    pub level: u8,
    pub class: u8,
    pub tier: u8,
    pub mark: u8,
    pub totem: u8,
    pub country: i8,
    pub magic_number: u8,
    pub exp: u32,
    pub tnl: u32,
    pub next_level_xp: u32,
    pub max_tnl: u32,
    pub real_tnl: u32,
    pub class_rank: i32,
    pub clan_rank: i32,
    pub percentage: f32,
    pub int_percentage: i32,
    pub expsold_magic: u64,
    pub expsold_health: u64,
    pub expsold_stats: u64,
}

impl Default for PlayerProgression {
    fn default() -> Self {
        Self {
            level: 0, class: 0, tier: 0, mark: 0, totem: 0,
            country: 0, magic_number: 0,
            exp: 0, tnl: 0, next_level_xp: 0, max_tnl: 0, real_tnl: 0,
            class_rank: 0, clan_rank: 0,
            percentage: 0.0, int_percentage: 0,
            expsold_magic: 0, expsold_health: 0, expsold_stats: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_progression() {
        let p = PlayerProgression::default();
        assert_eq!(p.level, 0);
        assert_eq!(p.exp, 0);
        assert_eq!(p.percentage, 0.0);
    }
}
