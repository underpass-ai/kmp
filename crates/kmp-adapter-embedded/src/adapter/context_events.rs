use kmp_domain::{ContextEventStore, ContextUpdatedEvent, IdempotentOutcome, PortError};

use super::engine::{Key, Table};
use super::serdes::{AggregateRecord, decode, encode};
use super::store::{EmbeddedKernelStore, aggregate_key};

impl ContextEventStore for EmbeddedKernelStore {
    async fn append(
        &self,
        event: ContextUpdatedEvent,
        expected_revision: u64,
    ) -> Result<u64, PortError> {
        self.run(move |store| {
            let mut tx = store.begin_write()?;

            let key = aggregate_key(&event.root_node_id, &event.role);
            let current = match tx.get(Table::Aggregates, Key::Str(&key))? {
                Some(raw) => decode::<AggregateRecord>("aggregate head", &raw)?.revision,
                None => 0,
            };
            if current != expected_revision {
                return Err(PortError::Conflict(format!(
                    "expected revision {expected_revision}, current is {current}"
                )));
            }
            let new_revision = current + 1;

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
            tx.insert(Table::Aggregates, Key::Str(&key), &aggregate_bytes)?;

            let next_sequence = tx
                .last_u64(Table::EventLog)?
                .map_or(1, |(sequence, _)| sequence + 1);
            let event_bytes = encode("context event", &event)?;
            tx.insert(Table::EventLog, Key::U64(next_sequence), &event_bytes)?;

            if let Some(idempotency_key) = event.idempotency_key.as_deref() {
                let outcome_bytes = encode(
                    "idempotency outcome",
                    &IdempotentOutcome {
                        revision: new_revision,
                        content_hash: event.content_hash.clone(),
                        logical_digest: event.logical_digest.clone(),
                    },
                )?;
                tx.insert(
                    Table::Idempotency,
                    Key::Str(idempotency_key),
                    &outcome_bytes,
                )?;
            }

            tx.commit()?;
            Ok(new_revision)
        })
        .await
    }

    async fn current_revision(&self, root_node_id: &str, role: &str) -> Result<u64, PortError> {
        let key = aggregate_key(root_node_id, role);
        self.run(move |store| {
            let tx = store.begin_read()?;
            match tx.get(Table::Aggregates, Key::Str(&key))? {
                Some(raw) => Ok(decode::<AggregateRecord>("aggregate head", &raw)?.revision),
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
            match tx.get(Table::Aggregates, Key::Str(&key))? {
                Some(raw) => Ok(Some(
                    decode::<AggregateRecord>("aggregate head", &raw)?.content_hash,
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
            match tx.get(Table::Idempotency, Key::Str(&key))? {
                Some(raw) => Ok(Some(decode("idempotency outcome", &raw)?)),
                None => Ok(None),
            }
        })
        .await
    }
}

impl EmbeddedKernelStore {
    /// The event log, read synchronously. The migration path uses this
    /// before any runtime exists around the source copy.
    pub(crate) fn read_event_log_blocking(&self) -> Result<Vec<ContextUpdatedEvent>, PortError> {
        self.read_event_log()
    }

    /// Reads the full append-only event log in sequence order (audit and
    /// replay surface).
    pub(crate) fn read_event_log(&self) -> Result<Vec<ContextUpdatedEvent>, PortError> {
        let tx = self.begin_read()?;
        tx.scan_u64(Table::EventLog)?
            .into_iter()
            .map(|(_, raw)| decode("context event", &raw))
            .collect()
    }
}
