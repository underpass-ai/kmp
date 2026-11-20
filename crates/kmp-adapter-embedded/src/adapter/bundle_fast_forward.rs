//! Catching a live store up with a committed bundle that is ahead of it.
//!
//! After a `git pull` or a fresh clone, `.kmp/memory.jsonl` holds events the
//! store has not seen. When the store's authored history is an exact prefix
//! of the bundle's, appending the bundle's tail in bundle order is exactly
//! what the writes that produced it did, so the result is the history the
//! teammate's store holds. Anything else is left alone and reported.
//!
//! The tail is appended in the bundle's global order, never stream by
//! stream: a per-about import of a multi-about tail would interleave the
//! abouts differently and leave a store whose history no longer equals the
//! bundle, which every later guarded write refuses.

use kmp_domain::{ContextUpdatedEvent, PortError, ProjectionMutation};

use super::about_import_source::SourceAbouts;
use super::context_events::append_in_transaction;
use super::engine::Table;
use super::history_relation::HistoryRelation;
use super::portability::{filter_events_excluding_abouts, parse_bundle};
use super::projection_write::apply_mutations_in_transaction;
use super::serdes::decode;
use super::store::EmbeddedKernelStore;

impl EmbeddedKernelStore {
    /// The async form of [`Self::fast_forward_to_bundle_blocking`], on the
    /// blocking pool.
    pub async fn fast_forward_to_bundle<F>(
        &self,
        bundle: String,
        excluded_abouts: Vec<String>,
        derive: F,
    ) -> Result<HistoryRelation, PortError>
    where
        F: Fn(&ContextUpdatedEvent) -> Result<Vec<ProjectionMutation>, PortError> + Send + 'static,
    {
        self.run(move |store| {
            store.fast_forward_to_bundle_blocking(&bundle, &excluded_abouts, derive)
        })
        .await
    }

    /// Compares the store's history, without `excluded_abouts`, with
    /// `bundle` (already projected through the same policy) and, when the
    /// bundle is strictly ahead, appends its tail. One write transaction:
    /// the verdict and the append happen under the same SQLite write lock, so
    /// no other writer can commit in between, and a refusal leaves the store
    /// exactly as it was.
    ///
    /// Returns the relation found *before* any append.
    pub fn fast_forward_to_bundle_blocking<F>(
        &self,
        bundle: &str,
        excluded_abouts: &[String],
        derive: F,
    ) -> Result<HistoryRelation, PortError>
    where
        F: Fn(&ContextUpdatedEvent) -> Result<Vec<ProjectionMutation>, PortError>,
    {
        let verified = parse_bundle(bundle)?;
        let abouts = verified.header.abouts.clone();
        let source_events = filter_events_excluding_abouts(verified.events, excluded_abouts);

        let mut tx = self.begin_write()?;
        let destination = tx
            .scan_u64(Table::EventLog)?
            .into_iter()
            .map(|(_, raw)| decode::<ContextUpdatedEvent>("destination event", &raw))
            .collect::<Result<Vec<_>, _>>()?;
        let authored = filter_events_excluding_abouts(destination, excluded_abouts);
        let relation = HistoryRelation::classify(&authored, &source_events);
        let HistoryRelation::BundleAhead { store_events, .. } = relation else {
            return Ok(relation);
        };

        // The tail is derived and checked as an about import would check it:
        // every payload projects, no idempotency key already answers for
        // another write here, and no relation points at a node nobody holds.
        let abouts = abouts
            .into_iter()
            .filter(|about| !excluded_abouts.contains(about))
            .collect::<Vec<_>>();
        let source = SourceAbouts::read(bundle, &abouts, derive)?;
        if source.events() != source_events.as_slice() {
            return Err(PortError::InvalidState(
                "committed bundle changed its own order while it was read; nothing was written"
                    .to_string(),
            ));
        }
        let tail = (store_events as usize..source.events().len()).collect::<Vec<_>>();
        source.refuse_idempotency_collisions(tx.as_ref(), &tail)?;
        source.refuse_dangling_relations(tx.as_ref(), &tail)?;
        for position in &tail {
            let event = source.events()[*position].clone();
            let recorded = event.revision;
            let assigned = append_in_transaction(tx.as_mut(), event, recorded - 1)?;
            if assigned != recorded {
                return Err(PortError::Conflict(format!(
                    "fast-forward integrity violation: assigned revision {assigned}, the \
                     committed bundle recorded {recorded}; nothing was written"
                )));
            }
            apply_mutations_in_transaction(tx.as_mut(), source.mutations(*position).to_vec())?;
        }
        tx.commit()?;
        Ok(relation)
    }
}
