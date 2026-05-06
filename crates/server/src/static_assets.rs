//! SPA bundle embedded at build time.
//!
//! The desktop/ directory ships a Vite + React app; `pnpm build` writes
//! the production bundle into `desktop/dist/`. We embed that directory
//! into the binary so the server has zero runtime filesystem dependency.
//!
//! If the dist/ directory hasn't been built (CI before frontend build,
//! development without npm), `rust-embed`'s `RustEmbed` derive won't fail
//! — it'll just have no entries — and the routes here gracefully serve a
//! "frontend not built" placeholder instead.

use axum::body::Body;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../desktop/dist"]
pub struct SpaAssets;

const PLACEHOLDER: &str = include_str!("placeholder.html");

pub async fn serve_index() -> Response {
    serve_path("index.html").await
}

pub async fn serve_spa(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        return serve_index().await;
    }
    serve_path(path).await
}

async fn serve_path(path: &str) -> Response {
    if let Some(file) = SpaAssets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime.as_ref())
            .body(Body::from(file.data.into_owned()))
            .unwrap();
    }
    // SPA fallback: any unknown route should serve index.html so the
    // client-side router can take over.
    if let Some(file) = SpaAssets::get("index.html") {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(file.data.into_owned()))
            .unwrap();
    }
    // No frontend at all — show the placeholder.
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(PLACEHOLDER))
        .unwrap()
        .into_response()
}
