use sqlx::MySqlPool;

/// Returns true if `ip` (dotted-decimal string) is in `BannedIP`.
pub async fn is_ip_banned(pool: &MySqlPool, ip: &str) -> bool {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM `BannedIP` WHERE `BndIP` = ?"
    )
    .bind(ip)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    row.map(|(n,)| n > 0).unwrap_or(false)
}

/// Returns true if the `Maintenance` table flag is non-zero.
pub async fn get_maintenance_mode(pool: &MySqlPool) -> bool {
    let row: Option<(i32,)> = sqlx::query_as(
        "SELECT `MaintenanceMode` FROM `Maintenance` LIMIT 1"
    )
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    row.map(|(n,)| n != 0).unwrap_or(false)
}

/// Returns the GM level for `char_name`, or 0 if not found.
pub async fn get_char_gm_level(pool: &MySqlPool, char_name: &str) -> u32 {
    let row: Option<(u32,)> = sqlx::query_as(
        "SELECT `ChaGMLevel` FROM `Character` WHERE `ChaName` = ?"
    )
    .bind(char_name)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    row.map(|(n,)| n).unwrap_or(0)
}

/// Returns the AccountId that owns `char_name`, or 0 if not found/unattached.
pub async fn get_account_for_char(pool: &MySqlPool, char_name: &str) -> u32 {
    let char_id: Option<(u32,)> = sqlx::query_as(
        "SELECT `ChaId` FROM `Character` WHERE `ChaName` = ?"
    )
    .bind(char_name)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let char_id = match char_id.map(|(id,)| id) {
        Some(id) if id > 0 => id,
        _ => return 0,
    };

    let account: Option<(u32,)> = sqlx::query_as(
        "SELECT `AccountId` FROM `Accounts` WHERE
         `AccountCharId1` = ? OR `AccountCharId2` = ? OR `AccountCharId3` = ? OR
         `AccountCharId4` = ? OR `AccountCharId5` = ? OR `AccountCharId6` = ?"
    )
    .bind(char_id).bind(char_id).bind(char_id)
    .bind(char_id).bind(char_id).bind(char_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    account.map(|(id,)| id).unwrap_or(0)
}

/// Updates `ChaLastIP` for a character on successful login.
pub async fn update_char_last_ip(pool: &MySqlPool, char_name: &str, ip: &str) {
    let _ = sqlx::query(
        "UPDATE `Character` SET `ChaLastIP` = ? WHERE `ChaName` = ?"
    )
    .bind(ip)
    .bind(char_name)
    .execute(pool)
    .await;
}

#[cfg(test)]
mod tests {
    // DB integration tests require a live DATABASE_URL; skipped in CI.
    // Pattern matches src/database/mob_db.rs convention.
}
