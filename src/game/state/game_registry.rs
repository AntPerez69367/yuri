//! Game-global and per-map registry — key/value integer stores backed by DB.

use std::sync::{Mutex, OnceLock};

use crate::common::constants::world::MAX_GAMEREG;
use crate::database::get_pool;

// ---------------------------------------------------------------------------
// GameData — game-global registry
// ---------------------------------------------------------------------------

/// Game-global registry entry.
///
/// ```c
/// struct game_data {
///     struct global_reg *registry;
///     int registry_num;
/// };
/// ```
#[repr(C)]
pub struct GameData {
    pub registry: *mut crate::database::map_db::GlobalReg,
    pub registry_num: i32,
}

// SAFETY: `gamereg` is only accessed on the single-threaded game loop.
// No Rust code takes shared references to it across threads.
unsafe impl Send for GameData {}
unsafe impl Sync for GameData {}

impl std::fmt::Debug for GameData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameData").finish_non_exhaustive()
    }
}

/// The game-wide registry global.
///
/// Populated by `map_loadgameregistry` and mutated by `map_setglobalgamereg`.
static GAMEREG: OnceLock<Mutex<GameData>> = OnceLock::new();

#[inline]
pub fn gamereg() -> std::sync::MutexGuard<'static, GameData> {
    GAMEREG
        .get_or_init(|| {
            Mutex::new(GameData {
                registry: std::ptr::null_mut(),
                registry_num: 0,
            })
        })
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------------------
// Allocation helpers
// ---------------------------------------------------------------------------

/// Allocate a zeroed array of `GlobalReg` entries of the given length via the
/// global allocator.  The caller is responsible for freeing via the same allocator.
fn alloc_zeroed_gamereg_registry(len: usize) -> *mut crate::database::map_db::GlobalReg {
    use crate::database::map_db::GlobalReg;
    let layout = std::alloc::Layout::array::<GlobalReg>(len).expect("GlobalReg layout overflow");
    let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
    if ptr.is_null() {
        std::alloc::handle_alloc_error(layout);
    }
    ptr as *mut GlobalReg
}

// ---------------------------------------------------------------------------
// String helpers for registry [i8; 64] fields
// ---------------------------------------------------------------------------

/// ASCII case-insensitive comparison of a `GlobalReg.str` field against a C string.
///
/// Returns `true` if the two null-terminated byte sequences are equal ignoring ASCII case.
/// Equivalent to `strcasecmp` used in the C registry search loops.
pub(crate) unsafe fn reg_str_eq(arr: &[i8; 64], cstr: *const i8) -> bool {
    if cstr.is_null() {
        return false;
    }
    for (i, arr_byte) in arr.iter().enumerate().take(64) {
        let a = *arr_byte as u8;
        let b = *cstr.add(i) as u8;
        if !a.eq_ignore_ascii_case(&b) {
            return false;
        }
        if a == 0 {
            return true; // both null-terminated at the same position
        }
    }
    false
}

/// Copy a C string into a `[i8; 64]` field, null-terminating. Truncates at 63 chars.
unsafe fn copy_cstr_to_reg_str(dest: &mut [i8; 64], src: *const i8) {
    let mut i = 0usize;
    while i < 63 {
        let b = *src.add(i);
        dest[i] = b;
        if b == 0 {
            return;
        }
        i += 1;
    }
    dest[63] = 0; // ensure null termination
}

// ---------------------------------------------------------------------------
// map_readglobalgamereg — read a game-global registry value by name.
// ---------------------------------------------------------------------------

/// Read a game-global registry value by name (case-insensitive).
pub fn map_readglobalgamereg(reg: &str) -> i32 {
    let gr = gamereg();
    if gr.registry.is_null() {
        return 0;
    }
    let ckey = std::ffi::CString::new(reg).unwrap_or_default();
    for i in 0..gr.registry_num as usize {
        let entry = unsafe { &*gr.registry.add(i) };
        if unsafe { reg_str_eq(&entry.str, ckey.as_ptr()) } {
            return entry.val;
        }
    }
    0
}

