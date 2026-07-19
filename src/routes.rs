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
    cache::CacheEntry,
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
///
/// `HEAD` cache hits never download the object body: they use the size stored in
/// Redis when available, otherwise S3 `HeadObject`.
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
    if let Some(entry) = state.cache_get(&cache_key).await? {
        match serve_cached(&state, &method, &cache_key, entry, format).await {
            Ok(response) => return Ok(response),
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

    let content_type = format.content_type();
    let entry = CacheEntry::new(&cache_path_key, output.len() as u64, content_type);

    // Persist to the cache (both the object and its Redis marker).
    state
        .upload(&cache_path_key, output.clone(), content_type)
        .await?;
    state.cache_set(&cache_key, &entry).await?;

    Ok(build_response(
        &method,
        Some(output),
        entry.size.unwrap_or(0),
        content_type,
        &state,
    ))
}

/// Serves a cached transform for `GET` or `HEAD` without regenerating.
async fn serve_cached(
    state: &AppState,
    method: &Method,
    cache_key: &str,
    entry: CacheEntry,
    format: OutputFormat,
) -> Result<Response, AppError> {
    let fallback_content_type = format.content_type();

    if *method == Method::HEAD {
        let (content_length, content_type) =
            resolve_head_meta(state, cache_key, &entry, fallback_content_type).await?;
        return Ok(build_response(
            method,
            None,
            content_length,
            content_type.as_str(),
            state,
        ));
    }

    let bytes = state.get_file(&entry.s3_key).await?;
    let content_type = entry
        .content_type
        .as_deref()
        .unwrap_or(fallback_content_type);

    // Backfill size/content-type for legacy plain-string Redis markers.
    if entry.size.is_none() || entry.content_type.is_none() {
        let refreshed = CacheEntry::new(&entry.s3_key, bytes.len() as u64, content_type);
        let _ = state.cache_set(cache_key, &refreshed).await;
    }

    Ok(build_response(
        method,
        Some(bytes.to_vec()),
        bytes.len() as u64,
        content_type,
        state,
    ))
}

/// Resolves `Content-Length` / `Content-Type` for a HEAD cache hit.
///
/// Prefers Redis metadata; falls back to S3 `HeadObject` and backfills Redis.
async fn resolve_head_meta(
    state: &AppState,
    cache_key: &str,
    entry: &CacheEntry,
    fallback_content_type: &str,
) -> Result<(u64, String), AppError> {
    if let Some(size) = entry.size {
        let content_type = entry
            .content_type
            .clone()
            .unwrap_or_else(|| fallback_content_type.to_owned());
        return Ok((size, content_type));
    }

    let meta = state.head_file(&entry.s3_key).await?;
    let content_type = meta
        .content_type
        .or_else(|| entry.content_type.clone())
        .unwrap_or_else(|| fallback_content_type.to_owned());

    let refreshed = CacheEntry::new(&entry.s3_key, meta.content_length, &content_type);
    let _ = state.cache_set(cache_key, &refreshed).await;

    Ok((meta.content_length, content_type))
}

fn build_response(
    method: &Method,
    body: Option<Vec<u8>>,
    content_length: u64,
    content_type: &str,
    state: &AppState,
) -> Response {
    let cached_time = state.config().cached_time;
    let expires = httpdate::fmt_http_date(SystemTime::now() + Duration::from_secs(cached_time));

    let builder = Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(
            header::CACHE_CONTROL,
            format!("public, max-age={cached_time}, must-revalidate"),
        )
        .header(header::EXPIRES, expires);

    let response = if *method == Method::HEAD {
        builder
            .header(header::CONTENT_LENGTH, content_length)
            .body(Body::empty())
    } else {
        builder.body(Body::from(body.unwrap_or_default()))
    };

    response.unwrap_or_else(|_| {
        Response::builder()
            .status(500)
            .body(Body::empty())
            .expect("empty body is always valid")
    })
}
