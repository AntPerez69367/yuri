use std::collections::HashMap;
use std::convert::TryFrom;
use std::ffi::CStr;
use std::ptr::null_mut;
use std::sync::{Arc, Mutex, OnceLock};

use sqlx::Row;

use super::item_db::str_to_fixed;
use super::{blocking_run, get_pool};

pub struct ClanBank {
    pub item_id: u32,
    pub amount: u32,
    pub owner: u32,
    pub time: u32,
    pub custom_icon: u32,
    pub custom_look: u32,
    pub pos: u32,
    pub real_name: [i8; 64],
    pub custom_look_color: u32,
    pub custom_icon_color: u32,
    pub protected_flag: u32,
    pub note: [i8; 300],
}

pub struct ClanData {
    pub id: i32,
    pub name: [i8; 64],
    pub maxslots: i32,
    pub maxperslot: i32,
    pub level: i32,
    /// Set to null on init; map_loadclanbank() fills this in after init.
    pub clanbanks: *mut ClanBank,
}

unsafe impl Send for ClanBank {}
unsafe impl Sync for ClanBank {}
unsafe impl Send for ClanData {}
unsafe impl Sync for ClanData {}

pub(crate) static CLAN_DB: OnceLock<Mutex<HashMap<i32, Arc<ClanData>>>> = OnceLock::new();

fn db() -> &'static Mutex<HashMap<i32, Arc<ClanData>>> {
    CLAN_DB.get().expect("[clan_db] not initialized")
}

fn make_default(id: i32) -> ClanData {
    let mut c = ClanData {
        id,
        name: [0; 64],
        maxslots: 0,
        maxperslot: 0,
        level: 0,
        clanbanks: null_mut(),
    };
    str_to_fixed(&mut c.name, "??");
    c
}

pub(crate) async fn load_clans() -> Result<usize, sqlx::Error> {
    let pool = get_pool();
    let rows = sqlx::query("SELECT ClnId, ClnName FROM Clans")
        .fetch_all(pool)
        .await?;

    let count = rows.len();
    let mut map = CLAN_DB.get().unwrap().lock().unwrap();
    for row in rows {
        let raw_id: u32 = row.try_get(0)?;
        let id: i32 = i32::try_from(raw_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let entry = map.entry(id).or_insert_with(|| Arc::new(make_default(id)));
        let c = Arc::get_mut(entry).expect("exclusive during init");
        c.id = id;
        let name: String = row.try_get(1)?;
        str_to_fixed(&mut c.name, &name);
    }
    Ok(count)
}

// ---- Public interface -------------------------------------------------------

pub fn init() -> i32 {
    CLAN_DB.get_or_init(|| Mutex::new(HashMap::new()));
    match blocking_run(load_clans()) {
        Ok(n) => { tracing::info!("[clan_db] read done count={n}"); 0 }
        Err(e) => { tracing::error!("[clan_db] load failed: {e}"); -1 }
    }
}

pub fn term() {
    if let Some(m) = CLAN_DB.get() {
        m.lock().unwrap().clear();
    }
}

/// Returns the `ClanData` for `id`, inserting a default entry if absent.
pub fn search(id: i32) -> Arc<ClanData> {
    let mut map = db().lock().unwrap();
    let entry = map.entry(id).or_insert_with(|| Arc::new(make_default(id)));
    Arc::clone(entry)
}

/// Returns the `ClanData` for `id` if it exists.
pub fn searchexist(id: i32) -> Option<Arc<ClanData>> {
    let map = db().lock().unwrap();
    map.get(&id).cloned()
}

/// Searches by name, case-insensitive.
pub fn searchname(name: &str) -> Option<Arc<ClanData>> {
    let target = name.to_lowercase();
    let map = db().lock().unwrap();
    for c in map.values() {
        let cname = unsafe { CStr::from_ptr(c.name.as_ptr()) }
            .to_string_lossy()
            .to_lowercase();
        if cname == target {
            return Some(Arc::clone(c));
        }
    }
    None
}

/// Returns a pointer to the clan name for `id`, or `"??"` if not found.
pub fn name(id: i32) -> *const i8 {
    let map = db().lock().unwrap();
    match map.get(&id) {
        Some(c) => c.name.as_ptr(),
        None => c"??".as_ptr(),
    }
}