// ---------------------------------------------------------------------------
// map_registrysave — persist one map registry slot to the `MapRegistry` table.
// ---------------------------------------------------------------------------

/// Persist one map registry slot at index `i` on map `m` to the `MapRegistry` table.
///
/// # Safety
/// `crate::database::map_db::raw_map_ptr()` must be a valid initialised pointer.  `m` must be a
/// loaded map index and `i` must be within `[0, MAX_MAPREG)`.
pub async unsafe fn map_registrysave(m: i32, i: i32) -> i32 {
    use crate::database::map_db::{GlobalReg, MAP_SLOTS, MAX_MAPREG};

    if m < 0 || m as usize >= MAP_SLOTS {
        return 0;
    }
    if i < 0 || i as usize >= MAX_MAPREG {
        return 0;
    }

    let (identifier, val, m_u32, i_u32) = {
        let slot = &mut *crate::database::map_db::raw_map_ptr().add(m as usize);
        if slot.registry.is_null() {
            return 0;
        }

        let p: &GlobalReg = &*slot.registry.add(i as usize);

        let identifier = {
            let bytes: &[u8] = std::slice::from_raw_parts(p.str.as_ptr() as *const u8, 64);
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(64);
            String::from_utf8_lossy(&bytes[..end]).into_owned()
        };
        let val = p.val;
        (identifier, val, m as u32, i as u32)
    };

    let save_id: Option<u32> = sqlx::query_scalar::<_, u32>(
        "SELECT MrgPosition FROM MapRegistry \
             WHERE MrgMapId = ? AND MrgIdentifier = ?",
    )
    .bind(m_u32)
    .bind(identifier.clone())
    .fetch_optional(get_pool())
    .await
    .unwrap_or(None);

    match save_id {
        Some(pos) => {
            if val == 0 {
                let _ = sqlx::query(
                    "DELETE FROM MapRegistry \
                         WHERE MrgMapId = ? AND MrgIdentifier = ?",
                )
                .bind(m_u32)
                .bind(identifier.clone())
                .execute(get_pool())
                .await;
            } else {
                let _ = sqlx::query(
                    "UPDATE MapRegistry SET MrgIdentifier = ?, MrgValue = ? \
                         WHERE MrgMapId = ? AND MrgPosition = ?",
                )
                .bind(identifier.clone())
                .bind(val)
                .bind(m_u32)
                .bind(pos)
                .execute(get_pool())
                .await;
            }
        }
        None => {
            if val > 0 {
                let _ = sqlx::query(
                    "INSERT INTO MapRegistry \
                         (MrgMapId, MrgIdentifier, MrgValue, MrgPosition) \
                         VALUES (?, ?, ?, ?)",
                )
                .bind(m_u32)
                .bind(identifier)
                .bind(val)
                .bind(i_u32)
                .execute(get_pool())
                .await;
            }
        }
    }

    0
}

// ---------------------------------------------------------------------------
// map_setglobalreg — set a map-level registry key/value in memory and persist.
// ---------------------------------------------------------------------------

