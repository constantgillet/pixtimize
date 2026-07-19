//! Redis-backed cache helpers and the scheduled cleanup routine.

use redis::AsyncCommands;

use crate::{error::AppError, state::AppState};

/// Prefix applied to every cache marker key stored in Redis.
const CACHE_PREFIX: &str = "cache:";

impl AppState {
    /// Reads the S3 object key cached under `key`, if any.
    pub async fn cache_get(&self, key: &str) -> Result<Option<String>, AppError> {
        let mut conn = self.redis();
        conn.get(key)
            .await
            .map_err(|err| AppError::Cache(err.to_string()))
    }

    /// Stores `value` (an S3 object key) under `key`.
    pub async fn cache_set(&self, key: &str, value: &str) -> Result<(), AppError> {
        let mut conn = self.redis();
        conn.set::<_, _, ()>(key, value)
            .await
            .map_err(|err| AppError::Cache(err.to_string()))
    }

    /// Removes every cached transform: the Redis markers (`cache:*`) and the
    /// matching transformed objects in S3.
    ///
    /// Returns the number of cached entries removed.
    pub async fn delete_cache(&self) -> Result<u64, AppError> {
        let mut conn = self.redis();
        let mut cursor: u64 = 0;
        let mut deleted: u64 = 0;

        loop {
            let (next, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(format!("{CACHE_PREFIX}*"))
                .arg("COUNT")
                .arg(1000)
                .query_async(&mut conn)
                .await
                .map_err(|err| AppError::Cache(err.to_string()))?;

            if !keys.is_empty() {
                let s3_keys: Vec<String> = keys
                    .iter()
                    .map(|key| key.trim_start_matches(CACHE_PREFIX).to_owned())
                    .collect();

                conn.del::<_, ()>(&keys)
                    .await
                    .map_err(|err| AppError::Cache(err.to_string()))?;

                self.delete_multiple(s3_keys).await?;
                deleted += keys.len() as u64;
            }

            cursor = next;
            if cursor == 0 {
                break;
            }
        }

        Ok(deleted)
    }
}
