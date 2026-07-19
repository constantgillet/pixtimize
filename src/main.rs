//! Pixtimize: an ImageKit-compatible image transform API built on Axum.

mod cache;
mod config;
mod error;
mod image_ops;
mod routes;
mod state;
mod storage;
mod transform;

use anyhow::Context;
use axum::{Router, routing::get};
use tokio::net::TcpListener;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing_subscriber::EnvFilter;

use crate::{config::Config, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,pixtimize=debug")),
        )
        .init();

    let config = Config::from_env().context("failed to load configuration")?;
    let address = config.address;
    let cron = config.cache_delete_cron.clone();

    let state = AppState::build(config).await?;

    start_cache_cleanup(&state, &cron).await?;

    let app = Router::new()
        .route("/", get(routes::root))
        .fallback(routes::render_image)
        .with_state(state);

    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("failed to bind to {address}"))?;

    tracing::info!("Pixtimize is running at http://{address}");

    axum::serve(listener, app)
        .await
        .context("server error")?;

    Ok(())
}

/// Schedules the periodic cache cleanup job.
async fn start_cache_cleanup(state: &AppState, cron: &str) -> anyhow::Result<()> {
    let scheduler = JobScheduler::new()
        .await
        .context("failed to create job scheduler")?;

    let schedule = normalize_cron(cron);
    let job_state = state.clone();

    let job = Job::new_async(schedule.as_str(), move |_uuid, _lock| {
        let state = job_state.clone();
        Box::pin(async move {
            match state.delete_cache().await {
                Ok(count) => tracing::info!(deleted = count, "cache cleanup complete"),
                Err(err) => tracing::error!(error = %err, "cache cleanup failed"),
            }
        })
    })
    .with_context(|| format!("invalid CACHE_DELETE_CRON: {cron}"))?;

    scheduler.add(job).await.context("failed to schedule cleanup")?;
    scheduler.start().await.context("failed to start scheduler")?;

    Ok(())
}

/// The scheduler expects a 6-field cron (with seconds). Standard 5-field
/// expressions (as used by the original project) are upgraded by prepending a
/// `0` seconds field.
fn normalize_cron(cron: &str) -> String {
    let fields = cron.split_whitespace().count();
    if fields == 5 {
        format!("0 {cron}")
    } else {
        cron.to_owned()
    }
}
