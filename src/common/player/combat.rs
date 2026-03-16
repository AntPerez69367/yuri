/// Combat state — hot path, small. Contains hp/mp, stats, and entity state.
#[derive(Debug, Clone, Default)]
pub struct PlayerCombat {
    pub hp: u32,
    pub max_hp: u32,
    pub mp: u32,
    pub max_mp: u32,
    pub might: u32,
    pub will: u32,
    pub grace: u32,
    pub base_might: u32,
    pub base_will: u32,
    pub base_grace: u32,
    pub base_armor: i32,
    pub state: i8,
    pub side: i8,
}

impl PlayerCombat {
    /// Returns true if the entity is dead (state < 0).
    pub fn is_alive(&self) -> bool {
        self.state >= 0
    }

    /// Apply damage, clamping HP to 0. Returns true if this killed the entity.
    pub fn apply_damage(&mut self, amount: u32) -> bool {
        let was_alive = self.is_alive();
        self.hp = self.hp.saturating_sub(amount);
        was_alive && self.hp == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_zeroed() {
        let c = PlayerCombat::default();
        assert_eq!(c.hp, 0);
        assert_eq!(c.state, 0);
    }

    #[test]
    fn is_alive_checks_state() {
        let mut c = PlayerCombat::default();
        assert!(c.is_alive());
        c.state = -1;
        assert!(!c.is_alive());
    }

    #[test]
    fn apply_damage_returns_killed() {
        let mut c = PlayerCombat { hp: 10, max_hp: 100, ..Default::default() };
        assert!(!c.apply_damage(5));
        assert_eq!(c.hp, 5);
        assert!(c.apply_damage(5));
        assert_eq!(c.hp, 0);
    }

    #[test]
    fn apply_damage_saturates() {
        let mut c = PlayerCombat { hp: 3, max_hp: 100, ..Default::default() };
        assert!(c.apply_damage(100));
        assert_eq!(c.hp, 0);
    }

    #[test]
    fn apply_damage_when_already_dead() {
        let mut c = PlayerCombat { hp: 0, max_hp: 100, state: -1, ..Default::default() };
        assert!(!c.apply_damage(50));
        assert_eq!(c.hp, 0);
    }
}
