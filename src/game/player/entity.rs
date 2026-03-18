#![allow(non_snake_case, dead_code, unused_variables)]

use std::sync::atomic::{AtomicU64};
use crate::common::traits::LegacyEntity;
use crate::session::SessionId;
use super::types::MapSessionData;

/// Linked-list node for parcels/NPC posts.
#[repr(C)]
pub struct NPost {
    pub prev: *mut NPost,
    pub pos:  u32,
}

/// Tracks batched "object look" packet assembly for one viewer.
#[derive(Clone, Copy, Default)]
pub struct LookAccum {
    pub len:   i32,
    pub count: i32,
    pub item:  i32,
}

/// Network-facing session state for a connected player.
pub struct PcNetworkState {
    pub look: LookAccum,
}

/// Player entity — the top-level handle stored in PLAYER_MAP.
///
/// Level -1 fields (`id`, `fd`) are lockless — set once at connection, never mutated.
/// Level 1 fields are per-domain `RwLock`s. `legacy` holds everything not yet
/// decomposed and shrinks over time as fields migrate to proper domains.
pub struct PlayerEntity {
    // Level -1: Identity (lockless, set once at connection)
    pub id: u32,
    pub fd: SessionId,
    // Player position m: 0-15, x: 16-31, y:32-47, packed into a single atomic for lockless reads in hot code paths.
    pub pos_atomic: AtomicU64,

    // Level 1: Decomposed domains
    pub net: parking_lot::RwLock<PcNetworkState>,

    // Level 1: Legacy bucket (shrinks as domains are extracted)
    pub legacy: parking_lot::RwLock<MapSessionData>,
}

// SAFETY: PlayerEntity fields are only accessed from the single game thread.
// The RwLocks enforce correct access patterns and prepare for future multi-threading.
unsafe impl Send for PlayerEntity {}
unsafe impl Sync for PlayerEntity {}

impl PlayerEntity {
    /// Compatibility shim — delegates to legacy lock. Remove as callers migrate.
    #[inline]
    pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, MapSessionData> {
        self.legacy.read()
    }
    /// Compatibility shim — delegates to legacy lock. Remove as callers migrate.
    #[inline]
    pub fn write(&self) -> parking_lot::RwLockWriteGuard<'_, MapSessionData> {
        self.legacy.write()
    }
    /// Compatibility shim — delegates to legacy lock. Remove as callers migrate.
    #[inline]
    pub fn try_read(&self) -> Option<parking_lot::RwLockReadGuard<'_, MapSessionData>> {
        self.legacy.try_read()
    }

    /// Compatibility shim — raw pointer to legacy data. Remove as callers migrate.
    #[inline]
    pub fn data_ptr(&self) -> *mut MapSessionData {
        self.legacy.data_ptr()
    }
}

impl LegacyEntity for PlayerEntity {
    type Data = MapSessionData;
    fn read_legacy(&self) -> parking_lot::RwLockReadGuard<'_, Self::Data> {
        self.legacy.read()
    }
    fn write_legacy(&self) -> parking_lot::RwLockWriteGuard<'_, Self::Data> {
        self.legacy.write()
    }
}

/// Integer registry slot.
#[repr(C)]
pub struct ScriptReg {
    pub index: i32,
    pub data:  i32,
}

/// String registry slot.
#[repr(C)]
pub struct ScriptRegStr {
    pub index: i32,
    pub data:  [i8; 256],
}

/// Linked-list node for the player ignore list.
#[repr(C)]
pub struct SdIgnoreList {
    pub name: [i8; 100],
    pub Next: *mut SdIgnoreList,
}

// SAFETY: Single game thread with appropriate locks.
unsafe impl Send for NPost {}
unsafe impl Send for SdIgnoreList {}
