//! Axum routing and HTTP transport adapters.

mod error_response;
mod health;
mod images;
mod response;

use axum::{Router, routing::get};

use crate::app::AppState;

/// Builds the complete HTTP router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(health::root))
        .fallback(images::render)
        .with_state(state)
}
