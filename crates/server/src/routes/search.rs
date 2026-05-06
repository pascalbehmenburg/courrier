use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use courrier_core::search::SearchHit;
use serde::Deserialize;

use crate::app_state::AppState;
use crate::routes::server_error;

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub account_id: Option<i64>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

pub async fn search(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Vec<SearchHit>>, StatusCode> {
    if q.q.trim().is_empty() {
        return Ok(Json(Vec::new()));
    }
    state
        .db
        .search_messages(q.q, q.account_id, q.limit.clamp(1, 200))
        .await
        .map(Json)
        .map_err(server_error)
}
