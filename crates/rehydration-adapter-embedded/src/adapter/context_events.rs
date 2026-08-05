use redb::ReadableTable;
use rehydration_domain::{ContextEventStore, ContextUpdatedEvent, IdempotentOutcome, PortError};

use super::serdes::{AggregateRecord, decode, encode};
use super::store::{
    AGGREGATES, EVENT_LOG, EmbeddedKernelStore, IDEMPOTENCY, aggregate_key, commit_error,
    range_error, storage_error, table_error,
};

impl ContextEventStore for EmbeddedKernelStore {
    async fn append(
        &self,
        event: ContextUpdatedEvent,
        expected_revision: u64,
    ) -> Result<u64, PortError> {
        self.run(move |store| {
            let tx = store.begin_write()?;
            let new_revision;
            {
                let mut aggregates = tx.open_table(AGGREGATES).map_err(table_error)?;
                let key = aggregate_key(&event.root_node_id, &event.role);
                let current = match aggregates.get(key.as_str()).map_err(storage_error)? {
                    Some(guard) => {
                        decode::<AggregateRecord>("aggregate head", guard.value())?.revision
                    }
                    None => 0,
                };
                if current != expected_revision {
                    return Err(PortError::Conflict(format!(
                        "expected revision {expected_revision}, current is {current}"
                    )));
                }
                new_revision = current + 1;

                // Stamp the assigned revision on the stored event so replay
                // derives projections with the same revision the aggregate
                // recorded.
                let mut event = event;
                event.revision = new_revision;

                let aggregate_bytes = encode(
                    "aggregate head",
                    &AggregateRecord {
                        revision: new_revision,
                        content_hash: event.content_hash.clone(),
                    },
                )?;
                aggregates
                    .insert(key.as_str(), aggregate_bytes.as_slice())
                    .map_err(storage_error)?;

                let mut log = tx.open_table(EVENT_LOG).map_err(table_error)?;
                let next_sequence = match log.last().map_err(storage_error)? {
                    Some((key, _)) => key.value() + 1,
                    None => 1,
                };
                let event_bytes = encode("context event", &event)?;
                log.insert(next_sequence, event_bytes.as_slice())
                    .map_err(storage_error)?;

                if let Some(idempotency_key) = event.idempotency_key.as_deref() {
                    let outcome_bytes = encode(
                        "idempotency outcome",
                        &IdempotentOutcome {
                            revision: new_revision,
                            content_hash: event.content_hash.clone(),
                            logical_digest: event.logical_digest.clone(),
                        },
                    )?;
                    let mut idempotency = tx.open_table(IDEMPOTENCY).map_err(table_error)?;
                    idempotency
                        .insert(idempotency_key, outcome_bytes.as_slice())
                        .map_err(storage_error)?;
                }
            }
            tx.commit().map_err(commit_error)?;
            Ok(new_revision)
        })
        .await
    }

    async fn current_revision(&self, root_node_id: &str, role: &str) -> Result<u64, PortError> {
        let key = aggregate_key(root_node_id, role);
        self.run(move |store| {
            let tx = store.begin_read()?;
            let aggregates = tx.open_table(AGGREGATES).map_err(table_error)?;
            match aggregates.get(key.as_str()).map_err(storage_error)? {
                Some(guard) => {
                    Ok(decode::<AggregateRecord>("aggregate head", guard.value())?.revision)
                }
                None => Ok(0),
            }
        })
        .await
    }

    async fn current_content_hash(
        &self,
        root_node_id: &str,
        role: &str,
    ) -> Result<Option<String>, PortError> {
        let key = aggregate_key(root_node_id, role);
        self.run(move |store| {
            let tx = store.begin_read()?;
            let aggregates = tx.open_table(AGGREGATES).map_err(table_error)?;
            match aggregates.get(key.as_str()).map_err(storage_error)? {
                Some(guard) => Ok(Some(
                    decode::<AggregateRecord>("aggregate head", guard.value())?.content_hash,
                )),
                None => Ok(None),
            }
        })
        .await
    }

    async fn find_by_idempotency_key(
        &self,
        key: &str,
    ) -> Result<Option<IdempotentOutcome>, PortError> {
        let key = key.to_string();
        self.run(move |store| {
            let tx = store.begin_read()?;
            let idempotency = tx.open_table(IDEMPOTENCY).map_err(table_error)?;
            match idempotency.get(key.as_str()).map_err(storage_error)? {
                Some(guard) => Ok(Some(decode("idempotency outcome", guard.value())?)),
                None => Ok(None),
            }
        })
        .await
    }
}

impl EmbeddedKernelStore {
    /// Reads the full append-only event log in sequence order (audit and
    /// replay surface).
    pub(crate) fn read_event_log(&self) -> Result<Vec<ContextUpdatedEvent>, PortError> {
        let tx = self.begin_read()?;
        let log = tx.open_table(EVENT_LOG).map_err(table_error)?;
        let mut events = Vec::new();
        for row in log.iter().map_err(range_error)? {
            let (_, value) = row.map_err(range_error)?;
            events.push(decode("context event", value.value())?);
        }
        Ok(events)
    }
}
