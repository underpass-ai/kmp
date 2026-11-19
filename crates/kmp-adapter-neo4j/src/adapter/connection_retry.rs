use std::future::Future;
use std::time::Duration;

use tokio::time::{Instant, sleep};

/// Bounded retry for Neo4j connection failures.
///
/// neo4rs 0.8 retried a failed connection inside the driver with an
/// exponential backoff for about a minute; 0.9 reports it at once. This
/// policy restores that tolerance in the adapter: a query whose connection
/// cannot be opened or breaks is tried again with a doubling delay until the
/// next wait would pass the budget. Errors the server answers (bad Cypher,
/// constraint violations, authentication) are returned on the first try; the
/// driver already retries the server's transient errors itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ConnectionRetry {
    initial_delay: Duration,
    max_delay: Duration,
    budget: Duration,
}

impl Default for ConnectionRetry {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_millis(250),
            max_delay: Duration::from_secs(8),
            budget: Duration::from_secs(60),
        }
    }
}

impl ConnectionRetry {
    #[cfg(test)]
    pub(crate) fn new(initial_delay: Duration, max_delay: Duration, budget: Duration) -> Self {
        Self {
            initial_delay,
            max_delay,
            budget,
        }
    }

    /// Whether the error means the database could not be reached, as opposed
    /// to the database answering with a failure.
    pub(crate) fn is_connection_failure(error: &neo4rs::Error) -> bool {
        matches!(
            error,
            neo4rs::Error::IOError { .. }
                | neo4rs::Error::ConnectionError
                | neo4rs::Error::ConnectionTimedOut
                | neo4rs::Error::IncompleteBoltExchange
                | neo4rs::Error::ServerUnavailableError(_)
        )
    }

    /// The wait before retry number `retry` (0-based): doubling, capped.
    pub(crate) fn delay_before(&self, retry: u32) -> Duration {
        self.initial_delay
            .saturating_mul(2u32.saturating_pow(retry))
            .min(self.max_delay)
    }

    /// Runs `attempt` until it succeeds, fails with an error that is not a
    /// connection failure, or the next wait would exceed the budget.
    pub(crate) async fn run<T, F, Fut>(&self, mut attempt: F) -> Result<T, neo4rs::Error>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, neo4rs::Error>>,
    {
        let started = Instant::now();
        let mut retry = 0;
        loop {
            match attempt().await {
                Ok(value) => return Ok(value),
                Err(error) if Self::is_connection_failure(&error) => {
                    let delay = self.delay_before(retry);
                    if started.elapsed() + delay > self.budget {
                        return Err(error);
                    }
                    sleep(delay).await;
                    retry += 1;
                }
                Err(error) => return Err(error),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn io_error() -> neo4rs::Error {
        std::io::Error::from(std::io::ErrorKind::ConnectionReset).into()
    }

    #[test]
    fn connection_failures_retry_and_answers_do_not() {
        assert!(ConnectionRetry::is_connection_failure(&io_error()));
        assert!(ConnectionRetry::is_connection_failure(
            &neo4rs::Error::ConnectionError
        ));
        assert!(ConnectionRetry::is_connection_failure(
            &neo4rs::Error::ConnectionTimedOut
        ));
        assert!(!ConnectionRetry::is_connection_failure(
            &neo4rs::Error::AuthenticationError("bad credentials".to_string())
        ));
        assert!(!ConnectionRetry::is_connection_failure(
            &neo4rs::Error::UnexpectedMessage("syntax".to_string())
        ));
        assert!(!ConnectionRetry::is_connection_failure(
            &neo4rs::Error::ConversionError
        ));
    }

    #[test]
    fn delays_double_up_to_the_cap() {
        let retry = ConnectionRetry::default();
        assert_eq!(retry.delay_before(0), Duration::from_millis(250));
        assert_eq!(retry.delay_before(1), Duration::from_millis(500));
        assert_eq!(retry.delay_before(4), Duration::from_secs(4));
        assert_eq!(retry.delay_before(5), Duration::from_secs(8));
        assert_eq!(retry.delay_before(40), Duration::from_secs(8));
    }

    #[tokio::test(start_paused = true)]
    async fn an_unreachable_database_is_given_up_within_the_budget() {
        let attempts = Cell::new(0u32);
        let started = Instant::now();
        let result: Result<(), _> = ConnectionRetry::default()
            .run(|| {
                attempts.set(attempts.get() + 1);
                async { Err(io_error()) }
            })
            .await;

        assert!(matches!(result, Err(neo4rs::Error::IOError { .. })));
        let elapsed = started.elapsed();
        assert!(elapsed <= Duration::from_secs(60), "waited {elapsed:?}");
        assert!(
            elapsed >= Duration::from_secs(50),
            "gave up after {elapsed:?}"
        );
        assert!(attempts.get() > 5, "only {} attempts", attempts.get());
    }

    #[tokio::test(start_paused = true)]
    async fn a_server_answer_is_returned_on_the_first_attempt() {
        let attempts = Cell::new(0u32);
        let result: Result<(), _> = ConnectionRetry::default()
            .run(|| {
                attempts.set(attempts.get() + 1);
                async { Err(neo4rs::Error::UnexpectedMessage("constraint".to_string())) }
            })
            .await;

        assert!(matches!(result, Err(neo4rs::Error::UnexpectedMessage(_))));
        assert_eq!(attempts.get(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_database_that_comes_up_is_used() {
        let attempts = Cell::new(0u32);
        let retry = ConnectionRetry::new(
            Duration::from_millis(10),
            Duration::from_millis(40),
            Duration::from_secs(1),
        );
        let result = retry
            .run(|| {
                attempts.set(attempts.get() + 1);
                let attempt = attempts.get();
                async move {
                    if attempt < 3 {
                        Err(neo4rs::Error::ConnectionError)
                    } else {
                        Ok(attempt)
                    }
                }
            })
            .await;

        assert_eq!(result.expect("third attempt connects"), 3);
    }
}
