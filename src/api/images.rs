//! Image transform HTTP handler.

use axum::{
    extract::{Query, State},
    http::{Method, Uri},
    response::Response,
};
use serde::Deserialize;

use crate::{
    api::response::image_response,
    app::AppState,
    application::render_image::{self, RenderImageRequest},
    error::AppError,
};

#[derive(Debug, Deserialize)]
pub struct TransformQuery {
    tr: Option<String>,
}

/// Translates an image request into the render-image use case.
pub async fn render(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    Query(query): Query<TransformQuery>,
) -> Result<Response, AppError> {
    let head_only = method == Method::HEAD;
    let image = render_image::execute(
        &state,
        RenderImageRequest {
            image_path: uri.path(),
            transform_query: query.tr.as_deref(),
            head_only,
        },
    )
    .await?;

    Ok(image_response(image, head_only, state.config().cached_time))
}
