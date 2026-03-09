use std::collections::HashMap;
use std::convert::TryFrom;
use std::ffi::{CStr, CString};
use std::fs;
use std::path::PathBuf;
use std::ptr::null_mut;
use std::sync::{Arc, Mutex, OnceLock};

use sqlx::Row;

use super::{blocking_run, get_pool};
use super::item_db::str_to_fixed;

#[derive(Clone)]
pub struct ClassData {
    /// 16 rank name strings, each 32 bytes (rank0..rank15)
    pub ranks: [[i8; 32]; 16],
    pub id: u16,
    pub path: u16,
    pub level: [u32; 99],
    pub chat: i32,
    pub icon: i32,
}

unsafe impl Send for ClassData {}
unsafe impl Sync for ClassData {}

// Issue 2: Arc<ClassData> instead of Box so cloned references keep data alive
// after term() clears the HashMap.
static CLASS_DB: OnceLock<Mutex<HashMap<u32, Arc<ClassData>>>> = OnceLock::new();

fn db() -> &'static Mutex<HashMap<u32, Arc<ClassData>>> {
    CLASS_DB.get().expect("[class_db] not initialized")
}

fn make_default(id: u32) -> Arc<ClassData> {
    let mut c = ClassData {
        ranks: [[0; 32]; 16],
        id: u16::try_from(id).expect("class path ID exceeds u16::MAX — C struct field too narrow"),
        path: 0,
        level: [0; 99],
        chat: 0,
        icon: 0,
    };
    str_to_fixed(&mut c.ranks[0], "??");
    Arc::new(c)
}

async fn load_classes() -> Result<usize, sqlx::Error> {
    let pool = get_pool();
    let rows = sqlx::query(
        "SELECT PthId, PthType, PthChat, PthIcon, \
         PthMark0, PthMark1, PthMark2, PthMark3, \
         PthMark4, PthMark5, PthMark6, PthMark7, \
         PthMark8, PthMark9, PthMark10, PthMark11, \
         PthMark12, PthMark13, PthMark14, PthMark15 \
         FROM Paths",
    )
    .fetch_all(pool)
    .await?;

    let count = rows.len();
    let mut map = CLASS_DB.get().unwrap().lock().unwrap();
    for row in rows {
        let id: u32 = row.try_get::<u32, _>(0).unwrap_or(0);
        let arc = map.entry(id).or_insert_with(|| make_default(id));
        let c = Arc::make_mut(arc);
        c.id   = u16::try_from(id).expect("class path ID exceeds u16::MAX");
        c.path = row.try_get::<u32, _>(1).map(|v| v as u16).unwrap_or(0);
        c.chat = row.try_get::<u32, _>(2).map(|v| v as i32).unwrap_or(0);
        c.icon = row.try_get::<u32, _>(3).map(|v| v as i32).unwrap_or(0);
        for rank_idx in 0..16usize {
            let s: String = row.try_get::<String, _>(4 + rank_idx).unwrap_or_default();
            str_to_fixed(&mut c.ranks[rank_idx], &s);
        }
    }
    Ok(count)
}

fn load_leveldb(data_dir: &str) -> Result<usize, std::io::Error> {
    // Issue 4: use PathBuf::join so a missing trailing separator is handled
    // correctly (e.g. "data" + "tnl_exp.csv" → "data/tnl_exp.csv", not
    // "datatnl_exp.csv").
    let path = PathBuf::from(data_dir).join("tnl_exp.csv");
    let contents = fs::read_to_string(&path)
        .map_err(|e| { tracing::error!("Can't read level db ({}): {e}", path.display()); e })?;

    let mut count = 0;
    let mut map = CLASS_DB.get().unwrap().lock().unwrap();
    for line in contents.lines() {
        if line.starts_with("//") || line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(99 + 1, ',').collect();
        if parts.is_empty() {
            continue;
        }
        let path_id: u32 = parts[0].trim().parse().unwrap_or(0);
        let arc = map.entry(path_id).or_insert_with(|| make_default(path_id));
        let c = Arc::make_mut(arc);
        for x in 1..parts.len().min(99) {
            c.level[x] = parts[x].trim().parse().unwrap_or(0);
        }
        count += 1;
    }
    Ok(count)
}

// ─── Public interface (called by ffi::class_db) ─────────────────────────────

