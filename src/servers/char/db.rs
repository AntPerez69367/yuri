use sqlx::{MySqlPool, Row, Transaction, MySql};
use anyhow::Result;
use md5::{Md5, Digest};
use crate::common::types::{Item, SkillInfo, Legend, BankData};
use crate::common::player::inventory::{MAX_EQUIP, MAX_INVENTORY, MAX_BANK_SLOTS};
use crate::common::player::spells::{MAX_SPELLS, MAX_MAGIC_TIMERS};
use crate::common::player::legends::MAX_LEGENDS;
use crate::common::player::{
    PlayerData, PlayerIdentity, PlayerCombat, PlayerProgression,
    PlayerSpells, PlayerInventory, PlayerAppearance, PlayerSocial,
    PlayerRegistries, PlayerLegends,
};

/// Compute MD5 of `input` and return it as a lowercase hex string.
/// Kept for legacy password verification only.
fn md5_hex(input: &str) -> String {
    hex::encode(Md5::new().chain_update(input).finalize())
}

/// Returns true if `stored` is a legacy MD5 hash (not a bcrypt hash).
/// Recognises all four bcrypt version prefixes the `bcrypt` crate accepts:
/// $2b$ (canonical), $2a$ (original), $2y$ (PHP compat), $2x$ (rare bugfix).
pub(crate) fn is_legacy_hash(stored: &str) -> bool {
    !stored.starts_with("$2b$")
        && !stored.starts_with("$2a$")
        && !stored.starts_with("$2y$")
        && !stored.starts_with("$2x$")
}

/// Hash a plaintext password with bcrypt at cost 10.
/// Runs on a blocking thread to avoid stalling the async executor.
pub async fn hash_password(pass: &str) -> Result<String> {
    let pass = pass.to_owned();
    tokio::task::spawn_blocking(move || {
        bcrypt::hash(&pass, 10).map_err(|e| anyhow::anyhow!("bcrypt hash failed: {}", e))
    })
    .await
    .map_err(|e| anyhow::anyhow!("spawn_blocking failed: {}", e))?
}

/// Verify password against stored hash.
/// If stored is a bcrypt hash ($2b$/$2a$ prefix), uses bcrypt::verify on a blocking thread.
/// Otherwise falls back to MD5("lowercase_name password") or MD5(password).
pub async fn ispass(name: &str, pass: &str, stored_hash: &str) -> bool {
    if !is_legacy_hash(stored_hash) {
        let pass = pass.to_owned();
        let stored_hash = stored_hash.to_owned();
        return tokio::task::spawn_blocking(move || {
            bcrypt::verify(&pass, &stored_hash).unwrap_or_else(|e| {
                tracing::error!("[auth] bcrypt::verify error: {}", e);
                false
            })
        })
        .await
        .unwrap_or(false);
    }
    let form1 = md5_hex(&format!("{} {}", name.to_lowercase(), pass));
    let form2 = md5_hex(pass);
    stored_hash == form1 || stored_hash == form2
}

/// Returns true if master password matches and hasn't expired.
/// Supports both bcrypt and legacy MD5 stored hashes.
pub async fn ismastpass(pass: &str, stored: &str, expire: u32) -> bool {
    let now = chrono::Utc::now().timestamp();
    if now > expire as i64 { return false; }
    if !is_legacy_hash(stored) {
        let pass = pass.to_owned();
        let stored = stored.to_owned();
        return tokio::task::spawn_blocking(move || {
            bcrypt::verify(&pass, &stored).unwrap_or_else(|e| {
                tracing::error!("[auth] bcrypt::verify error: {}", e);
                false
            })
        })
        .await
        .unwrap_or(false);
    }
    md5_hex(pass) == stored
}

/// Returns true if character name is already taken.
pub async fn is_name_used(pool: &MySqlPool, name: &str) -> Result<bool> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM `Character` WHERE `ChaName` = ?"
    )
    .bind(name)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(n,)| n > 0).unwrap_or(false))
}

/// Create a new character. Returns 0 on success, 1 if name taken, 2 on DB error.
pub async fn create_char(
    pool: &MySqlPool,
    name: &str, pass: &str, totem: u8, sex: u8,
    country: u8, face: u16, hair: u16, face_color: u16, hair_color: u16,
    start_m: u32, start_x: u32, start_y: u32,
) -> i32 {
    match is_name_used(pool, name).await {
        Err(_)       => return 2,
        Ok(true)     => return 1,
        Ok(false)    => {}
    }
    let hashed = match hash_password(pass).await {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("[char] hash_password failed: {}", e);
            return 2;
        }
    };
    let res = sqlx::query(
        "INSERT INTO `Character` (`ChaName`, `ChaPassword`, `ChaTotem`, `ChaSex`,
         `ChaNation`, `ChaFace`, `ChaMapId`, `ChaX`, `ChaY`,
         `ChaHair`, `ChaHairColor`, `ChaFaceColor`)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(name).bind(hashed).bind(totem).bind(sex)
    .bind(country).bind(face).bind(start_m).bind(start_x).bind(start_y)
    .bind(hair).bind(hair_color).bind(face_color)
    .execute(pool)
    .await;
    if res.is_err() { 2 } else { 0 }
}

/// Fetch stored MD5 password hash for a character name.
pub async fn get_char_password(pool: &MySqlPool, name: &str) -> Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT `ChaPassword` FROM `Character` WHERE `ChaName` = ?"
    )
    .bind(name)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(s,)| s))
}

