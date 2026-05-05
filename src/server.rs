use crate::config::AccountConfig;
use crate::database::Database;
use crate::fetcher::fetch_all_accounts;
use anyhow::Result;
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{Html, Json, Response},
    routing::{get, post},
    Router,
};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub config: Arc<Vec<AccountConfig>>,
    pub output_dir: Arc<PathBuf>,
    pub fetch_task: Arc<Mutex<Option<tokio::task::JoinHandle<Result<usize>>>>>,
    pub fetch_interval_seconds: Option<u64>,
}

#[derive(Serialize)]
struct AccountInfo {
    email: String,
    server: String,
    port: u16,
}

#[derive(Serialize)]
struct ServerInfo {
    host: String,
    port: u16,
    accounts: Vec<AccountInfo>,
}

#[derive(Serialize)]
struct StatsResponse {
    accounts: Vec<ServerInfo>,
    total_emails: i64,
    total_storage_bytes: i64,
    per_account_stats: Vec<AccountStats>,
}

#[derive(Serialize)]
struct AccountStats {
    account_email: String,
    mailbox: String,
    email_count: i64,
    storage_bytes: i64,
    last_fetch: Option<String>,
}

#[derive(Serialize)]
struct FetchStatusResponse {
    is_running: bool,
    started_at: Option<String>,
    completed_at: Option<String>,
    messages_fetched: i64,
}

async fn dashboard_handler() -> Html<&'static str> {
    Html(include_str!("../assets/dashboard.html"))
}

fn group_servers(accounts: &[AccountConfig]) -> Vec<ServerInfo> {
    use std::collections::HashMap;
    let mut servers: HashMap<String, ServerInfo> = HashMap::new();

    for account in accounts {
        let server_key = format!("{}:{}", account.server, account.port);
        let server_info = servers.entry(server_key).or_insert_with(|| ServerInfo {
            host: account.server.clone(),
            port: account.port,
            accounts: Vec::new(),
        });

        server_info.accounts.push(AccountInfo {
            email: account.email.clone(),
            server: account.server.clone(),
            port: account.port,
        });
    }

    servers.into_values().collect()
}

async fn stats_handler(State(state): State<AppState>) -> Result<Json<StatsResponse>, StatusCode> {
    let stats = state
        .db
        .get_stats()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (total_emails, total_storage_bytes) = state
        .db
        .get_total_stats()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let per_account_stats: Vec<AccountStats> = stats
        .into_iter()
        .map(|s| AccountStats {
            account_email: s.account_email,
            mailbox: s.mailbox,
            email_count: s.count,
            storage_bytes: s.total_size_bytes,
            last_fetch: s.last_fetch.map(|dt| dt.to_rfc3339()),
        })
        .collect();

    Ok(Json(StatsResponse {
        accounts: group_servers(&state.config),
        total_emails,
        total_storage_bytes,
        per_account_stats,
    }))
}

async fn fetch_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if trigger_fetch(&state).await {
        Ok(Json(serde_json::json!({
            "status": "started",
            "message": "Fetch operation started (all mailboxes will be fetched)"
        })))
    } else {
        Ok(Json(serde_json::json!({
            "status": "already_running",
            "message": "A fetch operation is already in progress"
        })))
    }
}

