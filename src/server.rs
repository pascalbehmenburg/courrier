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

fn group_accounts_by_server(accounts: &[AccountConfig]) -> Vec<ServerInfo> {
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

async fn dashboard_handler() -> Html<&'static str> {
    Html(include_str!("../assets/dashboard.html"))
}

async fn accounts_handler(State(state): State<AppState>) -> Json<Vec<ServerInfo>> {
    Json(group_accounts_by_server(&state.config))
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
        accounts: group_accounts_by_server(&state.config),
        total_emails,
        total_storage_bytes,
        per_account_stats,
    }))
}

async fn fetch_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Check if a fetch is already running
    let mut task_handle = state.fetch_task.lock().await;
    if task_handle.is_some() {
        return Ok(Json(serde_json::json!({
            "status": "already_running",
            "message": "A fetch operation is already in progress"
        })));
    }

    let accounts = state.config.clone();
    let output_dir = state.output_dir.clone();
    let db = Arc::clone(&state.db);

    // Spawn fetch task - fetch all mailboxes automatically
    let handle = tokio::spawn(async move { fetch_all_accounts(&accounts, &output_dir, &db).await });

    *task_handle = Some(handle);

    Ok(Json(serde_json::json!({
        "status": "started",
        "message": "Fetch operation started (all mailboxes will be fetched)"
    })))
}

