use kmp_domain::{ContextUpdatedEvent, PortError, ProjectionMutation};

use super::engine::Table;
use super::projection_write::apply_mutations_in_transaction;
use super::store::EmbeddedKernelStore;

/// Outcome of a projection rebuild from the append-only event log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionRebuildReport {
    pub events_replayed: u64,
    pub mutations_applied: u64,
}

impl EmbeddedKernelStore {
    /// Drops every projection table and rebuilds them by replaying the event
    /// log in sequence order — the recovery and migration story in one.
    ///
    /// The mutation derivation is injected so this adapter stays free of
    /// application-layer dependencies; the composition root passes
    /// `kmp_application::projection_mutations_for_context_event`.
    /// The whole rebuild is one transaction: a crash mid-rebuild leaves the
    /// previous projections intact.
    pub async fn rebuild_projections<F>(
        &self,
        derive: F,
    ) -> Result<ProjectionRebuildReport, PortError>
    where
        F: Fn(&ContextUpdatedEvent) -> Result<Vec<ProjectionMutation>, PortError> + Send + 'static,
    {
        let events = self.run(EmbeddedKernelStore::read_event_log).await?;

        let mut mutations = Vec::new();
        for event in &events {
            mutations.extend(derive(event)?);
        }
        let events_replayed = events.len() as u64;

        self.run(move |store| {
            let mut tx = store.begin_write()?;
            tx.clear(Table::Nodes)?;
            tx.clear(Table::Relations)?;
            tx.clear(Table::RelationsByTarget)?;
            tx.clear(Table::Details)?;
            tx.clear(Table::Anchors)?;

            let mutations_applied = apply_mutations_in_transaction(tx.as_mut(), mutations)?;
            tx.commit()?;

            Ok(ProjectionRebuildReport {
                events_replayed,
                mutations_applied,
            })
        })
        .await
    }
}
