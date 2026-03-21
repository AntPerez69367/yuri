//! NPC timer and registry systems.

use std::sync::atomic::Ordering;

use super::entity::{NpcData, NpcEntity, NPC_ID, NPCTEMP_ID};
use crate::common::traits::LegacyEntity;
use crate::common::constants::entity::npc::{NPC_START_NUM, NPCT_START_NUM};
use crate::game::lua::dispatch::dispatch;

/// Dispatch a Lua event with a single entity ID argument.
fn sl_doscript_simple(root: &str, method: Option<&str>, id: u32) -> bool {
    dispatch(root, method, &[id])
}

/// Dispatch a Lua event with two entity ID arguments.
fn sl_doscript_2(root: &str, method: Option<&str>, id1: u32, id2: u32) -> bool {
    dispatch(root, method, &[id1, id2])
}

// ---------------------------------------------------------------------------
// Registries
// ---------------------------------------------------------------------------

/// Reads an NPC's global registry value for the key `reg`.
///
/// Walks `nd.registry` looking for a case-insensitive match on the key
/// string.  Returns the stored `val` if found, or `0` if the key is absent.
///
/// # Safety
///
/// `nd` must be a valid, aligned, non-null pointer to an `NpcData`.
/// `reg` must be a valid null-terminated C string.
pub unsafe fn npc_readglobalreg(nd: *mut NpcData, reg: *const i8) -> i32 {
    let nd = &*nd;
    let reg_key = std::ffi::CStr::from_ptr(reg).to_bytes();
    for entry in &nd.registry {
        if entry.str[0] != 0
            && std::ffi::CStr::from_ptr(entry.str.as_ptr())
                .to_bytes()
                .eq_ignore_ascii_case(reg_key)
        {
            return entry.val;
        }
    }
    0
}

/// Sets an NPC's global registry value for key `reg` to `val`.
///
/// If the key already exists it is updated in place; setting `val` to `0`
/// additionally clears the key slot so it can be reused.  If no existing
/// entry is found, the first empty slot is used.
///
/// Returns `0` on success, `1` if the registry is full.
///
/// # Safety
///
/// `nd` must be a valid, aligned, non-null pointer to an `NpcData`.
/// `reg` must be a valid null-terminated C string whose length (including
/// the NUL) fits within 64 bytes.
pub unsafe fn npc_setglobalreg(nd: *mut NpcData, reg: *const i8, val: i32) -> i32 {
    let nd = &mut *nd;
    let reg_cstr = std::ffi::CStr::from_ptr(reg);
    let reg_key = reg_cstr.to_bytes();

    // Update existing entry if present.
    for entry in nd.registry.iter_mut() {
        if entry.str[0] != 0
            && std::ffi::CStr::from_ptr(entry.str.as_ptr())
                .to_bytes()
                .eq_ignore_ascii_case(reg_key)
        {
            if val == 0 {
                entry.str[0] = 0; // clear key — slot is now free
            }
            entry.val = val;
            return 0;
        }
    }

    // Allocate a new slot.
    for entry in nd.registry.iter_mut() {
        if entry.str[0] == 0 {
            let bytes = reg_cstr.to_bytes();
            let copy_len = bytes.len().min(entry.str.len() - 1);
            for (dst, &src) in entry.str.iter_mut().zip(bytes[..copy_len].iter()) {
                *dst = src as i8;
            }
            entry.str[copy_len] = 0;
            entry.val = val;
            return 0;
        }
    }

    tracing::error!(
        "npc_setglobalreg: registry full, could not set {:?}",
        reg_cstr
    );
    1
}

// ---------------------------------------------------------------------------
// Timers
// ---------------------------------------------------------------------------

/// Advances the action timer for `nd` by 100 ms and fires the `"action"` Lua
/// event when the timer reaches `nd.actiontime`.
///
///
// TODO: dead code — npc_tick_and_dispatch replaced these three functions
/// # Safety
///
/// `nd` must be a valid, aligned, non-null pointer to a live `NpcData`.
/// Caller must hold the server-wide lock.
pub unsafe fn npc_action(nd: *mut NpcData) -> i32 {
    if nd.is_null() {
        return 0;
    }
    let nd = &mut *nd;

    nd.time = nd.time.wrapping_add(100);

    let name = crate::game::scripting::carray_to_str(&nd.name);

    if nd.time >= nd.actiontime {
        nd.time = 0;
        if nd.owner != 0 {
            sl_doscript_2(name, Some("action"), nd.id, nd.owner);
        } else {
            sl_doscript_simple(name, Some("action"), nd.id);
        }
    }
    0
}

/// Advances the move timer for `nd` by 100 ms and fires the `"move"` Lua event
/// when `nd.movetimer` reaches `nd.movetime`.
///
///
/// # Safety
///
/// Same requirements as [`npc_action`].
pub unsafe fn npc_movetime(nd: *mut NpcData) -> i32 {
    if nd.is_null() {
        return 0;
    }
    let nd = &mut *nd;

    nd.movetimer = nd.movetimer.wrapping_add(100);

    let name = crate::game::scripting::carray_to_str(&nd.name);

    if nd.movetimer >= nd.movetime {
        nd.movetimer = 0;
        if nd.owner != 0 {
            sl_doscript_2(name, Some("move"), nd.id, nd.owner);
        } else {
            sl_doscript_simple(name, Some("move"), nd.id);
        }
    }
    0
}

