use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use courrier_core::database::{Account, AccountInput};
use courrier_core::fetcher;
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::routes::server_error;

/// Wire-format input for creating/updating an account. The plaintext
/// password is encrypted server-side before it ever hits the DB.
#[derive(Debug, Clone, Deserialize)]
pub struct AccountPayload {
    pub label: String,
    pub email: String,
    pub username: String,
    pub password: String,
    pub host: String,
    pub port: u16,
    pub provider_id: String,
    pub sync_interval_seconds: Option<u64>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct TestResult {
    pub ok: bool,
    pub message: String,
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<Account>>, StatusCode> {
    state
        .db
        .list_accounts()
        .await
        .map(Json)
        .map_err(server_error)
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Account>, StatusCode> {
    match state.db.get_account(id).await.map_err(server_error)? {
        Some(a) => Ok(Json(a)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn create(
    State(state): State<AppState>,
    Json(payload): Json<AccountPayload>,
) -> Result<Json<Account>, StatusCode> {
    let ciphertext = state
        .encryptor
        .encrypt(&payload.password)
        .map_err(server_error)?;
    let input = AccountInput {
        label: payload.label,
        email: payload.email,
        username: payload.username,
        password_ciphertext: ciphertext,
        host: payload.host,
        port: payload.port,
        provider_id: payload.provider_id,
        sync_interval_seconds: payload.sync_interval_seconds,
        enabled: payload.enabled,
    };
    state
        .db
        .insert_account(input)
        .await
        .map(Json)
        .map_err(server_error)
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<AccountPayload>,
) -> Result<Json<Account>, StatusCode> {
    // Empty password means "leave existing untouched". Anything else
    // re-encrypts.
    let ciphertext = if payload.password.is_empty() {
        let existing = state
            .db
            .get_account(id)
            .await
            .map_err(server_error)?
            .ok_or(StatusCode::NOT_FOUND)?;
        existing.password_ciphertext
    } else {
        state
            .encryptor
            .encrypt(&payload.password)
            .map_err(server_error)?
    };
    let input = AccountInput {
        label: payload.label,
        email: payload.email,
        username: payload.username,
        password_ciphertext: ciphertext,
        host: payload.host,
        port: payload.port,
        provider_id: payload.provider_id,
        sync_interval_seconds: payload.sync_interval_seconds,
        enabled: payload.enabled,
    };
    state
        .db
        .update_account(id, input)
        .await
        .map(Json)
        .map_err(server_error)
}

pub async fn delete_one(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    let deleted = state.db.delete_account(id).await.map_err(server_error)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// Smoke-test a connection without persisting changes. Tries to log in;
/// returns ok/error so the UI can confirm credentials before save.
pub async fn test(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<TestResult> {
    let Ok(Some(account)) = state.db.get_account(id).await else {
        return Json(TestResult {
            ok: false,
            message: "account not found".into(),
        });
    };
    let Ok(password) = state.encryptor.decrypt(&account.password_ciphertext) else {
        return Json(TestResult {
            ok: false,
            message: "password decryption failed (encryption key mismatch?)".into(),
        });
    };
    match fetcher::test_connection(account.host, account.port, account.username, password).await {
        Ok(()) => Json(TestResult {
            ok: true,
            message: "Connected and authenticated successfully".into(),
        }),
        Err(e) => Json(TestResult {
            ok: false,
            message: format!("{e}"),
        }),
    }
}
