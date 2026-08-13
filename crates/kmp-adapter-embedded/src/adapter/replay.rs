use kmp_domain::{ContextUpdatedEvent, PortError, ProjectionMutation};

use super::projection_write::apply_mutations_in_transaction;
use super::store::{
    ANCHORS, DETAILS, EmbeddedKernelStore, NODES, RELATIONS, RELATIONS_BY_TARGET, commit_error,
    table_error,
};

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
            let tx = store.begin_write()?;
            tx.delete_table(NODES).map_err(table_error)?;
            tx.delete_table(RELATIONS).map_err(table_error)?;
            tx.delete_table(RELATIONS_BY_TARGET).map_err(table_error)?;
            tx.delete_table(DETAILS).map_err(table_error)?;
            tx.delete_table(ANCHORS).map_err(table_error)?;

            let mutations_applied = apply_mutations_in_transaction(&tx, mutations)?;
            tx.commit().map_err(commit_error)?;

            Ok(ProjectionRebuildReport {
                events_replayed,
                mutations_applied,
            })
        })
        .await
    }
}
