use sqlx::{MySqlPool, Row};
use anyhow::Result;
use md5::{Md5, Digest};

/// Compute MD5 of `input` and return it as a lowercase hex string.
fn md5_hex(input: &str) -> String {
    hex::encode(Md5::new().chain_update(input).finalize())
}

/// Verify password: checks MD5("lowercase_name password") or MD5(password).
/// Returns true if either form matches `stored_hash` from DB.
pub fn ispass(name: &str, pass: &str, stored_hash: &str) -> bool {
    let form1 = md5_hex(&format!("{} {}", name.to_lowercase(), pass));
    let form2 = md5_hex(pass);
    stored_hash == form1 || stored_hash == form2
}

/// Returns true if master password matches and hasn't expired.
/// `expire` is `AdmTimer` which is `int(10) unsigned` → u32.
pub fn ismastpass(pass: &str, mast_md5: &str, expire: u32) -> bool {
    md5_hex(pass) == mast_md5 && chrono::Utc::now().timestamp() <= expire as i64
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
    if is_name_used(pool, name).await.unwrap_or(true) {
        return 1;
    }
    let res = sqlx::query(
        "INSERT INTO `Character` (`ChaName`, `ChaPassword`, `ChaTotem`, `ChaSex`,
         `ChaNation`, `ChaFace`, `ChaMapId`, `ChaX`, `ChaY`,
         `ChaHair`, `ChaHairColor`, `ChaFaceColor`)
         VALUES (?, MD5(?), ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(name).bind(pass).bind(totem).bind(sex)
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

pub async fn set_online(pool: &MySqlPool, char_id: u32, online: bool) {
    let val: u8 = if online { 1 } else { 0 };
    let _ = sqlx::query("UPDATE `Character` SET `ChaOnline` = ? WHERE `ChaId` = ?")
        .bind(val).bind(char_id)
        .execute(pool).await;
}

pub async fn set_all_online(pool: &MySqlPool, online: bool) {
    let val: u8 = if online { 1 } else { 0 };
    let _ = sqlx::query("UPDATE `Character` SET `ChaOnline` = ?")
        .bind(val)
        .execute(pool).await;
}

/// Change password after verifying old password. Returns 0=ok, -2=no user, -3=wrong pass, -1=db error.
pub async fn set_char_password(pool: &MySqlPool, name: &str, pass: &str, newpass: &str) -> i32 {
    let stored = match get_char_password(pool, name).await {
        Ok(Some(h)) => h,
        Ok(None) => return -2,
        Err(_) => return -1,
    };
    if !ispass(name, pass, &stored) { return -3; }
    let res = sqlx::query(
        "UPDATE `Character` SET `ChaPassword` = MD5(?) WHERE `ChaName` = ?"
    )
    .bind(newpass).bind(name)
    .execute(pool).await;
    if res.is_err() { -1 } else { 0 }
}

/// Load a character from DB and return it as a raw byte blob for zlib transfer.
/// Mirrors mmo_char_fromdb in char_db.c.
pub async fn load_char_bytes(pool: &MySqlPool, char_id: u32, login_name: &str) -> Result<Vec<u8>> {
    use crate::servers::char::charstatus::*;

    // Update character name to match login name (mirrors C line 427)
    let _ = sqlx::query("UPDATE `Character` SET `ChaName` = ? WHERE `ChaId` = ?")
        .bind(login_name).bind(char_id).execute(pool).await;

    // ── Main character row ────────────────────────────────────────────────────
    // Use manual row access because 67 columns exceeds sqlx's tuple FromRow limit (16).
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

    // Allocate directly on heap — MmoCharStatus is 3MB, so Box::new(zeroed()) would
    // stack-allocate it first and overflow the tokio worker thread stack.
    let mut s: Box<MmoCharStatus> = unsafe {
        let layout = std::alloc::Layout::new::<MmoCharStatus>();
        let ptr = std::alloc::alloc_zeroed(layout) as *mut MmoCharStatus;
        Box::from_raw(ptr)
    };
    s.id = char_id;
    copy_str_to_i8(&mut s.name,       &row.try_get::<String, _>(0).unwrap_or_default());
    s.clan           = row.try_get::<u32, _>(1).unwrap_or(0);
    copy_str_to_i8(&mut s.clan_title,  &row.try_get::<String, _>(2).unwrap_or_default());
    copy_str_to_i8(&mut s.title,       &row.try_get::<String, _>(3).unwrap_or_default());
    copy_str_to_i8(&mut s.f1name,      &row.try_get::<String, _>(4).unwrap_or_default());
    // col 5: ChaLevel — int(10) unsigned → u32, cast to u8
    s.level          = row.try_get::<u32, _>(5).unwrap_or(0) as u8;
    // col 6: ChaPthId — int(10) unsigned → u32, cast to u8
    s.class          = row.try_get::<u32, _>(6).unwrap_or(0) as u8;
    // col 7: ChaMark — int(10) unsigned → u32, cast to u8
    s.mark           = row.try_get::<u32, _>(7).unwrap_or(0) as u8;
    // col 8: ChaTotem — int(10) unsigned → u32, cast to u8
    s.totem          = row.try_get::<u32, _>(8).unwrap_or(0) as u8;
    s.karma          = row.try_get::<f32, _>(9).unwrap_or(0.0);
    s.hp             = row.try_get::<u32, _>(10).unwrap_or(0);
    s.basehp         = row.try_get::<u32, _>(11).unwrap_or(0);
    s.mp             = row.try_get::<u32, _>(12).unwrap_or(0);
    s.basemp         = row.try_get::<u32, _>(13).unwrap_or(0);
    s.exp            = row.try_get::<u32, _>(14).unwrap_or(0);
    s.money          = row.try_get::<u32, _>(15).unwrap_or(0);
    // col 16: ChaSex — int(10) unsigned → u32, cast to i8
    s.sex            = row.try_get::<u32, _>(16).unwrap_or(0) as i8;
    // col 17: ChaNation — int(10) unsigned → u32, cast to i8
    s.country        = row.try_get::<u32, _>(17).unwrap_or(0) as i8;
    // col 18: ChaFace — int(10) unsigned → u32, cast to u16
    s.face           = row.try_get::<u32, _>(18).unwrap_or(0) as u16;
    // col 19: ChaHairColor — int(10) unsigned → u32, cast to u16
    s.hair_color     = row.try_get::<u32, _>(19).unwrap_or(0) as u16;
    // col 20: ChaArmorColor — int(10) unsigned → u32, cast to u16
    s.armor_color    = row.try_get::<u32, _>(20).unwrap_or(0) as u16;
    // col 21: ChaMapId — int(10) unsigned → u32, cast to u16
    s.last_pos.m     = row.try_get::<u32, _>(21).unwrap_or(0) as u16;
    // col 22: ChaX — int(10) unsigned → u32, cast to u16
    s.last_pos.x     = row.try_get::<u32, _>(22).unwrap_or(0) as u16;
    // col 23: ChaY — int(10) unsigned → u32, cast to u16
    s.last_pos.y     = row.try_get::<u32, _>(23).unwrap_or(0) as u16;
    // col 24: ChaSide — int(10) unsigned → u32, cast to i8
    s.side           = row.try_get::<u32, _>(24).unwrap_or(0) as i8;
    // col 25: ChaState — int(10) unsigned → u32, cast to i8
    s.state          = row.try_get::<u32, _>(25).unwrap_or(0) as i8;
    // col 26: ChaHair — int(10) unsigned → u32, cast to u16
    s.hair           = row.try_get::<u32, _>(26).unwrap_or(0) as u16;
    // col 27: ChaFaceColor — int(10) unsigned → u32, cast to u16
    s.face_color     = row.try_get::<u32, _>(27).unwrap_or(0) as u16;
    // col 28: ChaSkinColor — int(10) unsigned → u32, cast to u16
    s.skin_color     = row.try_get::<u32, _>(28).unwrap_or(0) as u16;
    s.partner        = row.try_get::<u32, _>(29).unwrap_or(0);
    // col 30: ChaClanChat — int(10) unsigned → u32, cast to i8
    s.clan_chat      = row.try_get::<u32, _>(30).unwrap_or(0) as i8;
    // col 31: ChaPathChat — int(10) unsigned → u32, cast to i8
    s.subpath_chat   = row.try_get::<u32, _>(31).unwrap_or(0) as i8;
    // col 32: ChaNoviceChat — int(10) unsigned → u32, cast to i8
    s.novice_chat    = row.try_get::<u32, _>(32).unwrap_or(0) as i8;
    // col 33: ChaSettings — int(10) unsigned → u32, cast to u16
    s.setting_flags  = row.try_get::<u32, _>(33).unwrap_or(0) as u16;
    // col 34: ChaGMLevel — int(10) unsigned → u32, cast to i8
    s.gm_level       = row.try_get::<u32, _>(34).unwrap_or(0) as i8;
    // col 35: ChaDisguise — int(10) unsigned → u32, cast to u16
    s.disguise       = row.try_get::<u32, _>(35).unwrap_or(0) as u16;
    // col 36: ChaDisguiseColor — int(10) unsigned → u32, cast to u16
    s.disguise_color = row.try_get::<u32, _>(36).unwrap_or(0) as u16;
    s.maxslots       = row.try_get::<u32, _>(37).unwrap_or(0);
    s.bankmoney      = row.try_get::<u32, _>(38).unwrap_or(0);
    // col 39: ChaMaximumInventory — int(10) unsigned → u32, cast to u8
    s.maxinv         = row.try_get::<u32, _>(39).unwrap_or(0) as u8;
    // col 40: ChaPK — int(10) unsigned → u32, cast to u8
    s.pk             = row.try_get::<u32, _>(40).unwrap_or(0) as u8;
    s.killedby       = row.try_get::<u32, _>(41).unwrap_or(0);
    s.killspk        = row.try_get::<u32, _>(42).unwrap_or(0);
    s.pkduration     = row.try_get::<u32, _>(43).unwrap_or(0);
    // col 44: ChaMuted — int(10) unsigned → u32, cast to i8
    s.mute           = row.try_get::<u32, _>(44).unwrap_or(0) as i8;
    s.heroes         = row.try_get::<u32, _>(45).unwrap_or(0);
    // col 46: ChaTier — int(10) unsigned → u32, cast to u8
    s.tier           = row.try_get::<u32, _>(46).unwrap_or(0) as u8;
    s.expsold_magic  = row.try_get::<u64, _>(47).unwrap_or(0);
    s.expsold_health = row.try_get::<u64, _>(48).unwrap_or(0);
    s.expsold_stats  = row.try_get::<u64, _>(49).unwrap_or(0);
    s.basemight      = row.try_get::<u32, _>(50).unwrap_or(0);
    s.basewill       = row.try_get::<u32, _>(51).unwrap_or(0);
    s.basegrace      = row.try_get::<u32, _>(52).unwrap_or(0);
    s.basearmor      = row.try_get::<i32, _>(53).unwrap_or(0);
    s.mini_map_toggle = row.try_get::<u32, _>(54).unwrap_or(0);
    copy_str_to_i8(&mut s.ipaddress,   &row.try_get::<String, _>(55).unwrap_or_default());
    copy_str_to_i8(&mut s.afkmessage,  &row.try_get::<String, _>(56).unwrap_or_default());
    // col 57: ChaTutor — tinyint unsigned → u8 (correct as-is)
    s.tutor              = row.try_get::<u8, _>(57).unwrap_or(0);
    // col 58: ChaAlignment — tinyint signed → i8 (correct as-is)
    s.alignment          = row.try_get::<i8, _>(58).unwrap_or(0);
    // col 59-64: profile_* — int(10) unsigned → u32, cast to u8
    s.profile_vitastats  = row.try_get::<u32, _>(59).unwrap_or(0) as u8;
    s.profile_equiplist  = row.try_get::<u32, _>(60).unwrap_or(0) as u8;
    s.profile_legends    = row.try_get::<u32, _>(61).unwrap_or(0) as u8;
    s.profile_spells     = row.try_get::<u32, _>(62).unwrap_or(0) as u8;
    s.profile_inventory  = row.try_get::<u32, _>(63).unwrap_or(0) as u8;
    s.profile_bankitems  = row.try_get::<u32, _>(64).unwrap_or(0) as u8;
    // col 65: ChaPthRank — int(10) unsigned → u32, cast to i32
    s.class_rank         = row.try_get::<u32, _>(65).unwrap_or(0) as i32;
    // col 66: ChaClnRank — int(10) unsigned → u32, cast to i32
    s.clan_rank          = row.try_get::<u32, _>(66).unwrap_or(0) as i32;
    // mirror C line 616: overwrite name with login_name
    copy_str_to_i8(&mut s.name, login_name);

    // ── Banks ─────────────────────────────────────────────────────────────────
    // BnkPosition is int(10) unsigned → u32 (not u8)
    let banks: Vec<(String, u32, u32, u32, u32, u32, u32, u32, u32, u32, String)> =
        sqlx::query_as(
            "SELECT `BnkEngrave`, `BnkItmId`, `BnkAmount`, `BnkChaIdOwner`, \
             `BnkPosition`, `BnkCustomLook`, `BnkCustomLookColor`, \
             `BnkCustomIcon`, `BnkCustomIconColor`, `BnkProtected`, `BnkNote` \
             FROM `Banks` WHERE `BnkChaId` = ? LIMIT 255"
        ).bind(char_id).fetch_all(pool).await.unwrap_or_default();
    for (engrave, item_id, amount, owner, pos, custom_look, custom_look_color,
         custom_icon, custom_icon_color, protected, note) in banks {
        let p = pos as usize;
        if p >= MAX_BANK_SLOTS { continue; }
        copy_str_to_i8(&mut s.banks[p].real_name, &engrave);
        s.banks[p].item_id          = item_id;
        s.banks[p].amount           = amount;
        s.banks[p].owner            = owner;
        s.banks[p].custom_look      = custom_look;
        s.banks[p].custom_look_color = custom_look_color;
        s.banks[p].custom_icon      = custom_icon;
        s.banks[p].custom_icon_color = custom_icon_color;
        s.banks[p].protected        = protected;
        copy_str_to_i8(&mut s.banks[p].note, &note);
    }

    // ── Inventory ─────────────────────────────────────────────────────────────
    // InvDurability is int(10) unsigned → u32 (not i32)
    // InvPosition is int(10) unsigned → u32 (not u8)
    let items: Vec<(String, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, String)> =
        sqlx::query_as(
            "SELECT `InvEngrave`, `InvItmId`, `InvAmount`, `InvDurability`, \
             `InvChaIdOwner`, `InvTimer`, `InvPosition`, `InvCustom`, \
             `InvCustomLook`, `InvCustomLookColor`, `InvCustomIcon`, \
             `InvCustomIconColor`, `InvProtected`, `InvNote` \
             FROM `Inventory` WHERE `InvChaId` = ? LIMIT 52"
        ).bind(char_id).fetch_all(pool).await.unwrap_or_default();
    for (engrave, id, amount, dura, owner, time, pos, custom,
         custom_look, custom_look_color, custom_icon, custom_icon_color, protected, note) in items {
        let p = pos as usize;
        if p >= MAX_INVENTORY { continue; }
        copy_str_to_i8(&mut s.inventory[p].real_name, &engrave);
        s.inventory[p].id               = id;
        s.inventory[p].amount           = amount as i32;
        s.inventory[p].dura             = dura as i32;
        s.inventory[p].owner            = owner;
        s.inventory[p].time             = time;
        s.inventory[p].custom           = custom;
        s.inventory[p].custom_look      = custom_look;
        s.inventory[p].custom_look_color = custom_look_color;
        s.inventory[p].custom_icon      = custom_icon;
        s.inventory[p].custom_icon_color = custom_icon_color;
        s.inventory[p].protected        = protected;
        copy_str_to_i8(&mut s.inventory[p].note, &note);
    }

    // ── Equipment ─────────────────────────────────────────────────────────────
    // EqpDurability is int(10) unsigned → u32, EqpSlot is int(10) unsigned → u32
    let equips: Vec<(String, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, String)> =
        sqlx::query_as(
            "SELECT `EqpEngrave`, `EqpItmId`, '1', `EqpDurability`, \
             `EqpChaIdOwner`, `EqpTimer`, `EqpSlot`, `EqpCustom`, \
             `EqpCustomLook`, `EqpCustomLookColor`, `EqpCustomIcon`, \
             `EqpCustomIconColor`, `EqpProtected`, `EqpNote` \
             FROM `Equipment` WHERE `EqpChaId` = ? LIMIT 15"
        ).bind(char_id).fetch_all(pool).await.unwrap_or_default();
    for (engrave, id, amount, dura, owner, time, pos, custom,
         custom_look, custom_look_color, custom_icon, custom_icon_color, protected, note) in equips {
        let p = pos as usize;
        if p >= MAX_EQUIP { continue; }
        copy_str_to_i8(&mut s.equip[p].real_name, &engrave);
        s.equip[p].id               = id;
        s.equip[p].amount           = amount as i32;
        s.equip[p].dura             = dura as i32;
        s.equip[p].owner            = owner;
        s.equip[p].time             = time;
        s.equip[p].custom           = custom;
        s.equip[p].custom_look      = custom_look;
        s.equip[p].custom_look_color = custom_look_color;
        s.equip[p].custom_icon      = custom_icon;
        s.equip[p].custom_icon_color = custom_icon_color;
        s.equip[p].protected        = protected;
        copy_str_to_i8(&mut s.equip[p].note, &note);
    }

    // ── SpellBook ─────────────────────────────────────────────────────────────
    // SbkSplId and SbkPosition are int(10) unsigned → u32
    let spells: Vec<(u32, u32)> = sqlx::query_as(
        "SELECT `SbkSplId`, `SbkPosition` FROM `SpellBook` WHERE `SbkChaId` = ?"
    ).bind(char_id).fetch_all(pool).await.unwrap_or_default();
    for (spell_id, pos) in spells {
        let p = pos as usize;
        if p < MAX_SPELLS { s.skill[p] = spell_id as u16; }
    }

    // ── Aethers ───────────────────────────────────────────────────────────────
    // AthAether, AthDuration, AthPosition are int(10) unsigned → u32; AthSplId stays u16-ish
    let aethers: Vec<(u32, u32, u32, u32)> = sqlx::query_as(
        "SELECT `AthAether`, `AthSplId`, `AthDuration`, `AthPosition` \
         FROM `Aethers` WHERE `AthChaId` = ? LIMIT 200"
    ).bind(char_id).fetch_all(pool).await.unwrap_or_default();
    for (aether, spell_id, duration, pos) in aethers {
        let p = pos as usize;
        if p >= MAX_MAGIC_TIMERS { continue; }
        s.dura_aether[p].aether   = aether as i32;
        s.dura_aether[p].id       = spell_id as u16;
        s.dura_aether[p].duration = duration as i32;
    }

    // ── Registry (int) ────────────────────────────────────────────────────────
    // RegValue is int(10) unsigned → u32, cast to i32 in struct
    let regs: Vec<(String, u32)> = sqlx::query_as(
        "SELECT `RegIdentifier`, `RegValue` FROM `Registry` WHERE `RegChaId` = ? LIMIT 500"
    ).bind(char_id).fetch_all(pool).await.unwrap_or_default();
    s.global_reg_num = regs.len() as i32;
    for (i, (key, val)) in regs.into_iter().enumerate() {
        if i >= MAX_GLOBALREG { break; }
        copy_str_to_i8(&mut s.global_reg[i].str, &key);
        s.global_reg[i].val = val as i32;
    }

    // ── Registry (string) ─────────────────────────────────────────────────────
    let regstrs: Vec<(String, String)> = sqlx::query_as(
        "SELECT `RegIdentifier`, `RegValue` FROM `RegistryString` WHERE `RegChaId` = ? LIMIT 500"
    ).bind(char_id).fetch_all(pool).await.unwrap_or_default();
    s.global_regstring_num = regstrs.len() as i32;
    for (i, (key, val)) in regstrs.into_iter().enumerate() {
        if i >= MAX_GLOBALREG { break; }
        copy_str_to_i8(&mut s.global_regstring[i].str, &key);
        copy_str_to_i8(&mut s.global_regstring[i].val, &val);
    }

    // ── NPC Registry ──────────────────────────────────────────────────────────
    // NrgValue is int(10) unsigned → u32, cast to i32
    let npcregs: Vec<(String, u32)> = sqlx::query_as(
        "SELECT `NrgIdentifier`, `NrgValue` FROM `NPCRegistry` WHERE `NrgChaId` = ? LIMIT 100"
    ).bind(char_id).fetch_all(pool).await.unwrap_or_default();
    for (i, (key, val)) in npcregs.into_iter().enumerate() {
        if i >= MAX_GLOBALREG { break; }
        copy_str_to_i8(&mut s.npcintreg[i].str, &key);
        s.npcintreg[i].val = val as i32;
    }

    // ── Quest Registry ────────────────────────────────────────────────────────
    // QrgValue is int(10) unsigned → u32, cast to i32
    let questregs: Vec<(String, u32)> = sqlx::query_as(
        "SELECT `QrgIdentifier`, `QrgValue` FROM `QuestRegistry` WHERE `QrgChaId` = ? LIMIT 250"
    ).bind(char_id).fetch_all(pool).await.unwrap_or_default();
    for (i, (key, val)) in questregs.into_iter().enumerate() {
        if i >= MAX_GLOBALQUESTREG { break; }
        copy_str_to_i8(&mut s.questreg[i].str, &key);
        s.questreg[i].val = val as i32;
    }

    // ── Legends ───────────────────────────────────────────────────────────────
    // LegPosition, LegIcon, LegColor are int(10) unsigned → u32, cast to u16 where needed
    let legends: Vec<(u32, u32, u32, String, String, u32)> = sqlx::query_as(
        "SELECT `LegPosition`, `LegIcon`, `LegColor`, `LegDescription`, \
         `LegIdentifier`, `LegTChaId` FROM `Legends` WHERE `LegChaId` = ? LIMIT 1000"
    ).bind(char_id).fetch_all(pool).await.unwrap_or_default();
    for (pos, icon, color, text, name, tchaid) in legends {
        let p = pos as usize;
        if p >= MAX_LEGENDS { continue; }
        s.legends[p].icon   = icon as u16;
        s.legends[p].color  = color as u16;
        copy_str_to_i8(&mut s.legends[p].text, &text);
        copy_str_to_i8(&mut s.legends[p].name, &name);
        s.legends[p].tchaid = tchaid;
    }

    // ── Kill counts ───────────────────────────────────────────────────────────
    // KilPosition, KilMobId, KilAmount are all int(10) unsigned → u32 (already correct)
    let kills: Vec<(u32, u32, u32)> = sqlx::query_as(
        "SELECT `KilPosition`, `KilMobId`, `KilAmount` FROM `Kills` WHERE `KilChaId` = ? LIMIT 5000"
    ).bind(char_id).fetch_all(pool).await.unwrap_or_default();
    for (pos, mob_id, amount) in kills {
        let p = pos as usize;
        if p >= MAX_KILLREG { continue; }
        s.killreg[p].mob_id = mob_id;
        s.killreg[p].amount = amount;
    }

    Ok(char_status_to_bytes(&s).to_vec())
}

/// Save a character from a raw byte blob back to the DB.
/// Mirrors mmo_char_todb + sub-table save functions in char_db.c.
pub async fn save_char_bytes(pool: &MySqlPool, raw: &[u8]) -> Result<()> {
    use crate::servers::char::charstatus::*;

    let s = match char_status_from_bytes(raw) {
        Some(s) => s,
        None => return Ok(()),
    };
    if s.id == 0 { return Ok(()); }

    let name      = i8_slice_to_str(&s.name);
    let clan_title = i8_slice_to_str(&s.clan_title);
    let title     = i8_slice_to_str(&s.title);
    let f1name    = i8_slice_to_str(&s.f1name);
    let afkmsg    = i8_slice_to_str(&s.afkmessage);

    tracing::info!("[char] [save_char] name={}", name);

    sqlx::query(
        "UPDATE `Character` SET \
         `ChaName`=?, `ChaClnId`=?, `ChaClanTitle`=?, `ChaTitle`=?, `ChaLevel`=?, \
         `ChaPthId`=?, `ChaMark`=?, `ChaTotem`=?, `ChaKarma`=?, \
         `ChaCurrentVita`=?, `ChaBaseVita`=?, `ChaCurrentMana`=?, `ChaBaseMana`=?, \
         `ChaExperience`=?, `ChaGold`=?, `ChaSex`=?, `ChaNation`=?, `ChaFace`=?, \
         `ChaHairColor`=?, `ChaArmorColor`=?, `ChaMapId`=?, `ChaX`=?, `ChaY`=?, \
         `ChaSide`=?, `ChaState`=?, `ChaHair`=?, `ChaFaceColor`=?, `ChaSkinColor`=?, \
         `ChaPartner`=?, `ChaClanChat`=?, `ChaPathChat`=?, `ChaNoviceChat`=?, \
         `ChaSettings`=?, `ChaGMLevel`=?, `ChaDisguise`=?, `ChaDisguiseColor`=?, \
         `ChaMaximumBankSlots`=?, `ChaBankGold`=?, `ChaF1Name`=?, `ChaMaximumInventory`=?, \
         `ChaPK`=?, `ChaKilledBy`=?, `ChaKillsPK`=?, `ChaPKDuration`=?, `ChaMuted`=?, \
         `ChaHeroes`=?, `ChaTier`=?, `ChaExperienceSoldMagic`=?, `ChaExperienceSoldHealth`=?, \
         `ChaExperienceSoldStats`=?, `ChaBaseMight`=?, `ChaBaseWill`=?, `ChaBaseGrace`=?, \
         `ChaBaseArmor`=?, `ChaMiniMapToggle`=?, `ChaHunter`=0, `ChaAFKMessage`=?, \
         `ChaTutor`=?, `ChaAlignment`=?, `ChaProfileVitaStats`=?, `ChaProfileEquipList`=?, \
         `ChaProfileLegends`=?, `ChaProfileSpells`=?, `ChaProfileInventory`=?, \
         `ChaProfileBankItems`=?, `ChaPthRank`=?, `ChaClnRank`=? \
         WHERE `ChaId`=?"
    )
    .bind(&name).bind(s.clan).bind(&clan_title).bind(&title)
    .bind(s.level).bind(s.class).bind(s.mark).bind(s.totem).bind(s.karma)
    .bind(s.hp).bind(s.basehp).bind(s.mp).bind(s.basemp)
    .bind(s.exp).bind(s.money).bind(s.sex).bind(s.country).bind(s.face)
    .bind(s.hair_color).bind(s.armor_color)
    .bind(s.last_pos.m).bind(s.last_pos.x).bind(s.last_pos.y)
    .bind(s.side).bind(s.state).bind(s.hair).bind(s.face_color).bind(s.skin_color)
    .bind(s.partner).bind(s.clan_chat).bind(s.subpath_chat).bind(s.novice_chat)
    .bind(s.setting_flags).bind(s.gm_level).bind(s.disguise).bind(s.disguise_color)
    .bind(s.maxslots).bind(s.bankmoney).bind(&f1name).bind(s.maxinv)
    .bind(s.pk).bind(s.killedby).bind(s.killspk).bind(s.pkduration).bind(s.mute)
    .bind(s.heroes).bind(s.tier)
    .bind(s.expsold_magic).bind(s.expsold_health).bind(s.expsold_stats)
    .bind(s.basemight).bind(s.basewill).bind(s.basegrace)
    .bind(s.basearmor).bind(s.mini_map_toggle)
    .bind(&afkmsg).bind(s.tutor).bind(s.alignment)
    .bind(s.profile_vitastats).bind(s.profile_equiplist).bind(s.profile_legends)
    .bind(s.profile_spells).bind(s.profile_inventory).bind(s.profile_bankitems)
    .bind(s.class_rank as u8).bind(s.clan_rank as u8)
    .bind(s.id)
    .execute(pool).await?;

    // ── Sub-table saves (position-keyed upsert matching C pattern) ────────────

    // Inventory
    save_items_inventory(pool, s.id, &s.inventory).await;
    // Equipment
    save_items_equipment(pool, s.id, &s.equip).await;
    // SpellBook
    save_spells(pool, s.id, &s.skill).await;
    // Aethers
    save_aethers(pool, s.id, &s.dura_aether).await;
    // Registry (int)
    save_registry(pool, s.id, &s.global_reg, s.global_reg_num as usize).await;
    // Registry (string)
    save_registry_string(pool, s.id, &s.global_regstring, s.global_regstring_num as usize).await;
    // NPC Registry
    save_npc_registry(pool, s.id, &s.npcintreg).await;
    // Quest Registry
    save_quest_registry(pool, s.id, &s.questreg).await;
    // Kill counts
    save_kills(pool, s.id, &s.killreg).await;
    // Legends
    save_legends(pool, s.id, &s.legends).await;
    // Banks
    save_banks(pool, s.id, &s.banks).await;

    Ok(())
}

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

// ── Sub-table save helpers ────────────────────────────────────────────────────

async fn existing_positions(pool: &MySqlPool, sql: &str, id: u32) -> Vec<usize> {
    let rows: Vec<(i32,)> = sqlx::query_as(sql)
        .bind(id).fetch_all(pool).await.unwrap_or_default();
    rows.into_iter().map(|(p,)| p as usize).collect()
}

async fn save_items_inventory(pool: &MySqlPool, char_id: u32, items: &[crate::servers::char::charstatus::Item]) {
    use crate::servers::char::charstatus::MAX_INVENTORY;
    let existing: Vec<usize> = existing_positions(
        pool, "SELECT `InvPosition` FROM `Inventory` WHERE `InvChaId` = ? LIMIT 52", char_id
    ).await;
    for (i, item) in items.iter().enumerate().take(MAX_INVENTORY) {
        let name = i8_slice_to_str(&item.real_name);
        let note = i8_slice_to_str(&item.note);
        if existing.contains(&i) {
            if item.id == 0 {
                let _ = sqlx::query("DELETE FROM `Inventory` WHERE `InvChaId`=? AND `InvPosition`=?")
                    .bind(char_id).bind(i as u32).execute(pool).await;
            } else {
                let _ = sqlx::query(
                    "UPDATE `Inventory` SET `InvItmId`=?,`InvAmount`=?,`InvDurability`=?,\
                     `InvChaIdOwner`=?,`InvCustom`=?,`InvTimer`=?,`InvEngrave`=?,\
                     `InvCustomLook`=?,`InvCustomLookColor`=?,`InvCustomIcon`=?,\
                     `InvCustomIconColor`=?,`InvProtected`=?,`InvNote`=? \
                     WHERE `InvChaId`=? AND `InvPosition`=?"
                ).bind(item.id).bind(item.amount).bind(item.dura).bind(item.owner)
                 .bind(item.custom).bind(item.time).bind(&name)
                 .bind(item.custom_look).bind(item.custom_look_color)
                 .bind(item.custom_icon).bind(item.custom_icon_color)
                 .bind(item.protected).bind(&note).bind(char_id).bind(i as u32)
                 .execute(pool).await;
            }
        } else if item.id > 0 {
            let _ = sqlx::query(
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
             .execute(pool).await;
        }
    }
}

async fn save_items_equipment(pool: &MySqlPool, char_id: u32, items: &[crate::servers::char::charstatus::Item]) {
    use crate::servers::char::charstatus::MAX_EQUIP;
    let existing = existing_positions(
        pool, "SELECT `EqpSlot` FROM `Equipment` WHERE `EqpChaId` = ? LIMIT 15", char_id
    ).await;
    for (i, item) in items.iter().enumerate().take(MAX_EQUIP) {
        let name = i8_slice_to_str(&item.real_name);
        let note = i8_slice_to_str(&item.note);
        if existing.contains(&i) {
            if item.id == 0 {
                let _ = sqlx::query("DELETE FROM `Equipment` WHERE `EqpChaId`=? AND `EqpSlot`=?")
                    .bind(char_id).bind(i as u32).execute(pool).await;
            } else {
                let _ = sqlx::query(
                    "UPDATE `Equipment` SET `EqpItmId`=?,`EqpDurability`=?,`EqpChaIdOwner`=?,\
                     `EqpTimer`=?,`EqpEngrave`=?,`EqpCustomLook`=?,`EqpCustomLookColor`=?,\
                     `EqpCustomIcon`=?,`EqpCustomIconColor`=?,`EqpProtected`=?,`EqpNote`=? \
                     WHERE `EqpChaId`=? AND `EqpSlot`=?"
                ).bind(item.id).bind(item.dura).bind(item.owner).bind(item.time).bind(&name)
                 .bind(item.custom_look).bind(item.custom_look_color)
                 .bind(item.custom_icon).bind(item.custom_icon_color)
                 .bind(item.protected).bind(&note).bind(char_id).bind(i as u32)
                 .execute(pool).await;
            }
        } else if item.id > 0 {
            let _ = sqlx::query(
                "INSERT INTO `Equipment` \
                 (`EqpChaId`,`EqpItmId`,`EqpDurability`,`EqpChaIdOwner`,`EqpTimer`,\
                  `EqpEngrave`,`EqpCustomLook`,`EqpCustomLookColor`,`EqpCustomIcon`,\
                  `EqpCustomIconColor`,`EqpProtected`,`EqpNote`,`EqpSlot`) \
                 VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)"
            ).bind(char_id).bind(item.id).bind(item.dura).bind(item.owner).bind(item.time)
             .bind(&name).bind(item.custom_look).bind(item.custom_look_color)
             .bind(item.custom_icon).bind(item.custom_icon_color)
             .bind(item.protected).bind(&note).bind(i as u32)
             .execute(pool).await;
        }
    }
}

async fn save_spells(pool: &MySqlPool, char_id: u32, skills: &[u16]) {
    use crate::servers::char::charstatus::MAX_SPELLS;
    let existing = existing_positions(
        pool, "SELECT `SbkPosition` FROM `SpellBook` WHERE `SbkChaId` = ? LIMIT 52", char_id
    ).await;
    for (i, &spell_id) in skills.iter().enumerate().take(MAX_SPELLS) {
        if existing.contains(&i) {
            if spell_id == 0 {
                let _ = sqlx::query("DELETE FROM `SpellBook` WHERE `SbkChaId`=? AND `SbkPosition`=?")
                    .bind(char_id).bind(i as u32).execute(pool).await;
            } else {
                let _ = sqlx::query("UPDATE `SpellBook` SET `SbkSplId`=? WHERE `SbkChaId`=? AND `SbkPosition`=?")
                    .bind(spell_id).bind(char_id).bind(i as u32).execute(pool).await;
            }
        } else if spell_id > 0 {
            let _ = sqlx::query("INSERT INTO `SpellBook` (`SbkChaId`,`SbkSplId`,`SbkPosition`) VALUES(?,?,?)")
                .bind(char_id).bind(spell_id).bind(i as u32).execute(pool).await;
        }
    }
}

async fn save_aethers(pool: &MySqlPool, char_id: u32, aethers: &[crate::servers::char::charstatus::SkillInfo]) {
    use crate::servers::char::charstatus::MAX_MAGIC_TIMERS;
    let existing = existing_positions(
        pool, "SELECT `AthPosition` FROM `Aethers` WHERE `AthChaId` = ? LIMIT 200", char_id
    ).await;
    for (i, a) in aethers.iter().enumerate().take(MAX_MAGIC_TIMERS) {
        if existing.contains(&i) {
            if a.id == 0 {
                let _ = sqlx::query("DELETE FROM `Aethers` WHERE `AthChaId`=? AND `AthPosition`=?")
                    .bind(char_id).bind(i as u32).execute(pool).await;
            } else {
                let _ = sqlx::query(
                    "UPDATE `Aethers` SET `AthSplId`=?,`AthDuration`=?,`AthAether`=? \
                     WHERE `AthChaId`=? AND `AthPosition`=?"
                ).bind(a.id).bind(a.duration).bind(a.aether).bind(char_id).bind(i as u32)
                 .execute(pool).await;
            }
        } else if a.id > 0 {
            let _ = sqlx::query(
                "INSERT INTO `Aethers` (`AthChaId`,`AthSplId`,`AthDuration`,`AthAether`,`AthPosition`) VALUES(?,?,?,?,?)"
            ).bind(char_id).bind(a.id).bind(a.duration).bind(a.aether).bind(i as u32)
             .execute(pool).await;
        }
    }
}

async fn save_registry(pool: &MySqlPool, char_id: u32, regs: &[crate::servers::char::charstatus::GlobalReg], count: usize) {
    let existing = existing_positions(
        pool, "SELECT `RegPosition` FROM `Registry` WHERE `RegChaId` = ? LIMIT 500", char_id
    ).await;
    for (i, reg) in regs.iter().enumerate().take(count.min(500)) {
        let key = i8_slice_to_str(&reg.str);
        if existing.contains(&i) {
            if reg.val == 0 {
                let _ = sqlx::query("DELETE FROM `Registry` WHERE `RegChaId`=? AND `RegPosition`=?")
                    .bind(char_id).bind(i as u32).execute(pool).await;
            } else {
                let _ = sqlx::query("UPDATE `Registry` SET `RegIdentifier`=?,`RegValue`=? WHERE `RegChaId`=? AND `RegPosition`=?")
                    .bind(&key).bind(reg.val).bind(char_id).bind(i as u32).execute(pool).await;
            }
        } else if reg.val > 0 {
            let _ = sqlx::query("INSERT INTO `Registry` (`RegChaId`,`RegIdentifier`,`RegValue`,`RegPosition`) VALUES(?,?,?,?)")
                .bind(char_id).bind(&key).bind(reg.val).bind(i as u32).execute(pool).await;
        }
    }
}

async fn save_registry_string(pool: &MySqlPool, char_id: u32, regs: &[crate::servers::char::charstatus::GlobalRegString], count: usize) {
    let existing = existing_positions(
        pool, "SELECT `RegPosition` FROM `RegistryString` WHERE `RegChaId` = ? LIMIT 500", char_id
    ).await;
    for (i, reg) in regs.iter().enumerate().take(count.min(500)) {
        let key = i8_slice_to_str(&reg.str);
        let val = i8_slice_to_str(&reg.val);
        if existing.contains(&i) {
            if val.is_empty() {
                let _ = sqlx::query("DELETE FROM `RegistryString` WHERE `RegChaId`=? AND `RegPosition`=?")
                    .bind(char_id).bind(i as u32).execute(pool).await;
            } else {
                let _ = sqlx::query("UPDATE `RegistryString` SET `RegIdentifier`=?,`RegValue`=? WHERE `RegChaId`=? AND `RegPosition`=?")
                    .bind(&key).bind(&val).bind(char_id).bind(i as u32).execute(pool).await;
            }
        } else if !val.is_empty() {
            let _ = sqlx::query("INSERT INTO `RegistryString` (`RegChaId`,`RegIdentifier`,`RegValue`,`RegPosition`) VALUES(?,?,?,?)")
                .bind(char_id).bind(&key).bind(&val).bind(i as u32).execute(pool).await;
        }
    }
}

async fn save_npc_registry(pool: &MySqlPool, char_id: u32, regs: &[crate::servers::char::charstatus::GlobalReg]) {
    let existing = existing_positions(
        pool, "SELECT `NrgPosition` FROM `NPCRegistry` WHERE `NrgChaId` = ? LIMIT 100", char_id
    ).await;
    for (i, reg) in regs.iter().enumerate().take(100) {
        let key = i8_slice_to_str(&reg.str);
        if existing.contains(&i) {
            if reg.val == 0 {
                let _ = sqlx::query("DELETE FROM `NPCRegistry` WHERE `NrgChaId`=? AND `NrgPosition`=?")
                    .bind(char_id).bind(i as u32).execute(pool).await;
            } else {
                let _ = sqlx::query("UPDATE `NPCRegistry` SET `NrgIdentifier`=?,`NrgValue`=? WHERE `NrgChaId`=? AND `NrgPosition`=?")
                    .bind(&key).bind(reg.val).bind(char_id).bind(i as u32).execute(pool).await;
            }
        } else if reg.val > 0 {
            let _ = sqlx::query("INSERT INTO `NPCRegistry` (`NrgChaId`,`NrgIdentifier`,`NrgValue`,`NrgPosition`) VALUES(?,?,?,?)")
                .bind(char_id).bind(&key).bind(reg.val).bind(i as u32).execute(pool).await;
        }
    }
}

async fn save_quest_registry(pool: &MySqlPool, char_id: u32, regs: &[crate::servers::char::charstatus::GlobalReg]) {
    use crate::servers::char::charstatus::MAX_GLOBALQUESTREG;
    let existing = existing_positions(
        pool, "SELECT `QrgPosition` FROM `QuestRegistry` WHERE `QrgChaId` = ? LIMIT 250", char_id
    ).await;
    for (i, reg) in regs.iter().enumerate().take(MAX_GLOBALQUESTREG) {
        let key = i8_slice_to_str(&reg.str);
        if existing.contains(&i) {
            if reg.val == 0 {
                let _ = sqlx::query("DELETE FROM `QuestRegistry` WHERE `QrgChaId`=? AND `QrgPosition`=?")
                    .bind(char_id).bind(i as u32).execute(pool).await;
            } else {
                let _ = sqlx::query("UPDATE `QuestRegistry` SET `QrgIdentifier`=?,`QrgValue`=? WHERE `QrgChaId`=? AND `QrgPosition`=?")
                    .bind(&key).bind(reg.val).bind(char_id).bind(i as u32).execute(pool).await;
            }
        } else if reg.val > 0 {
            let _ = sqlx::query("INSERT INTO `QuestRegistry` (`QrgChaId`,`QrgIdentifier`,`QrgValue`,`QrgPosition`) VALUES(?,?,?,?)")
                .bind(char_id).bind(&key).bind(reg.val).bind(i as u32).execute(pool).await;
        }
    }
}

async fn save_kills(pool: &MySqlPool, char_id: u32, kills: &[crate::servers::char::charstatus::KillReg]) {
    use crate::servers::char::charstatus::MAX_KILLREG;
    let existing = existing_positions(
        pool, "SELECT `KilPosition` FROM `Kills` WHERE `KilChaId` = ? LIMIT 5000", char_id
    ).await;
    for (i, k) in kills.iter().enumerate().take(MAX_KILLREG) {
        if existing.contains(&i) {
            if k.mob_id == 0 {
                let _ = sqlx::query("DELETE FROM `Kills` WHERE `KilChaId`=? AND `KilPosition`=?")
                    .bind(char_id).bind(i as u32).execute(pool).await;
            } else {
                let _ = sqlx::query("UPDATE `Kills` SET `KilAmount`=?,`KilMobId`=? WHERE `KilChaId`=? AND `KilPosition`=?")
                    .bind(k.amount).bind(k.mob_id).bind(char_id).bind(i as u32).execute(pool).await;
            }
        } else if k.mob_id > 0 {
            let _ = sqlx::query("INSERT INTO `Kills` (`KilChaId`,`KilMobId`,`KilAmount`,`KilPosition`) VALUES(?,?,?,?)")
                .bind(char_id).bind(k.mob_id).bind(k.amount).bind(i as u32).execute(pool).await;
        }
    }
}

async fn save_legends(pool: &MySqlPool, char_id: u32, legends: &[crate::servers::char::charstatus::Legend]) {
    use crate::servers::char::charstatus::MAX_LEGENDS;
    let existing = existing_positions(
        pool, "SELECT `LegPosition` FROM `Legends` WHERE `LegChaId` = ? LIMIT 1000", char_id
    ).await;
    for (i, leg) in legends.iter().enumerate().take(MAX_LEGENDS) {
        let name = i8_slice_to_str(&leg.name);
        let text = i8_slice_to_str(&leg.text);
        if existing.contains(&i) {
            if name.is_empty() {
                let _ = sqlx::query("DELETE FROM `Legends` WHERE `LegChaId`=? AND `LegPosition`=?")
                    .bind(char_id).bind(i as u32).execute(pool).await;
            } else {
                let _ = sqlx::query(
                    "UPDATE `Legends` SET `LegIcon`=?,`LegColor`=?,`LegDescription`=?,`LegIdentifier`=?,`LegTChaId`=? \
                     WHERE `LegChaId`=? AND `LegPosition`=?"
                ).bind(leg.icon).bind(leg.color).bind(&text).bind(&name).bind(leg.tchaid)
                 .bind(char_id).bind(i as u32).execute(pool).await;
            }
        } else if !name.is_empty() {
            let _ = sqlx::query(
                "INSERT INTO `Legends` (`LegChaId`,`LegIcon`,`LegColor`,`LegDescription`,`LegIdentifier`,`LegTChaId`,`LegPosition`) VALUES(?,?,?,?,?,?,?)"
            ).bind(char_id).bind(leg.icon).bind(leg.color).bind(&text).bind(&name).bind(leg.tchaid).bind(i as u32)
             .execute(pool).await;
        }
    }
}

async fn save_banks(pool: &MySqlPool, char_id: u32, banks: &[crate::servers::char::charstatus::BankData]) {
    use crate::servers::char::charstatus::MAX_BANK_SLOTS;
    let existing = existing_positions(
        pool, "SELECT `BnkPosition` FROM `Banks` WHERE `BnkChaId` = ? LIMIT 255", char_id
    ).await;
    for (i, bank) in banks.iter().enumerate().take(MAX_BANK_SLOTS) {
        let name = i8_slice_to_str(&bank.real_name);
        let note = i8_slice_to_str(&bank.note);
        if existing.contains(&i) {
            if bank.item_id == 0 {
                let _ = sqlx::query("DELETE FROM `Banks` WHERE `BnkChaId`=? AND `BnkPosition`=?")
                    .bind(char_id).bind(i as u32).execute(pool).await;
            } else {
                let _ = sqlx::query(
                    "UPDATE `Banks` SET `BnkItmId`=?,`BnkAmount`=?,`BnkChaIdOwner`=?,`BnkCustomLook`=?,\
                     `BnkCustomLookColor`=?,`BnkCustomIcon`=?,`BnkCustomIconColor`=?,\
                     `BnkProtected`=?,`BnkEngrave`=?,`BnkNote`=? \
                     WHERE `BnkChaId`=? AND `BnkPosition`=?"
                ).bind(bank.item_id).bind(bank.amount).bind(bank.owner)
                 .bind(bank.custom_look).bind(bank.custom_look_color)
                 .bind(bank.custom_icon).bind(bank.custom_icon_color)
                 .bind(bank.protected).bind(&name).bind(&note)
                 .bind(char_id).bind(i as u32).execute(pool).await;
            }
        } else if bank.item_id > 0 {
            let _ = sqlx::query(
                "INSERT INTO `Banks` (`BnkChaId`,`BnkItmId`,`BnkAmount`,`BnkChaIdOwner`,\
                 `BnkCustomLook`,`BnkCustomLookColor`,`BnkCustomIcon`,`BnkCustomIconColor`,\
                 `BnkProtected`,`BnkEngrave`,`BnkNote`,`BnkPosition`) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)"
            ).bind(char_id).bind(bank.item_id).bind(bank.amount).bind(bank.owner)
             .bind(bank.custom_look).bind(bank.custom_look_color)
             .bind(bank.custom_icon).bind(bank.custom_icon_color)
             .bind(bank.protected).bind(&name).bind(&note).bind(i as u32)
             .execute(pool).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ispass_form1() {
        let hash = md5_hex("alice password");
        assert!(ispass("Alice", "password", &hash));
    }

    #[test]
    fn test_ispass_form2() {
        let hash = md5_hex("mypass");
        assert!(ispass("bob", "mypass", &hash));
    }

    #[test]
    fn test_ispass_wrong() {
        let hash = md5_hex("correct");
        assert!(!ispass("bob", "wrong", &hash));
    }
}
