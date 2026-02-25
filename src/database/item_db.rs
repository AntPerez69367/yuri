#![allow(non_snake_case)]

use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uchar, c_uint};
use std::ptr::null_mut;
use std::sync::{Mutex, OnceLock};

use sqlx::Row;

use super::{blocking_run, get_pool};

const ITM_ETC: c_uchar = 18;

#[repr(C)]
pub struct ItemData {
    pub id: c_uint,
    pub sound: c_uint,
    pub min_sdam: c_uint,
    pub max_sdam: c_uint,
    pub min_ldam: c_uint,
    pub max_ldam: c_uint,
    pub sound_hit: c_uint,
    pub time: c_uint,
    pub amount: c_uint,
    pub name: [c_char; 64],
    pub yname: [c_char; 64],
    pub text: [c_char; 64],
    pub buytext: [c_char; 64],
    pub typ: c_uchar,
    pub class: c_uchar,
    pub sex: c_uchar,
    pub level: c_uchar,
    pub icon_color: c_uchar,
    pub ethereal: c_uchar,
    pub unequip: c_uchar,
    // 1 byte padding (implicit via #[repr(C)] alignment)
    pub price: c_int,
    pub sell: c_int,
    pub rank: c_int,
    pub stack_amount: c_int,
    pub look: c_int,
    pub look_color: c_int,
    pub dura: c_int,
    pub might: c_int,
    pub will: c_int,
    pub grace: c_int,
    pub ac: c_int,
    pub dam: c_int,
    pub hit: c_int,
    pub vita: c_int,
    pub mana: c_int,
    pub protection: c_int,
    pub protected: c_int,
    pub healing: c_int,
    pub wisdom: c_int,
    pub con: c_int,
    pub attack_speed: c_int,
    pub icon: c_int,
    pub mightreq: c_int,
    pub depositable: c_int,
    pub exchangeable: c_int,
    pub droppable: c_int,
    pub thrown: c_int,
    pub thrownconfirm: c_int,
    pub repairable: c_int,
    pub max_amount: c_int,
    pub skinnable: c_int,
    pub bod: c_int,
    pub script: *mut c_char,
    pub equip_script: *mut c_char,
    pub unequip_script: *mut c_char,
}

// SAFETY: `script`, `equip_script`, and `unequip_script` are always `null_mut()` —
// they are never set from the database and the FFI layer returns `null_mut()` for all
// three script accessors. With only null pointers in those fields, sharing across
// threads is safe under the `Mutex<HashMap<…>>` that guards all access.
unsafe impl Send for ItemData {}
unsafe impl Sync for ItemData {}

static ITEM_DB: OnceLock<Mutex<HashMap<u32, Box<ItemData>>>> = OnceLock::new();

fn db() -> &'static Mutex<HashMap<u32, Box<ItemData>>> {
    ITEM_DB.get().expect("[item_db] not initialized")
}

pub(crate) fn str_to_fixed<const N: usize>(dst: &mut [c_char; N], src: &str) {
    let bytes = src.as_bytes();
    let len = bytes.len().min(N - 1);
    for i in 0..len {
        dst[i] = bytes[i] as c_char;
    }
    dst[len] = 0;
}

fn make_default(id: u32) -> Box<ItemData> {
    let mut item = Box::new(ItemData {
        id,
        sound: 0,
        min_sdam: 0,
        max_sdam: 0,
        min_ldam: 0,
        max_ldam: 0,
        sound_hit: 0,
        time: 0,
        amount: 0,
        name: [0; 64],
        yname: [0; 64],
        text: [0; 64],
        buytext: [0; 64],
        typ: ITM_ETC,
        class: 0,
        sex: 0,
        level: 0,
        icon_color: 0,
        ethereal: 0,
        unequip: 0,
        price: 0,
        sell: 0,
        rank: 0,
        stack_amount: 1,
        look: 0,
        look_color: 0,
        dura: 0,
        might: 0,
        will: 0,
        grace: 0,
        ac: 0,
        dam: 0,
        hit: 0,
        vita: 0,
        mana: 0,
        protection: 0,
        protected: 0,
        healing: 0,
        wisdom: 0,
        con: 0,
        attack_speed: 0,
        icon: 0,
        mightreq: 0,
        depositable: 0,
        exchangeable: 0,
        droppable: 0,
        thrown: 0,
        thrownconfirm: 0,
        repairable: 0,
        max_amount: 0,
        skinnable: 0,
        bod: 0,
        script: null_mut(),
        equip_script: null_mut(),
        unequip_script: null_mut(),
    });
    str_to_fixed(&mut item.name, "??");
    str_to_fixed(&mut item.text, "??");
    str_to_fixed(&mut item.buytext, "??");
    item
}

