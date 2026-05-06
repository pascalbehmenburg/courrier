//! Courrier HTTP server.
//!
//! Wraps `courrier-core` with an axum REST API and serves the embedded
//! React SPA built by `desktop/`. The Tauri desktop app talks to the
//! exact same endpoints.

mod app_state;
mod routes;
mod static_assets;

use anyhow::Result;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::EnvFilter;

use courrier_core::{
    sync::SyncCoordinator, Database, Encryptor, Settings,
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let settings = Settings::from_env()?;
    info!("Storage: {}", settings.storage_path.display());
    info!("Database: {}", settings.db_path.display());

    std::fs::create_dir_all(&settings.storage_path)?;

    let db = Database::new(&settings.db_path)?;
    let encryptor = Encryptor::new(&settings.encryption_key);
    let coordinator = Arc::new(SyncCoordinator::new(
        db.clone(),
        encryptor.clone(),
        settings.storage_path.clone(),
    ));

    // Backfill messages for any existing .eml files left over from older
    // versions or DB restores.
    let backfilled = coordinator.backfill_parser(1000).await?;
    if backfilled > 0 {
        info!("Backfilled {} parsed message(s) at startup", backfilled);
    }

    coordinator.spawn_scheduler();
    if settings.fetch_on_startup {
        let started = coordinator.trigger_all().await?;
        if !started.is_empty() {
            info!("Initial sync started for {} account(s)", started.len());
        }
    }

    let state = app_state::AppState {
        db,
        encryptor,
        coordinator,
    };

    let app = routes::router(state);
    let listener = tokio::net::TcpListener::bind(&settings.bind_addr).await?;
    info!("Courrier server listening on http://{}", settings.bind_addr);
    axum::serve(listener, app).await?;
    Ok(())
}