/// Set a key/value pair in the per-map registry for map `m`, then persist to DB.
///
/// # Safety
/// `crate::database::map_db::raw_map_ptr()` must be a valid initialised pointer.  `m` must be within
/// `[0, MAP_SLOTS)`.  `reg` must be a valid non-null null-terminated C string.
pub async unsafe fn map_setglobalreg(m: i32, reg: *const i8, val: i32) -> i32 {
    use crate::database::map_db::MAP_SLOTS;

    if reg.is_null() {
        return 0;
    }
    if m < 0 || m as usize >= MAP_SLOTS {
        return 0;
    }

    let save_info: Option<(i32, bool)> = {
        let slot = &mut *crate::database::map_db::raw_map_ptr().add(m as usize);
        if slot.registry.is_null() {
            return 0;
        }
        let num = slot.registry_num as usize;

        let mut exist: Option<usize> = None;
        for idx in 0..num {
            let entry = &*slot.registry.add(idx);
            if reg_str_eq(&entry.str, reg) {
                exist = Some(idx);
                break;
            }
        }

        if let Some(idx) = exist {
            let entry = &mut *slot.registry.add(idx);
            entry.val = val;
            Some((idx as i32, val == 0))
        } else {
            let mut reuse_idx: Option<usize> = None;
            for idx in 0..num {
                let entry = &*slot.registry.add(idx);
                if entry.str[0] == 0 {
                    reuse_idx = Some(idx);
                    break;
                }
            }
            if let Some(idx) = reuse_idx {
                let entry = &mut *slot.registry.add(idx);
                copy_cstr_to_reg_str(&mut entry.str, reg);
                entry.val = val;
                Some((idx as i32, false))
            } else if num < crate::database::map_db::MAX_MAPREG {
                let new_num = num + 1;
                slot.registry_num = new_num as i32;
                let entry = &mut *slot.registry.add(num);
                copy_cstr_to_reg_str(&mut entry.str, reg);
                entry.val = val;
                Some((num as i32, false))
            } else {
                None
            }
        }
    };

    if let Some((save_idx, clear_str)) = save_info {
        map_registrysave(m, save_idx).await;
        if clear_str {
            let slot = &mut *crate::database::map_db::raw_map_ptr().add(m as usize);
            if !slot.registry.is_null() {
                let entry = &mut *slot.registry.add(save_idx as usize);
                entry.str = [0i8; 64];
            }
        }
    }

    0
}

/// Send-safe async function for persisting a map registry entry by name.
///
/// Updates the in-memory registry for map `m` and persists to DB. All parameters
/// are owned/Copy types so the future is `Send` — safe to `.await` from Lua callback
/// boundaries via `blocking_run_async`.
///
/// # Safety
/// `crate::database::map_db::raw_map_ptr()` must be a valid initialised pointer and `m` within
/// `[0, MAP_SLOTS)`.
pub async unsafe fn map_setglobalreg_str(m: i32, reg_name: String, val: i32) -> i32 {
    use crate::database::map_db::MAP_SLOTS;
    if m < 0 || m as usize >= MAP_SLOTS {
        return 0;
    }

    let save_info: Option<(i32, bool)> = {
        let slot = &mut *crate::database::map_db::raw_map_ptr().add(m as usize);
        if slot.registry.is_null() {
            return 0;
        }
        let num = slot.registry_num as usize;

        let mut exist: Option<usize> = None;
        for idx in 0..num {
            let entry = &*slot.registry.add(idx);
            let bytes: &[u8] = std::slice::from_raw_parts(entry.str.as_ptr() as *const u8, 64);
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(64);
            let entry_name = std::str::from_utf8_unchecked(&bytes[..end]);
            if entry_name.eq_ignore_ascii_case(&reg_name) {
                exist = Some(idx);
                break;
            }
        }

        if let Some(idx) = exist {
            let entry = &mut *slot.registry.add(idx);
            entry.val = val;
            Some((idx as i32, val == 0))
        } else {
            let mut reuse_idx: Option<usize> = None;
            for idx in 0..num {
                let entry = &*slot.registry.add(idx);
                if entry.str[0] == 0 {
                    reuse_idx = Some(idx);
                    break;
                }
            }
            if let Some(idx) = reuse_idx {
                let entry = &mut *slot.registry.add(idx);
                let bytes = reg_name.as_bytes();
                let n = bytes.len().min(63);
                for (i, &b) in bytes[..n].iter().enumerate() {
                    entry.str[i] = b as i8;
                }
                entry.str[n] = 0;
                entry.val = val;
                Some((idx as i32, false))
            } else if num < crate::database::map_db::MAX_MAPREG {
                slot.registry_num = (num + 1) as i32;
                let entry = &mut *slot.registry.add(num);
                let bytes = reg_name.as_bytes();
                let n = bytes.len().min(63);
                for (i, &b) in bytes[..n].iter().enumerate() {
                    entry.str[i] = b as i8;
                }
                entry.str[n] = 0;
                entry.val = val;
                Some((num as i32, false))
            } else {
                None
            }
        }
    };

    if let Some((save_idx, clear_str)) = save_info {
        map_registrysave(m, save_idx).await;
        if clear_str {
            let slot = &mut *crate::database::map_db::raw_map_ptr().add(m as usize);
            if !slot.registry.is_null() {
                let entry = &mut *slot.registry.add(save_idx as usize);
                entry.str = [0i8; 64];
            }
        }
    }
    0
}

