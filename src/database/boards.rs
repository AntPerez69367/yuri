//! Board and mail DB operations extracted from char server handlers.
//!
//! These functions contain the SQL queries previously embedded in
//! `src/servers/char/map.rs` (0x3008–0x300F handlers).

use sqlx::MySqlPool;

/// Delete a board post or mail. Returns: 0=ok, 1=not owner, 2=db error.
pub async fn delete_post(
    pool: &MySqlPool,
    board: u16,
    post: u16,
    name: &str,
    gm_level: u16,
    can_delete: u16,
) -> u8 {
    if board == 0 {
        // NMAIL delete
        let r = sqlx::query(
            "UPDATE `Mail` SET `MalDeleted` = 1, `MalNew` = 0 \
             WHERE `MalChaNameDestination` = ? AND `MalPosition` = ?"
        ).bind(name).bind(post).execute(pool).await;
        if r.is_err() { 2 } else { 0 }
    } else if gm_level >= 50 || can_delete != 0 {
        let r = sqlx::query(
            "DELETE FROM `Boards` WHERE `BrdBnmId` = ? AND `BrdPosition` = ?"
        ).bind(board).bind(post).execute(pool).await;
        if r.is_err() { 2 } else { 0 }
    } else {
        // Non-GM: verify ownership first
        let owns: Option<(i64,)> = sqlx::query_as(
            "SELECT COUNT(*) FROM `Boards` \
             WHERE `BrdBnmId` = ? AND `BrdPosition` = ? AND `BrdChaName` = ?"
        ).bind(board).bind(post).bind(name)
         .fetch_optional(pool).await.unwrap_or(None);
        match owns {
            Some((n,)) if n > 0 => {
                let r = sqlx::query(
                    "DELETE FROM `Boards` WHERE `BrdBnmId` = ? AND `BrdPosition` = ?"
                ).bind(board).bind(post).execute(pool).await;
                if r.is_err() { 2 } else { 0 }
            }
            _ => 1,
        }
    }
}

/// A row from the show-posts query.
pub struct PostListRow {
    pub board_name: u32,
    pub color: u32,
    pub post_id: u32,
    pub month: u32,
    pub day: u32,
    pub user: String,
    pub topic: String,
}

/// List posts on a board or mail inbox.
pub async fn list_posts(
    pool: &MySqlPool,
    board: u32,
    offset: u32,
    name: &str,
) -> Vec<PostListRow> {
    let rows: Vec<(u32, String, String, u32, u32, u32, u32)> = if board == 0 {
        sqlx::query_as(
            "SELECT `MalNew`, `MalChaName`, `MalSubject`, `MalPosition`, \
             `MalMonth`, `MalDay`, `MalId` FROM `Mail` \
             WHERE `MalChaNameDestination` = ? AND `MalDeleted` = 0 \
             ORDER BY `MalPosition` DESC LIMIT 20 OFFSET ?"
        ).bind(name).bind(offset).fetch_all(pool).await.unwrap_or_default()
    } else {
        sqlx::query_as(
            "SELECT CAST(`BrdHighlighted` AS UNSIGNED), `BrdChaName`, `BrdTopic`, `BrdPosition`, \
             `BrdMonth`, `BrdDay`, `BrdBtlId` FROM `Boards` \
             WHERE `BrdBnmId` = ? ORDER BY `BrdPosition` DESC LIMIT 20 OFFSET ?"
        ).bind(board).bind(offset).fetch_all(pool).await.unwrap_or_default()
    };
    rows.into_iter().map(|(color, user, topic, post_id, month, day, board_name)| {
        PostListRow { board_name, color, post_id, month, day, user, topic }
    }).collect()
}

/// A full post read result.
pub struct PostContent {
    pub user: String,
    pub topic: String,
    pub body: String,
    pub post_id: u32,
    pub month: u32,
    pub day: u32,
    pub board_name: u32,
}

/// Read a single post. Clamps post_id to max position.
pub async fn read_post(
    pool: &MySqlPool,
    board: u32,
    post: u32,
    name: &str,
) -> Option<PostContent> {
    // Get MAX position
    let max_row: Option<(Option<u32>,)> = if board == 0 {
        sqlx::query_as(
            "SELECT MAX(`MalPosition`) FROM `Mail` \
             WHERE `MalChaNameDestination` = ? AND `MalDeleted` = 0"
        ).bind(name).fetch_optional(pool).await.unwrap_or(None)
    } else {
        sqlx::query_as(
            "SELECT MAX(`BrdPosition`) FROM `Boards` WHERE `BrdBnmId` = ?"
        ).bind(board).fetch_optional(pool).await.unwrap_or(None)
    };
    let max_pos = max_row.and_then(|(v,)| v).unwrap_or(0);
    let post = if post > max_pos { 1 } else { post };

    let row: Option<(String, String, String, u32, u32, u32, u32)> = if board == 0 {
        sqlx::query_as(
            "SELECT `MalChaName`, `MalSubject`, `MalBody`, `MalPosition`, \
             `MalMonth`, `MalDay`, `MalId` FROM `Mail` \
             WHERE `MalChaNameDestination` = ? AND `MalPosition` >= ? \
             AND `MalDeleted` = 0 ORDER BY `MalPosition` LIMIT 1"
        ).bind(name).bind(post).fetch_optional(pool).await.unwrap_or(None)
    } else {
        sqlx::query_as(
            "SELECT `BrdChaName`, `BrdTopic`, `BrdPost`, `BrdPosition`, \
             `BrdMonth`, `BrdDay`, `BrdBtlId` FROM `Boards` \
             WHERE `BrdBnmId` = ? AND `BrdPosition` >= ? \
             ORDER BY `BrdPosition` LIMIT 1"
        ).bind(board).bind(post).fetch_optional(pool).await.unwrap_or(None)
    };

    row.map(|(user, topic, body, post_id, month, day, board_name)| {
        PostContent { user, topic, body, post_id, month, day, board_name }
    })
}

