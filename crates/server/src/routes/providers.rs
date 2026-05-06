use axum::Json;
use courrier_core::providers::{Provider, PROVIDERS};

pub async fn list_providers() -> Json<Vec<&'static Provider>> {
    Json(PROVIDERS.iter().collect())
}