// ---------------------------------------------------------------------------
// map_readglobalreg — read a map-level registry value from memory.
// ---------------------------------------------------------------------------

/// Return the value for registry key `reg` on map `m`, or 0 if not found.
///
/// # Safety
/// `crate::database::map_db::raw_map_ptr()` must be a valid initialised pointer.  `m` must be within
/// `[0, MAP_SLOTS)`.  `reg` must be a valid non-null null-terminated C string.
pub unsafe fn map_readglobalreg(m: i32, reg: *const i8) -> i32 {
    use crate::database::map_db::MAP_SLOTS;

    if m < 0 || m as usize >= MAP_SLOTS {
        return 0;
    }
    let slot = &*crate::database::map_db::raw_map_ptr().add(m as usize);
    if slot.registry.is_null() {
        return 0;
    }

    let num = slot.registry_num as usize;
    for idx in 0..num {
        let entry = &*slot.registry.add(idx);
        if reg_str_eq(&entry.str, reg) {
            return entry.val;
        }
    }
    0
}

// ---------------------------------------------------------------------------
// map_loadgameregistry — load game-global registry from `GameRegistry<id>` table.
// ---------------------------------------------------------------------------

/// Load the game-global registry from the `GameRegistry<serverid>` table.
///
/// # Safety
/// Must be called on the game thread after the database pool is initialised.
pub async unsafe fn map_loadgameregistry() -> i32 {
    #[derive(sqlx::FromRow)]
    struct GrgRow {
        #[sqlx(rename = "GrgIdentifier")]
        grg_identifier: String,
        #[sqlx(rename = "GrgValue")]
        grg_value: u32, // INT UNSIGNED in schema
    }

    let sid = crate::config::config().server_id;
    let limit = MAX_GAMEREG as u32;

    {
        let mut gr = gamereg();
        gr.registry_num = 0;

        // Free previous registry if reload.
        if !gr.registry.is_null() {
            let layout =
                std::alloc::Layout::array::<crate::database::map_db::GlobalReg>(MAX_GAMEREG)
                    .expect("layout computation is infallible for MAX_GAMEREG = 5000");
            std::alloc::dealloc(gr.registry as *mut u8, layout);
            gr.registry = std::ptr::null_mut();
        }

        gr.registry = alloc_zeroed_gamereg_registry(MAX_GAMEREG);
    }

    let sql = format!("SELECT GrgIdentifier, GrgValue FROM `GameRegistry{sid}` LIMIT {limit}");

    let rows_opt = match sqlx::query_as::<_, GrgRow>(&sql)
        .fetch_all(get_pool())
        .await
    {
        Ok(rows) => Some(
            rows.into_iter()
                .map(|r| (r.grg_identifier, r.grg_value as i32))
                .collect::<Vec<_>>(),
        ),
        Err(e) => {
            tracing::error!("[map] map_loadgameregistry failed: {e:#}");
            None
        }
    };

    let rows = match rows_opt {
        Some(r) => r,
        None => return 0,
    };

    let count = rows.len().min(MAX_GAMEREG);
    let mut gr = gamereg();
    gr.registry_num = count as i32;

    for (i, (identifier, val)) in rows.iter().take(count).enumerate() {
        let entry = &mut *gr.registry.add(i);
        let bytes = identifier.as_bytes();
        let copy_len = bytes.len().min(63);
        std::ptr::copy_nonoverlapping(
            bytes.as_ptr() as *const i8,
            entry.str.as_mut_ptr(),
            copy_len,
        );
        entry.str[copy_len] = 0;
        entry.val = *val;
    }

    tracing::info!("[map] [load_game_registry] count={count}");
    0
}

