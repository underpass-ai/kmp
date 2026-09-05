use std::sync::Arc;

use kmp_domain::{ContextEventStore, ProjectionWriter};

use crate::ApplicationError;
use crate::commands::{
    NoopProjectionWriter, UpdateContextCommand, UpdateContextOutcome, UpdateContextUseCase,
};

#[derive(Debug)]
pub struct CommandApplicationService<E, W = NoopProjectionWriter> {
    update_context: Arc<UpdateContextUseCase<E, W>>,
}

impl<E, W> CommandApplicationService<E, W>
where
    E: ContextEventStore + Send + Sync,
    W: ProjectionWriter + Send + Sync,
{
    pub fn new(update_context: Arc<UpdateContextUseCase<E, W>>) -> Self {
        Self { update_context }
    }

    pub async fn update_context(
        &self,
        command: UpdateContextCommand,
    ) -> Result<UpdateContextOutcome, ApplicationError> {
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
