use std::future::Future;
use std::sync::Arc;

use crate::{PortError, ProjectionMutation};

pub trait ProjectionWriter {
    fn apply_mutations(
        &self,
        mutations: Vec<ProjectionMutation>,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
}

impl<T> ProjectionWriter for Arc<T>
where
    T: ProjectionWriter + Send + Sync + ?Sized,
{
    async fn apply_mutations(&self, mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        self.as_ref().apply_mutations(mutations).await
    }
}