/// Fetch master password hash and expiry from AdminPassword table.
/// `AdmTimer` is `int(10) unsigned` → u32.
pub async fn get_master_password(pool: &MySqlPool) -> Result<Option<(String, u32)>> {
    let row: Option<(String, u32)> = sqlx::query_as(
        "SELECT `AdmPassword`, `AdmTimer` FROM `AdminPassword` WHERE `AdmId` = 1"
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub struct CharLoginResult {
    pub char_id: u32,
    pub map_id: u32,
    pub banned: bool,
}

/// Look up character for login: fetch id, map, ban status.
/// Does NOT verify password — caller must call ispass() first.
/// `ChaMapId` is `int(10) unsigned` → u32.
pub async fn char_login_lookup(pool: &MySqlPool, name: &str) -> Result<Option<CharLoginResult>> {
    let row: Option<(u32, u32, u32)> = sqlx::query_as(
        "SELECT `ChaId`, `ChaMapId`, `ChaBanned` FROM `Character` WHERE `ChaName` = ?"
    )
    .bind(name)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id, map_id, banned)| CharLoginResult {
        char_id: id,
        map_id,
        banned: banned != 0,
    }))
}

/// Returns true if the account owning char_id is banned.
pub async fn is_account_banned(pool: &MySqlPool, char_id: u32) -> bool {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM `Accounts` WHERE `AccountBanned` = 1
         AND ? IN (`AccountCharId1`, `AccountCharId2`, `AccountCharId3`,
                   `AccountCharId4`, `AccountCharId5`, `AccountCharId6`)"
    )
    .bind(char_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    row.map(|(n,)| n > 0).unwrap_or(false)
}

/// Clear all stale ChaOnline flags on startup (handles crashes/ungraceful shutdowns).
pub async fn reset_all_online(pool: &MySqlPool) {
    if let Err(e) = sqlx::query("UPDATE `Character` SET `ChaOnline` = 0 WHERE `ChaOnline` = 1")
        .execute(pool).await
    {
        tracing::error!("Failed to reset ChaOnline flags on startup: {}", e);
    }
}

pub async fn set_online(pool: &MySqlPool, char_id: u32, online: bool) {
    let val: u8 = if online { 1 } else { 0 };
    if let Err(e) = sqlx::query("UPDATE `Character` SET `ChaOnline` = ? WHERE `ChaId` = ?")
        .bind(val).bind(char_id)
        .execute(pool).await
    {
        tracing::error!("Failed to update ChaOnline={} for ChaId {}: {}", val, char_id, e);
    }
}

/// Change password after verifying old password. Returns 0=ok, -2=no user, -3=wrong pass, -1=db error.
pub async fn set_char_password(pool: &MySqlPool, name: &str, pass: &str, newpass: &str) -> i32 {
    let stored = match get_char_password(pool, name).await {
        Ok(Some(h)) => h,
        Ok(None) => return -2,
        Err(_) => return -1,
    };
    if !ispass(name, pass, &stored).await { return -3; }
    let hashed = match hash_password(newpass).await {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("[char] hash_password failed: {}", e);
            return -1;
        }
    };
    let res = sqlx::query(
        "UPDATE `Character` SET `ChaPassword` = ? WHERE `ChaName` = ?"
    )
    .bind(hashed).bind(name)
    .execute(pool).await;
    if res.is_err() { -1 } else { 0 }
}

