//! Redis adapter for transformed-image metadata.

use anyhow::Context;
use redis::{AsyncCommands, aio::ConnectionManager};

use crate::{domain::cache_entry::CacheEntry, error::AppError};

/// Prefix applied to transformed-image metadata keys.
pub const CACHE_PREFIX: &str = "cache:";

/// Redis-backed transformed-image cache.
#[derive(Clone)]
pub struct RedisCache {
    connection: ConnectionManager,
}

impl RedisCache {
    /// Opens a managed Redis connection.
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        let connection = redis::Client::open(url)
            .context("invalid REDIS_URL")?
            .get_connection_manager()
            .await
            .context("failed to connect to Redis")?;
        Ok(Self { connection })
    }

    /// Reads transformed-image metadata under `key`.
    pub async fn get(&self, key: &str) -> Result<Option<CacheEntry>, AppError> {
        let mut connection = self.connection.clone();
        let raw: Option<String> = connection
            .get(key)
            .await
            .map_err(|error| AppError::Cache(error.to_string()))?;
        Ok(raw.map(|value| CacheEntry::parse(&value)))
    }

    /// Stores transformed-image metadata under `key`.
    pub async fn set(&self, key: &str, entry: &CacheEntry) -> Result<(), AppError> {
        let mut connection = self.connection.clone();
        let value =
            serde_json::to_string(entry).map_err(|error| AppError::Cache(error.to_string()))?;
        connection
            .set::<_, _, ()>(key, value)
            .await
            .map_err(|error| AppError::Cache(error.to_string()))
    }

    /// Scans one page of transformed-image marker keys.
    pub async fn scan_keys(&self, cursor: u64) -> Result<(u64, Vec<String>), AppError> {
        let mut connection = self.connection.clone();
        redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(format!("{CACHE_PREFIX}*"))
            .arg("COUNT")
            .arg(1000)
            .query_async(&mut connection)
            .await
            .map_err(|error| AppError::Cache(error.to_string()))
    }

    /// Deletes a set of Redis marker keys.
    pub async fn delete_keys(&self, keys: &[String]) -> Result<(), AppError> {
        if keys.is_empty() {
            return Ok(());
        }

        let mut connection = self.connection.clone();
        connection
            .del::<_, ()>(keys)
            .await
            .map_err(|error| AppError::Cache(error.to_string()))
    }
}
