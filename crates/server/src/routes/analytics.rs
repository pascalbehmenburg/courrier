use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use courrier_core::analytics::{CountedString, DateBucket, ForwardingBreakdown, OverviewStats};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::routes::server_error;

#[derive(Debug, Deserialize)]
pub struct ScopeQuery {
    pub account_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ScopeLimitQuery {
    pub account_id: Option<i64>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

#[derive(Debug, Deserialize)]
pub struct TimelineQuery {
    pub account_id: Option<i64>,
    #[serde(default = "default_days")]
    pub days: i64,
}

fn default_limit() -> i64 {
    20
}

fn default_days() -> i64 {
    30
}

pub async fn overview(
    State(state): State<AppState>,
    Query(q): Query<ScopeQuery>,
) -> Result<Json<OverviewStats>, StatusCode> {
    state
        .db
        .overview_stats(q.account_id)
        .await
        .map(Json)
        .map_err(server_error)
}

pub async fn top_senders(
    State(state): State<AppState>,
    Query(q): Query<ScopeLimitQuery>,
) -> Result<Json<Vec<CountedString>>, StatusCode> {
    state
        .db
        .top_senders(q.account_id, q.limit.clamp(1, 200))
        .await
        .map(Json)
        .map_err(server_error)
}

pub async fn top_sender_domains(
    State(state): State<AppState>,
    Query(q): Query<ScopeLimitQuery>,
) -> Result<Json<Vec<CountedString>>, StatusCode> {
    state
        .db
        .top_sender_domains(q.account_id, q.limit.clamp(1, 200))
        .await
        .map(Json)
        .map_err(server_error)
}

pub async fn forwarding(
    State(state): State<AppState>,
    Query(q): Query<ScopeLimitQuery>,
) -> Result<Json<ForwardingBreakdown>, StatusCode> {
    state
        .db
        .forwarding_breakdown(q.account_id, q.limit.clamp(1, 200))
        .await
        .map(Json)
        .map_err(server_error)
}

pub async fn timeline(
    State(state): State<AppState>,
    Query(q): Query<TimelineQuery>,
) -> Result<Json<Vec<DateBucket>>, StatusCode> {
    state
        .db
        .messages_per_day(q.account_id, q.days.clamp(1, 365))
        .await
        .map(Json)
        .map_err(server_error)
}

pub async fn mailboxes(
    State(state): State<AppState>,
    Query(q): Query<ScopeQuery>,
) -> Result<Json<Vec<CountedString>>, StatusCode> {
    state
        .db
        .mailbox_distribution(q.account_id)
        .await
        .map(Json)
        .map_err(server_error)
}
