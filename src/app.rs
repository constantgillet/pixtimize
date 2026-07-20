//! Application composition and process startup.

use std::sync::Arc;

use anyhow::Context;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use crate::{
    api,
    config::Config,
    infrastructure::{
        redis::RedisCache, s3::S3Storage, scheduler::start_cache_cleanup, vips::VipsProcessor,
    },
};

/// Cheaply cloneable dependencies shared by request handlers.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    config: Config,
    cache: RedisCache,
    storage: S3Storage,
    _image_processor: VipsProcessor,
}

impl AppState {
    /// Constructs all infrastructure adapters used by the application.
    pub async fn build(config: Config) -> anyhow::Result<Self> {
        let image_processor = VipsProcessor::new()?;
        let storage = S3Storage::new(&config);
        let cache = RedisCache::connect(&config.redis_url).await?;

        Ok(Self {
            inner: Arc::new(AppStateInner {
                config,
                cache,
                storage,
                _image_processor: image_processor,
            }),
        })
    }

    /// Returns the loaded runtime configuration.
    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    /// Returns the Redis cache adapter.
    pub fn cache(&self) -> &RedisCache {
        &self.inner.cache
    }

    /// Returns the S3-compatible storage adapter.
    pub fn storage(&self) -> &S3Storage {
        &self.inner.storage
    }
}

/// Builds the application and serves requests until shutdown.
pub async fn run() -> anyhow::Result<()> {
    init_tracing();

    let config = Config::from_env().context("failed to load configuration")?;
    let address = config.address;
    let cron = config.cache_delete_cron.clone();
    let state = AppState::build(config).await?;

    start_cache_cleanup(&state, &cron).await?;

    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("failed to bind to {address}"))?;

    tracing::info!("Pixtimize is running at http://{address}");
    axum::serve(listener, api::router(state))
        .await
        .context("server error")
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,pixtimize=debug")),
        )
        .init();
}
