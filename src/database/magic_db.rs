use std::collections::HashMap;
use std::ffi::CStr;
use std::ptr::null_mut;
use std::sync::{Mutex, OnceLock};

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

static MAGIC_DB: OnceLock<Mutex<HashMap<i32, Box<MagicData>>>> = OnceLock::new();

fn db() -> &'static Mutex<HashMap<i32, Box<MagicData>>> {
    MAGIC_DB.get().expect("[magic_db] not initialized")
}

fn make_default(id: i32) -> Box<MagicData> {
    let mut m = Box::new(MagicData {
        id,
        typ: 0,
        name: [0; 32],
        yname: [0; 32],
        question: [0; 64],
        script: [0; 64],
        script2: [0; 64],
        script3: [0; 64],
        dispell: 0,
        aether: 0,
        mute: 0,
        level: 0,
        mark: 0,
        canfail: 0,
        alignment: 0,
        ticker: 0,
        class: 0,
    });
    str_to_fixed(&mut m.name, "??");
    m
}

async fn load_magic() -> Result<usize, sqlx::Error> {
    let pool = get_pool();
    // SplScript1/SplScript2/SplScript3 are intentionally omitted: the script
    // fields on MagicData are always left zeroed. See ffi/magic_db.rs where the
    // corresponding FFI accessors unconditionally return an empty string.
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
        let m = map.entry(id).or_insert_with(|| make_default(id));
        m.id = id;
        str_to_fixed(&mut m.name, &row.try_get::<String, _>(1).unwrap_or_default());
        str_to_fixed(&mut m.yname, &row.try_get::<String, _>(2).unwrap_or_default());
        m.typ      = row.try_get::<u32, _>(3).map(|v| v as i32).unwrap_or(0);
        str_to_fixed(&mut m.question, &row.try_get::<String, _>(4).unwrap_or_default());
        m.dispell  = row.try_get::<u32, _>(5).map(|v| v as u8).unwrap_or(0);
        m.aether   = row.try_get::<u32, _>(6).map(|v| v as u8).unwrap_or(0);
        m.mute     = row.try_get::<u32, _>(7).map(|v| v as u8).unwrap_or(0);
        m.class    = row.try_get::<i32, _>(8).map(|v| v as i8).unwrap_or(0); // INT signed
        m.level    = row.try_get::<u32, _>(9).map(|v| v as u8).unwrap_or(0);
        m.mark     = row.try_get::<u32, _>(10).map(|v| v as u8).unwrap_or(0);
        m.canfail  = row.try_get::<u32, _>(11).map(|v| v as u8).unwrap_or(0);
        m.alignment = row.try_get::<i8, _>(12).unwrap_or(0);                  // TINYINT signed
        m.ticker   = row.try_get::<u32, _>(13).map(|v| v as u8).unwrap_or(0);
    }
    Ok(count)
}

// ─── Public interface ────────────────────────────────────────────────────────

pub fn init() -> i32 {
    MAGIC_DB.get_or_init(|| Mutex::new(HashMap::new()));
    match blocking_run(load_magic()) {
        Ok(n) => { tracing::info!("[magic_db] read done count={n}"); 0 }
        Err(e) => { tracing::error!("[magic_db] load failed: {e}"); -1 }
    }
}

pub fn term() {
    if let Some(m) = MAGIC_DB.get() {
        m.lock().unwrap().clear();
    }
}

/// Returns a pointer to the `MagicData` for `id`, inserting a zeroed default
/// entry if one does not already exist.
///
/// # Safety
/// The returned pointer is valid until `term()` is called. Each map value is a
/// `Box<MagicData>` whose heap allocation is independent of the HashMap's
/// internal array, so HashMap growth never invalidates it. Callers must not
/// use the pointer after `term()` has been called (server shutdown).
pub fn search(id: i32) -> *mut MagicData {
    let mut map = db().lock().unwrap();
    let m = map.entry(id).or_insert_with(|| make_default(id));
    m.as_mut() as *mut MagicData
}

/// Returns a pointer to the `MagicData` for `id`, or null if no entry exists.
///
/// # Safety
/// Same lifetime contract as `search`: valid until `term()` is called.
pub fn searchexist(id: i32) -> *mut MagicData {
    let map = db().lock().unwrap();
    match map.get(&id) {
        Some(m) => m.as_ref() as *const MagicData as *mut MagicData,
        None => null_mut(),
    }
}

