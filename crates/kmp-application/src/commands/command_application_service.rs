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
}
