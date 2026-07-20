//! Render-image use case.

use crate::{
    app::AppState,
    domain::{
        cache_entry::CacheEntry,
        limits::MAX_IMAGE_FILE_SIZE,
        transform::{OutputFormat, ParsedRequest},
    },
    error::AppError,
    infrastructure::vips::VipsProcessor,
};

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

    let source = state.storage().get(&parsed.image_path).await?;
    if source.len() > MAX_IMAGE_FILE_SIZE {
        return Err(AppError::PayloadTooLarge(format!(
            "image exceeds max file size of {MAX_IMAGE_FILE_SIZE} bytes"
        )));
    }

    let transformations = parsed.transformations;
    let output =
        tokio::task::spawn_blocking(move || VipsProcessor::process(&source, &transformations))
            .await
            .map_err(|error| AppError::ImageProcessing(error.to_string()))??;
    let content_type = format.content_type();
    let entry = CacheEntry::new(&cache_path_key, output.len() as u64, content_type);

    state
        .storage()
        .upload(&cache_path_key, output.clone(), content_type)
        .await?;
    state.cache().set(&cache_key, &entry).await?;

    Ok(RenderedImage {
        content_length: entry.size.unwrap_or(0),
        content_type: content_type.to_owned(),
        body: (!request.head_only).then_some(output),
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
