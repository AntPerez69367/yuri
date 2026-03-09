use std::collections::HashMap;
use std::ffi::CStr;
use std::ptr::null_mut;
use std::sync::{Mutex, OnceLock};

use sqlx::Row;

use super::{blocking_run, get_pool};
use super::item_db::str_to_fixed;

const MAX_EQUIP: usize = 15;

/// Live item instance (not the item template).
/// Also used by pc.rs inventory — move to a shared types module when pc.rs is written.
pub struct MobItem {
    pub id: u32,
    pub owner: u32,
    pub custom: u32,
    pub time: u32,
    pub dura: i32,
    pub amount: i32,
    pub pos: u8,
    pub _pad: [u8; 3],
    pub custom_look: u32,
    pub custom_icon: u32,
    pub custom_look_color: u32,
    pub custom_icon_color: u32,
    pub protected_: u32,
    pub traps_table: [u32; 100],
    pub buytext: [u8; 64],
    pub note: [i8; 300],
    pub repair: i8,
    pub real_name: [i8; 64],
}

/// Mob database entry.
/// The `equip` array is populated by a MobEquipment sub-query for NPC mobs (mobtype == 1).
pub struct MobDbData {
    pub equip: [MobItem; 15], // MAX_EQUIP from mmo.h
    pub vita: i32,
    pub r#type: i32,
    pub subtype: i32,
    pub look: i32,
    pub look_color: i32,
    pub hit: i32,
    pub level: i32,
    pub might: i32,
    pub grace: i32,
    pub will: i32,
    pub movetime: i32,
    pub atktime: i32,
    pub spawntime: i32,
    pub baseac: i32,
    pub sound: i32,
    pub mana: i32,
    pub owner: u32,
    pub id: u32,
    pub mindam: u32,
    pub maxdam: u32,
    pub exp: u32,
    pub name: [i8; 45],
    pub yname: [i8; 45],
    pub block: i8,
    pub retdist: i8,
    pub mobtype: u8,
    pub state: i8,
    pub race: i8,
    pub seeinvis: i8,
    pub tier: i8,
    pub mark: u8,
    pub isnpc: u8,
    pub isboss: u8,
    pub protection: i16,
    pub miss: i16,
    pub sex: u16,
    pub face: u16,
    pub face_color: u16,
    pub hair: u16,
    pub hair_color: u16,
    pub armor_color: u16,
    pub skin_color: u16,
    pub startm: u16,
    pub startx: u16,
    pub starty: u16,
}

unsafe impl Send for MobDbData {}
unsafe impl Sync for MobDbData {}

static MOB_DB: OnceLock<Mutex<HashMap<u32, Box<MobDbData>>>> = OnceLock::new();

fn db() -> &'static Mutex<HashMap<u32, Box<MobDbData>>> {
    MOB_DB.get().expect("[mob_db] not initialized")
}

fn make_default(id: u32) -> Box<MobDbData> {
    // SAFETY: all-zero is a valid initial state for this repr(C) struct.
    let mut m: Box<MobDbData> = unsafe { Box::new(std::mem::zeroed()) };
    m.id = id;
    str_to_fixed(&mut m.name, "??");
    m
}