// ---------------------------------------------------------------------------
// map_savegameregistry — persist one game-global registry slot to DB.
// ---------------------------------------------------------------------------

/// Persist one game-global registry slot at index `i` to `GameRegistry<serverid>`.
///
/// # Safety
/// Must be called on the game thread.  `i` must be within `[0, registry_num)`.
/// `gamereg.registry` must be a valid allocated array.
pub async unsafe fn map_savegameregistry(i: i32) -> i32 {
    let (sid, identifier, val) = {
        let gr = gamereg();
        if gr.registry.is_null() {
            return 0;
        }
        if i < 0 || i as usize >= gr.registry_num as usize {
            return 0;
        }
        let sid = crate::config::config().server_id;
        let entry = &*gr.registry.add(i as usize);
        let identifier = {
            let bytes: &[u8] = std::slice::from_raw_parts(entry.str.as_ptr() as *const u8, 64);
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(64);
            String::from_utf8_lossy(&bytes[..end]).into_owned()
        };
        let val = entry.val;
        (sid, identifier, val)
    };

    let id2 = identifier.clone();
    let save_id: Option<u32> = sqlx::query_scalar::<_, u32>(&format!(
        "SELECT GrgId FROM `GameRegistry{sid}` WHERE GrgIdentifier = ?",
    ))
    .bind(id2)
    .fetch_optional(get_pool())
    .await
    .unwrap_or(None);

    match save_id {
        Some(grg_id) if grg_id != 0 => {
            if val == 0 {
                let id2 = identifier.clone();
                let _ = sqlx::query(&format!(
                    "DELETE FROM `GameRegistry{sid}` WHERE GrgIdentifier = ?",
                ))
                .bind(id2)
                .execute(get_pool())
                .await;
            } else {
                let id2 = identifier.clone();
                let _ = sqlx::query(&format!(
                    "UPDATE `GameRegistry{sid}` \
                         SET GrgIdentifier = ?, GrgValue = ? \
                         WHERE GrgId = ?",
                ))
                .bind(id2)
                .bind(val)
                .bind(grg_id)
                .execute(get_pool())
                .await;
            }
        }
        _ => {
            if val > 0 {
                let _ = sqlx::query(&format!(
                    "INSERT INTO `GameRegistry{sid}` \
                         (GrgIdentifier, GrgValue) VALUES (?, ?)",
                ))
                .bind(identifier)
                .bind(val)
                .execute(get_pool())
                .await;
            }
        }
    }

    0
}

// ---------------------------------------------------------------------------
// map_setglobalgamereg — set a game-global registry key/value and persist.
// ---------------------------------------------------------------------------

/// Set a key/value pair in the game-global registry, then persist to DB.
///
/// # Safety
/// Must be called on the game thread.  `reg` must be a valid non-null
/// null-terminated C string.  `gamereg.registry` must be initialised.
pub async unsafe fn map_setglobalgamereg(reg: *const i8, val: i32) -> i32 {
    if reg.is_null() {
        return 0;
    }

    let save_info: Option<(i32, bool)> = {
        let mut gr = gamereg();
        if gr.registry.is_null() {
            return 0;
        }
        let num = gr.registry_num as usize;

        let mut exist: Option<usize> = None;
        for idx in 0..num {
            let entry = &*gr.registry.add(idx);
            if reg_str_eq(&entry.str, reg) {
                exist = Some(idx);
                break;
            }
        }

        if let Some(idx) = exist {
            let entry = &mut *gr.registry.add(idx);
            if entry.val == val {
                return 0;
            }
            entry.val = val;
            Some((idx as i32, val == 0))
        } else {
            let mut reuse_idx: Option<usize> = None;
            for idx in 0..num {
                let entry = &*gr.registry.add(idx);
                if entry.str[0] == 0 {
                    reuse_idx = Some(idx);
                    break;
                }
            }
            if let Some(idx) = reuse_idx {
                let entry = &mut *gr.registry.add(idx);
                copy_cstr_to_reg_str(&mut entry.str, reg);
                entry.val = val;
                Some((idx as i32, false))
            } else if num < MAX_GAMEREG {
                gr.registry_num = (num + 1) as i32;
                let entry = &mut *gr.registry.add(num);
                copy_cstr_to_reg_str(&mut entry.str, reg);
                entry.val = val;
                Some((num as i32, false))
            } else {
                None
            }
        }
    };

    if let Some((save_idx, clear_str)) = save_info {
        map_savegameregistry(save_idx).await;
        if clear_str {
            let gr = gamereg();
            if !gr.registry.is_null() {
                let entry = &mut *gr.registry.add(save_idx as usize);
                entry.str = [0i8; 64];
            }
        }
    }

    0
}

