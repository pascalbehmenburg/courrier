use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use courrier_core::database::{Message, MessageSummary};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::routes::server_error;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub account_id: Option<i64>,
    pub mailbox: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<MessageSummary>>, StatusCode> {
    state
        .db
        .list_messages(q.account_id, q.mailbox, q.limit.clamp(1, 500), q.offset.max(0))
        .await
        .map(Json)
        .map_err(server_error)
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Message>, StatusCode> {
    match state.db.get_message(id).await.map_err(server_error)? {
        Some(m) => Ok(Json(m)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// Stream the raw .eml bytes back. Useful for "view source" in the UI.
pub async fn raw(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, StatusCode> {
    let message = state
        .db
        .get_message(id)
        .await
        .map_err(server_error)?
        .ok_or(StatusCode::NOT_FOUND)?;
    // Look up the file path via fetched_emails. We don't have a direct
    // helper — issue an ad-hoc query through the run() helper would be
    // cleaner; for now do a separate accessor.
    let path = state
        .db
        .raw_email_path(message.fetched_email_id)
        .await
        .map_err(server_error)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let bytes = tokio::fs::read(&path).await.map_err(server_error)?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "message/rfc822")
        .body(Body::from(bytes))
        .map_err(server_error)
}