/// Advances the duration timer for `nd` by 100 ms and fires the `"endAction"`
/// Lua event when `nd.duratime` reaches `nd.duration`.
///
///
/// # Safety
///
/// Same requirements as [`npc_action`].
pub unsafe fn npc_duration(nd: *mut NpcData) -> i32 {
    if nd.is_null() {
        return 0;
    }
    let nd = &mut *nd;

    nd.duratime = nd.duratime.wrapping_add(100);

    let name = crate::game::scripting::carray_to_str(&nd.name);

    if nd.duratime >= nd.duration {
        nd.duratime = 0;
        if nd.owner != 0 {
            sl_doscript_2(name, Some("endAction"), nd.id, nd.owner);
        } else {
            sl_doscript_simple(name, Some("endAction"), nd.id);
        }
    }
    0
}

/// Advance all timers for one NPC and dispatch any pending Lua events.
///
/// The write lock is scoped tightly around timer mutation only. Lua dispatch
/// happens after the lock is dropped to satisfy the "No lock across Lua" rule
/// (holding an NpcEntity write guard across `sl_doscript_*` causes a
/// self-deadlock when the Lua method reads the same NpcEntity).
///
/// # Safety
///
/// Caller must hold the server-wide lock.
pub unsafe fn npc_tick_and_dispatch(entity: &NpcEntity) {
    type Ev = Option<(u32, u32)>; // (id, owner)

    let (action_ev, move_ev, dur_ev): (Ev, Ev, Ev) = {
        let nd = &mut *entity.write();

        let action_ev = if nd.actiontime > 0 {
            nd.time = nd.time.wrapping_add(100);
            if nd.time >= nd.actiontime {
                nd.time = 0;
                Some((nd.id, nd.owner))
            } else {
                None
            }
        } else {
            None
        };

        let move_ev = if nd.movetime > 0 {
            nd.movetimer = nd.movetimer.wrapping_add(100);
            if nd.movetimer >= nd.movetime {
                nd.movetimer = 0;
                Some((nd.id, nd.owner))
            } else {
                None
            }
        } else {
            None
        };

        let dur_ev = if nd.duration > 0 {
            nd.duratime = nd.duratime.wrapping_add(100);
            if nd.duratime >= nd.duration {
                nd.duratime = 0;
                Some((nd.id, nd.owner))
            } else {
                None
            }
        } else {
            None
        };

        (action_ev, move_ev, dur_ev)
    }; // write lock dropped here — safe to call Lua below

    let name = &entity.name;

    if let Some((id, owner)) = action_ev {
        if owner != 0 {
            sl_doscript_2(name, Some("action"), id, owner);
        } else {
            sl_doscript_simple(name, Some("action"), id);
        }
    }
    if let Some((id, owner)) = move_ev {
        if owner != 0 {
            sl_doscript_2(name, Some("move"), id, owner);
        } else {
            sl_doscript_simple(name, Some("move"), id);
        }
    }
    if let Some((id, owner)) = dur_ev {
        if owner != 0 {
            sl_doscript_2(name, Some("endAction"), id, owner);
        } else {
            sl_doscript_simple(name, Some("endAction"), id);
        }
    }
}

/// Timer callback fired every 100 ms by the map-server timer wheel.
///
/// Iterates all regular NPCs (IDs `NPC_START_NUM..=NPC_ID`) and all temp NPCs
/// (`NPCT_START_NUM..=NPCTEMP_ID`).  For each non-null NPC it dispatches
/// `npc_action`, `npc_movetime`, and `npc_duration` as appropriate based on
/// their configured intervals.
///
///
/// # Safety
///
/// Caller must hold the server-wide lock.  `map_id2npc` must be safe to call
/// for any ID in the scanned ranges.
pub unsafe fn npc_runtimers() {
    // regular NPCs
    let mut x = NPC_START_NUM;
    let npc_hi = NPC_ID.load(Ordering::Relaxed);
    while x <= npc_hi {
        if let Some(arc) = crate::game::map_server::map_id2npc_ref(x) {
            npc_tick_and_dispatch(&arc);
        }
        x += 1;
    }

    // temp NPCs
    let mut x = NPCT_START_NUM;
    let npct_hi = NPCTEMP_ID.load(Ordering::Relaxed);
    while x <= npct_hi {
        if let Some(arc) = crate::game::map_server::map_id2npc_ref(x) {
            npc_tick_and_dispatch(&arc);
        }
        x += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globalreg_read_missing_returns_zero() {
        let mut nd = unsafe { Box::<NpcData>::new_zeroed().assume_init() };
        assert_eq!(
            unsafe { npc_readglobalreg(&raw mut *nd, b"nosuchkey\0".as_ptr() as _) },
            0
        );
    }

    #[test]
    fn globalreg_set_then_read() {
        let mut nd = unsafe { Box::<NpcData>::new_zeroed().assume_init() };
        let key = b"mykey\0".as_ptr() as *const i8;
        unsafe {
            assert_eq!(npc_setglobalreg(&raw mut *nd, key, 42), 0);
            assert_eq!(npc_readglobalreg(&raw mut *nd, key), 42);
        }
    }

    #[test]
    fn globalreg_set_zero_clears_key() {
        let mut nd = unsafe { Box::<NpcData>::new_zeroed().assume_init() };
        let key = b"mykey\0".as_ptr() as *const i8;
        unsafe {
            npc_setglobalreg(&raw mut *nd, key, 99);
            npc_setglobalreg(&raw mut *nd, key, 0);
            // After setting to 0, key should be cleared — re-reading returns 0
            assert_eq!(npc_readglobalreg(&raw mut *nd, key), 0);
        }
    }
}
