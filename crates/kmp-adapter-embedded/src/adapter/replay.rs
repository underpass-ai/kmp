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
        self.run(move |store| {
            // Freeze the event frontier under the same write lock as the
            // rebuild. A concurrent condense cannot commit between the read
            // of the log and replacement of its card projections.
            let mut tx = store.begin_write()?;
            let events = tx.scan_u64(Table::EventLog)?;
            let events_replayed = events.len() as u64;
            tx.clear(Table::Nodes)?;
            tx.clear(Table::Relations)?;
            tx.clear(Table::RelationsByTarget)?;
            tx.clear(Table::Details)?;
            // Cleared and rebuilt with Details, in this same transaction: the
            // two tables are never observable out of step.
            tx.clear(Table::DetailHeaders)?;
            tx.clear(Table::Anchors)?;
            tx.clear(Table::Cards)?;
            tx.clear(Table::CardVersions)?;

            let mut mutations_applied = 0;
            for (_, raw) in events {
                let event = super::serdes::decode::<ContextUpdatedEvent>("replay event", &raw)?;
                mutations_applied += apply_mutations_in_transaction(tx.as_mut(), derive(&event)?)?;
            }
            tx.commit()?;

            Ok(ProjectionRebuildReport {
                events_replayed,
                mutations_applied,
            })
        })
        .await
    }
}
