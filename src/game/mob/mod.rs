//! Mob game logic.

#![allow(non_snake_case, dead_code)]

pub mod entity;
pub mod spatial;
pub mod systems;
pub mod combat;
pub mod items;
pub mod init;

// ── Sub-module re-exports ──────────────────────────────────────────────────

pub use entity::{
    MobEntity, MobSpawnData, ThreatTable,
    MOB_ID, MAX_NORMAL_ID, CMOB_ID,
    MOB_SPAWN_MAX, MOB_SPAWN_START, MOB_ONETIME_MAX, MOB_ONETIME_START,
    MIN_TIMER,
    mob_get_new_id, mob_get_free_id, free_onetime,
};

pub use spatial::{
    mob_warp, move_mob, move_mob_ignore_object, moveghost_mob,
    mob_move2, move_mob_intent, mob_move_inner_id, sl_mob_checkmove,
};

pub use systems::{
    mob_duratimer, mob_secondduratimer, mob_thirdduratimer, mob_fourthduratimer,
    mob_flushmagic, mob_calcstat, mob_handle_sub, mob_timer_spawns,
    mob_readglobalreg, mob_setglobalreg, mob_trap_look_inner,
    mob_respawn_getstats,
};

pub use combat::{
    mob_attack, mob_calc_critical, mob_find_target_inner,
    sl_mob_checkthreat, sl_mob_setinddmg, sl_mob_setgrpdmg, sl_mob_callbase,
    sl_mob_addhealth, sl_mob_removehealth,
    sl_mob_setduration, sl_mob_flushduration, sl_mob_flushdurationnouncast,
    kill_mob,
};

pub use items::{
    mob_dropitem, mobdb_drops, mob_addtocurrent_inner, mob_thing_yeah_inner, DropItemSpec,
};

pub use init::{
    mobspawn_read, mobspawn_onetime, mob_respawn, mob_respawn_nousers,
    mobspawn2_read, mobspeech_read, SpawnConfig,
};

// ── Constant re-exports (external callers depend on these paths) ───────────

pub use crate::common::constants::entity::item::FLOORITEM_START_NUM;
pub use crate::common::constants::entity::mob::{
    MAX_GLOBALMOBREG, MAX_INVENTORY, MAX_MAGIC_TIMERS, MAX_THREATCOUNT,
    MOBOT_START_NUM, MOB_START_NUM,
    MOB_ALIVE, MOB_BLIND, MOB_DEAD, MOB_ESCAPE, MOB_HIT, MOB_PARA,
};
pub use crate::common::constants::entity::npc::NPC_START_NUM;
pub use crate::common::constants::entity::{BL_ITEM, BL_MOB, BL_NPC, BL_PC};
