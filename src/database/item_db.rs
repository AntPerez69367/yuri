#![allow(non_snake_case)]

use std::collections::HashMap;
use std::ffi::CStr;
use std::ptr::null_mut;
use std::sync::{Mutex, OnceLock};

use sqlx::Row;

use super::{blocking_run, get_pool};

const ITM_ETC: u8 = 18;

pub struct ItemData {
    pub id: u32,
    pub sound: u32,
    pub min_sdam: u32,
    pub max_sdam: u32,
    pub min_ldam: u32,
    pub max_ldam: u32,
    pub sound_hit: u32,
    pub time: u32,
    pub amount: u32,
    pub name: [i8; 64],
    pub yname: [i8; 64],
    pub text: [i8; 64],
    pub buytext: [i8; 64],
    pub typ: u8,
    pub class: u8,
    pub sex: u8,
    pub level: u8,
    pub icon_color: u8,
    pub ethereal: u8,
    pub unequip: u8,
    // 1 byte padding (implicit alignment)
    pub price: i32,
    pub sell: i32,
    pub rank: i32,
    pub stack_amount: i32,
    pub look: i32,
    pub look_color: i32,
    pub dura: i32,
    pub might: i32,
    pub will: i32,
    pub grace: i32,
    pub ac: i32,
    pub dam: i32,
    pub hit: i32,
    pub vita: i32,
    pub mana: i32,
    pub protection: i32,
    pub protected: i32,
    pub healing: i32,
    pub wisdom: i32,
    pub con: i32,
    pub attack_speed: i32,
    pub icon: i32,
    pub mightreq: i32,
    pub depositable: i32,
    pub exchangeable: i32,
    pub droppable: i32,
    pub thrown: i32,
    pub thrownconfirm: i32,
    pub repairable: i32,
    pub max_amount: i32,
    pub skinnable: i32,
    pub bod: i32,
    pub script: *mut i8,
    pub equip_script: *mut i8,
    pub unequip_script: *mut i8,
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

pub(crate) fn str_to_fixed<const N: usize>(dst: &mut [i8; N], src: &str) {
    let bytes = src.as_bytes();
    let len = bytes.len().min(N - 1);
    for i in 0..len {
        dst[i] = bytes[i] as i8;
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

pub fn init() -> i32 {
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
pub unsafe fn searchname(s: *const i8) -> *mut ItemData {
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


pub fn rust_itemdb_init() -> i32 { ffi_catch!(-1, init()) }

pub fn rust_itemdb_term() { ffi_catch!((), term()) }

pub fn rust_itemdb_search(id: u32) -> *mut ItemData { ffi_catch!(null_mut(), search(id)) }

pub fn rust_itemdb_searchexist(id: u32) -> *mut ItemData { ffi_catch!(null_mut(), searchexist(id)) }

pub unsafe fn rust_itemdb_searchname(s: *const i8) -> *mut ItemData { ffi_catch!(null_mut(), unsafe { searchname(s) }) }

pub unsafe fn rust_itemdb_id(s: *const i8) -> u32 {
    if s.is_null() { return 0; }
    ffi_catch!(0, {
        let ptr = unsafe { searchname(s) };
        if !ptr.is_null() {
            unsafe { (*ptr).id }
        } else {
            let str_val = unsafe { std::ffi::CStr::from_ptr(s) }.to_string_lossy();
            if let Ok(n) = str_val.trim().parse::<u32>() {
                if n > 0 {
                    let p = searchexist(n);
                    if !p.is_null() { unsafe { (*p).id } } else { 0 }
                } else { 0 }
            } else { 0 }
        }
    })
}

pub fn rust_itemdb_type(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).typ as i32 } } })
}

static UNKNOWN_ITEM_NAME: &[u8] = b"??\0";

