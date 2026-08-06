//! In-process request coalescing for concurrent identical work.

use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
};

use tokio::sync::{Notify, OnceCell};

use crate::error::AppError;

/// Ensures only one task runs per key; concurrent callers share its result.
///
/// The leader work is detached with [`tokio::spawn`] so a cancelled HTTP
/// request (for example a browser refresh) cannot abort an in-flight build.
#[derive(Clone, Default)]
pub struct SingleFlight {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    inflight: Mutex<HashMap<String, Flight>>,
}

#[derive(Clone)]
struct Flight {
    cell: Arc<OnceCell<Result<SharedValue, AppError>>>,
    done: Arc<Notify>,
}

/// Cloneable payload shared with coalesced waiters.
#[derive(Clone)]
pub struct SharedValue {
    pub bytes: Arc<Vec<u8>>,
    pub content_type: String,
}

impl SingleFlight {
    /// Runs `init` once per `key` while concurrent callers await the same result.
    pub async fn run<F, Fut>(&self, key: String, init: F) -> Result<SharedValue, AppError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<SharedValue, AppError>> + Send + 'static,
    {
        let flight = {
            let mut inflight = self
                .inner
                .inflight
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            if let Some(existing) = inflight.get(&key) {
                existing.clone()
            } else {
                let flight = Flight {
                    cell: Arc::new(OnceCell::new()),
                    done: Arc::new(Notify::new()),
                };
                inflight.insert(key.clone(), flight.clone());

                let inner = Arc::clone(&self.inner);
                let flight_key = key.clone();
                let worker = flight.clone();
                tokio::spawn(async move {
                    let result = init().await;
                    // Drop the map entry before publishing so a failed flight does
                    // not trap later retries on the same OnceCell error.
                    inner.remove(&flight_key);
                    let _ = worker.cell.set(result);
                    worker.done.notify_waiters();
                });

                flight
            }
        };

        loop {
            // Register for wakeup before checking so we cannot miss a notify
            // that lands between get() and await.
            let notified = flight.done.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            if let Some(result) = flight.cell.get() {
                return result.clone();
            }

            notified.await;
        }
    }
}

impl Inner {
    fn remove(&self, key: &str) {
        let mut inflight = self
            .inflight
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inflight.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[tokio::test]
    async fn run_should_coalesce_concurrent_calls_for_same_key() {
        let flight = SingleFlight::default();
        let calls = Arc::new(AtomicUsize::new(0));

        let mut tasks = Vec::new();
        for _ in 0..8 {
            let flight = flight.clone();
            let calls = Arc::clone(&calls);
            tasks.push(tokio::spawn(async move {
                flight
                    .run("same".to_owned(), move || {
                        let calls = Arc::clone(&calls);
                        async move {
                            calls.fetch_add(1, Ordering::SeqCst);
                            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                            Ok(SharedValue {
                                bytes: Arc::new(vec![1, 2, 3]),
                                content_type: "image/webp".to_owned(),
                            })
                        }
                    })
                    .await
            }));
        }

        let results: Vec<_> = join_shared(tasks).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(
            results
                .iter()
                .all(|value| value.bytes.as_slice() == [1, 2, 3])
        );
    }

    #[tokio::test]
    async fn run_should_allow_retry_after_failure() {
        let flight = SingleFlight::default();
        let calls = Arc::new(AtomicUsize::new(0));

        let first = flight
            .run("key".to_owned(), {
                let calls = Arc::clone(&calls);
                move || {
                    let calls = Arc::clone(&calls);
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        Err(AppError::ImageProcessing("boom".to_owned()))
                    }
                }
            })
            .await;
        assert!(first.is_err());

        let second = flight
            .run("key".to_owned(), {
                let calls = Arc::clone(&calls);
                move || {
                    let calls = Arc::clone(&calls);
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        Ok(SharedValue {
                            bytes: Arc::new(vec![9]),
                            content_type: "image/png".to_owned(),
                        })
                    }
                }
            })
            .await
            .expect("retry after failure");

        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(second.bytes.as_slice(), [9]);
    }

    #[tokio::test]
    async fn run_should_finish_build_when_leader_request_is_cancelled() {
        let flight = SingleFlight::default();
        let started = Arc::new(Notify::new());
        let calls = Arc::new(AtomicUsize::new(0));

        let leader = tokio::spawn({
            let flight = flight.clone();
            let started = Arc::clone(&started);
            let calls = Arc::clone(&calls);
            async move {
                flight
                    .run("cancel".to_owned(), move || {
                        let started = Arc::clone(&started);
                        let calls = Arc::clone(&calls);
                        async move {
                            calls.fetch_add(1, Ordering::SeqCst);
                            started.notify_one();
                            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                            Ok(SharedValue {
                                bytes: Arc::new(vec![7]),
                                content_type: "image/webp".to_owned(),
                            })
                        }
                    })
                    .await
            }
        });

        started.notified().await;

        let follower = tokio::spawn({
            let flight = flight.clone();
            let calls = Arc::clone(&calls);
            async move {
                flight
                    .run("cancel".to_owned(), move || {
                        let calls = Arc::clone(&calls);
                        async move {
                            calls.fetch_add(1, Ordering::SeqCst);
                            Ok(SharedValue {
                                bytes: Arc::new(vec![0]),
                                content_type: "image/png".to_owned(),
                            })
                        }
                    })
                    .await
            }
        });

        tokio::task::yield_now().await;
        leader.abort();
        let _ = leader.await;

        let value = follower
            .await
            .expect("join")
            .expect("detached build result");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(value.bytes.as_slice(), [7]);
    }

    async fn join_shared(
        tasks: Vec<tokio::task::JoinHandle<Result<SharedValue, AppError>>>,
    ) -> Vec<SharedValue> {
        let mut values = Vec::with_capacity(tasks.len());
        for task in tasks {
            values.push(task.await.expect("join").expect("shared value"));
        }
        values
    }
}
