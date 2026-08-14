use std::future::Future;
use std::sync::Arc;

use crate::{PortError, ProjectionCheckpoint};

pub trait ProjectionCheckpointStore {
    fn load_checkpoint(
        &self,
        consumer_name: &str,
        stream_name: &str,
    ) -> impl Future<Output = Result<Option<ProjectionCheckpoint>, PortError>> + Send;

    fn save_checkpoint(
        &self,
        checkpoint: ProjectionCheckpoint,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
}

impl<T> ProjectionCheckpointStore for Arc<T>
where
    T: ProjectionCheckpointStore + Send + Sync + ?Sized,
{
    async fn load_checkpoint(
        &self,
        consumer_name: &str,
        stream_name: &str,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        self.as_ref()
            .load_checkpoint(consumer_name, stream_name)
            .await
    }

    async fn save_checkpoint(&self, checkpoint: ProjectionCheckpoint) -> Result<(), PortError> {
        self.as_ref().save_checkpoint(checkpoint).await
    }
}