/// Set a game-global registry value by name and persist to DB.
/// Uses owned `String` so the future is `Send`.
pub async fn map_setglobalgamereg_str(reg_name: String, val: i32) -> i32 {
    // SAFETY: raw pointer access to the GlobalReg array — single-threaded, guarded by Mutex.
    let save_info: Option<(i32, bool)> = unsafe {
        let mut gr = gamereg();
        if gr.registry.is_null() {
            return 0;
        }
        let num = gr.registry_num as usize;

        let mut exist: Option<usize> = None;
        for idx in 0..num {
            let entry = &*gr.registry.add(idx);
            let bytes: &[u8] = std::slice::from_raw_parts(entry.str.as_ptr() as *const u8, 64);
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(64);
            let entry_name = std::str::from_utf8_unchecked(&bytes[..end]);
            if entry_name.eq_ignore_ascii_case(&reg_name) {
                exist = Some(idx);
                break;
            }
        }

        if let Some(idx) = exist {
            let entry = &mut *gr.registry.add(idx);
            if entry.val == val {
                return 0;
            } // value unchanged — skip save
            entry.val = val;
            Some((idx as i32, val == 0))
        } else {
            let mut reuse_idx: Option<usize> = None;
            for idx in 0..num {
                let entry = &*gr.registry.add(idx);
                if entry.str[0] == 0 {
                    reuse_idx = Some(idx);
                    break;
                }
            }
            if let Some(idx) = reuse_idx {
                let entry = &mut *gr.registry.add(idx);
                let bytes = reg_name.as_bytes();
                let n = bytes.len().min(63);
                for (i, &b) in bytes[..n].iter().enumerate() {
                    entry.str[i] = b as i8;
                }
                entry.str[n] = 0;
                entry.val = val;
                Some((idx as i32, false))
            } else if num < MAX_GAMEREG {
                gr.registry_num = (num + 1) as i32;
                let entry = &mut *gr.registry.add(num);
                let bytes = reg_name.as_bytes();
                let n = bytes.len().min(63);
                for (i, &b) in bytes[..n].iter().enumerate() {
                    entry.str[i] = b as i8;
                }
                entry.str[n] = 0;
                entry.val = val;
                Some((num as i32, false))
            } else {
                None
            }
        }
    }; // MutexGuard dropped here — safe to await below

    if let Some((save_idx, clear_str)) = save_info {
        (unsafe { map_savegameregistry(save_idx) }).await;
        if clear_str {
            let gr = gamereg();
            if !gr.registry.is_null() {
                unsafe {
                    let entry = &mut *gr.registry.add(save_idx as usize);
                    entry.str = [0i8; 64];
                }
            }
        }
    }
    0
}

/// No-op stub — `map_registrydelete` was commented out in C and has no current callers.
/// Retained for ABI completeness.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
#[allow(dead_code)]
pub unsafe fn map_registrydelete(_m: i32, _i: i32) -> i32 {
    0
}
