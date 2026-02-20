use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uint};
use std::ptr::null_mut;
use std::sync::{Mutex, OnceLock};

use sqlx::Row;

use super::{blocking_run, get_pool};
use super::item_db::str_to_fixed;

#[repr(C)]
pub struct RecipeData {
    pub id: c_int,
    pub tokens_required: c_int,
    /// Alternating [material_id, amount] pairs × 5: [mat1, amt1, mat2, amt2, ...]
    pub materials: [c_int; 10],
    pub superior_materials: [c_int; 2],
    pub identifier: [c_char; 64],
    pub description: [c_char; 64],
    pub crit_identifier: [c_char; 64],
    pub crit_description: [c_char; 64],
    pub craft_time: c_uint,
    pub success_rate: c_uint,
    pub skill_advance: c_uint,
    pub crit_rate: c_uint,
    pub bonus: c_uint,
    pub skill_required: c_uint,
}

unsafe impl Send for RecipeData {}
unsafe impl Sync for RecipeData {}

static RECIPE_DB: OnceLock<Mutex<HashMap<u32, Box<RecipeData>>>> = OnceLock::new();

fn db() -> &'static Mutex<HashMap<u32, Box<RecipeData>>> {
    RECIPE_DB.get().expect("[recipe_db] not initialized")
}

fn make_default(id: u32) -> Box<RecipeData> {
    let mut r = Box::new(RecipeData {
        id: id as c_int,
        tokens_required: 0,
        materials: [0; 10],
        superior_materials: [0; 2],
        identifier: [0; 64],
        description: [0; 64],
        crit_identifier: [0; 64],
        crit_description: [0; 64],
        craft_time: 0,
        success_rate: 0,
        skill_advance: 0,
        crit_rate: 0,
        bonus: 0,
        skill_required: 0,
    });
    str_to_fixed(&mut r.identifier, "??");
    str_to_fixed(&mut r.description, "??");
    str_to_fixed(&mut r.crit_identifier, "??");
    str_to_fixed(&mut r.crit_description, "??");
    r
}

async fn load_recipes() -> Result<usize, sqlx::Error> {
    let pool = get_pool();
    let rows = sqlx::query(
        "SELECT RecId, RecIdentifier, RecDescription, \
         RecSuccessRate, RecCraftTime, RecSkillAdvance, \
         RecCritIdentifier, RecCritDescription, RecTokensRequired, \
         RecMaterial1, RecAmount1, RecMaterial2, RecAmount2, \
         RecMaterial3, RecAmount3, RecMaterial4, RecAmount4, \
         RecMaterial5, RecAmount5, \
         RecCritRate, RecBonus, RecSkillRequired, \
         RecSuperiorMaterial1, RecSuperiorAmount1 \
         FROM Recipes",
    )
    .fetch_all(pool)
    .await?;

    let count = rows.len();
    let mut map = RECIPE_DB.get().unwrap().lock().unwrap();
    for row in rows {
        let id: u32 = row.try_get(0)?;
        let r = map.entry(id).or_insert_with(|| make_default(id));
        r.id = id as c_int;
        str_to_fixed(&mut r.identifier, &row.try_get::<String, _>(1).unwrap_or_default());
        str_to_fixed(&mut r.description, &row.try_get::<String, _>(2).unwrap_or_default());
        r.success_rate = row.try_get::<u32, _>(3).unwrap_or(0);
        r.craft_time = row.try_get::<u32, _>(4).unwrap_or(0);
        r.skill_advance = row.try_get::<u32, _>(5).unwrap_or(0);
        str_to_fixed(&mut r.crit_identifier, &row.try_get::<String, _>(6).unwrap_or_default());
        str_to_fixed(&mut r.crit_description, &row.try_get::<String, _>(7).unwrap_or_default());
        r.tokens_required = row.try_get::<u32, _>(8).unwrap_or(0) as c_int;
        for i in 0..10usize {
            r.materials[i] = row.try_get::<u32, _>(9 + i).unwrap_or(0) as c_int;
        }
        r.crit_rate = row.try_get::<u32, _>(19).unwrap_or(0);
        r.bonus = row.try_get::<u32, _>(20).unwrap_or(0);
        r.skill_required = row.try_get::<u32, _>(21).unwrap_or(0);
        r.superior_materials[0] = row.try_get::<u32, _>(22).unwrap_or(0) as c_int;
        r.superior_materials[1] = row.try_get::<u32, _>(23).unwrap_or(0) as c_int;
    }
    Ok(count)
}

// ─── Public interface ────────────────────────────────────────────────────────

pub fn init() -> c_int {
    RECIPE_DB.get_or_init(|| Mutex::new(HashMap::new()));
    match blocking_run(load_recipes()) {
        Ok(n) => { println!("[recipedb] read done count={}", n); 0 }
        Err(e) => { eprintln!("[recipe_db] load failed: {}", e); -1 }
    }
}

pub fn term() {
    if let Some(m) = RECIPE_DB.get() {
        m.lock().unwrap().clear();
    }
}

pub fn search(id: u32) -> *mut RecipeData {
    let mut map = db().lock().unwrap();
    let r = map.entry(id).or_insert_with(|| make_default(id));
    r.as_mut() as *mut RecipeData
}

pub fn searchexist(id: u32) -> *mut RecipeData {
    let map = db().lock().unwrap();
    match map.get(&id) {
        Some(r) => r.as_ref() as *const RecipeData as *mut RecipeData,
        None => null_mut(),
    }
}

pub fn searchname(s: *const c_char) -> *mut RecipeData {
    if s.is_null() { return null_mut(); }
    let target = unsafe { CStr::from_ptr(s) }.to_string_lossy().to_lowercase();
    let map = db().lock().unwrap();
    for r in map.values() {
        let ident = unsafe { CStr::from_ptr(r.identifier.as_ptr()) }.to_string_lossy().to_lowercase();
        let desc = unsafe { CStr::from_ptr(r.description.as_ptr()) }.to_string_lossy().to_lowercase();
        let crit_ident = unsafe { CStr::from_ptr(r.crit_identifier.as_ptr()) }.to_string_lossy().to_lowercase();
        let crit_desc = unsafe { CStr::from_ptr(r.crit_description.as_ptr()) }.to_string_lossy().to_lowercase();
        if ident == target || desc == target || crit_ident == target || crit_desc == target {
            return r.as_ref() as *const RecipeData as *mut RecipeData;
        }
    }
    null_mut()
}
