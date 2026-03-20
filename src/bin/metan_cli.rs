//! metan_cli — generates binary .metan metadata files from the item database
//!
//! Meta file CLI utility. Connects to MySQL via sqlx (same pool used by item_db/class_db),
//! loads itemdb + classdb, queries the Items table, and writes .metan files.

use anyhow::{Context, Result};
use sqlx::mysql::MySqlPoolOptions;
use sqlx::Row;
use std::ffi::CStr;
use std::fs::File;
use std::io::{BufWriter, Write};

use yuri::config::ServerConfig;
use yuri::database::{item_db, class_db};

fn swap16(val: u16) -> u16 {
    val.swap_bytes()
}

/// Write a u8-length-prefixed byte field.
fn write_u8len_field(w: &mut impl Write, data: &[u8]) -> std::io::Result<()> {
    w.write_all(&[data.len() as u8])?;
    w.write_all(data)
}

/// Write a u16be-length-prefixed byte field (length is swap16'd as the C code does).
fn write_u16_field(w: &mut impl Write, text: &str) -> std::io::Result<()> {
    let bytes = text.as_bytes();
    let len = bytes.len() as u16;
    let swapped = swap16(len);
    w.write_all(&swapped.to_ne_bytes())?;
    w.write_all(bytes)
}

fn write_int_field(w: &mut impl Write, v: i32) -> std::io::Result<()> {
    write_u16_field(w, &v.to_string())
}

/// Write a .metan file for a chunk of 1000 items starting at `num * 1000`.
fn output_meta(filename: &str, num: usize, list: &[u32]) -> Result<()> {
    let offset = if list.len() < (num + 1) * 1000 {
        list.len().saturating_sub(num * 1000)
    } else {
        1000
    };
    let base = num * 1000;

    let f = File::create(filename)
        .with_context(|| format!("Cannot create {}", filename))?;
    let mut w = BufWriter::new(f);

    // Write count as swap16(offset)
    let size_field: u16 = swap16(offset as u16);
    w.write_all(&size_field.to_ne_bytes())?;

    for id in list.iter().copied().skip(base).take(offset) {

        // name (u8-length-prefixed)
        let db = item_db::search(id);
        let name_bytes = {
            let cstr = unsafe { CStr::from_ptr(db.name.as_ptr()) };
            cstr.to_bytes().to_vec()
        };
        write_u8len_field(&mut w, &name_bytes)?;

        // write 20 (swap16'd)
        let twenty: u16 = swap16(20);
        w.write_all(&twenty.to_ne_bytes())?;

        // All integer fields
        let mightreq  = db.mightreq;
        let price     = db.price;
        let dura      = db.dura;
        let ac        = db.ac;
        let hit       = db.hit;
        let dam       = db.dam;
        let vita      = db.vita;
        let mana      = db.mana;
        let might     = db.might;
        let grace     = db.grace;
        let will      = db.will;
        let wisdom    = db.wisdom;
        let con       = db.con;
        let class_id  = db.class as i32;
        let rank      = db.rank;
        let level     = db.level as i32;
        let healing   = db.healing;
        let protection = db.protection;
        let min_sdam  = db.min_sdam as i32;
        let max_sdam  = db.max_sdam as i32;
        let min_ldam  = db.min_ldam as i32;
        let max_ldam  = db.max_ldam as i32;

        write_int_field(&mut w, mightreq)?;
        write_int_field(&mut w, price)?;
        write_int_field(&mut w, dura)?;
        write_int_field(&mut w, ac)?;
        write_int_field(&mut w, hit)?;
        write_int_field(&mut w, dam)?;
        write_int_field(&mut w, vita)?;
        write_int_field(&mut w, mana)?;
        write_int_field(&mut w, might)?;
        write_int_field(&mut w, grace)?;
        write_int_field(&mut w, will)?;
        write_int_field(&mut w, wisdom)?;
        write_int_field(&mut w, con)?;

        // classdb_path(itemdb_class(id))
        let path = class_db::path(class_id);
        write_int_field(&mut w, path)?;

        write_int_field(&mut w, rank)?;
        write_int_field(&mut w, level)?;
        write_int_field(&mut w, healing)?;
        write_int_field(&mut w, protection)?;

        // short damage range (spell damage)
        let sdam_str = if min_sdam == 0 {
            "0".to_string()
        } else {
            format!("{}m{}", min_sdam, max_sdam)
        };
        write_u16_field(&mut w, &sdam_str)?;

        // long damage range
        let ldam_str = if min_ldam == 0 {
            if min_sdam == 0 {
                "0".to_string()
            } else {
                format!("{}m{}", min_sdam, max_sdam)
            }
        } else {
            format!("{}m{}", min_ldam, max_ldam)
        };
        write_u16_field(&mut w, &ldam_str)?;
    }

    w.flush()?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = ServerConfig::from_file("conf/server.yaml")
        .context("[metan] [config_error] conf/server.yaml")?;

    // Connect to database
    let db_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL environment variable not set")?;
    let pool = MySqlPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .with_context(|| format!("Cannot connect to MySQL: {}", db_url))?;

    // Register pool with the Rust DB module layer.
    // Use set_pool() (async-safe) instead of connect() which would panic inside tokio.
    yuri::database::set_pool(pool.clone())
        .context("Failed to register DB pool")?;

    // Query Items table for charic (equippable, types 3–16)
    let charicinfo: Vec<u32> = sqlx::query(
        "SELECT `ItmId` FROM `Items` WHERE `ItmType` > 2 AND `ItmType` < 17 ORDER BY `ItmId`",
    )
    .fetch_all(&pool)
    .await
    .context("Failed to query charicinfo items")?
    .iter()
    .map(|row| row.try_get::<u32, _>(0).unwrap_or(0))
    .collect();

    // Query Items table for iteminfo (non-equippable, types outside 3–16)
    let iteminfo: Vec<u32> = sqlx::query(
        "SELECT `ItmId` FROM `Items` WHERE `ItmType` < 3 OR `ItmType` > 16 ORDER BY `ItmId`",
    )
    .fetch_all(&pool)
    .await
    .context("Failed to query iteminfo items")?
    .iter()
    .map(|row| row.try_get::<u32, _>(0).unwrap_or(0))
    .collect();

    // item_db::init() and class_db::init() call blocking_run() internally,
    // which panics when called from inside a tokio runtime context.
    // Offload to a blocking thread via spawn_blocking.
    tokio::task::spawn_blocking(move || {
        item_db::init();
        class_db::init();
    })
    .await
    .context("DB init thread panicked")?;

    let meta_dir = config.meta_dir.clone();

    // Write CharicInfo files
    let ci_max = charicinfo.len();
    let filecount = ci_max / 1000 + 1;
    for x in 0..filecount {
        let filename = format!("{}CharicInfo{}", meta_dir, x);
        println!("\t{}", filename);
        output_meta(&filename, x, &charicinfo)?;
    }

    // Write ItemInfo files
    let ii_max = iteminfo.len();
    if ii_max > 0 {
        let filecount = ii_max / 1000 + 1;
        for x in 0..filecount {
            let filename = format!("{}ItemInfo{}", meta_dir, x);
            println!("\t{}", filename);
            output_meta(&filename, x, &iteminfo)?;
        }
    }

    Ok(())
}
