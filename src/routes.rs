//! HTTP handlers: the health root and the image transform pipeline.

use std::time::{Duration, SystemTime};

use axum::{
    body::Body,
    extract::{Query, State},
    http::{Method, Uri, header},
    response::Response,
};
use serde::Deserialize;

use crate::{
    error::AppError,
    image_ops,
    limits::MAX_IMAGE_FILE_SIZE,
    state::AppState,
    transform::{OutputFormat, ParsedRequest},
};

/// Query parameters accepted on image requests.
#[derive(Debug, Deserialize)]
pub struct TrParams {
    /// ImageKit transform string, e.g. `w-606,h-450,f-jpeg`.
    tr: Option<String>,
}

/// `GET /` — liveness response.
pub async fn root() -> &'static str {
    "OK"
}

/// Fallback handler serving `GET`/`HEAD` image requests.
///
/// Looks up the transformed image in the cache first; on a miss it fetches the
/// source from S3, transforms it, stores it back in the cache, and serves it.
pub async fn render_image(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    Query(params): Query<TrParams>,
) -> Result<Response, AppError> {
    let parsed = ParsedRequest::parse(uri.path(), params.tr.as_deref(), state.config())?;
    let format = parsed.transformations.format;
    let cache_path_key = parsed.cache_path_key();
    let cache_key = format!("cache:{cache_path_key}");

    // Cache hit: serve the previously transformed object.
    if let Some(cached_key) = state.cache_get(&cache_key).await? {
        match state.get_file(&cached_key).await {
            Ok(bytes) => return Ok(build_response(&method, bytes.to_vec(), format, &state)),
            // The marker was stale; regenerate below.
            Err(AppError::NotFound) => {}
            Err(err) => return Err(err),
        }
    }

    // Cache miss: fetch the source image (404 if it does not exist).
    let source = state.get_file(&parsed.image_path).await?;
    if source.len() > MAX_IMAGE_FILE_SIZE {
        return Err(AppError::PayloadTooLarge(format!(
            "image exceeds max file size of {MAX_IMAGE_FILE_SIZE} bytes"
        )));
    }
    let transformations = parsed.transformations;

    let output = tokio::task::spawn_blocking(move || {
        image_ops::process(source.as_ref(), &transformations)
    })
    .await
    .map_err(|err| AppError::ImageProcessing(err.to_string()))??;

    // Persist to the cache (both the object and its Redis marker).
    state
        .upload(&cache_path_key, output.clone(), format.content_type())
        .await?;
    state.cache_set(&cache_key, &cache_path_key).await?;

    Ok(build_response(&method, output, format, &state))
}

fn build_response(method: &Method, bytes: Vec<u8>, format: OutputFormat, state: &AppState) -> Response {
    let cached_time = state.config().cached_time;
    let expires = httpdate::fmt_http_date(SystemTime::now() + Duration::from_secs(cached_time));

    let builder = Response::builder()
        .header(header::CONTENT_TYPE, format.content_type())
        .header(
            header::CACHE_CONTROL,
            format!("public, max-age={cached_time}, must-revalidate"),
        )
        .header(header::EXPIRES, expires);

    let response = if method == Method::HEAD {
        builder
            .header(header::CONTENT_LENGTH, bytes.len())
            .body(Body::empty())
    } else {
        builder.body(Body::from(bytes))
    };

    response.unwrap_or_else(|_| {
        Response::builder()
            .status(500)
            .body(Body::empty())
            .expect("empty body is always valid")
    })
}
