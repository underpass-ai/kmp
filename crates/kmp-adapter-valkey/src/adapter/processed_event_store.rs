use kmp_ports::{PortError, ProcessedEventStore};

use crate::adapter::endpoint::{DEFAULT_PROCESSED_EVENT_KEY_PREFIX, ValkeyEndpoint};
use crate::adapter::io::{execute_get_command, execute_set_command};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValkeyProcessedEventStore {
    endpoint: ValkeyEndpoint,
}

impl ValkeyProcessedEventStore {
    pub fn new(runtime_state_uri: impl Into<String>) -> Result<Self, PortError> {
        let endpoint = ValkeyEndpoint::parse_with_default_key_prefix(
            runtime_state_uri.into(),
            "processed event",
            DEFAULT_PROCESSED_EVENT_KEY_PREFIX,
        )?;
        Ok(Self { endpoint })
    }

    pub(crate) fn processed_event_key(&self, consumer_name: &str, event_id: &str) -> String {
        format!(
            "{}:{}:{}",
            self.endpoint.key_prefix, consumer_name, event_id
        )
    }
}

impl ProcessedEventStore for ValkeyProcessedEventStore {
    async fn has_processed(&self, consumer_name: &str, event_id: &str) -> Result<bool, PortError> {
        let key = self.processed_event_key(consumer_name, event_id);
        Ok(execute_get_command(&self.endpoint, &key).await?.is_some())
    }

    async fn record_processed(&self, consumer_name: &str, event_id: &str) -> Result<(), PortError> {
        let key = self.processed_event_key(consumer_name, event_id);
        execute_set_command(&self.endpoint, &key, "1", None).await
    }
}
