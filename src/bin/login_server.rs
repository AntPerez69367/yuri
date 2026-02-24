use std::sync::Arc;
use anyhow::{Context, Result};
use sqlx::mysql::MySqlPoolOptions;
use yuri::config::ServerConfig;
use yuri::servers::login::{LoginState, parse_lang_file};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let mut conf_file = "conf/server.yaml".to_string();
    let mut lang_file = "conf/lang.yaml".to_string();

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "--h" | "--?" | "/?" => {
                println!("Usage: login_server [--conf FILE] [--lang FILE]");
                return Ok(());
            }
            "--conf" => {
                if i + 1 < args.len() {
                    i += 1;
                    conf_file = args[i].clone();
                } else {
                    eprintln!("Error: --conf requires a FILE argument");
                    return Ok(());
                }
            }
            "--lang" => {
                if i + 1 < args.len() {
                    i += 1;
                    lang_file = args[i].clone();
                } else {
                    eprintln!("Error: --lang requires a FILE argument");
                    return Ok(());
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

    let lang_content = std::fs::read_to_string(&lang_file).unwrap_or_default();
    let messages = parse_lang_file(&lang_content)?;

    let db_url = format!(
        "mysql://{}:{}@{}:{}/{}",
        config.sql_id, config.sql_pw, config.sql_ip, config.sql_port, config.sql_db
    );
    let pool = MySqlPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .with_context(|| format!("Cannot connect to DB: {}", config.sql_ip))?;

    tracing::info!("[login] [started] Login Server Started");

    let bind = format!("{}:{}", config.login_ip, config.login_port);
    let state = Arc::new(LoginState::new(pool, config, messages));

    LoginState::run(state, &bind).await?;
    Ok(())
}
