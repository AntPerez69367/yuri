/// metan_cli — generates binary .metan metadata files from the item database
///
/// Rust port of c_src/metan_cli.c.
/// Connects to MySQL via sqlx (same pool used by item_db/class_db),
/// loads itemdb + classdb, queries the Items table, and writes .metan files.

use anyhow::{Context, Result};
use sqlx::mysql::MySqlPoolOptions;
use sqlx::Row;
use std::ffi::{CStr, CString};
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

    for x in base..base + offset {
        let id = list[x];

        // name (u8-length-prefixed)
        let name_ptr = unsafe { item_db::search(id) };
        let name_bytes = if !name_ptr.is_null() {
            let cstr = unsafe { CStr::from_ptr((*name_ptr).name.as_ptr()) };
            cstr.to_bytes().to_vec()
        } else {
            b"??".to_vec()
        };
        write_u8len_field(&mut w, &name_bytes)?;

        // write 20 (swap16'd)
        let twenty: u16 = swap16(20);
        w.write_all(&twenty.to_ne_bytes())?;

        // All integer fields
        let mightreq  = if !name_ptr.is_null() { unsafe { (*name_ptr).mightreq  } } else { 0 };
        let price     = if !name_ptr.is_null() { unsafe { (*name_ptr).price     } } else { 0 };
        let dura      = if !name_ptr.is_null() { unsafe { (*name_ptr).dura      } } else { 0 };
        let ac        = if !name_ptr.is_null() { unsafe { (*name_ptr).ac        } } else { 0 };
        let hit       = if !name_ptr.is_null() { unsafe { (*name_ptr).hit       } } else { 0 };
        let dam       = if !name_ptr.is_null() { unsafe { (*name_ptr).dam       } } else { 0 };
        let vita      = if !name_ptr.is_null() { unsafe { (*name_ptr).vita      } } else { 0 };
        let mana      = if !name_ptr.is_null() { unsafe { (*name_ptr).mana      } } else { 0 };
        let might     = if !name_ptr.is_null() { unsafe { (*name_ptr).might     } } else { 0 };
        let grace     = if !name_ptr.is_null() { unsafe { (*name_ptr).grace     } } else { 0 };
        let will      = if !name_ptr.is_null() { unsafe { (*name_ptr).will      } } else { 0 };
        let wisdom    = if !name_ptr.is_null() { unsafe { (*name_ptr).wisdom    } } else { 0 };
        let con       = if !name_ptr.is_null() { unsafe { (*name_ptr).con       } } else { 0 };
        let class_id  = if !name_ptr.is_null() { unsafe { (*name_ptr).class as i32 } } else { 0 };
        let rank      = if !name_ptr.is_null() { unsafe { (*name_ptr).rank      } } else { 0 };
        let level     = if !name_ptr.is_null() { unsafe { (*name_ptr).level as i32 } } else { 0 };
        let healing   = if !name_ptr.is_null() { unsafe { (*name_ptr).healing   } } else { 0 };
        let protection = if !name_ptr.is_null() { unsafe { (*name_ptr).protection } } else { 0 };
        let min_sdam  = if !name_ptr.is_null() { unsafe { (*name_ptr).min_sdam as i32 } } else { 0 };
        let max_sdam  = if !name_ptr.is_null() { unsafe { (*name_ptr).max_sdam as i32 } } else { 0 };
        let min_ldam  = if !name_ptr.is_null() { unsafe { (*name_ptr).min_ldam as i32 } } else { 0 };
        let max_ldam  = if !name_ptr.is_null() { unsafe { (*name_ptr).max_ldam as i32 } } else { 0 };

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
    let db_url = format!(
        "mysql://{}:{}@{}:{}/{}",
        config.sql_id, config.sql_pw, config.sql_ip, config.sql_port, config.sql_db
    );
    let pool = MySqlPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .with_context(|| format!(
            "Cannot connect to MySQL (host={}:{} db={} user={})",
            config.sql_ip, config.sql_port, config.sql_db, config.sql_id
        ))?;

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
    let data_dir = config.data_dir.clone();
    tokio::task::spawn_blocking(move || {
        item_db::init();
        let data_dir_c = CString::new(data_dir.as_str())
            .expect("data_dir contains nul byte");
        unsafe { class_db::init(data_dir_c.as_ptr()); }
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
