use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use courrier_core::database::FetchRun;
use serde::Serialize;
use serde_json::json;

use crate::app_state::AppState;
use crate::routes::server_error;

#[derive(Serialize)]
pub struct PerAccountStatus {
    pub account_id: i64,
    pub running: bool,
    pub latest_run: Option<FetchRun>,
}

pub async fn trigger_all(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let started = state.coordinator.trigger_all().await.map_err(server_error)?;
    Ok(Json(json!({"started": started})))
}

pub async fn trigger_one(
    State(state): State<AppState>,
    Path(account_id): Path<i64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.coordinator.trigger_one(account_id).await {
        Ok(true) => Ok(Json(json!({"started": true, "account_id": account_id}))),
        Ok(false) => Ok(Json(
            json!({"started": false, "reason": "already_running", "account_id": account_id}),
        )),
        Err(e) => {
            tracing::error!("trigger_one({account_id}) failed: {e:?}");
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

pub async fn status(
    State(state): State<AppState>,
) -> Result<Json<Vec<PerAccountStatus>>, StatusCode> {
    let accounts = state.db.list_accounts().await.map_err(server_error)?;
    let in_flight: std::collections::HashSet<i64> = state
        .coordinator
        .in_flight_account_ids()
        .into_iter()
        .collect();
    let latest_runs = state
        .db
        .latest_run_per_account()
        .await
        .map_err(server_error)?;
    let by_account: std::collections::HashMap<i64, FetchRun> =
        latest_runs.into_iter().filter_map(|r| r.account_id.map(|id| (id, r))).collect();

    let out: Vec<PerAccountStatus> = accounts
        .into_iter()
        .map(|a| PerAccountStatus {
            account_id: a.id,
            running: in_flight.contains(&a.id),
            latest_run: by_account.get(&a.id).cloned(),
        })
        .collect();
    Ok(Json(out))
}