/// Searches by yname only (matches C behavior).
///
/// # Safety
/// - `s` must be a valid null-terminated C string or null (null returns null).
/// - The returned pointer shares the same lifetime contract as `search`: valid
///   until `term()` is called.
/// - `m.yname` is always null-terminated because `str_to_fixed` writes a zero
///   byte at `dst[len]` where `len ≤ N-1`, guaranteeing termination even for
///   maximum-length inputs.
pub unsafe fn searchname(s: *const i8) -> *mut MagicData {
    if s.is_null() { return null_mut(); }
    let target = unsafe { CStr::from_ptr(s) }.to_string_lossy().to_lowercase();
    let map = db().lock().unwrap();
    for m in map.values() {
        let yname = unsafe { CStr::from_ptr(m.yname.as_ptr()) }
            .to_string_lossy()
            .to_lowercase();
        if yname == target {
            return m.as_ref() as *const MagicData as *mut MagicData;
        }
    }
    null_mut()
}

pub unsafe fn id(s: *const i8) -> i32 {
    if s.is_null() { return 0; }
    let ptr = unsafe { searchname(s) };
    if !ptr.is_null() {
        return unsafe { (*ptr).id };
    }
    let str_val = unsafe { CStr::from_ptr(s) }.to_string_lossy();
    if let Ok(n) = str_val.trim().parse::<i32>() {
        if n > 0 {
            let p = searchexist(n);
            if !p.is_null() {
                return unsafe { (*p).id };
            }
        }
    }
    0
}

/// Takes a spell name string, returns the level field.
pub unsafe fn level_by_name(s: *const i8) -> i32 {
    if s.is_null() { return 0; }
    let spell_id = unsafe { id(s) };
    if spell_id != 0 {
        unsafe { (*search(spell_id)).level as i32 }
    } else {
        0
    }
}


static EMPTY: &[u8] = b"\0";

pub fn rust_magicdb_init() -> i32 { ffi_catch!(-1, init()) }

pub fn rust_magicdb_term() { ffi_catch!((), term()) }

pub fn rust_magicdb_search(id: i32) -> *mut MagicData { ffi_catch!(null_mut(), search(id)) }

pub fn rust_magicdb_searchexist(id: i32) -> *mut MagicData { ffi_catch!(null_mut(), searchexist(id)) }

pub unsafe fn rust_magicdb_searchname(s: *const i8) -> *mut MagicData { ffi_catch!(null_mut(), unsafe { searchname(s) }) }

pub unsafe fn rust_magicdb_id(s: *const i8) -> i32 { ffi_catch!(0, unsafe { id(s) }) }

pub fn rust_magicdb_name(id: i32) -> *mut i8 {
    ffi_catch!(null_mut(), {
        let p = search(id);
        if p.is_null() { null_mut() } else { unsafe { (*p).name.as_mut_ptr() } }
    })
}
pub fn rust_magicdb_yname(id: i32) -> *mut i8 {
    ffi_catch!(null_mut(), {
        let p = search(id);
        if p.is_null() { null_mut() } else { unsafe { (*p).yname.as_mut_ptr() } }
    })
}
pub fn rust_magicdb_question(id: i32) -> *mut i8 {
    ffi_catch!(null_mut(), {
        let p = search(id);
        if p.is_null() { null_mut() } else { unsafe { (*p).question.as_mut_ptr() } }
    })
}
pub fn rust_magicdb_type(id: i32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).typ } } })
}
pub fn rust_magicdb_dispel(id: i32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).dispell as i32 } } })
}
pub fn rust_magicdb_aether(id: i32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).aether as i32 } } })
}
pub fn rust_magicdb_mute(id: i32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).mute as i32 } } })
}
pub fn rust_magicdb_canfail(id: i32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).canfail as i32 } } })
}
pub fn rust_magicdb_alignment(id: i32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).alignment as i32 } } })
}
pub fn rust_magicdb_ticker(id: i32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).ticker as i32 } } })
}
pub unsafe fn rust_magicdb_level(s: *const i8) -> i32 { ffi_catch!(0, unsafe { level_by_name(s) }) }

pub fn rust_magicdb_script(_id: i32) -> *const i8 {
    EMPTY.as_ptr() as *const i8
}
pub fn rust_magicdb_script2(_id: i32) -> *const i8 {
    EMPTY.as_ptr() as *const i8
}
pub fn rust_magicdb_script3(_id: i32) -> *const i8 {
    EMPTY.as_ptr() as *const i8
}
