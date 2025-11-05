mod config;
mod database;
mod fetcher;
mod server;

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str());

    // Initialize database
    let db = database::Database::new("mailster.db")?;

    // Load configuration
    let app_config = config::load_config()?;
    let accounts = config::extract_accounts(&app_config);
    println!("Loaded {} account(s) from config.toml", accounts.len());

    // Create output directory from config
    let output_dir = PathBuf::from(&app_config.email_storage_path);
    std::fs::create_dir_all(&output_dir)?;
    println!("Output directory: {}", output_dir.display());

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
                .unwrap_or(3000);

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
            eprintln!("Unknown command: {}", cmd);
            eprintln!("Usage: mailster [fetch|server] [port]");
            eprintln!("  fetch  - Run one-time fetch and exit");
            eprintln!("  server - Start web dashboard (default)");
            eprintln!("  port   - Port number for server (default: 3000)");
            std::process::exit(1);
        }
    }

    Ok(())
}

async fn run_fetch(
    accounts: &[config::AccountConfig],
    output_dir: &PathBuf,
    db: &database::Database,
) -> Result<()> {
    let mailboxes_to_fetch = vec!["INBOX", "Junk"];

    println!("\n{}", "=".repeat(80));
    println!("Starting fetch operation");
    println!("{}", "=".repeat(80));

    let total_saved = fetcher::fetch_all_accounts(accounts, &mailboxes_to_fetch, output_dir, db)
        .await?;

    println!("\n{}", "=".repeat(80));
    println!("✓ Done! Total messages saved: {}", total_saved);
    println!("Messages saved to: {}", output_dir.display());
    println!("{}", "=".repeat(80));

    Ok(())
}
