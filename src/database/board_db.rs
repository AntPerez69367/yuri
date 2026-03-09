use std::collections::HashMap;
use std::ffi::CStr;
use std::ptr::null_mut;
use std::sync::{Mutex, OnceLock};

use sqlx::Row;

use super::{blocking_run, get_pool};
use super::item_db::str_to_fixed;

pub struct BoardData {
    pub id: i32,
    pub level: i32,
    pub gmlevel: i32,
    pub path: i32,
    pub clan: i32,
    pub special: i32,
    pub sort: i32,
    pub name: [i8; 64],
    pub yname: [i8; 64],
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

static BOARD_DB: OnceLock<Mutex<HashMap<i32, Box<BoardData>>>> = OnceLock::new();
static BN_DB: OnceLock<Mutex<HashMap<i32, Box<BnData>>>> = OnceLock::new();

fn board_db() -> &'static Mutex<HashMap<i32, Box<BoardData>>> {
    BOARD_DB.get().expect("[board_db] not initialized")
}

fn bn_db() -> &'static Mutex<HashMap<i32, Box<BnData>>> {
    BN_DB.get().expect("[bn_db] not initialized")
}

fn make_default_board(id: i32) -> Box<BoardData> {
    let mut b = Box::new(BoardData {
        id,
        level: 0,
        gmlevel: 0,
        path: 0,
        clan: 0,
        special: 0,
        sort: 0,
        name: [0; 64],
        yname: [0; 64],
        script: 0,
    });
    str_to_fixed(&mut b.name, "??");
    b
}

fn make_default_bn(id: i32) -> Box<BnData> {
    let mut b = Box::new(BnData { id, name: [0; 255] });
    str_to_fixed(&mut b.name, "??");
    b
}

