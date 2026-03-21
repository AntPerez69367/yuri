//! NPC game logic.

#![allow(non_snake_case, dead_code)]

pub mod entity;
pub mod spatial;
pub mod systems;
pub mod init;

pub mod prelude {
    // Traits
    pub use crate::common::traits::{
        Combatant, InventoryHolder, LegacyEntity, ScriptTarget, Spatial,
    };

    // Core Structs
    pub use crate::game::npc::{NpcData, NpcEntity};

    // Common type signatures
    pub use crate::common::types::{Item, Point};
}

// ── Sub-module re-exports ──────────────────────────────────────────────────

pub use entity::{
    NpcEntity, NpcData,
    NPC_ID, NPCTEMP_ID,
    npc_get_new_npcid, npc_get_new_npctempid, npc_idlower,
};

pub use spatial::{npc_warp, npc_move, npc_move_sub_id};

pub use systems::{
    npc_runtimers, npc_tick_and_dispatch,
    npc_action, npc_movetime, npc_duration,
    npc_readglobalreg, npc_setglobalreg,
};

pub use init::{
    npc_init, npc_init_async,
    warp_init, warp_init_async,
};

// ── Constant re-exports (external callers depend on these paths) ───────────

pub use crate::common::constants::entity::npc::{F1_NPC, NPCT_START_NUM, NPC_START_NUM};
pub use crate::common::constants::entity::player::MAX_GLOBALNPCREG;
pub use crate::common::constants::entity::{BL_MOB, BL_NPC, BL_PC};