/// Mark a mail as read and return whether unread count is now zero.
pub async fn mark_mail_read(pool: &MySqlPool, post: u32, name: &str) -> bool {
    let _ = sqlx::query(
        "UPDATE `Mail` SET `MalNew` = 0 \
         WHERE `MalPosition` = ? AND `MalChaNameDestination` = ?"
    ).bind(post).bind(name).execute(pool).await;

    let unread: Option<(i64,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM `Mail` WHERE `MalChaNameDestination` = ? AND `MalNew` = 1"
    ).bind(name).fetch_optional(pool).await.unwrap_or(None);

    unread.map(|(n,)| n).unwrap_or(0) <= 0
}

/// List online heroes for the user list.
pub struct OnlineHero {
    pub class: u32,
    pub mark: u32,
    pub clan: u32,
    pub name: String,
    pub hunter: u32,
    pub nation: u32,
}

pub async fn list_online_heroes(pool: &MySqlPool) -> Vec<OnlineHero> {
    let rows: Vec<(u32, u32, u32, String, u32, u32)> = sqlx::query_as(
        "SELECT `ChaPthId`, `ChaMark`, `ChaClnId`, `ChaName`, \
         `ChaHunter`, `ChaNation` FROM `Character` WHERE `ChaOnline` = 1 \
         AND `ChaHeroes` = 1 GROUP BY `ChaName`, `ChaId` ORDER BY \
         `ChaMark` DESC, `ChaLevel` DESC, \
         SUM((`ChaBaseMana`*2) + `ChaBaseVita`) DESC, `ChaId` ASC"
    ).fetch_all(pool).await.unwrap_or_default();
    rows.into_iter().map(|(class, mark, clan, name, hunter, nation)| {
        OnlineHero { class, mark, clan, name, hunter, nation }
    }).collect()
}

/// Create a new board post. Returns: 0=ok, 1=error.
pub async fn create_board_post(
    pool: &MySqlPool,
    board: u32,
    nval: i32,
    name: &str,
    topic: &str,
    post: &str,
) -> u16 {
    let max_row: Option<(Option<u32>,)> = sqlx::query_as(
        "SELECT MAX(`BrdPosition`) FROM `Boards` WHERE `BrdBnmId` = ?"
    ).bind(board).fetch_optional(pool).await.unwrap_or(None);
    let new_id = max_row.and_then(|(v,)| v).unwrap_or(0) + 1;

    let result = sqlx::query(
        "INSERT INTO `Boards` \
         (`BrdBnmId`, `BrdHighlighted`, `BrdChaName`, `BrdTopic`, `BrdPost`, \
          `BrdPosition`, `BrdMonth`, `BrdDay`, `BrdBtlId`) \
         VALUES(?, 0, ?, ?, ?, ?, DATE_FORMAT(CURDATE(),'%m'), \
                DATE_FORMAT(CURDATE(),'%d'), ?)"
    ).bind(board).bind(name).bind(topic).bind(post)
     .bind(new_id).bind(nval)
     .execute(pool).await;

    if result.is_err() { 1 } else { 0 }
}

/// Send mail. Returns: 0=ok, 1=db error, 2=recipient not found.
pub async fn nmail_insert(
    pool: &MySqlPool,
    from: &str,
    to: &str,
    topic: &str,
    msg: &str,
) -> u16 {
    let exists: Option<(u32,)> = sqlx::query_as(
        "SELECT `ChaId` FROM `Character` WHERE `ChaName` = ?"
    ).bind(to).fetch_optional(pool).await.unwrap_or(None);
    if exists.is_none() { return 2; }

    let max_row: Option<(Option<u32>,)> = sqlx::query_as(
        "SELECT MAX(`MalPosition`) FROM `Mail` WHERE `MalChaNameDestination` = ?"
    ).bind(to).fetch_optional(pool).await.unwrap_or(None);
    let new_id = max_row.and_then(|(v,)| v).unwrap_or(0) + 1;

    let result = sqlx::query(
        "INSERT INTO `Mail` \
         (`MalChaName`, `MalChaNameDestination`, `MalPosition`, `MalSubject`, \
          `MalBody`, `MalMonth`, `MalDay`, `MalNew`) \
         VALUES(?, ?, ?, ?, ?, DATE_FORMAT(CURDATE(),'%m'), \
                DATE_FORMAT(CURDATE(),'%d'), 1)"
    ).bind(from).bind(to).bind(new_id).bind(topic).bind(msg)
     .execute(pool).await;

    if result.is_err() { 1 } else { 0 }
}
