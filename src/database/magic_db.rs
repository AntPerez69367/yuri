use std::collections::HashMap;
use std::ffi::CStr;
use std::sync::{Arc, Mutex, OnceLock};

use sqlx::Row;

use super::{blocking_run, get_pool};
use super::item_db::str_to_fixed;

pub struct MagicData {
    pub id: i32,
    pub typ: i32,
    pub name: [i8; 32],
    pub yname: [i8; 32],
    pub question: [i8; 64],
    pub script: [i8; 64],
    pub script2: [i8; 64],
    pub script3: [i8; 64],
    pub dispell: u8,
    pub aether: u8,
    pub mute: u8,
    pub level: u8,
    pub mark: u8,
    pub canfail: u8,
    pub alignment: i8,
    pub ticker: u8,
    pub class: i8,
}

unsafe impl Send for MagicData {}
unsafe impl Sync for MagicData {}

pub(crate) static MAGIC_DB: OnceLock<Mutex<HashMap<i32, Arc<MagicData>>>> = OnceLock::new();

fn db() -> &'static Mutex<HashMap<i32, Arc<MagicData>>> {
    MAGIC_DB.get().expect("[magic_db] not initialized")
}

fn make_default(id: i32) -> MagicData {
    let mut m = MagicData {
        id, typ: 0,
        name: [0; 32], yname: [0; 32], question: [0; 64],
        script: [0; 64], script2: [0; 64], script3: [0; 64],
        dispell: 0, aether: 0, mute: 0, level: 0, mark: 0,
        canfail: 0, alignment: 0, ticker: 0, class: 0,
    };
    str_to_fixed(&mut m.name, "??");
    m
}

pub(crate) async fn load_magic() -> Result<usize, sqlx::Error> {
    let pool = get_pool();
    let rows = sqlx::query(
        "SELECT SplId, SplDescription, SplIdentifier, SplType, \
         SplQuestion, SplDispel, SplAether, SplMute, SplPthId, \
         SplLevel, SplMark, SplCanFail, SplAlignment, SplTicker \
         FROM Spells WHERE SplActive = '1'",
    )
    .fetch_all(pool)
    .await?;

    let count = rows.len();
    let mut map = MAGIC_DB.get().unwrap().lock().unwrap();
    for row in rows {
        let id: i32 = row.try_get::<u32, _>(0)? as i32;
        let entry = map.entry(id).or_insert_with(|| Arc::new(make_default(id)));
        let m = Arc::get_mut(entry).expect("exclusive during init");
        m.id = id;
        str_to_fixed(&mut m.name, &row.try_get::<String, _>(1).unwrap_or_default());
        str_to_fixed(&mut m.yname, &row.try_get::<String, _>(2).unwrap_or_default());
        m.typ      = row.try_get::<u32, _>(3).map(|v| v as i32).unwrap_or(0);
        str_to_fixed(&mut m.question, &row.try_get::<String, _>(4).unwrap_or_default());
        m.dispell  = row.try_get::<u32, _>(5).map(|v| v as u8).unwrap_or(0);
        m.aether   = row.try_get::<u32, _>(6).map(|v| v as u8).unwrap_or(0);
        m.mute     = row.try_get::<u32, _>(7).map(|v| v as u8).unwrap_or(0);
        m.class    = row.try_get::<i32, _>(8).map(|v| v as i8).unwrap_or(0);
        m.level    = row.try_get::<u32, _>(9).map(|v| v as u8).unwrap_or(0);
        m.mark     = row.try_get::<u32, _>(10).map(|v| v as u8).unwrap_or(0);
        m.canfail  = row.try_get::<u32, _>(11).map(|v| v as u8).unwrap_or(0);
        m.alignment = row.try_get::<i8, _>(12).unwrap_or(0);
        m.ticker   = row.try_get::<u32, _>(13).map(|v| v as u8).unwrap_or(0);
    }
    Ok(count)
}

// ---- Public interface -------------------------------------------------------

pub fn init() -> i32 {
    MAGIC_DB.get_or_init(|| Mutex::new(HashMap::new()));
    match blocking_run(load_magic()) {
        Ok(n) => { tracing::info!("[magic_db] read done count={n}"); 0 }
        Err(e) => { tracing::error!("[magic_db] load failed: {e}"); -1 }
    }
}

pub fn term() {
    if let Some(m) = MAGIC_DB.get() { m.lock().unwrap().clear(); }
}

/// Returns the `MagicData` for `id`, inserting a default entry if absent.
pub fn search(id: i32) -> Arc<MagicData> {
    let mut map = db().lock().unwrap();
    let entry = map.entry(id).or_insert_with(|| Arc::new(make_default(id)));
    Arc::clone(entry)
}

/// Returns the `MagicData` for `id` if it exists.
pub fn searchexist(id: i32) -> Option<Arc<MagicData>> {
    let map = db().lock().unwrap();
    map.get(&id).cloned()
}

/// Searches by yname only (case-insensitive).
pub fn searchname(name: &str) -> Option<Arc<MagicData>> {
    let target = name.to_lowercase();
    let map = db().lock().unwrap();
    for m in map.values() {
        let yname = unsafe { CStr::from_ptr(m.yname.as_ptr()) }
            .to_string_lossy().to_lowercase();
        if yname == target { return Some(Arc::clone(m)); }
    }
    None
}

/// Look up spell ID by name string. Falls back to parsing as numeric ID.
pub fn id_by_name(name: &str) -> i32 {
    if let Some(m) = searchname(name) { return m.id; }
    if let Ok(n) = name.trim().parse::<i32>() {
        if n > 0 { if let Some(m) = searchexist(n) { return m.id; } }
    }
    0
}

/// Takes a spell name string, returns the level field.
pub fn level_by_name(name: &str) -> i32 {
    let spell_id = id_by_name(name);
    if spell_id != 0 { search(spell_id).level as i32 } else { 0 }
}

// ---- Field accessors ----
// Pointers point into Arc-owned heap allocations in the global map;
// stable until `term()` clears the map.

/// Returns a pointer to the yname field for the spell `id`.
pub fn yname_ptr(id: i32) -> *const i8 {
    search(id).yname.as_ptr()
}

/// Returns a pointer to the name field for the spell `id`.
pub fn name_ptr(id: i32) -> *const i8 {
    search(id).name.as_ptr()
}

/// Returns a pointer to the question field for the spell `id`.
pub fn question_ptr(id: i32) -> *const i8 {
    search(id).question.as_ptr()
}

pub fn typ(id: i32) -> i32 { search(id).typ }
pub fn dispel(id: i32) -> i32 { search(id).dispell as i32 }
pub fn mute(id: i32) -> i32 { search(id).mute as i32 }
pub fn canfail(id: i32) -> i32 { search(id).canfail as i32 }
pub fn ticker(id: i32) -> i32 { search(id).ticker as i32 }
