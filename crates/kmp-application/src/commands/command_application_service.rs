use std::sync::Arc;

#[cfg(test)]
#[path = "command_application_service_tests.rs"]
mod tests;

use kmp_domain::{ContextEventStore, ProjectionWriter};

use crate::ApplicationError;
use crate::commands::{
    NoopProjectionWriter, UpdateContextCommand, UpdateContextOutcome, UpdateContextUseCase,
};

#[derive(Debug)]
pub struct CommandApplicationService<E, W = NoopProjectionWriter> {
    update_context: Arc<UpdateContextUseCase<E, W>>,
    // One active engine owns a store. Keep review reads out of the interval
    // between event append and projection materialization in that engine.
    projection_access: tokio::sync::RwLock<()>,
}

impl<E, W> CommandApplicationService<E, W>
where
    E: ContextEventStore + Send + Sync,
    W: ProjectionWriter + Send + Sync,
{
    pub fn new(update_context: Arc<UpdateContextUseCase<E, W>>) -> Self {
        Self {
            update_context,
            projection_access: tokio::sync::RwLock::new(()),
        }
    }

    pub async fn update_context(
        &self,
        command: UpdateContextCommand,
    ) -> Result<UpdateContextOutcome, ApplicationError> {
        self.update_context_after_read(command, &std::collections::BTreeMap::new())
            .await
    }

    pub(crate) async fn projection_read(&self) -> tokio::sync::RwLockReadGuard<'_, ()> {
        self.projection_access.read().await
    }

    pub(crate) async fn memory_revision(&self, about: &str) -> Result<u64, ApplicationError> {
        self.update_context.memory_revision(about).await
    }

    /// Recheck every explicitly read about while excluding concurrent writes,
    /// including writes to a foreign equivalence endpoint. A stale view applies
    /// nothing. Existing idempotent outcomes take precedence over fresh context.
    pub(crate) async fn update_context_after_read(
        &self,
        command: UpdateContextCommand,
        revisions: &std::collections::BTreeMap<String, u64>,
    ) -> Result<UpdateContextOutcome, ApplicationError> {
        let _guard = self.projection_access.write().await;
        let accepted = match command.idempotency_key.as_deref() {
            Some(key) => self.accepted_outcome(key).await?.is_some(),
            None => false,
        };
        if !accepted {
            for (about, revision) in revisions {
                if self.memory_revision(about).await? != *revision {
                    return Err(ApplicationError::RetryableConflict(
                        "write neighborhood changed during commit; retry the same logical write to refresh it".into(),
                    ));
                }
            }
        }
        self.update_context.execute(command).await
    }

    /// What an idempotency key was already accepted with, if anything.
    pub async fn accepted_outcome(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<kmp_domain::IdempotentOutcome>, ApplicationError> {
        self.update_context.accepted_outcome(idempotency_key).await
    }
}
