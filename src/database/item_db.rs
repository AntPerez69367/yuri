#![allow(non_snake_case)]

use std::collections::HashMap;
use std::ffi::CStr;
use std::ptr::null_mut;
use std::sync::{Arc, Mutex, OnceLock};

use sqlx::Row;

use super::{blocking_run, get_pool};

use crate::common::constants::entity::player::ITM_ETC_U8;

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

pub(crate) static ITEM_DB: OnceLock<Mutex<HashMap<u32, Arc<ItemData>>>> = OnceLock::new();

fn db() -> &'static Mutex<HashMap<u32, Arc<ItemData>>> {
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

fn make_default(id: u32) -> ItemData {
    let mut item = ItemData {
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
        typ: ITM_ETC_U8,
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
    };
    str_to_fixed(&mut item.name, "??");
    str_to_fixed(&mut item.text, "??");
    str_to_fixed(&mut item.buytext, "??");
    item
}

pub(crate) async fn load_items() -> Result<usize, sqlx::Error> {
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
        let entry = map.entry(id).or_insert_with(|| Arc::new(make_default(id)));
        let item = Arc::get_mut(entry).expect("exclusive during init");
        item.id = id;
        str_to_fixed(&mut item.name, &row.try_get::<String, _>(1).unwrap_or_default());
        str_to_fixed(&mut item.yname, &row.try_get::<String, _>(2).unwrap_or_default());
        item.rank        = row.try_get::<u32, _>(3).map(|v| v as i32).unwrap_or(0);
        item.typ         = row.try_get::<u32, _>(4).map(|v| v as u8).unwrap_or(ITM_ETC_U8);
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

/// Returns item by ID, creating a default entry if missing.
pub fn search(id: u32) -> Arc<ItemData> {
    let mut map = db().lock().unwrap();
    let item = map.entry(id).or_insert_with(|| Arc::new(make_default(id)));
    Arc::clone(item)
}

/// Returns item by ID if it exists.
pub fn searchexist(id: u32) -> Option<Arc<ItemData>> {
    let map = db().lock().unwrap();
    map.get(&id).cloned()
}

/// Linear scan by name or yname (case-insensitive).
pub fn searchname(name: &str) -> Option<Arc<ItemData>> {
    let target = name.to_lowercase();
    let map = db().lock().unwrap();
    for item in map.values() {
        // SAFETY: name/yname are always null-terminated by str_to_fixed
        let item_name = unsafe { CStr::from_ptr(item.name.as_ptr()) }
            .to_string_lossy()
            .to_lowercase();
        let item_yname = unsafe { CStr::from_ptr(item.yname.as_ptr()) }
            .to_string_lossy()
            .to_lowercase();
        if item_name == target || item_yname == target {
            return Some(Arc::clone(item));
        }
    }
    None
}

/// Look up item ID by name string. Falls back to parsing as numeric ID.
pub fn id_by_name(name: &str) -> u32 {
    if let Some(item) = searchname(name) {
        return item.id;
    }
    if let Ok(n) = name.trim().parse::<u32>() {
        if n > 0 {
            if let Some(item) = searchexist(n) {
                return item.id;
            }
        }
    }
    0
}
