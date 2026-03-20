pub mod player;
pub mod mob;
pub mod npc;
pub mod item;
pub mod types;

pub mod prelude {
    pub use crate::game::lua::entity::player::*;
    pub use crate::game::lua::entity::npc::*;
    pub use crate::game::lua::entity::mob::*;
    pub use crate::game::lua::entity::item::*;
    pub use crate::game::lua::entity::types::*;
}