async fn load_items() -> Result<usize, sqlx::Error> {
    let pool = get_pool();
    let rows = sqlx::query(
        "SELECT ItmId, ItmDescription, ItmIdentifier, ItmMark, \
         ItmType, ItmBuyPrice, ItmSellPrice, ItmStackAmount, \
         ItmPthId, ItmSex, ItmLevel, ItmLook, \
         ItmLookColor, ItmIcon, ItmIconColor, ItmSound, \
         ItmDurability, ItmMight, ItmWill, ItmGrace, ItmArmor, \
         ItmHit, ItmDam, ItmVita, ItmMana, ItmProtection, \
         ItmMinimumSDamage, ItmMaximumSDamage, ItmText, ItmBuyText, \
         ItmExchangeable, ItmDepositable, ItmDroppable, ItmRepairable, \
         ItmWisdom, ItmCon, ItmThrown, ItmThrownConfirm, ItmMaximumAmount, \
         ItmIndestructible, ItmTimer, ItmMightRequired, \
         ItmSkinnable, ItmBoD, ItmMinimumLDamage, ItmMaximumLDamage, \
         ItmHealing, ItmProtected, ItmUnequip \
         FROM Items",
    )
    .fetch_all(pool)
    .await?;

    let count = rows.len();
    let mut map = ITEM_DB.get().unwrap().lock().unwrap();
    for row in rows {
        let id: u32 = row.try_get(0)?;
        let item = map.entry(id).or_insert_with(|| make_default(id));
        item.id = id;
        str_to_fixed(&mut item.name, &row.try_get::<String, _>(1).unwrap_or_default());
        str_to_fixed(&mut item.yname, &row.try_get::<String, _>(2).unwrap_or_default());
        item.rank        = row.try_get::<u32, _>(3).map(|v| v as i32).unwrap_or(0);
        item.typ         = row.try_get::<u32, _>(4).map(|v| v as u8).unwrap_or(ITM_ETC);
        item.price       = row.try_get::<u32, _>(5).map(|v| v as i32).unwrap_or(0);
        item.sell        = row.try_get::<u32, _>(6).map(|v| v as i32).unwrap_or(0);
        item.stack_amount = row.try_get::<u32, _>(7).map(|v| v as i32).unwrap_or(1);
        item.class       = row.try_get::<u32, _>(8).map(|v| v as u8).unwrap_or(0);
        item.sex         = row.try_get::<u32, _>(9).map(|v| v as u8).unwrap_or(0);
        item.level       = row.try_get::<u32, _>(10).map(|v| v as u8).unwrap_or(0);
        item.look        = row.try_get::<i32, _>(11).unwrap_or(0);  // INT (signed)
        item.look_color  = row.try_get::<u32, _>(12).map(|v| v as i32).unwrap_or(0);
        item.icon        = row.try_get::<u32, _>(13).map(|v| v as i32).unwrap_or(0);
        item.icon_color  = row.try_get::<u32, _>(14).map(|v| v as u8).unwrap_or(0);
        item.sound       = row.try_get::<u32, _>(15).unwrap_or(0);
        item.dura        = row.try_get::<u32, _>(16).map(|v| v as i32).unwrap_or(0);
        item.might       = row.try_get::<i32, _>(17).unwrap_or(0);  // INT (signed)
        item.will        = row.try_get::<i32, _>(18).unwrap_or(0);
        item.grace       = row.try_get::<i32, _>(19).unwrap_or(0);
        item.ac          = row.try_get::<i32, _>(20).unwrap_or(0);
        item.hit         = row.try_get::<i32, _>(21).unwrap_or(0);
        item.dam         = row.try_get::<i32, _>(22).unwrap_or(0);
        item.vita        = row.try_get::<i32, _>(23).unwrap_or(0);
        item.mana        = row.try_get::<i32, _>(24).unwrap_or(0);
        item.protection  = row.try_get::<i32, _>(25).unwrap_or(0);
        item.min_sdam    = row.try_get::<u32, _>(26).unwrap_or(0);
        item.max_sdam    = row.try_get::<u32, _>(27).unwrap_or(0);
        str_to_fixed(&mut item.text,    &row.try_get::<String, _>(28).unwrap_or_default());
        str_to_fixed(&mut item.buytext, &row.try_get::<String, _>(29).unwrap_or_default());
        item.exchangeable  = row.try_get::<u32, _>(30).map(|v| v as i32).unwrap_or(0);
        item.depositable   = row.try_get::<u32, _>(31).map(|v| v as i32).unwrap_or(0);
        item.droppable     = row.try_get::<u32, _>(32).map(|v| v as i32).unwrap_or(0);
        item.repairable    = row.try_get::<u32, _>(33).map(|v| v as i32).unwrap_or(0);
        item.wisdom        = row.try_get::<u32, _>(34).map(|v| v as i32).unwrap_or(0);
        item.con           = row.try_get::<i32, _>(35).unwrap_or(0);  // INT (signed)
        item.thrown        = row.try_get::<u32, _>(36).map(|v| v as i32).unwrap_or(0);
        item.thrownconfirm = row.try_get::<u32, _>(37).map(|v| v as i32).unwrap_or(0);
        item.max_amount    = row.try_get::<u32, _>(38).map(|v| v as i32).unwrap_or(0);
        item.ethereal      = row.try_get::<u32, _>(39).map(|v| v as u8).unwrap_or(0);
        item.time          = row.try_get::<u32, _>(40).unwrap_or(0);
        item.mightreq      = row.try_get::<u32, _>(41).map(|v| v as i32).unwrap_or(0);
        item.skinnable     = row.try_get::<u32, _>(42).map(|v| v as i32).unwrap_or(0);
        item.bod           = row.try_get::<u32, _>(43).map(|v| v as i32).unwrap_or(0);
        item.min_ldam      = row.try_get::<u32, _>(44).unwrap_or(0);
        item.max_ldam      = row.try_get::<u32, _>(45).unwrap_or(0);
        item.healing       = row.try_get::<i32, _>(46).unwrap_or(0);  // INT (signed)
        item.protected     = row.try_get::<i32, _>(47).unwrap_or(0);  // INT (signed)
        item.unequip       = row.try_get::<u32, _>(48).map(|v| v as u8).unwrap_or(0);
        item.icon += 49152;
    }
    Ok(count)
}

