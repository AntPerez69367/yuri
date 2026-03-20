use serde::{Serialize, Deserialize};

/// Clan, PK, karma, and chat state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSocial {
    pub partner: u32,
    pub partner_name: String,
    pub clan: u32,
    pub clan_title: String,
    pub clan_chat: i8,
    pub pk: u8,
    pub killed_by: u32,
    pub kills_pk: u32,
    pub pk_duration: u32,
    pub karma: f32,
    pub alignment: i8,
    pub novice_chat: i8,
    pub subpath_chat: i8,
    pub mute: i8,
    pub tutor: u8,
    pub afk_message: String,
}

impl Default for PlayerSocial {
    fn default() -> Self {
        Self {
            partner: 0, partner_name: String::new(), clan: 0,
            clan_title: String::new(), clan_chat: 0,
            pk: 0, killed_by: 0, kills_pk: 0, pk_duration: 0,
            karma: 0.0, alignment: 0,
            novice_chat: 0, subpath_chat: 0, mute: 0, tutor: 0,
            afk_message: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_social() {
        let s = PlayerSocial::default();
        assert_eq!(s.clan, 0);
        assert!(s.clan_title.is_empty());
        assert_eq!(s.karma, 0.0);
    }
}
