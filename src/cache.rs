//! Redis-backed cache helpers and the scheduled cleanup routine.

use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::{error::AppError, state::AppState};

/// Prefix applied to every cache marker key stored in Redis.
const CACHE_PREFIX: &str = "cache:";

/// Metadata for a cached transform stored in Redis.
///
/// Older deployments stored only the S3 key as a plain string; [`CacheEntry::parse`]
/// accepts both shapes so existing markers keep working.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheEntry {
    /// S3 object key of the transformed image.
    pub s3_key: String,
    /// Byte length of the transformed object, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Content-Type of the transformed object, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

impl CacheEntry {
    /// Builds a fully populated entry for a freshly written transform.
    pub fn new(s3_key: impl Into<String>, size: u64, content_type: impl Into<String>) -> Self {
        Self {
            s3_key: s3_key.into(),
            size: Some(size),
            content_type: Some(content_type.into()),
        }
    }

    /// Parses a Redis value as JSON metadata or a legacy plain S3 key.
    pub fn parse(raw: &str) -> Self {
        if let Ok(entry) = serde_json::from_str::<Self>(raw)
            && !entry.s3_key.is_empty()
        {
            return entry;
        }

        Self {
            s3_key: raw.to_owned(),
            size: None,
            content_type: None,
        }
    }
}

impl AppState {
    /// Reads the cached transform metadata under `key`, if any.
    pub async fn cache_get(&self, key: &str) -> Result<Option<CacheEntry>, AppError> {
        let mut conn = self.redis();
        let raw: Option<String> = conn
            .get(key)
            .await
            .map_err(|err| AppError::Cache(err.to_string()))?;
        Ok(raw.map(|value| CacheEntry::parse(&value)))
    }

    /// Stores transform metadata under `key`.
    pub async fn cache_set(&self, key: &str, entry: &CacheEntry) -> Result<(), AppError> {
        let mut conn = self.redis();
        let value = serde_json::to_string(entry)
            .map_err(|err| AppError::Cache(err.to_string()))?;
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
                // Redis keys are `cache:{s3_key}`; strip the prefix for S3 deletes.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_should_read_json_entry() {
        let entry = CacheEntry::parse(
            r#"{"s3_key":"cached/abc","size":1234,"content_type":"image/webp"}"#,
        );
        assert_eq!(entry.s3_key, "cached/abc");
        assert_eq!(entry.size, Some(1234));
        assert_eq!(entry.content_type.as_deref(), Some("image/webp"));
    }

    #[test]
    fn parse_should_accept_legacy_plain_s3_key() {
        let entry = CacheEntry::parse("cached/abc");
        assert_eq!(entry.s3_key, "cached/abc");
        assert_eq!(entry.size, None);
        assert_eq!(entry.content_type, None);
    }

    #[test]
    fn new_should_populate_all_fields() {
        let entry = CacheEntry::new("cached/abc", 42, "image/jpeg");
        assert_eq!(entry.s3_key, "cached/abc");
        assert_eq!(entry.size, Some(42));
        assert_eq!(entry.content_type.as_deref(), Some("image/jpeg"));
    }
}