pub unsafe fn init(data_dir: *const i8) -> i32 {
    // Issue 3: clear stale entries on re-initialization so old data does not
    // persist if init() is called more than once.
    let lock = CLASS_DB.get_or_init(|| Mutex::new(HashMap::new()));
    lock.lock().unwrap().clear();

    match blocking_run(load_classes()) {
        Ok(n) => tracing::info!("[class_db] read done count={n}"),
        Err(e) => { tracing::error!("[class_db] load failed: {e}"); return -1; }
    }

    let dir = if data_dir.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(data_dir) }.to_string_lossy().into_owned()
    };
    match load_leveldb(&dir) {
        Ok(n) => tracing::info!("[leveldb] read done count={n}"),
        Err(_) => return -1,
    }
    0
}

pub fn term() {
    if let Some(m) = CLASS_DB.get() {
        m.lock().unwrap().clear();
    }
}

/// Returns a cloned Arc so the caller holds a strong reference independent of
/// the map. Creates a default entry if the id is not present.
pub fn search(id: i32) -> Arc<ClassData> {
    let key = id as u32;
    let mut map = db().lock().unwrap();
    map.entry(key).or_insert_with(|| make_default(key)).clone()
}

/// Returns a cloned Arc if the entry exists, None otherwise.
pub fn searchexist(id: i32) -> Option<Arc<ClassData>> {
    let map = db().lock().unwrap();
    map.get(&(id as u32)).cloned()
}

pub fn level(path: i32, lvl: i32) -> u32 {
    let map = db().lock().unwrap();
    match map.get(&(path as u32)) {
        Some(c) if (lvl as usize) < 99 => c.level[lvl as usize],
        _ => 0,
    }
}

/// Returns an owned CString (allocated on the Rust heap). The returned pointer
/// must be freed by the caller via rust_classdb_free_name().
pub fn name(id: i32, rank: i32) -> *mut i8 {
    // Issue 1: clone the rank bytes while holding the lock, then release the
    // lock before constructing CString, so the returned pointer is fully
    // caller-owned and not tied to the HashMap's lifetime.
    let bytes: Option<Vec<u8>> = {
        let map = db().lock().unwrap();
        map.get(&(id as u32)).map(|c| {
            let idx = (rank as usize).min(15);
            let slice = &c.ranks[idx];
            let len = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
            slice[..len].iter().map(|&b| b as u8).collect()
        })
    };
    match bytes {
        Some(b) => CString::new(b).map(|s| s.into_raw()).unwrap_or(null_mut()),
        None => null_mut(),
    }
}

pub fn path(id: i32) -> i32 {
    let map = db().lock().unwrap();
    match map.get(&(id as u32)) {
        Some(c) => c.path as i32,
        None => 0,
    }
}

/// Issue 5: direct map lookup, no unsafe dereference of a raw pointer.
pub fn chat(id: i32) -> i32 {
    let map = db().lock().unwrap();
    map.get(&(id as u32)).map(|c| c.chat).unwrap_or(0)
}

/// Issue 5: direct map lookup, no unsafe dereference of a raw pointer.
pub fn icon(id: i32) -> i32 {
    let map = db().lock().unwrap();
    map.get(&(id as u32)).map(|c| c.icon).unwrap_or(0)
}


pub unsafe fn rust_classdb_init(data_dir: *const i8) -> i32 {
    ffi_catch!(-1, unsafe { init(data_dir) })
}

pub fn rust_classdb_term() {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(term));
}

pub fn rust_classdb_search(id: i32) -> *mut ClassData {
    ffi_catch!(null_mut(), Arc::into_raw(search(id)) as *mut ClassData)
}

pub fn rust_classdb_searchexist(id: i32) -> *mut ClassData {
    ffi_catch!(null_mut(), match searchexist(id) {
        Some(arc) => Arc::into_raw(arc) as *mut ClassData,
        None => null_mut(),
    })
}

/// Decrements the Arc reference count for a pointer returned by
/// rust_classdb_search or rust_classdb_searchexist.
pub fn rust_classdb_free(ptr: *mut ClassData) {
    if !ptr.is_null() {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            unsafe { drop(Arc::from_raw(ptr as *const ClassData)); }
        }));
    }
}

pub fn rust_classdb_level(path: i32, lvl: i32) -> u32 {
    ffi_catch!(0, level(path, lvl))
}

pub fn rust_classdb_name(id: i32, rank: i32) -> *mut i8 {
    ffi_catch!(null_mut(), name(id, rank))
}

pub unsafe fn rust_classdb_free_name(ptr: *mut i8) {
    if !ptr.is_null() {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            unsafe { drop(std::ffi::CString::from_raw(ptr)); }
        }));
    }
}

pub fn rust_classdb_path(id: i32) -> i32 {
    ffi_catch!(0, path(id))
}

pub fn rust_classdb_chat(id: i32) -> i32 {
    ffi_catch!(0, chat(id))
}

pub fn rust_classdb_icon(id: i32) -> i32 {
    ffi_catch!(0, icon(id))
}
