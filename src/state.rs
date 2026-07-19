//! Shared application state passed to every handler.

use std::sync::Arc;

use anyhow::Context;
use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
use redis::aio::ConnectionManager;

use crate::config::Config;

/// Cheaply cloneable handle to shared application state.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    config: Config,
    s3: aws_sdk_s3::Client,
    redis: ConnectionManager,
}

impl AppState {
    /// Builds the shared state, constructing the S3 and Redis clients.
    pub async fn build(config: Config) -> anyhow::Result<Self> {
        let s3 = build_s3_client(&config);
        let redis = redis::Client::open(config.redis_url.clone())
            .context("invalid REDIS_URL")?
            .get_connection_manager()
            .await
            .context("failed to connect to Redis")?;

        Ok(Self {
            inner: Arc::new(AppStateInner { config, s3, redis }),
        })
    }

    /// Returns the loaded application configuration.
    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    /// Returns the shared S3 client.
    pub fn s3(&self) -> &aws_sdk_s3::Client {
        &self.inner.s3
    }

    /// Returns a cloned Redis connection manager (cheap, internally shared).
    pub fn redis(&self) -> ConnectionManager {
        self.inner.redis.clone()
    }
}

fn build_s3_client(config: &Config) -> aws_sdk_s3::Client {
    let credentials = Credentials::new(
        config.s3.access_key.clone(),
        config.s3.secret_key.clone(),
        None,
        None,
        "pixtimize",
    );

    let s3_config = aws_sdk_s3::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .region(Region::new(config.s3.region.clone()))
        .endpoint_url(config.s3.endpoint.clone())
        .credentials_provider(credentials)
        .force_path_style(false)
        .build();

    aws_sdk_s3::Client::from_conf(s3_config)
}