/// Load character data from the database into a PlayerData struct.
#[allow(clippy::type_complexity)]
pub async fn load_player(pool: &MySqlPool, char_id: u32, login_name: &str) -> Result<PlayerData> {

    // Update character name to match login name.
    let _ = sqlx::query("UPDATE `Character` SET `ChaName` = ? WHERE `ChaId` = ?")
        .bind(login_name).bind(char_id).execute(pool).await;

    // ── Main character row ────────────────────────────────────────────────────
    let row = sqlx::query(
        "SELECT `ChaName`, `ChaClnId`, `ChaClanTitle`, `ChaTitle`, \
         `ChaF1Name`, `ChaLevel`, `ChaPthId`, `ChaMark`, \
         `ChaTotem`, `ChaKarma`, `ChaCurrentVita`, `ChaBaseVita`, \
         `ChaCurrentMana`, `ChaBaseMana`, `ChaExperience`, `ChaGold`, `ChaSex`, \
         `ChaNation`, `ChaFace`, `ChaHairColor`, `ChaArmorColor`, \
         `ChaMapId`, `ChaX`, `ChaY`, `ChaSide`, `ChaState`, `ChaHair`, `ChaFaceColor`, \
         `ChaSkinColor`, `ChaPartner`, `ChaClanChat`, `ChaPathChat`, `ChaNoviceChat`, \
         `ChaSettings`, `ChaGMLevel`, `ChaDisguise`, `ChaDisguiseColor`, \
         `ChaMaximumBankSlots`, `ChaBankGold`, `ChaMaximumInventory`, `ChaPK`, \
         `ChaKilledBy`, `ChaKillsPK`, `ChaPKDuration`, `ChaMuted`, `ChaHeroes`, `ChaTier`, \
         `ChaExperienceSoldMagic`, `ChaExperienceSoldHealth`, `ChaExperienceSoldStats`, \
         `ChaBaseMight`, `ChaBaseWill`, `ChaBaseGrace`, `ChaBaseArmor`, `ChaMiniMapToggle`, \
         `ChaLastIP`, `ChaAFKMessage`, `ChaTutor`, `ChaAlignment`, \
         `ChaProfileVitaStats`, `ChaProfileEquipList`, `ChaProfileLegends`, \
         `ChaProfileSpells`, `ChaProfileInventory`, `ChaProfileBankItems`, \
         `ChaPthRank`, `ChaClnRank` \
         FROM `Character` WHERE `ChaId` = ? LIMIT 1"
    ).bind(char_id).fetch_optional(pool).await?;

    let row = match row { Some(r) => r, None => anyhow::bail!("character not found") };

    let last_pos = crate::common::types::Point::new(
        row.try_get::<u32, _>(21).unwrap_or(0) as u16,
        row.try_get::<u32, _>(22).unwrap_or(0) as u16,
        row.try_get::<u32, _>(23).unwrap_or(0) as u16,
    );

    let mut pd = PlayerData {
        identity: PlayerIdentity {
            id: char_id,
            name: login_name.to_string(),
            pass: String::new(),
            f1name: row.try_get::<String, _>(4).unwrap_or_default(),
            title: row.try_get::<String, _>(3).unwrap_or_default(),
            ipaddress: row.try_get::<String, _>(55).unwrap_or_default(),
            gm_level: row.try_get::<u32, _>(34).unwrap_or(0) as i8,
            sex: row.try_get::<u32, _>(16).unwrap_or(0) as i8,
            map_server: 0,
            dest_pos: last_pos,
            last_pos,
        },
        combat: PlayerCombat {
            hp: row.try_get::<u32, _>(10).unwrap_or(0),
            max_hp: row.try_get::<u32, _>(11).unwrap_or(0),
            mp: row.try_get::<u32, _>(12).unwrap_or(0),
            max_mp: row.try_get::<u32, _>(13).unwrap_or(0),
            might: 0,
            will: 0,
            grace: 0,
            base_might: row.try_get::<u32, _>(50).unwrap_or(0),
            base_will: row.try_get::<u32, _>(51).unwrap_or(0),
            base_grace: row.try_get::<u32, _>(52).unwrap_or(0),
            base_armor: row.try_get::<i32, _>(53).unwrap_or(0),
            state: row.try_get::<u32, _>(25).unwrap_or(0) as i8,
            side: row.try_get::<u32, _>(24).unwrap_or(0) as i8,
        },
        progression: PlayerProgression {
            level: row.try_get::<u32, _>(5).unwrap_or(0) as u8,
            class: row.try_get::<u32, _>(6).unwrap_or(0) as u8,
            tier: row.try_get::<u32, _>(46).unwrap_or(0) as u8,
            mark: row.try_get::<u32, _>(7).unwrap_or(0) as u8,
            totem: row.try_get::<u32, _>(8).unwrap_or(0) as u8,
            country: row.try_get::<u32, _>(17).unwrap_or(0) as i8,
            magic_number: 0,
            exp: row.try_get::<u32, _>(14).unwrap_or(0),
            tnl: 0,
            next_level_xp: 0,
            max_tnl: 0,
            real_tnl: 0,
            class_rank: row.try_get::<u32, _>(65).unwrap_or(0) as i32,
            clan_rank: row.try_get::<u32, _>(66).unwrap_or(0) as i32,
            percentage: 0.0,
            int_percentage: 0,
            expsold_magic: row.try_get::<u64, _>(47).unwrap_or(0),
            expsold_health: row.try_get::<u64, _>(48).unwrap_or(0),
            expsold_stats: row.try_get::<u64, _>(49).unwrap_or(0),
        },
        spells: PlayerSpells::default(),
        inventory: PlayerInventory {
            equip: vec![crate::common::types::Item::default(); MAX_EQUIP],
            inventory: vec![crate::common::types::Item::default(); MAX_INVENTORY],
            banks: vec![crate::common::types::BankData::default(); MAX_BANK_SLOTS],
            money: row.try_get::<u32, _>(15).unwrap_or(0),
            bank_money: row.try_get::<u32, _>(38).unwrap_or(0),
            max_inv: row.try_get::<u32, _>(39).unwrap_or(0) as u8,
            max_slots: row.try_get::<u32, _>(37).unwrap_or(0),
        },
        appearance: PlayerAppearance {
            face: row.try_get::<u32, _>(18).unwrap_or(0) as u16,
            hair: row.try_get::<u32, _>(26).unwrap_or(0) as u16,
            face_color: row.try_get::<u32, _>(27).unwrap_or(0) as u16,
            hair_color: row.try_get::<u32, _>(19).unwrap_or(0) as u16,
            armor_color: row.try_get::<u32, _>(20).unwrap_or(0) as u16,
            skin_color: row.try_get::<u32, _>(28).unwrap_or(0) as u16,
            disguise: row.try_get::<u32, _>(35).unwrap_or(0) as u16,
            disguise_color: row.try_get::<u32, _>(36).unwrap_or(0) as u16,
            setting_flags: row.try_get::<u32, _>(33).unwrap_or(0) as u16,
            heroes: row.try_get::<u32, _>(45).unwrap_or(0),
            mini_map_toggle: row.try_get::<u32, _>(54).unwrap_or(0),
            profile_vitastats: row.try_get::<u32, _>(59).unwrap_or(0) as u8,
            profile_equiplist: row.try_get::<u32, _>(60).unwrap_or(0) as u8,
            profile_legends: row.try_get::<u32, _>(61).unwrap_or(0) as u8,
            profile_spells: row.try_get::<u32, _>(62).unwrap_or(0) as u8,
            profile_inventory: row.try_get::<u32, _>(63).unwrap_or(0) as u8,
            profile_bankitems: row.try_get::<u32, _>(64).unwrap_or(0) as u8,
        },
        social: PlayerSocial {
            partner: row.try_get::<u32, _>(29).unwrap_or(0),
            clan: row.try_get::<u32, _>(1).unwrap_or(0),
            clan_title: row.try_get::<String, _>(2).unwrap_or_default(),
            clan_chat: row.try_get::<u32, _>(30).unwrap_or(0) as i8,
            pk: row.try_get::<u32, _>(40).unwrap_or(0) as u8,
            killed_by: row.try_get::<u32, _>(41).unwrap_or(0),
            kills_pk: row.try_get::<u32, _>(42).unwrap_or(0),
            pk_duration: row.try_get::<u32, _>(43).unwrap_or(0),
            karma: row.try_get::<f32, _>(9).unwrap_or(0.0),
            alignment: row.try_get::<i8, _>(58).unwrap_or(0),
            novice_chat: row.try_get::<u32, _>(32).unwrap_or(0) as i8,
            subpath_chat: row.try_get::<u32, _>(31).unwrap_or(0) as i8,
            mute: row.try_get::<u32, _>(44).unwrap_or(0) as i8,
            tutor: row.try_get::<u8, _>(57).unwrap_or(0),
            afk_message: row.try_get::<String, _>(56).unwrap_or_default(),
        },
        registries: PlayerRegistries::default(),
        legends: PlayerLegends::default(),
    };

    // ── Banks ─────────────────────────────────────────────────────────────────
    let banks: Vec<(String, u32, u32, u32, u32, u32, u32, u32, u32, u32, String)> =
        sqlx::query_as(
            "SELECT `BnkEngrave`, `BnkItmId`, `BnkAmount`, `BnkChaIdOwner`, \
             `BnkPosition`, `BnkCustomLook`, `BnkCustomLookColor`, \
             `BnkCustomIcon`, `BnkCustomIconColor`, `BnkProtected`, `BnkNote` \
             FROM `Banks` WHERE `BnkChaId` = ? LIMIT 255"
        ).bind(char_id).fetch_all(pool).await?;
    for (engrave, item_id, amount, owner, pos, custom_look, custom_look_color,
         custom_icon, custom_icon_color, protected, note) in banks {
        let p = pos as usize;
        if p >= MAX_BANK_SLOTS { continue; }
        copy_str_to_i8(&mut pd.inventory.banks[p].real_name, &engrave);
        pd.inventory.banks[p].item_id          = item_id;
        pd.inventory.banks[p].amount           = amount;
        pd.inventory.banks[p].owner            = owner;
        pd.inventory.banks[p].custom_look      = custom_look;
        pd.inventory.banks[p].custom_look_color = custom_look_color;
        pd.inventory.banks[p].custom_icon      = custom_icon;
        pd.inventory.banks[p].custom_icon_color = custom_icon_color;
        pd.inventory.banks[p].protected        = protected;
        copy_str_to_i8(&mut pd.inventory.banks[p].note, &note);
    }

    // ── Inventory ─────────────────────────────────────────────────────────────
    let items: Vec<(String, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, String)> =
        sqlx::query_as(
            "SELECT `InvEngrave`, `InvItmId`, `InvAmount`, `InvDurability`, \
             `InvChaIdOwner`, `InvTimer`, `InvPosition`, `InvCustom`, \
             `InvCustomLook`, `InvCustomLookColor`, `InvCustomIcon`, \
             `InvCustomIconColor`, `InvProtected`, `InvNote` \
             FROM `Inventory` WHERE `InvChaId` = ? LIMIT 52"
        ).bind(char_id).fetch_all(pool).await?;
    for (engrave, id, amount, dura, owner, time, pos, custom,
         custom_look, custom_look_color, custom_icon, custom_icon_color, protected, note) in items {
        let p = pos as usize;
        if p >= MAX_INVENTORY { continue; }
        copy_str_to_i8(&mut pd.inventory.inventory[p].real_name, &engrave);
        pd.inventory.inventory[p].id               = id;
        pd.inventory.inventory[p].amount           = amount as i32;
        pd.inventory.inventory[p].dura             = dura as i32;
        pd.inventory.inventory[p].owner            = owner;
        pd.inventory.inventory[p].time             = time;
        pd.inventory.inventory[p].custom           = custom;
        pd.inventory.inventory[p].custom_look      = custom_look;
        pd.inventory.inventory[p].custom_look_color = custom_look_color;
        pd.inventory.inventory[p].custom_icon      = custom_icon;
        pd.inventory.inventory[p].custom_icon_color = custom_icon_color;
        pd.inventory.inventory[p].protected        = protected;
        copy_str_to_i8(&mut pd.inventory.inventory[p].note, &note);
    }

    // ── Equipment ─────────────────────────────────────────────────────────────
    let equips: Vec<(String, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, String)> =
        sqlx::query_as(
            "SELECT `EqpEngrave`, `EqpItmId`, CAST(1 AS UNSIGNED), `EqpDurability`, \
             `EqpChaIdOwner`, `EqpTimer`, `EqpSlot`, `EqpCustom`, \
             `EqpCustomLook`, `EqpCustomLookColor`, `EqpCustomIcon`, \
             `EqpCustomIconColor`, `EqpProtected`, `EqpNote` \
             FROM `Equipment` WHERE `EqpChaId` = ? LIMIT 15"
        ).bind(char_id).fetch_all(pool).await?;
    for (engrave, id, amount, dura, owner, time, pos, custom,
         custom_look, custom_look_color, custom_icon, custom_icon_color, protected, note) in equips {
        let p = pos as usize;
        if p >= MAX_EQUIP { continue; }
        copy_str_to_i8(&mut pd.inventory.equip[p].real_name, &engrave);
        pd.inventory.equip[p].id               = id;
        pd.inventory.equip[p].amount           = amount as i32;
        pd.inventory.equip[p].dura             = dura as i32;
        pd.inventory.equip[p].owner            = owner;
        pd.inventory.equip[p].time             = time;
        pd.inventory.equip[p].custom           = custom;
        pd.inventory.equip[p].custom_look      = custom_look;
        pd.inventory.equip[p].custom_look_color = custom_look_color;
        pd.inventory.equip[p].custom_icon      = custom_icon;
        pd.inventory.equip[p].custom_icon_color = custom_icon_color;
        pd.inventory.equip[p].protected        = protected;
        copy_str_to_i8(&mut pd.inventory.equip[p].note, &note);
    }

    // ── SpellBook ─────────────────────────────────────────────────────────────
    let spells: Vec<(u32, u32)> = sqlx::query_as(
        "SELECT `SbkSplId`, `SbkPosition` FROM `SpellBook` WHERE `SbkChaId` = ? LIMIT 52"
    ).bind(char_id).fetch_all(pool).await?;
    for (spell_id, pos) in spells {
        let p = pos as usize;
        if p < MAX_SPELLS { pd.spells.skills[p] = spell_id as u16; }
    }

    // ── Aethers ───────────────────────────────────────────────────────────────
    let aethers: Vec<(u32, u32, u32, u32)> = sqlx::query_as(
        "SELECT `AthAether`, `AthSplId`, `AthDuration`, `AthPosition` \
         FROM `Aethers` WHERE `AthChaId` = ? LIMIT 200"
    ).bind(char_id).fetch_all(pool).await?;
    for (aether, spell_id, duration, pos) in aethers {
        let p = pos as usize;
        if p >= MAX_MAGIC_TIMERS { continue; }
        pd.spells.dura_aether[p].aether   = aether as i32;
        pd.spells.dura_aether[p].id       = spell_id as u16;
        pd.spells.dura_aether[p].duration = duration as i32;
    }

    // ── Registry (int) ────────────────────────────────────────────────────────
    let regs: Vec<(String, u32)> = sqlx::query_as(
        "SELECT `RegIdentifier`, `RegValue` FROM `Registry` WHERE `RegChaId` = ? LIMIT 5000"
    ).bind(char_id).fetch_all(pool).await?;
    for (key, val) in regs {
        if val != 0 {
            pd.registries.global_reg.insert(key, val as i32);
        }
    }

    // ── Registry (string) ─────────────────────────────────────────────────────
    let regstrs: Vec<(String, String)> = sqlx::query_as(
        "SELECT `RegIdentifier`, `RegValue` FROM `RegistryString` WHERE `RegChaId` = ? LIMIT 5000"
    ).bind(char_id).fetch_all(pool).await?;
    for (key, val) in regstrs {
        if !val.is_empty() {
            pd.registries.global_regstring.insert(key, val);
        }
    }

    // ── NPC Registry ──────────────────────────────────────────────────────────
    let npcregs: Vec<(String, u32)> = sqlx::query_as(
        "SELECT `NrgIdentifier`, `NrgValue` FROM `NPCRegistry` WHERE `NrgChaId` = ? LIMIT 100"
    ).bind(char_id).fetch_all(pool).await?;
    for (key, val) in npcregs {
        if val != 0 {
            pd.registries.npc_int_reg.insert(key, val as i32);
        }
    }

    // ── Quest Registry ────────────────────────────────────────────────────────
    let questregs: Vec<(String, u32)> = sqlx::query_as(
        "SELECT `QrgIdentifier`, `QrgValue` FROM `QuestRegistry` WHERE `QrgChaId` = ? LIMIT 250"
    ).bind(char_id).fetch_all(pool).await?;
    for (key, val) in questregs {
        if val != 0 {
            pd.registries.quest_reg.insert(key, val as i32);
        }
    }

    // ── Legends ───────────────────────────────────────────────────────────────
    let legends: Vec<(u32, u32, u32, String, String, u32)> = sqlx::query_as(
        "SELECT `LegPosition`, `LegIcon`, `LegColor`, `LegDescription`, \
         `LegIdentifier`, `LegTChaId` FROM `Legends` WHERE `LegChaId` = ? LIMIT 1000"
    ).bind(char_id).fetch_all(pool).await?;
    for (pos, icon, color, text, name, tchaid) in legends {
        let p = pos as usize;
        if p >= MAX_LEGENDS { continue; }
        pd.legends.legends[p].icon   = icon as u16;
        pd.legends.legends[p].color  = color as u16;
        copy_str_to_i8(&mut pd.legends.legends[p].text, &text);
        copy_str_to_i8(&mut pd.legends.legends[p].name, &name);
        pd.legends.legends[p].tchaid = tchaid;
    }

    // ── Kill counts ───────────────────────────────────────────────────────────
    let kills: Vec<(u32, u32, u32)> = sqlx::query_as(
        "SELECT `KilPosition`, `KilMobId`, `KilAmount` FROM `Kills` WHERE `KilChaId` = ? LIMIT 5000"
    ).bind(char_id).fetch_all(pool).await?;
    for (_pos, mob_id, amount) in kills {
        if mob_id != 0 {
            *pd.registries.kill_reg.entry(mob_id).or_insert(0) += amount;
        }
    }

    tracing::info!("[char] [load_player] name={} map={} x={} y={}", pd.identity.name, pd.identity.last_pos.m, pd.identity.last_pos.x, pd.identity.last_pos.y);
    Ok(pd)
}

