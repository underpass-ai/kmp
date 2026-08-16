use kmp_domain::{PortError, ProcessedEventStore, ProjectionCheckpoint, ProjectionCheckpointStore};

use super::engine::{Key, Table};
use super::serdes::{CheckpointRecord, decode, encode};
use super::store::EmbeddedKernelStore;

impl ProcessedEventStore for EmbeddedKernelStore {
    async fn has_processed(&self, consumer_name: &str, event_id: &str) -> Result<bool, PortError> {
        let consumer_name = consumer_name.to_string();
        let event_id = event_id.to_string();
        self.run(move |store| {
            let tx = store.begin_read()?;
            Ok(tx
                .get(Table::Processed, Key::Str2(&consumer_name, &event_id))?
                .is_some())
        })
        .await
    }

    async fn record_processed(&self, consumer_name: &str, event_id: &str) -> Result<(), PortError> {
        let consumer_name = consumer_name.to_string();
        let event_id = event_id.to_string();
        self.run(move |store| {
            let mut tx = store.begin_write()?;
            tx.insert(Table::Processed, Key::Str2(&consumer_name, &event_id), &[])?;
            tx.commit()
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
            match tx.get(Table::Checkpoints, Key::Str2(&consumer_name, &stream_name))? {
                Some(raw) => Ok(Some(
                    decode::<CheckpointRecord>("projection checkpoint", &raw)?.into(),
                )),
                None => Ok(None),
            }
        })
        .await
    }

    async fn save_checkpoint(&self, checkpoint: ProjectionCheckpoint) -> Result<(), PortError> {
        self.run(move |store| {
            let consumer_name = checkpoint.consumer_name.clone();
            let stream_name = checkpoint.stream_name.clone();
            let bytes = encode("projection checkpoint", &CheckpointRecord::from(checkpoint))?;
            let mut tx = store.begin_write()?;
            tx.insert(
                Table::Checkpoints,
                Key::Str2(&consumer_name, &stream_name),
                &bytes,
            )?;
            tx.commit()
        })
        .await
    }
}