async fn fetch_status_handler(
    State(state): State<AppState>,
) -> Result<Json<FetchStatusResponse>, StatusCode> {
    let mut task_handle = state.fetch_task.lock().await;

    let is_running = match task_handle.as_ref() {
        Some(handle) if !handle.is_finished() => true,
        Some(_) => {
            let _ = task_handle.take();
            false
        }
        None => false,
    };

    let db_status = state
        .db
        .get_latest_fetch_status()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let completed_at = if !is_running {
        state
            .db
            .get_latest_completed_at()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        None
    };

    match db_status {
        Some(status) => Ok(Json(FetchStatusResponse {
            is_running,
            started_at: status.started_at.map(|dt| dt.to_rfc3339()),
            completed_at,
            messages_fetched: status.messages_fetched,
        })),
        None => Ok(Json(FetchStatusResponse {
            is_running: false,
            started_at: None,
            completed_at: None,
            messages_fetched: 0,
        })),
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

async fn trigger_fetch(state: &AppState) {
    let mut task_handle = state.fetch_task.lock().await;
    if task_handle.is_some() {
        return; // Already running
    }

    let accounts = state.config.clone();
    let output_dir = state.output_dir.clone();
    let db = Arc::clone(&state.db);

    // Spawn fetch task - fetch all mailboxes automatically
    let handle = tokio::spawn(async move { fetch_all_accounts(&accounts, &output_dir, &db).await });

    *task_handle = Some(handle);
}

pub async fn start_server(state: AppState, port: u16, fetch_on_startup: bool) -> Result<()> {
    // Trigger fetch on startup if configured
    if fetch_on_startup {
        println!("Starting initial fetch on startup...");
        trigger_fetch(&state).await;
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
                trigger_fetch(&state_clone).await;
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn create_test_db() -> Database {
        Database::new(":memory:").unwrap()
    }

    fn create_test_state() -> AppState {
        let db = create_test_db();
        let accounts = vec![
            AccountConfig {
                email: "user1@gmail.com".to_string(),
                username: "user1".to_string(),
                password: "pass1".to_string(),
                server: "imap.gmail.com".to_string(),
                port: 993,
            },
            AccountConfig {
                email: "user2@gmail.com".to_string(),
                username: "user2".to_string(),
                password: "pass2".to_string(),
                server: "imap.gmail.com".to_string(),
                port: 993,
            },
            AccountConfig {
                email: "user3@outlook.com".to_string(),
                username: "user3".to_string(),
                password: "pass3".to_string(),
                server: "imap.outlook.com".to_string(),
                port: 993,
            },
        ];

        AppState {
            db: Arc::new(db),
            config: Arc::new(accounts),
            output_dir: Arc::new(PathBuf::from("/tmp/test_emails")),
            fetch_task: Arc::new(Mutex::new(None)),
            fetch_interval_seconds: None,
        }
    }

    #[test]
    fn test_group_accounts_by_server_empty() {
        let accounts: Vec<AccountConfig> = vec![];
        let servers = group_accounts_by_server(&accounts);
        assert!(servers.is_empty());
    }

    #[test]
    fn test_group_accounts_by_server_single() {
        let accounts = vec![AccountConfig {
            email: "user@example.com".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            server: "imap.example.com".to_string(),
            port: 993,
        }];

        let servers = group_accounts_by_server(&accounts);

        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].host, "imap.example.com");
        assert_eq!(servers[0].port, 993);
        assert_eq!(servers[0].accounts.len(), 1);
        assert_eq!(servers[0].accounts[0].email, "user@example.com");
    }

    #[test]
    fn test_group_accounts_by_server_multiple_same_server() {
        let accounts = vec![
            AccountConfig {
                email: "user1@gmail.com".to_string(),
                username: "user1".to_string(),
                password: "pass1".to_string(),
                server: "imap.gmail.com".to_string(),
                port: 993,
            },
            AccountConfig {
                email: "user2@gmail.com".to_string(),
                username: "user2".to_string(),
                password: "pass2".to_string(),
                server: "imap.gmail.com".to_string(),
                port: 993,
            },
        ];

        let servers = group_accounts_by_server(&accounts);

        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].accounts.len(), 2);
    }

    #[test]
    fn test_group_accounts_by_server_multiple_different_servers() {
        let accounts = vec![
            AccountConfig {
                email: "user1@gmail.com".to_string(),
                username: "user1".to_string(),
                password: "pass1".to_string(),
                server: "imap.gmail.com".to_string(),
                port: 993,
            },
            AccountConfig {
                email: "user2@outlook.com".to_string(),
                username: "user2".to_string(),
                password: "pass2".to_string(),
                server: "imap.outlook.com".to_string(),
                port: 993,
            },
        ];

        let servers = group_accounts_by_server(&accounts);

        assert_eq!(servers.len(), 2);
    }

    #[test]
    fn test_group_accounts_by_server_different_ports() {
        let accounts = vec![
            AccountConfig {
                email: "user1@example.com".to_string(),
                username: "user1".to_string(),
                password: "pass1".to_string(),
                server: "imap.example.com".to_string(),
                port: 993,
            },
            AccountConfig {
                email: "user2@example.com".to_string(),
                username: "user2".to_string(),
                password: "pass2".to_string(),
                server: "imap.example.com".to_string(),
                port: 143,
            },
        ];

        let servers = group_accounts_by_server(&accounts);

        assert_eq!(servers.len(), 2);
    }

    #[tokio::test]
    async fn test_dashboard_handler() {
        let state = create_test_state();
        let app = create_router(state);

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(body_str.contains("<!DOCTYPE html>") || body_str.contains("<html"));
    }

    #[tokio::test]
    async fn test_accounts_handler() {
        let state = create_test_state();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/accounts")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();

        assert_eq!(json.len(), 2);
    }

    #[tokio::test]
    async fn test_stats_handler() {
        let state = create_test_state();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["total_emails"], 0);
        assert_eq!(json["total_storage_bytes"], 0);
        assert!(json["accounts"].is_array());
        assert!(json["per_account_stats"].is_array());
    }

    #[tokio::test]
    async fn test_fetch_status_handler_no_fetch() {
        let state = create_test_state();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/fetch/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["is_running"], false);
        assert_eq!(json["messages_fetched"], 0);
    }

    #[tokio::test]
    async fn test_router_routes_exist() {
        let state = create_test_state();
        let app = create_router(state);

        let routes = [
            ("/", "GET"),
            ("/api/accounts", "GET"),
            ("/api/stats", "GET"),
            ("/api/fetch/status", "GET"),
        ];

        for (uri, _method) in routes {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();

            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "Route {} should exist",
                uri
            );
        }
    }

    #[tokio::test]
    async fn test_nonexistent_route() {
        let state = create_test_state();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