pub fn rust_itemdb_name(id: u32) -> *mut i8 {
    ffi_catch!(UNKNOWN_ITEM_NAME.as_ptr() as *mut i8, {
        let p = search(id);
        if p.is_null() { UNKNOWN_ITEM_NAME.as_ptr() as *mut i8 } else { unsafe { (*p).name.as_mut_ptr() } }
    })
}
pub fn rust_itemdb_yname(id: u32) -> *mut i8 {
    ffi_catch!(UNKNOWN_ITEM_NAME.as_ptr() as *mut i8, {
        let p = search(id);
        if p.is_null() { UNKNOWN_ITEM_NAME.as_ptr() as *mut i8 } else { unsafe { (*p).yname.as_mut_ptr() } }
    })
}
pub fn rust_itemdb_text(id: u32) -> *mut i8 {
    ffi_catch!(UNKNOWN_ITEM_NAME.as_ptr() as *mut i8, {
        let p = search(id);
        if p.is_null() { UNKNOWN_ITEM_NAME.as_ptr() as *mut i8 } else { unsafe { (*p).text.as_mut_ptr() } }
    })
}
pub fn rust_itemdb_buytext(id: u32) -> *mut i8 {
    ffi_catch!(UNKNOWN_ITEM_NAME.as_ptr() as *mut i8, {
        let p = search(id);
        if p.is_null() { UNKNOWN_ITEM_NAME.as_ptr() as *mut i8 } else { unsafe { (*p).buytext.as_mut_ptr() } }
    })
}
pub fn rust_itemdb_price(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).price } } })
}
pub fn rust_itemdb_sell(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).sell } } })
}
pub fn rust_itemdb_rank(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).rank } } })
}
pub fn rust_itemdb_stackamount(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).stack_amount } } })
}
pub fn rust_itemdb_look(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).look } } })
}
pub fn rust_itemdb_lookcolor(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).look_color } } })
}
pub fn rust_itemdb_icon(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).icon } } })
}
pub fn rust_itemdb_iconcolor(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).icon_color as i32 } } })
}
pub fn rust_itemdb_sound(id: u32) -> u32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).sound } } })
}
pub fn rust_itemdb_soundhit(id: u32) -> u32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).sound_hit } } })
}
pub fn rust_itemdb_dura(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).dura } } })
}
pub fn rust_itemdb_might(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).might } } })
}
pub fn rust_itemdb_will(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).will } } })
}
pub fn rust_itemdb_grace(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).grace } } })
}
pub fn rust_itemdb_ac(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).ac } } })
}
pub fn rust_itemdb_dam(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).dam } } })
}
pub fn rust_itemdb_hit(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).hit } } })
}
pub fn rust_itemdb_vita(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).vita } } })
}
pub fn rust_itemdb_mana(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).mana } } })
}
pub fn rust_itemdb_protection(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).protection } } })
}
pub fn rust_itemdb_protected(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).protected } } })
}
pub fn rust_itemdb_minSdam(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).min_sdam as i32 } } })
}
pub fn rust_itemdb_maxSdam(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).max_sdam as i32 } } })
}
pub fn rust_itemdb_minLdam(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).min_ldam as i32 } } })
}
pub fn rust_itemdb_maxLdam(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).max_ldam as i32 } } })
}
pub fn rust_itemdb_mindam(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).min_sdam as i32 } } })
}
pub fn rust_itemdb_maxdam(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).max_sdam as i32 } } })
}
pub fn rust_itemdb_mincritdam(_id: u32) -> i32 { 0 }
pub fn rust_itemdb_maxcritdam(_id: u32) -> i32 { 0 }
pub fn rust_itemdb_mightreq(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).mightreq } } })
}
pub fn rust_itemdb_depositable(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).depositable } } })
}
pub fn rust_itemdb_exchangeable(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).exchangeable } } })
}
pub fn rust_itemdb_droppable(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).droppable } } })
}
pub fn rust_itemdb_thrown(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).thrown } } })
}
pub fn rust_itemdb_thrownconfirm(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).thrownconfirm } } })
}
pub fn rust_itemdb_repairable(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).repairable } } })
}
pub fn rust_itemdb_maxamount(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).max_amount } } })
}
pub fn rust_itemdb_skinnable(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).skinnable } } })
}
pub fn rust_itemdb_unequip(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).unequip as i32 } } })
}
pub fn rust_itemdb_ethereal(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).ethereal as i32 } } })
}
pub fn rust_itemdb_healing(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).healing } } })
}
pub fn rust_itemdb_wisdom(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).wisdom } } })
}
pub fn rust_itemdb_con(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).con } } })
}
pub fn rust_itemdb_attackspeed(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).attack_speed } } })
}
pub fn rust_itemdb_level(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).level as i32 } } })
}
pub fn rust_itemdb_class(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).class as i32 } } })
}
pub fn rust_itemdb_sex(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).sex as i32 } } })
}
pub fn rust_itemdb_time(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).time as i32 } } })
}
pub fn rust_itemdb_script(_id: u32) -> *mut i8 { null_mut() }
pub fn rust_itemdb_equipscript(_id: u32) -> *mut i8 { null_mut() }
pub fn rust_itemdb_unequipscript(_id: u32) -> *mut i8 { null_mut() }
pub fn rust_itemdb_breakondeath(id: u32) -> i32 {
    ffi_catch!(0, { let p = search(id); if p.is_null() { 0 } else { unsafe { (*p).bod } } })
}
pub fn rust_itemdb_reqvita(_id: u32) -> i32 { 0 }
pub fn rust_itemdb_reqmana(_id: u32) -> i32 { 0 }
pub fn rust_itemdb_dodge(_id: u32) -> i32 { 0 }
pub fn rust_itemdb_block(_id: u32) -> i32 { 0 }
pub fn rust_itemdb_parry(_id: u32) -> i32 { 0 }
pub fn rust_itemdb_resist(_id: u32) -> i32 { 0 }
pub fn rust_itemdb_physdeduct(_id: u32) -> i32 { 0 }
