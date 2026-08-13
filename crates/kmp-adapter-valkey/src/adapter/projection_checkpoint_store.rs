use kmp_ports::{PortError, ProjectionCheckpoint, ProjectionCheckpointStore};

use crate::adapter::endpoint::{DEFAULT_PROJECTION_CHECKPOINT_KEY_PREFIX, ValkeyEndpoint};
use crate::adapter::io::{execute_get_command, execute_set_command};
use crate::adapter::projection_checkpoint_serialization::{
    deserialize_projection_checkpoint, serialize_projection_checkpoint,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValkeyProjectionCheckpointStore {
    endpoint: ValkeyEndpoint,
}

impl ValkeyProjectionCheckpointStore {
    pub fn new(runtime_state_uri: impl Into<String>) -> Result<Self, PortError> {
        let endpoint = ValkeyEndpoint::parse_with_default_key_prefix(
            runtime_state_uri.into(),
            "projection checkpoint",
            DEFAULT_PROJECTION_CHECKPOINT_KEY_PREFIX,
        )?;
        Ok(Self { endpoint })
    }

    pub(crate) fn checkpoint_key(&self, consumer_name: &str, stream_name: &str) -> String {
        format!(
            "{}:{}:{}",
            self.endpoint.key_prefix, consumer_name, stream_name
        )
    }
}

impl ProjectionCheckpointStore for ValkeyProjectionCheckpointStore {
    async fn load_checkpoint(
        &self,
        consumer_name: &str,
        stream_name: &str,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        let key = self.checkpoint_key(consumer_name, stream_name);
        match execute_get_command(&self.endpoint, &key).await? {
            Some(payload) => deserialize_projection_checkpoint(&payload),
            None => Ok(None),
        }
    }

    async fn save_checkpoint(&self, checkpoint: ProjectionCheckpoint) -> Result<(), PortError> {
        let key = self.checkpoint_key(&checkpoint.consumer_name, &checkpoint.stream_name);
        let payload = serialize_projection_checkpoint(&checkpoint)?;
        execute_set_command(&self.endpoint, &key, &payload, None).await
    }
}