/// Save a PlayerData to the database.
pub async fn save_player(pool: &MySqlPool, player: &PlayerData) -> Result<()> {
    let char_id = player.identity.id;
    if char_id == 0 { return Ok(()); }

    tracing::info!("[char] [save_player] name={} map={} x={} y={}", player.identity.name, player.identity.last_pos.m, player.identity.last_pos.x, player.identity.last_pos.y);

    let mut tx = pool.begin().await?;

    sqlx::query(
        "UPDATE `Character` SET \
         `ChaName`=?,`ChaClnId`=?,`ChaClanTitle`=?,`ChaTitle`=?,\
         `ChaLevel`=?,`ChaPthId`=?,`ChaMark`=?,`ChaTotem`=?,`ChaKarma`=?,\
         `ChaCurrentVita`=?,`ChaBaseVita`=?,`ChaCurrentMana`=?,`ChaBaseMana`=?,\
         `ChaExperience`=?,`ChaGold`=?,`ChaSex`=?,`ChaNation`=?,\
         `ChaFace`=?,`ChaHairColor`=?,`ChaArmorColor`=?,\
         `ChaMapId`=?,`ChaX`=?,`ChaY`=?,`ChaSide`=?,`ChaState`=?,\
         `ChaHair`=?,`ChaFaceColor`=?,`ChaSkinColor`=?,\
         `ChaPartner`=?,`ChaClanChat`=?,`ChaPathChat`=?,`ChaNoviceChat`=?,\
         `ChaSettings`=?,`ChaGMLevel`=?,`ChaDisguise`=?,`ChaDisguiseColor`=?,\
         `ChaMaximumBankSlots`=?,`ChaBankGold`=?,`ChaF1Name`=?,\
         `ChaMaximumInventory`=?,`ChaPK`=?,`ChaKilledBy`=?,`ChaKillsPK`=?,\
         `ChaPKDuration`=?,`ChaMuted`=?,`ChaHeroes`=?,`ChaTier`=?,\
         `ChaExperienceSoldMagic`=?,`ChaExperienceSoldHealth`=?,`ChaExperienceSoldStats`=?,\
         `ChaBaseMight`=?,`ChaBaseWill`=?,`ChaBaseGrace`=?,`ChaBaseArmor`=?,\
         `ChaMiniMapToggle`=?,`ChaHunter`=0,`ChaAFKMessage`=?,\
         `ChaTutor`=?,`ChaAlignment`=?,\
         `ChaProfileVitaStats`=?,`ChaProfileEquipList`=?,`ChaProfileLegends`=?,\
         `ChaProfileSpells`=?,`ChaProfileInventory`=?,`ChaProfileBankItems`=?,\
         `ChaPthRank`=?,`ChaClnRank`=? \
         WHERE `ChaId`=?"
    )
    .bind(&player.identity.name)
    .bind(player.social.clan)
    .bind(&player.social.clan_title)
    .bind(&player.identity.title)
    .bind(player.progression.level)
    .bind(player.progression.class)
    .bind(player.progression.mark)
    .bind(player.progression.totem)
    .bind(player.social.karma)
    .bind(player.combat.hp)
    .bind(player.combat.max_hp)
    .bind(player.combat.mp)
    .bind(player.combat.max_mp)
    .bind(player.progression.exp)
    .bind(player.inventory.money)
    .bind(player.identity.sex)
    .bind(player.progression.country)
    .bind(player.appearance.face)
    .bind(player.appearance.hair_color)
    .bind(player.appearance.armor_color)
    .bind(player.identity.last_pos.m)
    .bind(player.identity.last_pos.x)
    .bind(player.identity.last_pos.y)
    .bind(player.combat.side)
    .bind(player.combat.state)
    .bind(player.appearance.hair)
    .bind(player.appearance.face_color)
    .bind(player.appearance.skin_color)
    .bind(player.social.partner)
    .bind(player.social.clan_chat)
    .bind(player.social.subpath_chat)
    .bind(player.social.novice_chat)
    .bind(player.appearance.setting_flags)
    .bind(player.identity.gm_level)
    .bind(player.appearance.disguise)
    .bind(player.appearance.disguise_color)
    .bind(player.inventory.max_slots)
    .bind(player.inventory.bank_money)
    .bind(&player.identity.f1name)
    .bind(player.inventory.max_inv)
    .bind(player.social.pk)
    .bind(player.social.killed_by)
    .bind(player.social.kills_pk)
    .bind(player.social.pk_duration)
    .bind(player.social.mute)
    .bind(player.appearance.heroes)
    .bind(player.progression.tier)
    .bind(player.progression.expsold_magic)
    .bind(player.progression.expsold_health)
    .bind(player.progression.expsold_stats)
    .bind(player.combat.base_might)
    .bind(player.combat.base_will)
    .bind(player.combat.base_grace)
    .bind(player.combat.base_armor)
    .bind(player.appearance.mini_map_toggle)
    .bind(&player.social.afk_message)
    .bind(player.social.tutor)
    .bind(player.social.alignment)
    .bind(player.appearance.profile_vitastats)
    .bind(player.appearance.profile_equiplist)
    .bind(player.appearance.profile_legends)
    .bind(player.appearance.profile_spells)
    .bind(player.appearance.profile_inventory)
    .bind(player.appearance.profile_bankitems)
    .bind(player.progression.class_rank as u32)
    .bind(player.progression.clan_rank as u32)
    .bind(char_id)
    .execute(&mut *tx).await?;

    save_items_inventory(&mut tx, char_id, &player.inventory.inventory).await?;
    save_items_equipment(&mut tx, char_id, &player.inventory.equip).await?;
    save_spells(&mut tx, char_id, &player.spells.skills).await?;
    save_aethers(&mut tx, char_id, &player.spells.dura_aether).await?;
    save_registry(&mut tx, char_id, &player.registries.global_reg).await?;
    save_registry_string(&mut tx, char_id, &player.registries.global_regstring).await?;
    save_npc_registry(&mut tx, char_id, &player.registries.npc_int_reg).await?;
    save_quest_registry(&mut tx, char_id, &player.registries.quest_reg).await?;
    save_kills(&mut tx, char_id, &player.registries.kill_reg).await?;
    save_legends(&mut tx, char_id, &player.legends.legends).await?;
    save_banks(&mut tx, char_id, &player.inventory.banks).await?;
    tx.commit().await?;
    Ok(())
}

