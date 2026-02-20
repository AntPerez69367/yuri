use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uint};
use std::ptr::null_mut;
use std::sync::{Mutex, OnceLock};

use sqlx::Row;

use super::{blocking_run, get_pool};
use super::item_db::str_to_fixed;

#[repr(C)]
pub struct ClassData {
    /// 16 rank name strings, each 32 bytes (rank0..rank15)
    pub ranks: [[c_char; 32]; 16],
    pub id: u16,
    pub path: u16,
    pub level: [c_uint; 99],
    pub chat: c_int,
    pub icon: c_int,
}

unsafe impl Send for ClassData {}
unsafe impl Sync for ClassData {}

static CLASS_DB: OnceLock<Mutex<HashMap<u32, Box<ClassData>>>> = OnceLock::new();

fn db() -> &'static Mutex<HashMap<u32, Box<ClassData>>> {
    CLASS_DB.get().expect("[class_db] not initialized")
}

fn make_default(id: u32) -> Box<ClassData> {
    let mut c = Box::new(ClassData {
        ranks: [[0; 32]; 16],
        id: id as u16,
        path: 0,
        level: [0; 99],
        chat: 0,
        icon: 0,
    });
    str_to_fixed(&mut c.ranks[0], "??");
    c
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
        let c = map.entry(id).or_insert_with(|| make_default(id));
        c.id   = id as u16;
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
    use std::fs;
    let path = format!("{}tnl_exp.csv", data_dir);
    let contents = fs::read_to_string(&path)
        .map_err(|e| { eprintln!("DB_ERR: Can't read level db ({}).", path); e })?;

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
        let c = map.entry(path_id).or_insert_with(|| make_default(path_id));
        for x in 1..parts.len().min(99) {
            c.level[x] = parts[x].trim().parse().unwrap_or(0);
        }
        count += 1;
    }
    Ok(count)
}

// ─── Public interface (called by ffi::class_db) ─────────────────────────────

pub fn init(data_dir: *const c_char) -> c_int {
    CLASS_DB.get_or_init(|| Mutex::new(HashMap::new()));

    match blocking_run(load_classes()) {
        Ok(n) => println!("[class_db] read done count={}", n),
        Err(e) => { eprintln!("[class_db] load failed: {}", e); return -1; }
    }

    let dir = if data_dir.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(data_dir) }.to_string_lossy().into_owned()
    };
    match load_leveldb(&dir) {
        Ok(n) => println!("[leveldb] read done count={}", n),
        Err(_) => return -1,
    }
    0
}

pub fn term() {
    if let Some(m) = CLASS_DB.get() {
        m.lock().unwrap().clear();
    }
}

pub fn search(id: i32) -> *mut ClassData {
    let key = id as u32;
    let mut map = db().lock().unwrap();
    let c = map.entry(key).or_insert_with(|| make_default(key));
    c.as_mut() as *mut ClassData
}

pub fn searchexist(id: i32) -> *mut ClassData {
    let map = db().lock().unwrap();
    match map.get(&(id as u32)) {
        Some(c) => c.as_ref() as *const ClassData as *mut ClassData,
        None => null_mut(),
    }
}

pub fn level(path: i32, lvl: i32) -> c_uint {
    let map = db().lock().unwrap();
    match map.get(&(path as u32)) {
        Some(c) if (lvl as usize) < 99 => c.level[lvl as usize],
        _ => 0,
    }
}

pub fn name(id: i32, rank: i32) -> *mut c_char {
    let map = db().lock().unwrap();
    match map.get(&(id as u32)) {
        Some(c) => {
            let idx = (rank as usize).min(15);
            c.ranks[idx].as_ptr() as *mut c_char
        }
        None => null_mut(),
    }
}

pub fn path(id: i32) -> c_int {
    let map = db().lock().unwrap();
    match map.get(&(id as u32)) {
        Some(c) => c.path as c_int,
        None => 0,
    }
}

pub fn chat(id: i32) -> c_int {
    unsafe { (*search(id)).chat }
}

pub fn icon(id: i32) -> c_int {
    unsafe { (*search(id)).icon }
}
