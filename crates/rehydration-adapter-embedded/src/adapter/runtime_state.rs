use rehydration_domain::{
    PortError, ProcessedEventStore, ProjectionCheckpoint, ProjectionCheckpointStore,
};

use super::serdes::{CheckpointRecord, decode, encode};
use super::store::{
    CHECKPOINTS, EmbeddedKernelStore, PROCESSED, commit_error, storage_error, table_error,
};

impl ProcessedEventStore for EmbeddedKernelStore {
    async fn has_processed(&self, consumer_name: &str, event_id: &str) -> Result<bool, PortError> {
        let consumer_name = consumer_name.to_string();
        let event_id = event_id.to_string();
        self.run(move |store| {
            let tx = store.begin_read()?;
            let processed = tx.open_table(PROCESSED).map_err(table_error)?;
            Ok(processed
                .get((consumer_name.as_str(), event_id.as_str()))
                .map_err(storage_error)?
                .is_some())
        })
        .await
    }

    async fn record_processed(&self, consumer_name: &str, event_id: &str) -> Result<(), PortError> {
        let consumer_name = consumer_name.to_string();
        let event_id = event_id.to_string();
        self.run(move |store| {
            let tx = store.begin_write()?;
            {
                let mut processed = tx.open_table(PROCESSED).map_err(table_error)?;
                processed
                    .insert((consumer_name.as_str(), event_id.as_str()), ())
                    .map_err(storage_error)?;
            }
            tx.commit().map_err(commit_error)
        })
        .await
    }
}

impl ProjectionCheckpointStore for EmbeddedKernelStore {
    async fn load_checkpoint(
        &self,
        consumer_name: &str,
        stream_name: &str,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        let consumer_name = consumer_name.to_string();
        let stream_name = stream_name.to_string();
        self.run(move |store| {
            let tx = store.begin_read()?;
            let checkpoints = tx.open_table(CHECKPOINTS).map_err(table_error)?;
            match checkpoints
                .get((consumer_name.as_str(), stream_name.as_str()))
                .map_err(storage_error)?
            {
                Some(guard) => Ok(Some(
                    decode::<CheckpointRecord>("projection checkpoint", guard.value())?.into(),
                )),
                None => Ok(None),
            }
        })
        .await
    }

    async fn save_checkpoint(&self, checkpoint: ProjectionCheckpoint) -> Result<(), PortError> {
        self.run(move |store| {
            let tx = store.begin_write()?;
            {
                let key = (
                    checkpoint.consumer_name.clone(),
                    checkpoint.stream_name.clone(),
                );
                let bytes = encode("projection checkpoint", &CheckpointRecord::from(checkpoint))?;
                let mut checkpoints = tx.open_table(CHECKPOINTS).map_err(table_error)?;
                checkpoints
                    .insert((key.0.as_str(), key.1.as_str()), bytes.as_slice())
                    .map_err(storage_error)?;
            }
            tx.commit().map_err(commit_error)
        })
        .await
    }
}
