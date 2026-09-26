use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Mutex};

use tokio::sync::OnceCell;

/// Outcomes by key, each computed once however many callers ask at the same
/// time: the first caller runs the work, the others await the same result
/// instead of queueing behind a lock that covers the remote call. With
/// `kept` above zero the latest `kept` outcomes stay (frozen selections that
/// continuation pages read back); with zero an outcome is forgotten as soon
/// as it is handed out (in-flight deduplication only).
pub(super) struct SharedOutcomes<V> {
    cells: Mutex<VecDeque<(String, Arc<OnceCell<V>>)>>,
    kept: usize,
}

impl<V: Clone> SharedOutcomes<V> {
    pub(super) fn new(kept: usize) -> Self {
        Self {
            cells: Mutex::new(VecDeque::new()),
            kept,
        }
    }

    /// The outcome for `key`: a kept or in-flight one, or — when `start` —
    /// a new one from `run`. `None` when nothing is known and starting is
    /// not allowed. The flag says whether this caller ran `run`.
    pub(super) async fn get_or_run<F, Fut>(
        &self,
        key: &str,
        start: bool,
        run: F,
    ) -> Option<(V, bool)>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = V>,
    {
        let cell = {
            let mut cells = self.lock();
            match cells.iter().find(|(kept, _)| kept == key) {
                Some((_, cell)) => Arc::clone(cell),
                None if !start => return None,
                None => {
                    let cell = Arc::new(OnceCell::new());
                    cells.push_back((key.to_string(), Arc::clone(&cell)));
                    while self.kept > 0 && cells.len() > self.kept {
                        cells.pop_front();
                    }
                    cell
                }
            }
        };
        let mut ran = false;
        let value = cell
            .get_or_init(|| {
                ran = true;
                run()
            })
            .await
            .clone();
        if self.kept == 0 {
            self.lock()
                .retain(|(kept, held)| kept != key || !Arc::ptr_eq(held, &cell));
        }
        Some((value, ran))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<(String, Arc<OnceCell<V>>)>> {
        self.cells
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn concurrent_callers_share_one_run() {
        let outcomes = Arc::new(SharedOutcomes::<usize>::new(0));
        let runs = Arc::new(AtomicUsize::new(0));
        let tasks = (0..8)
            .map(|_| {
                let outcomes = Arc::clone(&outcomes);
                let runs = Arc::clone(&runs);
                tokio::spawn(async move {
                    outcomes
                        .get_or_run("k", true, || async {
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            runs.fetch_add(1, Ordering::SeqCst) + 40
                        })
                        .await
                })
            })
            .collect::<Vec<_>>();
        let mut ran = 0;
        for task in tasks {
            let (value, mine) = task.await.expect("task").expect("started");
            assert_eq!(value, 40);
            ran += usize::from(mine);
        }
        assert_eq!((runs.load(Ordering::SeqCst), ran), (1, 1));
        assert_eq!(
            outcomes.len(),
            0,
            "in-flight only: forgotten once handed out"
        );
    }

    #[tokio::test]
    async fn kept_outcomes_answer_continuations_and_the_oldest_leaves_first() {
        let outcomes = SharedOutcomes::<String>::new(2);
        assert_eq!(
            outcomes
                .get_or_run("a", false, || async { "x".to_string() })
                .await,
            None
        );
        for key in ["a", "b", "c"] {
            outcomes
                .get_or_run(key, true, || async move { key.to_uppercase() })
                .await;
        }
        assert_eq!(
            outcomes
                .get_or_run("a", false, || async { "new".into() })
                .await,
            None
        );
        assert_eq!(
            outcomes
                .get_or_run("c", false, || async { "new".into() })
                .await,
            Some(("C".to_string(), false))
        );
    }

    #[tokio::test]
    async fn a_cancelled_run_leaves_the_key_to_the_next_caller() {
        let outcomes = SharedOutcomes::<u8>::new(4);
        let cancelled = tokio::time::timeout(
            Duration::from_millis(10),
            outcomes.get_or_run("k", true, || async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                1
            }),
        )
        .await;
        assert!(cancelled.is_err());
        assert_eq!(
            outcomes.get_or_run("k", false, || async { 2 }).await,
            Some((2, true))
        );
    }
}
