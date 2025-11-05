use crate::config::AccountConfig;
use crate::database::Database;
use crate::fetcher::fetch_all_accounts;
use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub config: Arc<Vec<AccountConfig>>,
    pub output_dir: Arc<PathBuf>,
    pub fetch_task: Arc<Mutex<Option<tokio::task::JoinHandle<Result<usize>>>>>,
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
    messages_fetched: i64,
}

async fn dashboard_handler() -> Html<&'static str> {
    Html(include_str!("../assets/dashboard.html"))
}

async fn accounts_handler(State(state): State<AppState>) -> Json<Vec<ServerInfo>> {
    // Group accounts by server
    use std::collections::HashMap;
    let mut servers: HashMap<String, ServerInfo> = HashMap::new();

    for account in state.config.iter() {
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

    Json(servers.into_values().collect())
}

async fn stats_handler(State(state): State<AppState>) -> Result<Json<StatsResponse>, StatusCode> {
    let stats = state.db.get_stats().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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

    // Group accounts by server
    use std::collections::HashMap;
    let mut servers: HashMap<String, ServerInfo> = HashMap::new();

    for account in state.config.iter() {
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

    Ok(Json(StatsResponse {
        accounts: servers.into_values().collect(),
        total_emails,
        total_storage_bytes,
        per_account_stats,
    }))
}

#[derive(Deserialize)]
struct FetchRequest {
    mailboxes: Option<Vec<String>>,
}

async fn fetch_handler(
    State(state): State<AppState>,
    Json(payload): Json<FetchRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Check if a fetch is already running
    let mut task_handle = state.fetch_task.lock().await;
    if task_handle.is_some() {
        return Ok(Json(serde_json::json!({
            "status": "already_running",
            "message": "A fetch operation is already in progress"
        })));
    }

    let mailboxes: Vec<String> = payload
        .mailboxes
        .unwrap_or_else(|| vec!["INBOX".to_string(), "Junk".to_string()]);

    let accounts = state.config.clone();
    let output_dir = state.output_dir.clone();
    let db = Arc::clone(&state.db);

    // Spawn fetch task - move mailboxes into the closure to keep it alive
    let mailboxes_for_task = mailboxes.clone();
    let handle = tokio::spawn(async move {
        let mailboxes_strs: Vec<&str> = mailboxes_for_task.iter().map(|s| s.as_str()).collect();
        fetch_all_accounts(&accounts, &mailboxes_strs, &output_dir, &db).await
    });

    *task_handle = Some(handle);

    Ok(Json(serde_json::json!({
        "status": "started",
        "message": "Fetch operation started"
    })))
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
                return Ok(Json(FetchStatusResponse {
                    is_running: false,
                    started_at: status.started_at.map(|dt| dt.to_rfc3339()),
                    messages_fetched: status.messages_fetched,
                }));
            }
            
            return Ok(Json(FetchStatusResponse {
                is_running: false,
                started_at: None,
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
        Ok(Json(FetchStatusResponse {
            is_running: false,
            started_at: status.started_at.map(|dt| dt.to_rfc3339()),
            messages_fetched: status.messages_fetched,
        }))
    } else {
        Ok(Json(FetchStatusResponse {
            is_running: false,
            started_at: None,
            messages_fetched: 0,
        }))
    }
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(dashboard_handler))
        .route("/api/accounts", get(accounts_handler))
        .route("/api/stats", get(stats_handler))
        .route("/api/fetch", post(fetch_handler))
        .route("/api/fetch/status", get(fetch_status_handler))
        .with_state(state)
}

pub async fn start_server(state: AppState, port: u16) -> Result<()> {
    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    println!("🚀 Mailster dashboard running on http://0.0.0.0:{}", port);
    axum::serve(listener, app).await?;
    Ok(())
}