// The C code runs a SELECT from `Mobs` (35 columns) then, for each mob where
// mobtype == 1, runs a per-mob SELECT from `MobEquipment` to fill equip[slot].
//
// In Rust you can choose:
//   a) Same N+1 pattern: main query then one sub-query per NPC mob.
//   b) Single LEFT JOIN on MobEquipment (repeats mob columns per equip row).
//   c) Two-phase: main query + one bulk MobEquipment query, joined in memory.
//
// Return Ok(count) where count is the number of mob templates loaded.
//
// Column order for Mobs SELECT (match C bind order):
//   0  MobId          u32  → id
//   1  MobDescription str  → name[45]
//   2  MobIdentifier  str  → yname[45]
//   3  MobBehavior    i32  → type
//   4  MobAI          i32  → subtype
//   5  MobLook        i32  → look
//   6  MobLookColor   i32  → look_color
//   7  MobVita        i32  → vita
//   8  MobMana        i32  → mana
//   9  MobExperience  u32  → exp
//   10 MobHit         i32  → hit
//   11 MobLevel       i32  → level
//   12 MobMight       i32  → might
//   13 MobGrace       i32  → grace
//   14 MobMoveTime    i32  → movetime
//   15 MobSpawnTime   i32  → spawntime
//   16 MobArmor       i32  → baseac
//   17 MobSound       i32  → sound
//   18 MobAttackTime  i32  → atktime
//   19 MobProtection  i16  → protection
//   20 MobReturnDistance u8 → retdist
//   21 MobSex         u16  → sex
//   22 MobFace        u16  → face
//   23 MobFaceColor   u16  → face_color
//   24 MobHair        u16  → hair
//   25 MobHairColor   u16  → hair_color
//   26 MobSkinColor   u16  → skin_color
//   27 MobState       i8   → state
//   28 MobIsChar      u8   → mobtype
//   29 MobWill        i32  → will
//   30 MobMinimumDamage u32 → mindam
//   31 MobMaximumDamage u32 → maxdam
//   32 MobMark        u8   → mark
//   33 MobIsNpc       u8   → isnpc
//   34 MobIsBoss      u8   → isboss
//
// MobEquipment columns: MeqLook→item.id, MeqColor→item.custom, MeqSlot→pos (index into equip[])
async fn load_mobs() -> Result<usize, sqlx::Error> {
    let pool = get_pool();
    let rows = sqlx::query(
        "SELECT `MobId`, `MobDescription`, `MobIdentifier`, \
         `MobBehavior`, `MobAI`, `MobLook`, `MobLookColor`, `MobVita`, \
         `MobMana`, `MobExperience`, `MobHit`, `MobLevel`, `MobMight`, \
         `MobGrace`, `MobMoveTime`, `MobSpawnTime`, `MobArmor`, \
         `MobSound`, `MobAttackTime`, `MobProtection`, \
         `MobReturnDistance`, `MobSex`, `MobFace`, `MobFaceColor`, \
         `MobHair`, `MobHairColor`, `MobSkinColor`, `MobState`, \
         `MobIsChar`, `MobWill`, `MobMinimumDamage`, `MobMaximumDamage`, \
         `MobMark`, `MobIsNpc`, `MobIsBoss` FROM `Mobs`",
    )
    .fetch_all(pool)
    .await?;

    let count = rows.len();
    let mut map = MOB_DB.get().unwrap().lock().unwrap();

    for row in &rows {
        let id: u32 = row.try_get(0)?;
        let m = map.entry(id).or_insert_with(|| make_default(id));
        m.id         = id;
        str_to_fixed(&mut m.name,  &row.try_get::<String, _>(1).unwrap_or_default());
        str_to_fixed(&mut m.yname, &row.try_get::<String, _>(2).unwrap_or_default());
        // All columns are int(10) unsigned in MySQL → read as u32, cast to target C type.
        // Only MobArmor (col 16) is signed int.
        m.r#type     = row.try_get::<u32, _>(3).unwrap_or(0)  as i32;
        m.subtype    = row.try_get::<u32, _>(4).unwrap_or(0)  as i32;
        m.look       = row.try_get::<u32, _>(5).unwrap_or(0)  as i32;
        m.look_color = row.try_get::<u32, _>(6).unwrap_or(0)  as i32;
        m.vita       = row.try_get::<u32, _>(7).unwrap_or(0)  as i32;
        m.mana       = row.try_get::<u32, _>(8).unwrap_or(0)  as i32;
        m.exp        = row.try_get::<u32, _>(9).unwrap_or(0);
        m.hit        = row.try_get::<u32, _>(10).unwrap_or(0) as i32;
        m.level      = row.try_get::<u32, _>(11).unwrap_or(0) as i32;
        m.might      = row.try_get::<u32, _>(12).unwrap_or(0) as i32;
        m.grace      = row.try_get::<u32, _>(13).unwrap_or(0) as i32;
        m.movetime   = row.try_get::<u32, _>(14).unwrap_or(0) as i32;
        m.spawntime  = row.try_get::<u32, _>(15).unwrap_or(0) as i32;
        m.baseac     = row.try_get::<i32, _>(16).unwrap_or(0);          // signed
        m.sound      = row.try_get::<u32, _>(17).unwrap_or(0) as i32;
        m.atktime    = row.try_get::<u32, _>(18).unwrap_or(0) as i32;
        m.protection = row.try_get::<u32, _>(19).unwrap_or(0) as i16;
        m.retdist    = row.try_get::<u32, _>(20).unwrap_or(0) as i8;
        m.sex        = row.try_get::<u32, _>(21).unwrap_or(0) as u16;
        m.face       = row.try_get::<u32, _>(22).unwrap_or(0) as u16;
        m.face_color = row.try_get::<u32, _>(23).unwrap_or(0) as u16;
        m.hair       = row.try_get::<u32, _>(24).unwrap_or(0) as u16;
        m.hair_color = row.try_get::<u32, _>(25).unwrap_or(0) as u16;
        m.skin_color = row.try_get::<u32, _>(26).unwrap_or(0) as u16;
        m.state      = row.try_get::<u32, _>(27).unwrap_or(0) as i8;
        m.mobtype    = row.try_get::<u32, _>(28).unwrap_or(0) as u8;
        m.will       = row.try_get::<u32, _>(29).unwrap_or(0) as i32;
        m.mindam     = row.try_get::<u32, _>(30).unwrap_or(0);
        m.maxdam     = row.try_get::<u32, _>(31).unwrap_or(0);
        m.mark       = row.try_get::<u32, _>(32).unwrap_or(0) as u8;
        m.isnpc      = row.try_get::<u32, _>(33).unwrap_or(0) as u8;
        m.isboss     = row.try_get::<u32, _>(34).unwrap_or(0) as u8;

        if m.mobtype == 1 {
            let eq_rows = sqlx::query(
                "SELECT `MeqLook`, `MeqColor`, `MeqSlot` \
                 FROM `MobEquipment` WHERE `MeqMobId` = ? LIMIT 14",
            )
            .bind(id)
            .fetch_all(pool)
            .await?;

            for eq in &eq_rows {
                let slot = eq.try_get::<u8, _>(2).unwrap_or(0) as usize;
                if slot < MAX_EQUIP {
                    m.equip[slot].id     = eq.try_get::<u32, _>(0).unwrap_or(0);
                    m.equip[slot].amount = 1;
                    m.equip[slot].custom = eq.try_get::<u32, _>(1).unwrap_or(0);
                }
            }
        }
    }

    Ok(count)
}

