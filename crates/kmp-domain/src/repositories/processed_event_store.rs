use std::future::Future;
use std::sync::Arc;

use crate::PortError;

pub trait ProcessedEventStore {
    fn has_processed(
        &self,
        consumer_name: &str,
        event_id: &str,
    ) -> impl Future<Output = Result<bool, PortError>> + Send;

    fn record_processed(
        &self,
        consumer_name: &str,
        event_id: &str,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
}

impl<T> ProcessedEventStore for Arc<T>
where
    T: ProcessedEventStore + Send + Sync + ?Sized,
{
    async fn has_processed(&self, consumer_name: &str, event_id: &str) -> Result<bool, PortError> {
        self.as_ref().has_processed(consumer_name, event_id).await
    }

    async fn record_processed(&self, consumer_name: &str, event_id: &str) -> Result<(), PortError> {
        self.as_ref()
            .record_processed(consumer_name, event_id)
            .await
    }
}
