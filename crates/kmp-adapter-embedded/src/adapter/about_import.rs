//! `import --from --about`: incorporate exact abouts of another memory into
//! this live store, all or nothing, and idempotently.
//!
//! The whole operation is one write transaction. The destination's log is
//! read, compared, checked and extended under the same SQLite write lock, so
//! no other writer can commit between the verdict and the append, and a
//! refusal or a crash leaves the destination exactly as it was.

use kmp_domain::{ContextUpdatedEvent, PortError, ProjectionMutation};

use super::about_import_plan::AboutImportPlan;
use super::about_import_report::AboutImportReport;
use super::about_import_source::SourceAbouts;
use super::context_events::append_in_transaction;
use super::engine::Table;
use super::projection_write::apply_mutations_in_transaction;
use super::serdes::decode;
use super::store::EmbeddedKernelStore;

impl EmbeddedKernelStore {
    /// Incorporates the events of `requested_abouts` from a verified bundle.
    ///
    /// Abouts are opaque and matched exactly. Every requested about must be
    /// in the bundle. Per `(about, role)` stream: absent here → replayed;
    /// identical (same revisions and content hashes) → left alone; an exact
    /// prefix here → the missing tail is appended on the revisions the
    /// source recorded; anything else is refused. The import is also refused
    /// when an appended event's idempotency key already answers for another
    /// write here, or when an appended relation would point at a node that
    /// neither the imported abouts nor this store hold — the projection
    /// would otherwise invent a placeholder for it.
    ///
    /// Projections are applied with each appended event inside the same
    /// transaction, as a live write does; appended events land at the end of
    /// the log, so a later full rebuild reproduces exactly these projections.
    pub async fn import_abouts<F>(
        &self,
        bundle: &str,
        requested_abouts: &[String],
        derive: F,
    ) -> Result<AboutImportReport, PortError>
    where
        F: Fn(&ContextUpdatedEvent) -> Result<Vec<ProjectionMutation>, PortError>,
    {
        let source = SourceAbouts::read(bundle, requested_abouts, derive)?;
        let requested = requested_abouts.to_vec();
        self.run(move |store| {
            let mut tx = store.begin_write()?;
            let destination = tx
                .scan_u64(Table::EventLog)?
                .into_iter()
                .map(|(_, raw)| decode::<ContextUpdatedEvent>("destination event", &raw))
                .collect::<Result<Vec<_>, _>>()?;
            let plan = AboutImportPlan::compare(&requested, source.events(), &destination)?;
            source.refuse_idempotency_collisions(tx.as_ref(), plan.appended())?;
            source.refuse_dangling_relations(tx.as_ref(), plan.appended())?;

            let mut mutations_applied = 0;
            for position in plan.appended() {
                let event = source.events()[*position].clone();
                let recorded = event.revision;
                let assigned = append_in_transaction(tx.as_mut(), event, recorded - 1)?;
                if assigned != recorded {
                    return Err(PortError::Conflict(format!(
                        "import integrity violation: assigned revision {assigned}, source \
                         recorded {recorded}; nothing was written"
                    )));
                }
                mutations_applied += apply_mutations_in_transaction(
                    tx.as_mut(),
                    source.mutations(*position).to_vec(),
                )?;
            }
            tx.commit()?;
            Ok(AboutImportReport {
                events_imported: plan.appended().len() as u64,
                mutations_applied,
                abouts: plan.into_outcomes(),
            })
        })
        .await
    }
}
