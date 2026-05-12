mod accounts;
mod analytics;
mod messages;
mod providers;
mod search;
mod sync;

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::app_state::AppState;
use crate::static_assets;

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/api/health", get(health))
        .route("/api/providers", get(providers::list_providers))
        .route("/api/accounts", get(accounts::list).post(accounts::create))
        .route(
            "/api/accounts/:id",
            get(accounts::get)
                .put(accounts::update)
                .delete(accounts::delete_one),
        )
        .route("/api/accounts/:id/test", post(accounts::test))
        .route("/api/sync", post(sync::trigger_all))
        .route("/api/sync/status", get(sync::status))
        .route("/api/sync/:account_id", post(sync::trigger_one))
        .route("/api/messages", get(messages::list))
        .route("/api/messages/:id", get(messages::get_one))
        .route("/api/messages/:id/raw", get(messages::raw))
        .route("/api/search", get(search::search))
        .route("/api/analytics/overview", get(analytics::overview))
        .route("/api/analytics/top-senders", get(analytics::top_senders))
        .route(
            "/api/analytics/top-sender-domains",
            get(analytics::top_sender_domains),
        )
        .route("/api/analytics/forwarding", get(analytics::forwarding))
        .route("/api/analytics/timeline", get(analytics::timeline))
        .route("/api/analytics/mailboxes", get(analytics::mailboxes))
        .layer(middleware::from_fn(require_xhr_for_writes));

    Router::new()
        .merge(api)
        .fallback(static_assets::serve_spa)
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({"status": "ok"}))
}

/// CSRF guard for state-changing requests. Browsers won't send
/// `X-Requested-With` on simple cross-origin form posts and adding it from
/// JS triggers a CORS preflight that the server needs to allow explicitly.
async fn require_xhr_for_writes(req: Request, next: Next) -> Result<Response, StatusCode> {
    let method = req.method();
    if method.is_safe() || req.headers().contains_key("x-requested-with") {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// Bridge between anyhow errors thrown by the core crate and HTTP
/// responses. We don't expose the message body to clients (it can leak
/// internals) — log it server-side and return a generic 500.
pub(crate) fn server_error<E: std::fmt::Debug>(err: E) -> StatusCode {
    tracing::error!("internal error: {:?}", err);
    StatusCode::INTERNAL_SERVER_ERROR
}
