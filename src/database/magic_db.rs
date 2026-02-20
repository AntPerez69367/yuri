use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uchar};
use std::ptr::null_mut;
use std::sync::{Mutex, OnceLock};

use sqlx::Row;

use super::{blocking_run, get_pool};
use super::item_db::str_to_fixed;

#[repr(C)]
pub struct MagicData {
    pub id: c_int,
    pub typ: c_int,
    pub name: [c_char; 32],
    pub yname: [c_char; 32],
    pub question: [c_char; 64],
    pub script: [c_char; 64],
    pub script2: [c_char; 64],
    pub script3: [c_char; 64],
    pub dispell: c_uchar,
    pub aether: c_uchar,
    pub mute: c_uchar,
    pub level: c_uchar,
    pub mark: c_uchar,
    pub canfail: c_uchar,
    pub alignment: c_char,
    pub ticker: c_uchar,
    pub class: c_char,
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

pub fn init() -> c_int {
    MAGIC_DB.get_or_init(|| Mutex::new(HashMap::new()));
    match blocking_run(load_magic()) {
        Ok(n) => { println!("[magic] read done count={}", n); 0 }
        Err(e) => { eprintln!("[magic_db] load failed: {}", e); -1 }
    }
}

pub fn term() {
    if let Some(m) = MAGIC_DB.get() {
        m.lock().unwrap().clear();
    }
}

pub fn search(id: i32) -> *mut MagicData {
    let mut map = db().lock().unwrap();
    let m = map.entry(id).or_insert_with(|| make_default(id));
    m.as_mut() as *mut MagicData
}

pub fn searchexist(id: i32) -> *mut MagicData {
    let map = db().lock().unwrap();
    match map.get(&id) {
        Some(m) => m.as_ref() as *const MagicData as *mut MagicData,
        None => null_mut(),
    }
}

/// Searches by yname only (matches C behavior).
pub fn searchname(s: *const c_char) -> *mut MagicData {
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

pub fn id(s: *const c_char) -> c_int {
    let ptr = searchname(s);
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
pub fn level_by_name(s: *const c_char) -> c_int {
    let spell_id = id(s);
    if spell_id != 0 {
        unsafe { (*search(spell_id)).level as c_int }
    } else {
        0
    }
}
