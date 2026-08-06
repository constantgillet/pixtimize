//! In-process request coalescing for concurrent identical work.

use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
};

use tokio::sync::OnceCell;

use crate::error::AppError;

type FlightCell = Arc<OnceCell<Result<SharedValue, AppError>>>;

/// Ensures only one task runs per key; concurrent callers share its result.
#[derive(Default)]
pub struct SingleFlight {
    inflight: Mutex<HashMap<String, FlightCell>>,
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
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<SharedValue, AppError>>,
    {
        let cell = {
            let mut inflight = self
                .inflight
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            inflight
                .entry(key.clone())
                .or_insert_with(|| Arc::new(OnceCell::new()))
                .clone()
        };

        let result = cell.get_or_init(init).await.clone();
        self.remove_if_current(&key, &cell);
        result
    }

    fn remove_if_current(&self, key: &str, cell: &FlightCell) {
        let mut inflight = self
            .inflight
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if inflight
            .get(key)
            .is_some_and(|existing| Arc::ptr_eq(existing, cell))
        {
            inflight.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[tokio::test]
    async fn run_should_coalesce_concurrent_calls_for_same_key() {
        let flight = Arc::new(SingleFlight::default());
        let calls = Arc::new(AtomicUsize::new(0));

        let mut tasks = Vec::new();
        for _ in 0..8 {
            let flight = Arc::clone(&flight);
            let calls = Arc::clone(&calls);
            tasks.push(tokio::spawn(async move {
                flight
                    .run("same".to_owned(), || {
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
        let calls = AtomicUsize::new(0);

        let first = flight
            .run("key".to_owned(), || async {
                calls.fetch_add(1, Ordering::SeqCst);
                Err(AppError::ImageProcessing("boom".to_owned()))
            })
            .await;
        assert!(first.is_err());

        let second = flight
            .run("key".to_owned(), || async {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(SharedValue {
                    bytes: Arc::new(vec![9]),
                    content_type: "image/png".to_owned(),
                })
            })
            .await
            .expect("retry after failure");

        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(second.bytes.as_slice(), [9]);
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
