//! Character table helpers — online status and name lookups.

use crate::common::traits::LegacyEntity;
use crate::game::entity_store::map_id2sd_pc;
use crate::session::session_get_client_ip;

use super::get_pool;

/// Updates `Character.ChaOnline`/`ChaLastIP`.
///
/// Returns `true` if the login Lua hook should be fired (character exists and val != 0).
/// The caller is responsible for firing the hook AFTER this future completes so that
/// any DB calls inside the hook do not re-enter DB_RUNTIME (which would deadlock).
///
/// # Safety
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn mmo_setonline(id: u32, val: i32) -> bool {
    let addr = {
        let Some(arc) = map_id2sd_pc(id) else {
            return false;
        };
        let sd = arc.read();
        let fd = sd.fd;
        let raw_ip = session_get_client_ip(fd);
        format!(
            "{}.{}.{}.{}",
            raw_ip & 0xff,
            (raw_ip >> 8) & 0xff,
            (raw_ip >> 16) & 0xff,
            (raw_ip >> 24) & 0xff,
        )
    };

    let pool = get_pool();
    let exists: bool =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM `Character` WHERE `ChaId` = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap_or(0)
            > 0;

    let pool = get_pool();
    let _ =
        sqlx::query("UPDATE `Character` SET `ChaOnline` = ?, `ChaLastIP` = ? WHERE `ChaId` = ?")
            .bind(val)
            .bind(&addr)
            .bind(id)
            .execute(pool)
            .await;

    exists && val != 0
}

/// Look up a character's name by ID.
/// Returns `"None"` for id=0, empty string if not found.
pub async fn map_id2name(id: u32) -> String {
    if id == 0 {
        return "None".to_string();
    }
    if let Some(pe) = map_id2sd_pc(id) {
        return pe.name.clone();
    }
    sqlx::query_scalar::<_, String>("SELECT `ChaName` FROM `Character` WHERE `ChaId`=?")
        .bind(id)
        .fetch_optional(get_pool())
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}