async fn load_boards() -> Result<usize, sqlx::Error> {
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
        let b = map.entry(id).or_insert_with(|| make_default_board(id));
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

async fn load_bn() -> Result<usize, sqlx::Error> {
    let pool = get_pool();
    let rows = sqlx::query("SELECT BtlId, BtlDescription FROM BoardTitles")
        .fetch_all(pool)
        .await?;

    let count = rows.len();
    let mut map = BN_DB.get().unwrap().lock().unwrap();
    for row in rows {
        let id: i32 = row.try_get::<u32, _>(0)? as i32;
        let b = map.entry(id).or_insert_with(|| make_default_bn(id));
        let desc: String = row.try_get(1).unwrap_or_default();
        str_to_fixed(&mut b.name, &desc);
        tracing::debug!("[board_db] [bn_read] id={id} name={desc}");
    }
    Ok(count)
}

// ─── Public interface ────────────────────────────────────────────────────────

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

/// Returns a raw pointer to the `BoardData` for `id`, inserting a default entry if absent.
///
/// # Safety
///
/// The returned pointer is valid only while the database is initialized and the map entry
/// remains present. Callers **must not** hold this pointer across any call that may modify
/// or clear the cache (e.g. `term()`). If a safer ownership model is needed, consider
/// returning `Arc<BoardData>` or confining access to within the lock scope.
pub fn search(id: i32) -> *mut BoardData {
    let mut map = board_db().lock().unwrap();
    let b = map.entry(id).or_insert_with(|| make_default_board(id));
    b.as_mut() as *mut BoardData
}

/// Returns a raw pointer to the `BoardData` for `id`, or null if not found.
///
/// # Safety
///
/// The returned pointer is valid only while the database is initialized and the map entry
/// remains present. Callers **must not** hold this pointer across any call that may modify
/// or clear the cache (e.g. `term()`). If a safer ownership model is needed, consider
/// returning `Arc<BoardData>` or confining access to within the lock scope.
pub fn searchexist(id: i32) -> *mut BoardData {
    let map = board_db().lock().unwrap();
    match map.get(&id) {
        Some(b) => b.as_ref() as *const BoardData as *mut BoardData,
        None => null_mut(),
    }
}

pub unsafe fn searchname(s: *const i8) -> *mut BoardData {
    if s.is_null() { return null_mut(); }
    let target = unsafe { CStr::from_ptr(s) }.to_string_lossy().to_lowercase();
    let map = board_db().lock().unwrap();
    for b in map.values() {
        let name = unsafe { CStr::from_ptr(b.name.as_ptr()) }.to_string_lossy().to_lowercase();
        let yname = unsafe { CStr::from_ptr(b.yname.as_ptr()) }.to_string_lossy().to_lowercase();
        if name == target || yname == target {
            return b.as_ref() as *const BoardData as *mut BoardData;
        }
    }
    null_mut()
}

pub unsafe fn board_id(s: *const i8) -> u32 {
    if s.is_null() { return 0; }
    let ptr = unsafe { searchname(s) };
    if !ptr.is_null() {
        return unsafe { (*ptr).id as u32 };
    }
    let str_val = unsafe { CStr::from_ptr(s) }.to_string_lossy();
    if let Ok(n) = str_val.trim().parse::<i32>() {
        if n > 0 {
            let p = searchexist(n);
            if !p.is_null() {
                return unsafe { (*p).id as u32 };
            }
        }
    }
    0
}

pub fn bn_search(id: i32) -> *mut BnData {
    let mut map = bn_db().lock().unwrap();
    let b = map.entry(id).or_insert_with(|| make_default_bn(id));
    b.as_mut() as *mut BnData
}

pub fn bn_searchexist(id: i32) -> *mut BnData {
    let mut map = bn_db().lock().unwrap();
    match map.get_mut(&id) {
        Some(b) => b.as_mut() as *mut BnData,
        None => null_mut(),
    }
}


pub fn rust_boarddb_init() -> i32 { ffi_catch!(-1, init()) }

pub fn rust_boarddb_term() { ffi_catch!((), term()) }

pub fn rust_boarddb_search(id: i32) -> *mut BoardData { ffi_catch!(null_mut(), search(id)) }

pub fn rust_boarddb_searchexist(id: i32) -> *mut BoardData { ffi_catch!(null_mut(), searchexist(id)) }

pub unsafe fn rust_boarddb_id(s: *const i8) -> u32 { ffi_catch!(0, unsafe { board_id(s) }) }

pub fn rust_boarddb_name(id: i32) -> *mut i8 {
    ffi_catch!(null_mut(), {
        let p = search(id);
        if p.is_null() { null_mut() } else { unsafe { (*p).name.as_mut_ptr() } }
    })
}
pub fn rust_boarddb_yname(id: i32) -> *mut i8 {
    ffi_catch!(null_mut(), {
        let p = search(id);
        if p.is_null() { null_mut() } else { unsafe { (*p).yname.as_mut_ptr() } }
    })
}
pub fn rust_boarddb_level(id: i32) -> i32 {
    ffi_catch!(-1, { let p = search(id); if p.is_null() { -1 } else { unsafe { (*p).level } } })
}
pub fn rust_boarddb_gmlevel(id: i32) -> i32 {
    ffi_catch!(-1, { let p = search(id); if p.is_null() { -1 } else { unsafe { (*p).gmlevel } } })
}
pub fn rust_boarddb_path(id: i32) -> i32 {
    ffi_catch!(-1, { let p = search(id); if p.is_null() { -1 } else { unsafe { (*p).path } } })
}
pub fn rust_boarddb_clan(id: i32) -> i32 {
    ffi_catch!(-1, { let p = search(id); if p.is_null() { -1 } else { unsafe { (*p).clan } } })
}
pub fn rust_boarddb_sort(id: i32) -> i32 {
    ffi_catch!(-1, { let p = search(id); if p.is_null() { -1 } else { unsafe { (*p).sort } } })
}
pub fn rust_boarddb_script(id: i32) -> i32 {
    ffi_catch!(-1, { let p = search(id); if p.is_null() { -1 } else { unsafe { (*p).script as i32 } } })
}

pub fn rust_bn_search(id: i32) -> *mut BnData { ffi_catch!(null_mut(), bn_search(id)) }

pub fn rust_bn_searchexist(id: i32) -> *mut BnData { ffi_catch!(null_mut(), bn_searchexist(id)) }

pub fn rust_bn_name(id: i32) -> *mut i8 {
    ffi_catch!(null_mut(), {
        let p = bn_search(id);
        if p.is_null() { null_mut() } else { unsafe { (*p).name.as_mut_ptr() } }
    })
}
