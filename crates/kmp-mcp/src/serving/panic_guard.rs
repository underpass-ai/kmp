//! One request that panics answers with a JSON-RPC error; it does not end the
//! session (B2 of the 1.0 review). The stdio server runs every request on its
//! main task, so before this an unforeseen panic in any tool killed the
//! process with exit 101 and every later call of the session failed until the
//! host restarted it. The schema check refuses the shapes that caused the
//! known panics; this is the line behind it, for the stdio server and the HTTP
//! gateway alike.

use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future whose panic is caught and returned as `Err`.
pub(crate) struct PanicGuard<'a, T>(Pin<Box<dyn Future<Output = T> + Send + 'a>>);

impl<'a, T> PanicGuard<'a, T> {
    pub(crate) fn new(future: impl Future<Output = T> + Send + 'a) -> Self {
        Self(Box::pin(future))
    }
}

impl<T> Future for PanicGuard<'_, T> {
    type Output = Result<T, String>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let inner = &mut self.0;
        match catch_unwind(AssertUnwindSafe(|| inner.as_mut().poll(context))) {
            Ok(Poll::Pending) => Poll::Pending,
            Ok(Poll::Ready(value)) => Poll::Ready(Ok(value)),
            Err(payload) => Poll::Ready(Err(payload
                .downcast_ref::<&str>()
                .map(ToString::to_string)
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "a panic without a message".to_string()))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_panic_becomes_an_error_and_a_value_passes_through() {
        assert_eq!(PanicGuard::new(async { 7 }).await, Ok(7));
        let caught = PanicGuard::new(async {
            let value = serde_json::json!(1);
            let mut value = value;
            value["page"]["cursor"] = serde_json::json!("x");
            value
        })
        .await;
        assert!(
            caught
                .expect_err("the index panics")
                .contains("cannot access key"),
            "the message travels"
        );
    }
}
