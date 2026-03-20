use std::fmt::{Display, Formatter, Result as FmtResult};

#[derive(Debug, Clone, Copy)]
pub enum EntityType {
    Player,
    Mob,
    Npc,
    Item,
}

impl Display for EntityType {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            EntityType::Player => write!(f, "Player"),
            EntityType::Mob => write!(f, "Mob"),
            EntityType::Npc => write!(f, "NPC"),
            EntityType::Item => write!(f, "Item"),
        }
    }
}