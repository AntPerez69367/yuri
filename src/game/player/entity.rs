#![allow(non_snake_case)]

use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicI32, AtomicU64};
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
    pub id: u32,
    pub fd: SessionId,
    // Level -1: Identity (lockless, set once at connection)
    pub name: String,
    pub pos_atomic: AtomicU64,
    pub hp_atomic: AtomicI32,
    pub mp_atomic: AtomicI32,
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

    pub fn new(id: u32, fd: SessionId, name: String, sd: Box<MapSessionData>) -> Box<Self> {
          Box::new(Self {                                                                                       
              id,
              fd,                                                                                                       
              name,
              hp_atomic: AtomicI32::new(0),
              mp_atomic: AtomicI32::new(0),
              state_flags: AtomicU64::new(0),                                                                                                  
              pos_atomic: AtomicU64::new(0),                                                                         
              net:    parking_lot::RwLock::new(PcNetworkState { look: LookAccum::default() }),
              legacy: parking_lot::RwLock::new(sd),                                      
          })  
        }

    #[inline]
    pub fn read(&self) -> impl Deref<Target = MapSessionData> + '_ {                                                   
      parking_lot::RwLockReadGuard::map(self.legacy.read(), |b| b.as_ref())                                             
  }      

    #[inline]
    pub fn write(&self) -> impl DerefMut<Target = MapSessionData> + '_ {                                                   
      parking_lot::RwLockWriteGuard::map(self.legacy.write(), |b| b.as_mut())                                             
  }      

    #[inline]
    #[deprecated(note = "Prefer domain-specific accessors, e.g. player.read().player for persistence data.")]
    pub fn data_ptr(&self) -> *mut MapSessionData {                                                                    
      unsafe { &mut **self.legacy.data_ptr() as *mut MapSessionData }                                                
    } 
}

impl LegacyEntity for PlayerEntity {
    type Data = Box<MapSessionData>;
    #[inline]
    fn read_legacy(&self) -> parking_lot::RwLockReadGuard<'_, Self::Data> {
        self.legacy.read()
    }
    #[inline]
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
