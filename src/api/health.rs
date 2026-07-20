//! Liveness endpoint.

/// Returns a minimal liveness response.
pub async fn root() -> &'static str {
    "OK"
}
