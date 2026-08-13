use std::future::Future;
use std::pin::Pin;

use crate::error::BoxError;

/// Context provided to seed strategies during fixture setup.
///
/// Exposes the infrastructure handles a strategy needs to publish
/// projection events or write directly to stores.
pub struct SeedContext {
    nats_client: Option<async_nats::Client>,
}

impl SeedContext {
    pub fn new(nats_client: Option<async_nats::Client>) -> Self {
        Self { nats_client }
    }

    /// Returns the NATS client for publishing projection events.
    ///
    /// # Panics
    ///
    /// Panics if the fixture was not built with `.with_nats()`.
    pub fn nats_client(&self) -> &async_nats::Client {
        self.nats_client
            .as_ref()
            .expect("SeedContext: NATS client not available — call .with_nats() on the builder")
    }
}

/// Port: decouples seed data from fixture lifecycle.
///
/// Each seed strategy encapsulates a specific dataset topology
/// (kernel E2E, explanatory failure diagnosis, generated stress graph, etc.).
/// The fixture builder accepts any `SeedStrategy` implementation,
/// satisfying OCP — new data variants need no fixture changes.
pub trait SeedStrategy: Send + Sync {
    fn seed<'a>(
        &'a self,
        ctx: &'a SeedContext,
    ) -> Pin<Box<dyn Future<Output = Result<(), BoxError>> + Send + 'a>>;
}

/// Adapter: wraps an async closure as a `SeedStrategy`.
///
/// Enables inline seed logic without defining a named struct:
/// ```ignore
/// builder.with_seed(ClosureSeed::new(|ctx| Box::pin(async move {
///     publish_events(ctx.nats_client()).await
/// })))
/// ```
pub struct ClosureSeed<F>(F)
where
    F: for<'a> Fn(
            &'a SeedContext,
        ) -> Pin<Box<dyn Future<Output = Result<(), BoxError>> + Send + 'a>>
        + Send
        + Sync;

impl<F> ClosureSeed<F>
where
    F: for<'a> Fn(
            &'a SeedContext,
        ) -> Pin<Box<dyn Future<Output = Result<(), BoxError>> + Send + 'a>>
        + Send
        + Sync,
{
    pub fn new(f: F) -> Self {
        Self(f)
    }
}

impl<F> SeedStrategy for ClosureSeed<F>
where
    F: for<'a> Fn(
            &'a SeedContext,
        ) -> Pin<Box<dyn Future<Output = Result<(), BoxError>> + Send + 'a>>
        + Send
        + Sync,
{
    fn seed<'a>(
        &'a self,
        ctx: &'a SeedContext,
    ) -> Pin<Box<dyn Future<Output = Result<(), BoxError>> + Send + 'a>> {
        (self.0)(ctx)
    }
}
