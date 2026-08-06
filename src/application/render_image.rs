//! Render-image use case.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use crate::{
    app::AppState,
    application::single_flight::SharedValue,
    domain::{
        cache_entry::CacheEntry,
        limits::MAX_IMAGE_FILE_SIZE,
        transform::{OutputFormat, ParsedRequest, Transformations},
    },
    error::AppError,
    infrastructure::{redis::LOCK_PREFIX, vips::VipsProcessor},
};

/// How long a build lock may be held before Redis expires it.
const BUILD_LOCK_TTL_SECS: u64 = 60;

/// How long a waiter polls for another instance's build to finish.
const BUILD_WAIT_TIMEOUT: Duration = Duration::from_secs(45);

/// Interval between waiter cache polls.
const BUILD_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(50);

static LOCK_OWNER_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Technology-neutral input required to render an image.
pub struct RenderImageRequest<'a> {
    pub image_path: &'a str,
    pub transform_query: Option<&'a str>,
    pub head_only: bool,
}

/// Rendered image data and metadata returned to the transport layer.
pub struct RenderedImage {
    pub body: Option<Vec<u8>>,
    pub content_length: u64,
    pub content_type: String,
}

/// Serves a cached transform or generates and persists a new one.
pub async fn execute(
    state: &AppState,
    request: RenderImageRequest<'_>,
) -> Result<RenderedImage, AppError> {
    let parsed = ParsedRequest::parse(request.image_path, request.transform_query, state.config())
        .map_err(|error| AppError::InvalidTransform(error.to_string()))?;
    let format = parsed.transformations.format;
    let cache_path_key = parsed.cache_path_key();
    let cache_key = format!("cache:{cache_path_key}");

    if let Some(entry) = state.cache().get(&cache_key).await? {
        match serve_cached(state, request.head_only, &cache_key, entry, format).await {
            Ok(image) => return Ok(image),
            Err(AppError::NotFound) => {}
            Err(error) => return Err(error),
        }
    }

    let image_path = parsed.image_path.clone();
    let transformations = parsed.transformations;
    let flight_key = cache_path_key.clone();
    let built = state
        .single_flight()
        .run(flight_key, || {
            let state = state.clone();
            let cache_path_key = cache_path_key.clone();
            let cache_key = cache_key.clone();
            async move {
                build_or_wait(
                    &state,
                    &image_path,
                    &cache_path_key,
                    &cache_key,
                    transformations,
                    format,
                )
                .await
            }
        })
        .await?;

    Ok(RenderedImage {
        content_length: built.bytes.len() as u64,
        content_type: built.content_type,
        body: (!request.head_only).then(|| built.bytes.as_ref().clone()),
    })
}

async fn build_or_wait(
    state: &AppState,
    image_path: &str,
    cache_path_key: &str,
    cache_key: &str,
    transformations: Transformations,
    format: OutputFormat,
) -> Result<SharedValue, AppError> {
    let lock_key = format!("{LOCK_PREFIX}{cache_path_key}");
    let owner_id = next_lock_owner();
    let deadline = Instant::now() + BUILD_WAIT_TIMEOUT;

    loop {
        if let Some(entry) = state.cache().get(cache_key).await? {
            match load_shared(state, entry, format).await {
                Ok(value) => return Ok(value),
                Err(AppError::NotFound) => {}
                Err(error) => return Err(error),
            }
        }

        if state
            .cache()
            .try_acquire_lock(&lock_key, &owner_id, BUILD_LOCK_TTL_SECS)
            .await?
        {
            let result = build_and_store(
                state,
                image_path,
                cache_path_key,
                cache_key,
                transformations,
                format,
            )
            .await;
            let _ = state.cache().release_lock(&lock_key, &owner_id).await;
            return result;
        }

        if Instant::now() >= deadline {
            return Err(AppError::ImageProcessing(
                "timed out waiting for in-flight transform".to_owned(),
            ));
        }

        tokio::time::sleep(BUILD_WAIT_POLL_INTERVAL).await;
    }
}

async fn build_and_store(
    state: &AppState,
    image_path: &str,
    cache_path_key: &str,
    cache_key: &str,
    transformations: Transformations,
    format: OutputFormat,
) -> Result<SharedValue, AppError> {
    // Another instance may have finished between the miss and lock acquisition.
    if let Some(entry) = state.cache().get(cache_key).await? {
        match load_shared(state, entry, format).await {
            Ok(value) => return Ok(value),
            Err(AppError::NotFound) => {}
            Err(error) => return Err(error),
        }
    }

    let source = state.storage().get(image_path).await?;
    if source.len() > MAX_IMAGE_FILE_SIZE {
        return Err(AppError::PayloadTooLarge(format!(
            "image exceeds max file size of {MAX_IMAGE_FILE_SIZE} bytes"
        )));
    }

    let output =
        tokio::task::spawn_blocking(move || VipsProcessor::process(&source, &transformations))
            .await
            .map_err(|error| AppError::ImageProcessing(error.to_string()))??;
    let content_type = format.content_type();
    let entry = CacheEntry::new(cache_path_key, output.len() as u64, content_type);

    state
        .storage()
        .upload(cache_path_key, output.clone(), content_type)
        .await?;
    state.cache().set(cache_key, &entry).await?;

    Ok(SharedValue {
        bytes: Arc::new(output),
        content_type: content_type.to_owned(),
    })
}

async fn load_shared(
    state: &AppState,
    entry: CacheEntry,
    format: OutputFormat,
) -> Result<SharedValue, AppError> {
    let bytes = state.storage().get(&entry.s3_key).await?;
    let content_type = entry
        .content_type
        .unwrap_or_else(|| format.content_type().to_owned());
    Ok(SharedValue {
        bytes: Arc::new(bytes.to_vec()),
        content_type,
    })
}

async fn serve_cached(
    state: &AppState,
    head_only: bool,
    cache_key: &str,
    entry: CacheEntry,
    format: OutputFormat,
) -> Result<RenderedImage, AppError> {
    let fallback_content_type = format.content_type();

    if head_only {
        let (content_length, content_type) =
            resolve_head_meta(state, cache_key, &entry, fallback_content_type).await?;
        return Ok(RenderedImage {
            body: None,
            content_length,
            content_type,
        });
    }

    let bytes = state.storage().get(&entry.s3_key).await?;
    let content_type = entry
        .content_type
        .as_deref()
        .unwrap_or(fallback_content_type);

    if entry.size.is_none() || entry.content_type.is_none() {
        let refreshed = CacheEntry::new(&entry.s3_key, bytes.len() as u64, content_type);
        let _ = state.cache().set(cache_key, &refreshed).await;
    }

    Ok(RenderedImage {
        body: Some(bytes.to_vec()),
        content_length: bytes.len() as u64,
        content_type: content_type.to_owned(),
    })
}

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

    let metadata = state.storage().head(&entry.s3_key).await?;
    let content_type = metadata
        .content_type
        .or_else(|| entry.content_type.clone())
        .unwrap_or_else(|| fallback_content_type.to_owned());
    let refreshed = CacheEntry::new(&entry.s3_key, metadata.content_length, &content_type);
    let _ = state.cache().set(cache_key, &refreshed).await;

    Ok((metadata.content_length, content_type))
}

fn next_lock_owner() -> String {
    let sequence = LOCK_OWNER_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{sequence}", std::process::id())
}
