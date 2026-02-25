use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_schar, c_short, c_uchar, c_uint, c_ushort};
use std::ptr::null_mut;
use std::sync::{Mutex, OnceLock};

use sqlx::Row;

use super::{blocking_run, get_pool};
use super::item_db::str_to_fixed;

const MAX_EQUIP: usize = 15;

/// Mirrors `struct item` from mmo.h (live item instance, not item template).
/// Also used by pc.rs inventory — move to a shared types module when pc.rs is written.
#[repr(C)]
pub struct MobItem {
    pub id: c_uint,
    pub owner: c_uint,
    pub custom: c_uint,
    pub time: c_uint,
    pub dura: c_int,
    pub amount: c_int,
    pub pos: c_uchar,
    pub _pad: [u8; 3],
    pub custom_look: c_uint,
    pub custom_icon: c_uint,
    pub custom_look_color: c_uint,
    pub custom_icon_color: c_uint,
    pub protected_: c_uint,
    pub traps_table: [c_uint; 100],
    pub buytext: [c_uchar; 64],
    pub note: [c_char; 300],
    pub repair: c_schar,
    pub real_name: [c_char; 64],
}

/// Mirrors `struct mobdb_data` from map_server.h.
/// The `equip` array is populated by a MobEquipment sub-query for NPC mobs (mobtype == 1).
#[repr(C)]
pub struct MobDbData {
    pub equip: [MobItem; 15], // MAX_EQUIP from mmo.h
    pub vita: c_int,
    pub r#type: c_int,
    pub subtype: c_int,
    pub look: c_int,
    pub look_color: c_int,
    pub hit: c_int,
    pub level: c_int,
    pub might: c_int,
    pub grace: c_int,
    pub will: c_int,
    pub movetime: c_int,
    pub atktime: c_int,
    pub spawntime: c_int,
    pub baseac: c_int,
    pub sound: c_int,
    pub mana: c_int,
    pub owner: c_uint,
    pub id: c_uint,
    pub mindam: c_uint,
    pub maxdam: c_uint,
    pub exp: c_uint,
    pub name: [c_char; 45],
    pub yname: [c_char; 45],
    pub block: c_schar,
    pub retdist: c_schar,
    pub mobtype: c_uchar,
    pub state: c_schar,
    pub race: c_schar,
    pub seeinvis: c_schar,
    pub tier: c_schar,
    pub mark: c_uchar,
    pub isnpc: c_uchar,
    pub isboss: c_uchar,
    pub protection: c_short,
    pub miss: c_short,
    pub sex: c_ushort,
    pub face: c_ushort,
    pub face_color: c_ushort,
    pub hair: c_ushort,
    pub hair_color: c_ushort,
    pub armor_color: c_ushort,
    pub skin_color: c_ushort,
    pub startm: c_ushort,
    pub startx: c_ushort,
    pub starty: c_ushort,
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
        m.r#type     = row.try_get::<u32, _>(3).unwrap_or(0)  as c_int;
        m.subtype    = row.try_get::<u32, _>(4).unwrap_or(0)  as c_int;
        m.look       = row.try_get::<u32, _>(5).unwrap_or(0)  as c_int;
        m.look_color = row.try_get::<u32, _>(6).unwrap_or(0)  as c_int;
        m.vita       = row.try_get::<u32, _>(7).unwrap_or(0)  as c_int;
        m.mana       = row.try_get::<u32, _>(8).unwrap_or(0)  as c_int;
        m.exp        = row.try_get::<u32, _>(9).unwrap_or(0);
        m.hit        = row.try_get::<u32, _>(10).unwrap_or(0) as c_int;
        m.level      = row.try_get::<u32, _>(11).unwrap_or(0) as c_int;
        m.might      = row.try_get::<u32, _>(12).unwrap_or(0) as c_int;
        m.grace      = row.try_get::<u32, _>(13).unwrap_or(0) as c_int;
        m.movetime   = row.try_get::<u32, _>(14).unwrap_or(0) as c_int;
        m.spawntime  = row.try_get::<u32, _>(15).unwrap_or(0) as c_int;
        m.baseac     = row.try_get::<i32, _>(16).unwrap_or(0);          // signed
        m.sound      = row.try_get::<u32, _>(17).unwrap_or(0) as c_int;
        m.atktime    = row.try_get::<u32, _>(18).unwrap_or(0) as c_int;
        m.protection = row.try_get::<u32, _>(19).unwrap_or(0) as c_short;
        m.retdist    = row.try_get::<u32, _>(20).unwrap_or(0) as c_schar;
        m.sex        = row.try_get::<u32, _>(21).unwrap_or(0) as c_ushort;
        m.face       = row.try_get::<u32, _>(22).unwrap_or(0) as c_ushort;
        m.face_color = row.try_get::<u32, _>(23).unwrap_or(0) as c_ushort;
        m.hair       = row.try_get::<u32, _>(24).unwrap_or(0) as c_ushort;
        m.hair_color = row.try_get::<u32, _>(25).unwrap_or(0) as c_ushort;
        m.skin_color = row.try_get::<u32, _>(26).unwrap_or(0) as c_ushort;
        m.state      = row.try_get::<u32, _>(27).unwrap_or(0) as c_schar;
        m.mobtype    = row.try_get::<u32, _>(28).unwrap_or(0) as c_uchar;
        m.will       = row.try_get::<u32, _>(29).unwrap_or(0) as c_int;
        m.mindam     = row.try_get::<u32, _>(30).unwrap_or(0);
        m.maxdam     = row.try_get::<u32, _>(31).unwrap_or(0);
        m.mark       = row.try_get::<u32, _>(32).unwrap_or(0) as c_uchar;
        m.isnpc      = row.try_get::<u32, _>(33).unwrap_or(0) as c_uchar;
        m.isboss     = row.try_get::<u32, _>(34).unwrap_or(0) as c_uchar;

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

pub fn init() -> c_int {
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
pub fn searchname(s: *const c_char) -> *mut MobDbData {
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

pub fn level(id: u32) -> c_int {
    let map = db().lock().unwrap();
    map.get(&id).map(|m| m.level).unwrap_or(0)
}

pub fn experience(id: u32) -> c_uint {
    let map = db().lock().unwrap();
    map.get(&id).map(|m| m.exp).unwrap_or(0)
}

/// Finds a mob id by yname string. Returns 0 if not found.
pub fn find_id(s: *const c_char) -> c_int {
    let ptr = searchname(s);
    if ptr.is_null() { return 0; }
    unsafe { (*ptr).id as c_int }
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn mob_item_size() {
        // struct item in mmo.h: verify layout matches C
        assert_eq!(std::mem::size_of::<MobItem>(), 880,
            "MobItem size mismatch — C struct item is 880 bytes");
    }

    #[test]
    fn mob_db_data_size() {
        // Calculated from C struct layout
        assert_eq!(std::mem::size_of::<MobDbData>(), 13408,
            "MobDbData size mismatch — check field ordering vs map_server.h");
    }
}
