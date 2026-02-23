use std::sync::Arc;
use anyhow::{Context, Result};
use sqlx::mysql::MySqlPoolOptions;
use yuri::config::ServerConfig;
use yuri::servers::char::CharState;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .init();

    let mut conf_file = "conf/server.yaml".to_string();

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "--h" | "--?" | "/?" => {
                println!("Usage: char_server [--conf FILE]");
                return Ok(());
            }
            "--conf" => {
                if i + 1 < args.len() {
                    i += 1;
                    conf_file = args[i].clone();
                } else {
                    return Err(anyhow::anyhow!("--conf requires a FILE argument"));
                }
            }
            _ => {}
        }
        i += 1;
    }

    let config: ServerConfig = {
        let content = std::fs::read_to_string(&conf_file)
            .with_context(|| format!("Cannot read config: {}", conf_file))?;
        ServerConfig::from_str(&content)
            .with_context(|| format!("Cannot parse config: {}", conf_file))?
    };

    let pool = {
        let db_url = format!(
            "mysql://{}:{}@{}:{}/{}",
            config.sql_id, config.sql_pw, config.sql_ip, config.sql_port, config.sql_db
        );
        MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await
            .with_context(|| format!(
                "Cannot connect to MySQL (host={}:{} db={} user={})",
                config.sql_ip, config.sql_port, config.sql_db, config.sql_id
            ))?
    };

    tracing::info!("[char] [started] Char Server Started.");

    let bind_addr = format!("{}:{}", config.char_ip, config.char_port);
    let state = Arc::new(CharState::new(pool, config));

    // Spawn login server reconnect loop
    {
        let s = Arc::clone(&state);
        tokio::spawn(async move {
            yuri::servers::char::login::connect_to_login(s).await;
        });
    }

    CharState::run(state, &bind_addr).await
}