/// Save a character from a raw byte blob back to the DB (thin wrapper for wire compat).
// ── String helpers ────────────────────────────────────────────────────────────

fn copy_str_to_i8<const N: usize>(dst: &mut [i8; N], src: &str) {
    let bytes = src.as_bytes();
    let n = bytes.len().min(N - 1);
    for (d, s) in dst[..n].iter_mut().zip(bytes[..n].iter()) {
        *d = *s as i8;
    }
    dst[n] = 0;
}

#[allow(dead_code)]
fn copy_str_to_i8_slice(dst: &mut [i8], src: &str) {
    let bytes = src.as_bytes();
    let n = bytes.len().min(dst.len().saturating_sub(1));
    for (d, s) in dst[..n].iter_mut().zip(bytes[..n].iter()) {
        *d = *s as i8;
    }
    if !dst.is_empty() { dst[n] = 0; }
}

fn i8_slice_to_str(src: &[i8]) -> String {
    let nul = src.iter().position(|&c| c == 0).unwrap_or(src.len());
    src[..nul].iter().map(|&c| c as u8 as char).collect()
}


async fn save_items_inventory(tx: &mut Transaction<'_, MySql>, char_id: u32, items: &[Item]) -> Result<()> {
    sqlx::query("DELETE FROM `Inventory` WHERE `InvChaId`=?")
        .bind(char_id).execute(&mut **tx).await?;
    for (i, item) in items.iter().enumerate().take(MAX_INVENTORY) {
        if item.id == 0 { continue; }
        let name = i8_slice_to_str(&item.real_name);
        let note = i8_slice_to_str(&item.note);
        sqlx::query(
            "INSERT INTO `Inventory` \
             (`InvChaId`,`InvItmId`,`InvAmount`,`InvDurability`,`InvChaIdOwner`,\
              `InvCustom`,`InvTimer`,`InvEngrave`,`InvCustomLook`,`InvCustomLookColor`,\
              `InvCustomIcon`,`InvCustomIconColor`,`InvProtected`,`InvNote`,`InvPosition`) \
             VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
        ).bind(char_id).bind(item.id).bind(item.amount).bind(item.dura).bind(item.owner)
         .bind(item.custom).bind(item.time).bind(&name)
         .bind(item.custom_look).bind(item.custom_look_color)
         .bind(item.custom_icon).bind(item.custom_icon_color)
         .bind(item.protected).bind(&note).bind(i as u32)
         .execute(&mut **tx).await?;
    }
    Ok(())
}

