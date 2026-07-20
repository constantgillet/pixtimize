//! HTTP response construction for rendered images.

use std::time::{Duration, SystemTime};

use axum::{body::Body, http::header, response::Response};

use crate::application::render_image::RenderedImage;

/// Adds cache headers and the correct GET or HEAD body.
pub fn image_response(image: RenderedImage, head_only: bool, cached_time: u64) -> Response {
    let expires = httpdate::fmt_http_date(SystemTime::now() + Duration::from_secs(cached_time));
    let builder = Response::builder()
        .header(header::CONTENT_TYPE, image.content_type)
        .header(
            header::CACHE_CONTROL,
            format!("public, max-age={cached_time}, must-revalidate"),
        )
        .header(header::EXPIRES, expires);

    let response = if head_only {
        builder
            .header(header::CONTENT_LENGTH, image.content_length)
            .body(Body::empty())
    } else {
        builder.body(Body::from(image.body.unwrap_or_default()))
    };

    response.unwrap_or_else(|_| {
        Response::builder()
            .status(500)
            .body(Body::empty())
            .expect("empty body is always valid")
    })
}
