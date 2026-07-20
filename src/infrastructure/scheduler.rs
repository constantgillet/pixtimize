//! Scheduler adapter for periodic maintenance jobs.

use anyhow::Context;
use tokio_cron_scheduler::{Job, JobScheduler};

use crate::{app::AppState, application::cache_cleanup};

/// Schedules the periodic transformed-image cache cleanup.
pub async fn start_cache_cleanup(state: &AppState, cron: &str) -> anyhow::Result<()> {
    let scheduler = JobScheduler::new()
        .await
        .context("failed to create job scheduler")?;
    let schedule = normalize_cron(cron);
    let job_state = state.clone();

    let job = Job::new_async(schedule.as_str(), move |_uuid, _lock| {
        let state = job_state.clone();
        Box::pin(async move {
            match cache_cleanup::execute(&state).await {
                Ok(count) => tracing::info!(deleted = count, "cache cleanup complete"),
                Err(error) => tracing::error!(error = %error, "cache cleanup failed"),
            }
        })
    })
    .with_context(|| format!("invalid CACHE_DELETE_CRON: {cron}"))?;

    scheduler
        .add(job)
        .await
        .context("failed to schedule cleanup")?;
    scheduler.start().await.context("failed to start scheduler")
}

fn normalize_cron(cron: &str) -> String {
    if cron.split_whitespace().count() == 5 {
        format!("0 {cron}")
    } else {
        cron.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_cron_should_add_seconds_to_five_field_expression() {
        assert_eq!(normalize_cron("0 1 * * 1"), "0 0 1 * * 1");
    }

    #[test]
    fn normalize_cron_should_preserve_six_field_expression() {
        assert_eq!(normalize_cron("30 0 1 * * 1"), "30 0 1 * * 1");
    }
}