async fn save_items_equipment(tx: &mut Transaction<'_, MySql>, char_id: u32, items: &[Item]) -> Result<()> {
    sqlx::query("DELETE FROM `Equipment` WHERE `EqpChaId`=?")
        .bind(char_id).execute(&mut **tx).await?;
    for (i, item) in items.iter().enumerate().take(MAX_EQUIP) {
        if item.id == 0 { continue; }
        let name = i8_slice_to_str(&item.real_name);
        let note = i8_slice_to_str(&item.note);
        sqlx::query(
            "INSERT INTO `Equipment` \
             (`EqpChaId`,`EqpItmId`,`EqpDurability`,`EqpChaIdOwner`,`EqpCustom`,`EqpTimer`,\
              `EqpEngrave`,`EqpCustomLook`,`EqpCustomLookColor`,`EqpCustomIcon`,\
              `EqpCustomIconColor`,`EqpProtected`,`EqpNote`,`EqpSlot`) \
             VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
        ).bind(char_id).bind(item.id).bind(item.dura).bind(item.owner)
         .bind(item.custom).bind(item.time)
         .bind(&name).bind(item.custom_look).bind(item.custom_look_color)
         .bind(item.custom_icon).bind(item.custom_icon_color)
         .bind(item.protected).bind(&note).bind(i as u32)
         .execute(&mut **tx).await?;
    }
    Ok(())
}