// ─── Public interface (called by ffi::item_db) ──────────────────────────────

pub fn init() -> c_int {
    ITEM_DB.get_or_init(|| Mutex::new(HashMap::new()));
    match blocking_run(load_items()) {
        Ok(n) => {
            tracing::info!("[item_db] read done count={n}");
            0
        }
        Err(e) => {
            tracing::error!("[item_db] load failed: {e}");
            -1
        }
    }
}

pub fn term() {
    if let Some(m) = ITEM_DB.get() {
        m.lock().unwrap().clear();
    }
}

/// Returns pointer to item, creating a default entry if missing.
///
/// # Safety
///
/// The returned pointer is valid only while the database is initialized and the map entry
/// remains present. Callers **must not** hold this pointer across any call that may modify
/// or clear the cache (e.g. `term()`). A safer alternative would be to return
/// `Arc<Mutex<ItemData>>` and update callers accordingly.
pub fn search(id: u32) -> *mut ItemData {
    let mut map = db().lock().unwrap();
    let item = map.entry(id).or_insert_with(|| make_default(id));
    item.as_mut() as *mut ItemData
}

/// Returns pointer to item if it exists, null otherwise.
///
/// # Safety
///
/// The returned pointer is valid only while the database is initialized and the map entry
/// remains present. Callers **must not** hold this pointer across any call that may modify
/// or clear the cache (e.g. `term()`). A safer alternative would be to return
/// `Option<Arc<Mutex<ItemData>>>` and update callers accordingly.
pub fn searchexist(id: u32) -> *mut ItemData {
    let map = db().lock().unwrap();
    match map.get(&id) {
        Some(item) => item.as_ref() as *const ItemData as *mut ItemData,
        None => null_mut(),
    }
}

/// Linear scan by name or yname (case-insensitive).
///
/// # Safety
///
/// The returned pointer points into a `Box<ItemData>` stored in the global map and is valid
/// only while the database is initialized and the entry is not removed (e.g. by `term()`).
/// A safer alternative would be to clone the matching entry and return `Box::into_raw` of
/// the clone, or change the return type to `Option<ItemData>`.
pub fn searchname(s: *const c_char) -> *mut ItemData {
    if s.is_null() {
        return null_mut();
    }
    let target = unsafe { CStr::from_ptr(s) }.to_string_lossy().to_lowercase();
    let map = db().lock().unwrap();
    for item in map.values() {
        let name = unsafe { CStr::from_ptr(item.name.as_ptr()) }
            .to_string_lossy()
            .to_lowercase();
        let yname = unsafe { CStr::from_ptr(item.yname.as_ptr()) }
            .to_string_lossy()
            .to_lowercase();
        if name == target || yname == target {
            return item.as_ref() as *const ItemData as *mut ItemData;
        }
    }
    null_mut()
}
