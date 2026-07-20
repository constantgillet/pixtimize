//! Translation from application errors to HTTP responses.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::error::AppError;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = status(&self);
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

fn status(error: &AppError) -> StatusCode {
    match error {
        AppError::InvalidTransform(_) | AppError::PayloadTooLarge(_) => StatusCode::BAD_REQUEST,
        AppError::NotFound => StatusCode::NOT_FOUND,
        AppError::ImageProcessing(_) | AppError::Storage(_) | AppError::Cache(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
