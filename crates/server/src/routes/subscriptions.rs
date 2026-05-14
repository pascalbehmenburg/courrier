use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{Duration, Utc};
use courrier_core::database::{Sender, SenderKind};
use courrier_core::subscriptions::{bulk_one_click, UnsubscribeOutcome};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::routes::server_error;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// `one_click` | `manual` | `other` | `unsubscribed` | `all`
    #[serde(default = "default_kind")]
    pub kind: String,
    /// Only senders seen in the past N days. Default 180 (≈ 6 months).
    #[serde(default = "default_window_days")]
    pub window_days: i64,
}

fn default_kind() -> String {
    "one_click".to_string()
}

fn default_window_days() -> i64 {
    180
}

#[derive(Debug, Deserialize)]
pub struct BulkUnsubscribeReq {
    pub ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
pub struct MarkUnsubReq {
    /// `manual_link` | `mailto` | `skip` (and we accept any short string).
    pub method: String,
}

fn parse_kind(s: &str) -> SenderKind {
    match s {
        "one_click" => SenderKind::OneClickSubscribed,
        "manual" => SenderKind::ManualSubscribed,
        "other" => SenderKind::OtherSubscribed,
        "unsubscribed" => SenderKind::Unsubscribed,
        _ => SenderKind::All,
    }
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<Sender>>, StatusCode> {
    let kind = parse_kind(&q.kind);
    let since = if q.window_days > 0 {
        Some(Utc::now() - Duration::days(q.window_days))
    } else {
        None
    };
    state
        .db
        .list_senders(kind, since)
        .await
        .map(Json)
        .map_err(server_error)
}

pub async fn bulk_unsubscribe(
    State(state): State<AppState>,
    Json(req): Json<BulkUnsubscribeReq>,
) -> Result<Json<Vec<UnsubscribeOutcome>>, StatusCode> {
    if req.ids.is_empty() {
        return Ok(Json(vec![]));
    }
    bulk_one_click(&state.db, &req.ids, 8)
        .await
        .map(Json)
        .map_err(server_error)
}

pub async fn mark_unsubscribed(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<MarkUnsubReq>,
) -> Result<StatusCode, StatusCode> {
    state
        .db
        .mark_unsubscribed(id, &req.method, None)
        .await
        .map_err(server_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn resubscribe(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    state.db.resubscribe(id).await.map_err(server_error)?;
    Ok(StatusCode::NO_CONTENT)
}
