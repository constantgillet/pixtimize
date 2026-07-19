//! Error types shared across the application.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// Errors raised while loading configuration from the environment.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required environment variable: {0}")]
    MissingVar(&'static str),

    #[error("invalid value for {key}: {value}")]
    InvalidValue { key: &'static str, value: String },
}

/// Errors surfaced while serving an image transform request.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The transform string could not be parsed.
    #[error("invalid transform: {0}")]
    InvalidTransform(String),

    /// The source image exceeds ImageKit-compatible size limits.
    #[error("{0}")]
    PayloadTooLarge(String),

    /// The requested source object does not exist.
    #[error("image not found")]
    NotFound,

    /// Image decoding or encoding failed.
    #[error("failed to process image: {0}")]
    ImageProcessing(String),

    /// A storage (S3) backend failure.
    #[error("storage error: {0}")]
    Storage(String),

    /// A cache (Redis) backend failure.
    #[error("cache error: {0}")]
    Cache(String),
}

impl AppError {
    fn status(&self) -> StatusCode {
        match self {
            Self::InvalidTransform(_) | Self::PayloadTooLarge(_) => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::ImageProcessing(_) | Self::Storage(_) | Self::Cache(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        if status.is_server_error() {
            tracing::error!(error = %self, "request failed");
        } else {
            tracing::warn!(error = %self, "request rejected");
        }
        let message = match status {
            StatusCode::NOT_FOUND => "Image not found".to_owned(),
            StatusCode::INTERNAL_SERVER_ERROR => "Internal server error".to_owned(),
            _ => self.to_string(),
        };
        (status, message).into_response()
    }
}
