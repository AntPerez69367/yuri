use std::collections::HashMap;
use std::ffi::CStr;
use std::sync::{Arc, Mutex, OnceLock};

use sqlx::Row;

use super::{blocking_run, get_pool};
use super::item_db::str_to_fixed;

pub struct BoardData {
    pub id: i32, pub level: i32, pub gmlevel: i32,
    pub path: i32, pub clan: i32, pub special: i32, pub sort: i32,
    pub name: [i8; 64], pub yname: [i8; 64],
    /// Single-byte boolean (not a pointer), matches `char script` in C struct.
    pub script: i8,
}

pub struct BnData {
    pub id: i32,
    pub name: [i8; 255],
}

unsafe impl Send for BoardData {}
unsafe impl Sync for BoardData {}
unsafe impl Send for BnData {}
unsafe impl Sync for BnData {}

pub(crate) static BOARD_DB: OnceLock<Mutex<HashMap<i32, Arc<BoardData>>>> = OnceLock::new();
pub(crate) static BN_DB: OnceLock<Mutex<HashMap<i32, Arc<BnData>>>> = OnceLock::new();

fn board_db() -> &'static Mutex<HashMap<i32, Arc<BoardData>>> {
    BOARD_DB.get().expect("[board_db] not initialized")
}

fn bn_db() -> &'static Mutex<HashMap<i32, Arc<BnData>>> {
    BN_DB.get().expect("[bn_db] not initialized")
}

fn make_default_board(id: i32) -> BoardData {
    let mut b = BoardData {
        id, level: 0, gmlevel: 0, path: 0, clan: 0, special: 0, sort: 0,
        name: [0; 64], yname: [0; 64], script: 0,
    };
    str_to_fixed(&mut b.name, "??");
    b
}

fn make_default_bn(id: i32) -> BnData {
    let mut b = BnData { id, name: [0; 255] };
    str_to_fixed(&mut b.name, "??");
    b
}

pub(crate) async fn load_boards() -> Result<usize, sqlx::Error> {
    let pool = get_pool();
    let rows = sqlx::query(
        "SELECT BnmId, BnmDescription, BnmLevel, BnmGMLevel, \
         BnmPthId, BnmClnId, BnmScripted, BnmIdentifier, BnmSortOrder \
         FROM BoardNames",
    )
    .fetch_all(pool)
    .await?;

    let count = rows.len();
    let mut map = BOARD_DB.get().unwrap().lock().unwrap();
    for row in rows {
        let id: i32 = row.try_get::<u32, _>(0)? as i32;
        let entry = map.entry(id).or_insert_with(|| Arc::new(make_default_board(id)));
        let b = Arc::get_mut(entry).expect("exclusive during init");
        b.id = id;
        str_to_fixed(&mut b.name, &row.try_get::<String, _>(1).unwrap_or_default());
        b.level   = row.try_get::<u32, _>(2).map(|v| v as i32).unwrap_or(0);
        b.gmlevel = row.try_get::<u32, _>(3).map(|v| v as i32).unwrap_or(0);
        b.path    = row.try_get::<u32, _>(4).map(|v| v as i32).unwrap_or(0);
        b.clan    = row.try_get::<u32, _>(5).map(|v| v as i32).unwrap_or(0);
        b.script  = row.try_get::<u32, _>(6).map(|v| v as i8).unwrap_or(0);
        str_to_fixed(&mut b.yname, &row.try_get::<String, _>(7).unwrap_or_default());
        b.sort    = row.try_get::<u32, _>(8).map(|v| v as i32).unwrap_or(0);
    }
    Ok(count)
}

pub(crate) async fn load_bn() -> Result<usize, sqlx::Error> {
    let pool = get_pool();
    let rows = sqlx::query("SELECT BtlId, BtlDescription FROM BoardTitles")
        .fetch_all(pool)
        .await?;

    let count = rows.len();
    let mut map = BN_DB.get().unwrap().lock().unwrap();
    for row in rows {
        let id: i32 = row.try_get::<u32, _>(0)? as i32;
        let entry = map.entry(id).or_insert_with(|| Arc::new(make_default_bn(id)));
        let b = Arc::get_mut(entry).expect("exclusive during init");
        let desc: String = row.try_get(1).unwrap_or_default();
        str_to_fixed(&mut b.name, &desc);
        tracing::debug!("[board_db] [bn_read] id={id} name={desc}");
    }
    Ok(count)
}

// ---- Public interface -------------------------------------------------------

pub fn init() -> i32 {
    BOARD_DB.get_or_init(|| Mutex::new(HashMap::new()));
    BN_DB.get_or_init(|| Mutex::new(HashMap::new()));
    match blocking_run(load_boards()) {
        Ok(n) => tracing::info!("[board_db] read done count={n}"),
        Err(e) => { tracing::error!("[board_db] load failed: {e}"); return -1; }
    }
    match blocking_run(load_bn()) {
        Ok(_) => {}
        Err(e) => { tracing::error!("[bn_db] load failed: {e}"); return -1; }
    }
    0
}

pub fn term() {
    if let Some(m) = BOARD_DB.get() { m.lock().unwrap().clear(); }
    if let Some(m) = BN_DB.get() { m.lock().unwrap().clear(); }
}

pub fn search(id: i32) -> Arc<BoardData> {
    let mut map = board_db().lock().unwrap();
    let entry = map.entry(id).or_insert_with(|| Arc::new(make_default_board(id)));
    Arc::clone(entry)
}

pub fn searchexist(id: i32) -> Option<Arc<BoardData>> {
    let map = board_db().lock().unwrap();
    map.get(&id).cloned()
}

pub fn searchname(name: &str) -> Option<Arc<BoardData>> {
    let target = name.to_lowercase();
    let map = board_db().lock().unwrap();
    for b in map.values() {
        let bname = unsafe { CStr::from_ptr(b.name.as_ptr()) }.to_string_lossy().to_lowercase();
        let byname = unsafe { CStr::from_ptr(b.yname.as_ptr()) }.to_string_lossy().to_lowercase();
        if bname == target || byname == target { return Some(Arc::clone(b)); }
    }
    None
}

pub fn board_id(name: &str) -> u32 {
    if let Some(b) = searchname(name) { return b.id as u32; }
    if let Ok(n) = name.trim().parse::<i32>() {
        if n > 0 { if let Some(b) = searchexist(n) { return b.id as u32; } }
    }
    0
}

pub fn bn_search(id: i32) -> Arc<BnData> {
    let mut map = bn_db().lock().unwrap();
    let entry = map.entry(id).or_insert_with(|| Arc::new(make_default_bn(id)));
    Arc::clone(entry)
}

pub fn bn_searchexist(id: i32) -> Option<Arc<BnData>> {
    let map = bn_db().lock().unwrap();
    map.get(&id).cloned()
}

fn fixed_to_string(arr: &[i8]) -> String {
    let bytes = unsafe { std::slice::from_raw_parts(arr.as_ptr() as *const u8, arr.len()) };
    let nul = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..nul]).into_owned()
}

pub fn board_name(id: i32) -> String {
    let map = board_db().lock().unwrap();
    match map.get(&id) { Some(b) => fixed_to_string(&b.name), None => String::from("??") }
}

pub fn bn_name(id: i32) -> String {
    let mut map = bn_db().lock().unwrap();
    let b = map.entry(id).or_insert_with(|| Arc::new(make_default_bn(id)));
    fixed_to_string(&b.name)
}

// ---- Field accessors ----

/// Returns a pointer to the yname field for board `id`.
pub fn yname_ptr(id: i32) -> *const i8 {
    search(id).yname.as_ptr()
}