async fn fetch_status_handler(
    State(state): State<AppState>,
) -> Result<Json<FetchStatusResponse>, StatusCode> {
    // Check if task is still running
    let mut task_handle = state.fetch_task.lock().await;

    if let Some(ref handle) = *task_handle {
        if handle.is_finished() {
            // Task completed, clean up
            let _ = task_handle.take();
            let db_status = state
                .db
                .get_latest_fetch_status()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            if let Some(status) = db_status {
                // Get completed_at from database - we need to query it directly
                let conn = state.db.conn.lock();
                let completed_at: Option<String> = conn
                    .query_row(
                        "SELECT completed_at FROM fetch_history ORDER BY started_at DESC LIMIT 1",
                        [],
                        |row| row.get::<_, Option<String>>(0),
                    )
                    .ok()
                    .flatten();
                drop(conn);

                return Ok(Json(FetchStatusResponse {
                    is_running: false,
                    started_at: status.started_at.map(|dt| dt.to_rfc3339()),
                    completed_at,
                    messages_fetched: status.messages_fetched,
                }));
            }

            return Ok(Json(FetchStatusResponse {
                is_running: false,
                started_at: None,
                completed_at: None,
                messages_fetched: 0,
            }));
        } else {
            // Task still running
            let db_status = state
                .db
                .get_latest_fetch_status()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            if let Some(status) = db_status {
                return Ok(Json(FetchStatusResponse {
                    is_running: true,
                    started_at: status.started_at.map(|dt| dt.to_rfc3339()),
                    completed_at: None,
                    messages_fetched: status.messages_fetched,
                }));
            }
        }
    }

    // No active task
    let db_status = state
        .db
        .get_latest_fetch_status()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(status) = db_status {
        // Get completed_at from database
        let conn = state.db.conn.lock();
        let completed_at: Option<String> = conn
            .query_row(
                "SELECT completed_at FROM fetch_history ORDER BY started_at DESC LIMIT 1",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten();
        drop(conn);

        Ok(Json(FetchStatusResponse {
            is_running: false,
            started_at: status.started_at.map(|dt| dt.to_rfc3339()),
            completed_at,
            messages_fetched: status.messages_fetched,
        }))
    } else {
        Ok(Json(FetchStatusResponse {
            is_running: false,
            started_at: None,
            completed_at: None,
            messages_fetched: 0,
        }))
    }
}

/// CSRF guard for state-changing requests. Requires the `X-Requested-With`
/// header on non-GET/HEAD requests. Browsers will not send this header on
/// simple cross-origin form submissions, and adding it from JS triggers a
/// CORS preflight that the server does not answer — so a malicious page the
/// user visits cannot trigger a fetch via their dashboard origin.
async fn require_xhr_header(req: Request, next: Next) -> Result<Response, StatusCode> {
    let method = req.method();
    if method.is_safe() || req.headers().contains_key("x-requested-with") {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(dashboard_handler))
        .route("/api/stats", get(stats_handler))
        .route("/api/fetch", post(fetch_handler))
        .route("/api/fetch/status", get(fetch_status_handler))
        .layer(middleware::from_fn(require_xhr_header))
        .with_state(state)
}

/// Spawn a fetch task if one isn't already running. Returns `true` if a new
/// task was spawned, `false` if a fetch was already in flight.
async fn trigger_fetch(state: &AppState) -> bool {
    let mut task_handle = state.fetch_task.lock().await;
    if task_handle.is_some() {
        return false;
    }

    let accounts = state.config.clone();
    let output_dir = state.output_dir.clone();
    let db = Arc::clone(&state.db);

    let handle = tokio::spawn(async move { fetch_all_accounts(&accounts, &output_dir, &db).await });
    *task_handle = Some(handle);
    true
}

pub async fn start_server(state: AppState, port: u16, fetch_on_startup: bool) -> Result<()> {
    // Trigger fetch on startup if configured
    if fetch_on_startup {
        println!("Starting initial fetch on startup...");
        let _ = trigger_fetch(&state).await;
    }

    // Start periodic fetch task if interval is configured
    if let Some(interval_seconds) = state.fetch_interval_seconds {
        let state_clone = state.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(interval_seconds));
            // Skip first tick to avoid immediate execution (already done on startup if enabled)
            interval.tick().await;

            loop {
                interval.tick().await;
                println!("Periodic fetch triggered (interval: {}s)", interval_seconds);
                let _ = trigger_fetch(&state_clone).await;
            }
        });
        println!("Periodic fetch enabled: every {} seconds", interval_seconds);
    }

    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    println!("🚀 Courrier dashboard running on http://0.0.0.0:{}", port);
    axum::serve(listener, app).await?;
    Ok(())
}