async fn save_spells(tx: &mut Transaction<'_, MySql>, char_id: u32, skills: &[u16]) -> Result<()> {
    sqlx::query("DELETE FROM `SpellBook` WHERE `SbkChaId`=?")
        .bind(char_id).execute(&mut **tx).await?;
    for (i, &spell_id) in skills.iter().enumerate().take(MAX_SPELLS) {
        if spell_id == 0 { continue; }
        sqlx::query(
            "INSERT INTO `SpellBook` (`SbkChaId`,`SbkSplId`,`SbkPosition`) VALUES(?,?,?)"
        ).bind(char_id).bind(spell_id).bind(i as u32).execute(&mut **tx).await?;
    }
    Ok(())
}

async fn save_aethers(tx: &mut Transaction<'_, MySql>, char_id: u32, aethers: &[SkillInfo]) -> Result<()> {
    sqlx::query("DELETE FROM `Aethers` WHERE `AthChaId`=?")
        .bind(char_id).execute(&mut **tx).await?;
    for (i, a) in aethers.iter().enumerate().take(MAX_MAGIC_TIMERS) {
        if a.id == 0 { continue; }
        sqlx::query(
            "INSERT INTO `Aethers` (`AthChaId`,`AthSplId`,`AthDuration`,`AthAether`,`AthPosition`) VALUES(?,?,?,?,?)"
        ).bind(char_id).bind(a.id).bind(a.duration).bind(a.aether).bind(i as u32)
         .execute(&mut **tx).await?;
    }
    Ok(())
}

async fn save_registry(tx: &mut Transaction<'_, MySql>, char_id: u32, regs: &std::collections::HashMap<String, i32>) -> Result<()> {
    sqlx::query("DELETE FROM `Registry` WHERE `RegChaId`=?")
        .bind(char_id).execute(&mut **tx).await?;
    let mut pos: u32 = 0;
    for (key, &val) in regs {
        if val == 0 { continue; }
        sqlx::query("INSERT INTO `Registry` (`RegChaId`,`RegIdentifier`,`RegValue`,`RegPosition`) VALUES(?,?,?,?)")
            .bind(char_id).bind(key).bind(val).bind(pos).execute(&mut **tx).await?;
        pos += 1;
    }
    Ok(())
}

async fn save_registry_string(tx: &mut Transaction<'_, MySql>, char_id: u32, regs: &std::collections::HashMap<String, String>) -> Result<()> {
    sqlx::query("DELETE FROM `RegistryString` WHERE `RegChaId`=?")
        .bind(char_id).execute(&mut **tx).await?;
    let mut pos: u32 = 0;
    for (key, val) in regs {
        if val.is_empty() { continue; }
        sqlx::query("INSERT INTO `RegistryString` (`RegChaId`,`RegIdentifier`,`RegValue`,`RegPosition`) VALUES(?,?,?,?)")
            .bind(char_id).bind(key).bind(val).bind(pos).execute(&mut **tx).await?;
        pos += 1;
    }
    Ok(())
}

async fn save_npc_registry(tx: &mut Transaction<'_, MySql>, char_id: u32, regs: &std::collections::HashMap<String, i32>) -> Result<()> {
    sqlx::query("DELETE FROM `NPCRegistry` WHERE `NrgChaId`=?")
        .bind(char_id).execute(&mut **tx).await?;
    let mut pos: u32 = 0;
    for (key, &val) in regs {
        if val == 0 { continue; }
        sqlx::query("INSERT INTO `NPCRegistry` (`NrgChaId`,`NrgIdentifier`,`NrgValue`,`NrgPosition`) VALUES(?,?,?,?)")
            .bind(char_id).bind(key).bind(val).bind(pos).execute(&mut **tx).await?;
        pos += 1;
    }
    Ok(())
}

async fn save_quest_registry(tx: &mut Transaction<'_, MySql>, char_id: u32, regs: &std::collections::HashMap<String, i32>) -> Result<()> {
    sqlx::query("DELETE FROM `QuestRegistry` WHERE `QrgChaId`=?")
        .bind(char_id).execute(&mut **tx).await?;
    let mut pos: u32 = 0;
    for (key, &val) in regs {
        if val == 0 { continue; }
        sqlx::query("INSERT INTO `QuestRegistry` (`QrgChaId`,`QrgIdentifier`,`QrgValue`,`QrgPosition`) VALUES(?,?,?,?)")
            .bind(char_id).bind(key).bind(val).bind(pos).execute(&mut **tx).await?;
        pos += 1;
    }
    Ok(())
}

