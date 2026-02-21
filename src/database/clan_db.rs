use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uint};
use std::ptr::null_mut;
use std::sync::{Mutex, OnceLock};

use sqlx::Row;

use super::{blocking_run, get_pool};
use super::item_db::str_to_fixed;

#[repr(C)]
pub struct ClanBank {
    pub item_id: c_uint,
    pub amount: c_uint,
    pub owner: c_uint,
    pub time: c_uint,
    pub custom_icon: c_uint,
    pub custom_look: c_uint,
    pub pos: c_uint,
    pub real_name: [c_char; 64],
    pub custom_look_color: c_uint,
    pub custom_icon_color: c_uint,
    pub protected_flag: c_uint,
    pub note: [c_char; 300],
}

#[repr(C)]
pub struct ClanData {
    pub id: c_int,
    pub name: [c_char; 64],
    pub maxslots: c_int,
    pub maxperslot: c_int,
    pub level: c_int,
    /// Set to null on init; map_loadclanbank() fills this in after init.
    pub clanbanks: *mut ClanBank,
}

// SAFETY: ClanBank contains only POD fields (no raw pointers) and is always
// accessed while holding CLAN_DB's Mutex, so cross-thread sharing is safe.
unsafe impl Send for ClanBank {}
unsafe impl Sync for ClanBank {}

// SAFETY: ClanData::clanbanks is a raw pointer filled in by map_loadclanbank()
// on the C side. The invariant is: clanbanks is written exactly once before any
// concurrent readers exist and is never reallocated or freed while the ClanData
// lives in CLAN_DB. All Rust-side access goes through the CLAN_DB Mutex.
unsafe impl Send for ClanData {}
unsafe impl Sync for ClanData {}

static CLAN_DB: OnceLock<Mutex<HashMap<i32, Box<ClanData>>>> = OnceLock::new();

fn db() -> &'static Mutex<HashMap<i32, Box<ClanData>>> {
    CLAN_DB.get().expect("[clan_db] not initialized")
}

fn make_default(id: i32) -> Box<ClanData> {
    let mut c = Box::new(ClanData {
        id,
        name: [0; 64],
        maxslots: 0,
        maxperslot: 0,
        level: 0,
        clanbanks: null_mut(),
    });
    str_to_fixed(&mut c.name, "??");
    c
}

async fn load_clans() -> Result<usize, sqlx::Error> {
    let pool = get_pool();
    let rows = sqlx::query("SELECT ClnId, ClnName FROM Clans")
        .fetch_all(pool)
        .await?;

    let count = rows.len();
    let mut map = CLAN_DB.get().unwrap().lock().unwrap();
    // Fix C bug: original only processed one row due to loop condition
    for row in rows {
        let raw_id: u32 = row.try_get(0)?;
        let id: i32 = i32::try_from(raw_id)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let c = map.entry(id).or_insert_with(|| make_default(id));
        c.id = id;
        let name: String = row.try_get(1)?;
        str_to_fixed(&mut c.name, &name);
    }
    Ok(count)
}

// ─── Public interface ────────────────────────────────────────────────────────

pub fn init() -> c_int {
    CLAN_DB.get_or_init(|| Mutex::new(HashMap::new()));
    match blocking_run(load_clans()) {
        Ok(n) => { println!("[clan_db] read done count={}", n); 0 }
        Err(e) => { eprintln!("[clan_db] load failed: {}", e); -1 }
    }
}

/// Drops all clan entries from the in-memory map.
///
/// # Safety
/// Must only be called at server shutdown, after all C-side code has stopped
/// using pointers returned by `search`/`searchexist`/`searchname`. Clearing
/// the map while outstanding raw pointers exist produces dangling pointers.
/// `OnceLock` cannot be re-initialized, so a subsequent `init()` call will
/// see an empty map but the same lock; callers must not call `init()` after
/// `term()` in production.
pub fn term() {
    if let Some(m) = CLAN_DB.get() {
        m.lock().unwrap().clear();
    }
}

/// Create-if-missing. Returns mutable pointer so C can write clanbanks into it.
///
/// # Pointer validity invariant
/// The returned pointer is valid as long as:
/// 1. The entry is not removed from the map (only `term()` removes entries).
/// 2. No reallocation of the `Box<ClanData>` occurs (it is heap-stable).
/// Callers must not hold this pointer across a call to `term()`.
pub fn search(id: i32) -> *mut ClanData {
    let mut map = db().lock().unwrap();
    let c = map.entry(id).or_insert_with(|| make_default(id));
    c.as_mut() as *mut ClanData
}

/// # Pointer validity invariant
/// Same as `search`: valid until `term()` is called. Caller must not hold this
/// pointer after the server teardown sequence begins.
pub fn searchexist(id: i32) -> *mut ClanData {
    let map = db().lock().unwrap();
    match map.get(&id) {
        Some(c) => c.as_ref() as *const ClanData as *mut ClanData,
        None => null_mut(),
    }
}

pub fn searchname(s: *const c_char) -> *mut ClanData {
    if s.is_null() { return null_mut(); }
    let target = unsafe { CStr::from_ptr(s) }.to_string_lossy().to_lowercase();
    let map = db().lock().unwrap();
    for c in map.values() {
        let name = unsafe { CStr::from_ptr(c.name.as_ptr()) }.to_string_lossy().to_lowercase();
        if name == target {
            return c.as_ref() as *const ClanData as *mut ClanData;
        }
    }
    null_mut()
}

/// Returns a pointer to the clan name string for `id`, or a pointer to the
/// static literal `"??"` if the id is not present in the map.
///
/// # Pointer validity invariant
/// When the id is found, the returned pointer points directly into the `name`
/// field of the `Box<ClanData>` stored in `CLAN_DB`. It is valid as long as:
/// 1. The entry is not removed from the map — only `term()` removes entries.
/// 2. No reallocation of the `Box<ClanData>` occurs (it is heap-stable and
///    owned by the map; `search()` and `searchexist()` do not reallocate it).
///
/// When the id is not found the pointer refers to a static `b"??\0"` literal
/// and is always valid.
///
/// Callers must not write through this pointer (it is `*const c_char`).
/// Callers must not hold this pointer across a call to `term()`, which drops
/// all map entries and invalidates every pointer previously returned by
/// `name()`, `search()`, or `searchexist()`. See also `db()` for the
/// underlying store that governs the lifetime of all returned pointers.
pub fn name(id: i32) -> *const c_char {
    let map = db().lock().unwrap();
    match map.get(&id) {
        Some(c) => c.name.as_ptr() as *const c_char,
        None => b"??\0".as_ptr() as *const c_char,
    }
}
