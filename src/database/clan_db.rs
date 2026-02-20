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

unsafe impl Send for ClanBank {}
unsafe impl Sync for ClanBank {}
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
        let id: i32 = row.try_get::<u32, _>(0)? as i32;
        let c = map.entry(id).or_insert_with(|| make_default(id));
        c.id = id;
        let name: String = row.try_get(1).unwrap_or_default();
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

pub fn term() {
    if let Some(m) = CLAN_DB.get() {
        m.lock().unwrap().clear();
    }
}

/// Create-if-missing. Returns mutable pointer so C can write clanbanks into it.
pub fn search(id: i32) -> *mut ClanData {
    let mut map = db().lock().unwrap();
    let c = map.entry(id).or_insert_with(|| make_default(id));
    c.as_mut() as *mut ClanData
}

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

/// Returns clan name or "??" if not found. Matches C clandb_name behavior.
pub fn name(id: i32) -> *mut c_char {
    let map = db().lock().unwrap();
    match map.get(&id) {
        Some(c) => c.name.as_ptr() as *mut c_char,
        None => b"??\0".as_ptr() as *mut c_char,
    }
}
