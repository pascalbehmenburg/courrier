mod config;
mod database;
mod fetcher;
mod server;

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;
use tracing_subscriber::EnvFilter;

const DEFAULT_DASHBOARD_PORT: u16 = 3000;
const DEFAULT_DB_PATH: &str = "courrier.db";

#[tokio::main]
async fn main() -> Result<()> {
    // Default to INFO; override with RUST_LOG (e.g. RUST_LOG=courrier=debug).
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str());

    let db_path =
        std::env::var("COURRIER_DB_PATH").unwrap_or_else(|_| DEFAULT_DB_PATH.to_string());
    let db = database::Database::new(&db_path)?;

    // Load configuration
    let app_config = config::load_config()?;
    let accounts = config::extract_accounts(&app_config);
    info!("Loaded {} account(s) from Config.toml", accounts.len());

    // Create output directory from config
    let output_dir = PathBuf::from(&app_config.email_storage_path);
    std::fs::create_dir_all(&output_dir)?;
    info!("Output directory: {}", output_dir.display());

    match command {
        Some("fetch") => {
            // CLI mode: one-time fetch
            run_fetch(&accounts, &output_dir, &db).await?;
        }
        Some("server") | None => {
            // Server mode: start dashboard
            let port = args
                .get(2)
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_DASHBOARD_PORT);

            let state = server::AppState {
                db: Arc::new(db),
                config: Arc::new(accounts),
                output_dir: Arc::new(output_dir),
                fetch_task: Arc::new(Mutex::new(None)),
                fetch_interval_seconds: app_config.fetch_interval_seconds,
            };

            server::start_server(state, port, app_config.fetch_on_startup).await?;
        }
        Some(cmd) => {
            // Usage messages go to stderr directly: this runs before the user
            // can sensibly enable a log filter, and we want a non-zero exit.
            eprintln!("Unknown command: {}", cmd);
            eprintln!("Usage: courrier [fetch|server] [port]");
            eprintln!("  fetch  - Run one-time fetch and exit");
            eprintln!("  server - Start web dashboard (default)");
            eprintln!(
                "  port   - Port number for server (default: {})",
                DEFAULT_DASHBOARD_PORT
            );
            std::process::exit(1);
        }
    }

    Ok(())
}

async fn run_fetch(
    accounts: &[config::AccountConfig],
    output_dir: &Path,
    db: &database::Database,
) -> Result<()> {
    info!("Starting fetch operation");
    let total_saved = fetcher::fetch_all_accounts(accounts, output_dir, db).await?;
    info!(
        "Done. Saved {} total message(s) to {}",
        total_saved,
        output_dir.display()
    );
    Ok(())
}
