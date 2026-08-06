//! Redis adapter for transformed-image metadata and build locks.

use anyhow::Context;
use redis::{AsyncCommands, aio::ConnectionManager};

use crate::{domain::cache_entry::CacheEntry, error::AppError};

/// Prefix applied to transformed-image metadata keys.
pub const CACHE_PREFIX: &str = "cache:";

/// Prefix applied to single-flight build locks.
pub const LOCK_PREFIX: &str = "lock:";

/// Deletes the lock only when it is still owned by the caller.
const RELEASE_LOCK_SCRIPT: &str = r#"
if redis.call("get", KEYS[1]) == ARGV[1] then
  return redis.call("del", KEYS[1])
else
  return 0
end
"#;

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

    /// Attempts to acquire an exclusive build lock with TTL `ttl_secs`.
    ///
    /// Returns `true` when this caller owns the lock.
    pub async fn try_acquire_lock(
        &self,
        lock_key: &str,
        owner_id: &str,
        ttl_secs: u64,
    ) -> Result<bool, AppError> {
        let mut connection = self.connection.clone();
        let acquired: Option<String> = redis::cmd("SET")
            .arg(lock_key)
            .arg(owner_id)
            .arg("NX")
            .arg("EX")
            .arg(ttl_secs)
            .query_async(&mut connection)
            .await
            .map_err(|error| AppError::Cache(error.to_string()))?;
        Ok(acquired.is_some())
    }

    /// Releases a build lock when `owner_id` still owns it.
    pub async fn release_lock(&self, lock_key: &str, owner_id: &str) -> Result<(), AppError> {
        let mut connection = self.connection.clone();
        redis::cmd("EVAL")
            .arg(RELEASE_LOCK_SCRIPT)
            .arg(1)
            .arg(lock_key)
            .arg(owner_id)
            .query_async::<()>(&mut connection)
            .await
            .map_err(|error| AppError::Cache(error.to_string()))
    }
}