async fn save_kills(tx: &mut Transaction<'_, MySql>, char_id: u32, kills: &std::collections::HashMap<u32, u32>) -> Result<()> {
    sqlx::query("DELETE FROM `Kills` WHERE `KilChaId`=?")
        .bind(char_id).execute(&mut **tx).await?;
    let mut pos: u32 = 0;
    for (&mob_id, &amount) in kills {
        if amount == 0 { continue; }
        sqlx::query(
            "INSERT INTO `Kills` (`KilChaId`,`KilMobId`,`KilAmount`,`KilPosition`) VALUES(?,?,?,?)"
        ).bind(char_id).bind(mob_id).bind(amount).bind(pos).execute(&mut **tx).await?;
        pos += 1;
    }
    Ok(())
}

async fn save_legends(tx: &mut Transaction<'_, MySql>, char_id: u32, legends: &[Legend]) -> Result<()> {
    sqlx::query("DELETE FROM `Legends` WHERE `LegChaId`=?")
        .bind(char_id).execute(&mut **tx).await?;
    for (i, leg) in legends.iter().enumerate().take(MAX_LEGENDS) {
        let name = i8_slice_to_str(&leg.name);
        let text = i8_slice_to_str(&leg.text);
        if name.is_empty() { continue; }
        sqlx::query(
            "INSERT INTO `Legends` (`LegChaId`,`LegIcon`,`LegColor`,`LegDescription`,`LegIdentifier`,`LegTChaId`,`LegPosition`) VALUES(?,?,?,?,?,?,?)"
        ).bind(char_id).bind(leg.icon).bind(leg.color).bind(&text).bind(&name).bind(leg.tchaid).bind(i as u32)
         .execute(&mut **tx).await?;
    }
    Ok(())
}

async fn save_banks(tx: &mut Transaction<'_, MySql>, char_id: u32, banks: &[BankData]) -> Result<()> {
    sqlx::query("DELETE FROM `Banks` WHERE `BnkChaId`=?")
        .bind(char_id).execute(&mut **tx).await?;
    for (i, bank) in banks.iter().enumerate().take(MAX_BANK_SLOTS) {
        if bank.item_id == 0 { continue; }
        let name = i8_slice_to_str(&bank.real_name);
        let note = i8_slice_to_str(&bank.note);
        sqlx::query(
            "INSERT INTO `Banks` (`BnkChaId`,`BnkItmId`,`BnkAmount`,`BnkChaIdOwner`,\
             `BnkCustomLook`,`BnkCustomLookColor`,`BnkCustomIcon`,`BnkCustomIconColor`,\
             `BnkProtected`,`BnkEngrave`,`BnkNote`,`BnkPosition`) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)"
        ).bind(char_id).bind(bank.item_id).bind(bank.amount).bind(bank.owner)
         .bind(bank.custom_look).bind(bank.custom_look_color)
         .bind(bank.custom_icon).bind(bank.custom_icon_color)
         .bind(bank.protected).bind(&name).bind(&note).bind(i as u32)
         .execute(&mut **tx).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_legacy_hash_md5() {
        assert!(is_legacy_hash("5f4dcc3b5aa765d61d8327deb882cf99")); // MD5("password")
    }

    #[test]
    fn test_is_legacy_hash_bcrypt() {
        assert!(!is_legacy_hash("$2b$04$somehashvalue"));
        assert!(!is_legacy_hash("$2a$04$somehashvalue"));
        assert!(!is_legacy_hash("$2y$12$L6Bc/AlTQHyd9liGgGEZyOFLPHNgyxeEPfgYfBCVxJ7JIlwxyVU3u"));
        assert!(!is_legacy_hash("$2x$04$somehashvalue"));
    }

    #[tokio::test]
    async fn test_ispass_legacy_md5_form1() {
        let hash = md5_hex("alice password");
        assert!(ispass("Alice", "password", &hash).await);
    }

    #[tokio::test]
    async fn test_ispass_legacy_md5_form2() {
        let hash = md5_hex("mypass");
        assert!(ispass("bob", "mypass", &hash).await);
    }

    #[tokio::test]
    async fn test_ispass_wrong_legacy() {
        let hash = md5_hex("correct");
        assert!(!ispass("bob", "wrong", &hash).await);
    }

    #[tokio::test]
    async fn test_ispass_bcrypt() {
        let hash = bcrypt::hash("secret", 4).unwrap();
        assert!(ispass("alice", "secret", &hash).await);
    }

    #[tokio::test]
    async fn test_ispass_bcrypt_wrong() {
        let hash = bcrypt::hash("secret", 4).unwrap();
        assert!(!ispass("alice", "wrong", &hash).await);
    }

    #[tokio::test]
    async fn test_hash_password_produces_bcrypt() {
        let h = hash_password("test").await.unwrap();
        assert!(h.starts_with("$2b$") || h.starts_with("$2a$"));
    }

    #[tokio::test]
    async fn test_ismastpass_expired() {
        let hash = bcrypt::hash("secret", 4).unwrap();
        assert!(!ismastpass("secret", &hash, 0).await); // expire=0 is always in the past
    }

    #[tokio::test]
    async fn test_ismastpass_bcrypt_valid() {
        let hash = bcrypt::hash("adminpass", 4).unwrap();
        let expire = (chrono::Utc::now().timestamp() + 3600) as u32;
        assert!(ismastpass("adminpass", &hash, expire).await);
    }

    #[tokio::test]
    async fn test_ismastpass_legacy_md5_valid() {
        let hash = md5_hex("adminpass");
        let expire = (chrono::Utc::now().timestamp() + 3600) as u32;
        assert!(ismastpass("adminpass", &hash, expire).await);
    }
}
