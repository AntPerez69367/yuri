#![allow(non_snake_case)]

use super::types::MapSessionData;
use crate::common::traits::LegacyEntity;
use crate::session::SessionId;
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};

/// Linked-list node for parcels/NPC posts.
#[repr(C)]
pub struct NPost {
    pub prev: *mut NPost,
    pub pos: u32,
}

/// Tracks batched "object look" packet assembly for one viewer.
#[derive(Clone, Copy, Default)]
pub struct LookAccum {
    pub len: i32,
    pub count: i32,
    pub item: i32,
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
    pub id: u32,
    pub fd: SessionId,
    // Level -1: Identity (lockless, set once at connection)
    pub name: String,
    pub gm_level: i8,
    pub pos_atomic: AtomicU64,
    pub hp_atomic: AtomicI32,
    pub mp_atomic: AtomicI32,
    pub exp_atomic: AtomicU32,
    /// Packed class (high byte) | level (low byte).
    pub class_level_atomic: AtomicU32,
    pub state_flags: AtomicU64,

    // Level 1: Decomposed domains
    pub net: parking_lot::RwLock<PcNetworkState>,

    // Level 1: Legacy bucket (shrinks as domains are extracted)
    pub legacy: parking_lot::RwLock<Box<MapSessionData>>,
}

// SAFETY: PlayerEntity fields are only accessed from the single game thread.
// The RwLocks enforce correct access patterns and prepare for future multi-threading.
unsafe impl Send for PlayerEntity {}
unsafe impl Sync for PlayerEntity {}

impl PlayerEntity {
    pub fn new(
        id: u32,
        fd: SessionId,
        name: String,
        gm_level: i8,
        sd: Box<MapSessionData>,
    ) -> Box<Self> {
        let exp = sd.player.progression.exp;
        let class = sd.player.progression.class;
        let level = sd.player.progression.level;
        Box::new(Self {
            id,
            fd,
            name,
            gm_level,
            hp_atomic: AtomicI32::new(0),
            mp_atomic: AtomicI32::new(0),
            exp_atomic: AtomicU32::new(exp),
            class_level_atomic: AtomicU32::new((class as u32) << 8 | level as u32),
            state_flags: AtomicU64::new(0),
            pos_atomic: AtomicU64::new(0),
            net: parking_lot::RwLock::new(PcNetworkState {
                look: LookAccum::default(),
            }),
            legacy: parking_lot::RwLock::new(sd),
        })
    }

    #[inline]
    pub fn position(&self) -> crate::config::Point {
        crate::config::Point::from_u64(self.pos_atomic.load(Ordering::Relaxed))
    }

    #[inline]
    pub fn exp(&self) -> u32 {
        self.exp_atomic.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn set_exp(&self, val: u32) {
        self.exp_atomic.store(val, Ordering::Relaxed);
    }

    #[inline]
    pub fn class(&self) -> u8 {
        (self.class_level_atomic.load(Ordering::Relaxed) >> 8) as u8
    }

    #[inline]
    pub fn level(&self) -> u8 {
        self.class_level_atomic.load(Ordering::Relaxed) as u8
    }

    #[inline]
    pub fn set_class_level(&self, class: u8, level: u8) {
        self.class_level_atomic
            .store((class as u32) << 8 | level as u32, Ordering::Relaxed);
    }
}

impl LegacyEntity for PlayerEntity {
    type Data = MapSessionData;
    #[inline]
    fn read(&self) -> parking_lot::MappedRwLockReadGuard<'_, Self::Data> {
        parking_lot::RwLockReadGuard::map(self.legacy.read(), |b| b.as_ref())
    }
    #[inline]
    fn write(&self) -> parking_lot::MappedRwLockWriteGuard<'_, Self::Data> {
        parking_lot::RwLockWriteGuard::map(self.legacy.write(), |b| b.as_mut())
    }
}

/// Integer registry slot.
#[repr(C)]
pub struct ScriptReg {
    pub index: i32,
    pub data: i32,
}

/// String registry slot.
#[repr(C)]
pub struct ScriptRegStr {
    pub index: i32,
    pub data: [i8; 256],
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