// ─── Public interface ────────────────────────────────────────────────────────

pub fn init() -> i32 {
    MOB_DB.get_or_init(|| Mutex::new(HashMap::new()));
    match blocking_run(load_mobs()) {
        Ok(n) => { tracing::info!("[mob_db] read done count={n}"); 0 }
        Err(e) => { tracing::error!("[mob_db] load failed: {e}"); -1 }
    }
}

pub fn term() {
    if let Some(m) = MOB_DB.get() {
        m.lock().unwrap().clear();
    }
}

/// Returns a pointer to the `MobDbData` for `id`, inserting a default entry if absent.
pub fn search(id: u32) -> *mut MobDbData {
    let mut map = db().lock().unwrap();
    let m = map.entry(id).or_insert_with(|| make_default(id));
    m.as_mut() as *mut MobDbData
}

pub fn searchexist(id: u32) -> *mut MobDbData {
    let map = db().lock().unwrap();
    match map.get(&id) {
        Some(m) => m.as_ref() as *const MobDbData as *mut MobDbData,
        None => null_mut(),
    }
}

/// Searches by `yname` (script identifier), case-insensitive.
pub unsafe fn searchname(s: *const i8) -> *mut MobDbData {
    if s.is_null() { return null_mut(); }
    let target = unsafe { CStr::from_ptr(s) }.to_string_lossy().to_lowercase();
    let map = db().lock().unwrap();
    for m in map.values() {
        // SAFETY: str_to_fixed always NUL-terminates within the array bounds.
        let yname = unsafe { CStr::from_ptr(m.yname.as_ptr()) }.to_string_lossy().to_lowercase();
        if yname == target {
            return m.as_ref() as *const MobDbData as *mut MobDbData;
        }
    }
    null_mut()
}

pub fn level(id: u32) -> i32 {
    let map = db().lock().unwrap();
    map.get(&id).map(|m| m.level).unwrap_or(0)
}

pub fn experience(id: u32) -> u32 {
    let map = db().lock().unwrap();
    map.get(&id).map(|m| m.exp).unwrap_or(0)
}

/// Finds a mob id by yname string. Returns 0 if not found.
pub unsafe fn find_id(s: *const i8) -> i32 {
    let ptr = unsafe { searchname(s) };
    if ptr.is_null() { return 0; }
    unsafe { (*ptr).id as i32 }
}



pub fn rust_mobdb_init() -> i32 { ffi_catch!(-1, init()) }

pub fn rust_mobdb_term() { ffi_catch!((), term()) }

pub fn rust_mobdb_search(id: u32) -> *mut MobDbData {
    ffi_catch!(null_mut(), search(id))
}

pub fn rust_mobdb_searchexist(id: u32) -> *mut MobDbData {
    ffi_catch!(null_mut(), searchexist(id))
}

pub unsafe fn rust_mobdb_searchname(s: *const i8) -> *mut MobDbData {
    if s.is_null() { return null_mut(); }
    ffi_catch!(null_mut(), unsafe { searchname(s) })
}

pub fn rust_mobdb_level(id: u32) -> i32 {
    ffi_catch!(0, level(id))
}

pub fn rust_mobdb_experience(id: u32) -> u32 {
    ffi_catch!(0, experience(id))
}

pub unsafe fn rust_mobdb_id(s: *const i8) -> i32 {
    if s.is_null() { return 0; }
    ffi_catch!(0, unsafe { find_id(s) })
}
