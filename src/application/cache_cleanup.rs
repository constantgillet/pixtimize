//! Periodic transformed-image cache cleanup use case.

use crate::{app::AppState, error::AppError, infrastructure::redis::CACHE_PREFIX};

/// Removes every transformed-image marker and its matching S3 object.
pub async fn execute(state: &AppState) -> Result<u64, AppError> {
    let mut cursor = 0;
    let mut deleted = 0;

    loop {
        let (next, keys) = state.cache().scan_keys(cursor).await?;

        if !keys.is_empty() {
            let storage_keys = keys
                .iter()
                .map(|key| key.trim_start_matches(CACHE_PREFIX).to_owned())
                .collect();
            state.cache().delete_keys(&keys).await?;
            state.storage().delete_multiple(storage_keys).await?;
            deleted += keys.len() as u64;
        }

        cursor = next;
        if cursor == 0 {
            return Ok(deleted);
        }
    }
}
