use std::collections::HashMap;
use std::ffi::CStr;
use std::sync::{Arc, Mutex, OnceLock};

use sqlx::Row;

use super::{blocking_run, get_pool};
use super::item_db::str_to_fixed;
use crate::common::types::Item;

const MAX_EQUIP: usize = 15;

/// Mob database entry.
pub struct MobDbData {
    pub equip: [Item; MAX_EQUIP],
    pub vita: i32, pub r#type: i32, pub subtype: i32,
    pub look: i32, pub look_color: i32, pub hit: i32,
    pub level: i32, pub might: i32, pub grace: i32,
    pub will: i32, pub movetime: i32, pub atktime: i32,
    pub spawntime: i32, pub baseac: i32, pub sound: i32,
    pub mana: i32, pub owner: u32, pub id: u32,
    pub mindam: u32, pub maxdam: u32, pub exp: u32,
    pub name: [i8; 45], pub yname: [i8; 45],
    pub block: i8, pub retdist: i8, pub mobtype: u8,
    pub state: i8, pub race: i8, pub seeinvis: i8,
    pub tier: i8, pub mark: u8, pub isnpc: u8,
    pub isboss: u8, pub protection: i16, pub miss: i16,
    pub sex: u16, pub face: u16, pub face_color: u16,
    pub hair: u16, pub hair_color: u16, pub armor_color: u16,
    pub skin_color: u16, pub startm: u16, pub startx: u16,
    pub starty: u16,
}

unsafe impl Send for MobDbData {}
unsafe impl Sync for MobDbData {}

static MOB_DB: OnceLock<Mutex<HashMap<u32, Arc<MobDbData>>>> = OnceLock::new();

fn db() -> &'static Mutex<HashMap<u32, Arc<MobDbData>>> {
    MOB_DB.get().expect("[mob_db] not initialized")
}

fn make_default(id: u32) -> Arc<MobDbData> {
    // SAFETY: all-zero is a valid initial state for this struct.
    // Construct directly as Arc to avoid stack overflow for this large struct.
    let mut m: Arc<MobDbData> = Arc::new(unsafe { std::mem::zeroed() });
    let m_mut = Arc::get_mut(&mut m).unwrap();
    m_mut.id = id;
    str_to_fixed(&mut m_mut.name, "??");
    m
}

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
        let entry = map.entry(id).or_insert_with(|| make_default(id));
        let m = Arc::get_mut(entry).expect("exclusive during init");
        m.id         = id;
        str_to_fixed(&mut m.name,  &row.try_get::<String, _>(1).unwrap_or_default());
        str_to_fixed(&mut m.yname, &row.try_get::<String, _>(2).unwrap_or_default());
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
        m.baseac     = row.try_get::<i32, _>(16).unwrap_or(0);
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

// ---- Public interface -------------------------------------------------------

pub fn init() -> i32 {
    MOB_DB.get_or_init(|| Mutex::new(HashMap::new()));
    match blocking_run(load_mobs()) {
        Ok(n) => { tracing::info!("[mob_db] read done count={n}"); 0 }
        Err(e) => { tracing::error!("[mob_db] load failed: {e}"); -1 }
    }
}

pub fn term() {
    if let Some(m) = MOB_DB.get() { m.lock().unwrap().clear(); }
}

/// Returns the `MobDbData` for `id`, inserting a default entry if absent.
pub fn search(id: u32) -> Arc<MobDbData> {
    let mut map = db().lock().unwrap();
    let entry = map.entry(id).or_insert_with(|| make_default(id));
    Arc::clone(entry)
}

/// Returns the `MobDbData` for `id` if it exists.
pub fn searchexist(id: u32) -> Option<Arc<MobDbData>> {
    let map = db().lock().unwrap();
    map.get(&id).cloned()
}

/// Searches by `yname` (script identifier), case-insensitive.
pub fn searchname(name: &str) -> Option<Arc<MobDbData>> {
    let target = name.to_lowercase();
    let map = db().lock().unwrap();
    for m in map.values() {
        let yname = unsafe { CStr::from_ptr(m.yname.as_ptr()) }.to_string_lossy().to_lowercase();
        if yname == target { return Some(Arc::clone(m)); }
    }
    None
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
pub fn find_id(name: &str) -> i32 {
    match searchname(name) { Some(m) => m.id as i32, None => 0 }
}

