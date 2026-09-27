use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use kmp_domain::{ContextUpdatedEvent, GraphPointReads, ProjectionMutation};

use super::about_change::AboutChange;
use super::about_reader::AboutReader;
use super::about_rebuild::AboutRebuild;
use super::about_refresh::{AboutRefresh, Refreshed};
use super::catch_up_report::CatchUpReport;
use super::relation_key::RelationKey;
use super::sidecar_meta::SidecarMeta;
use super::sqlite_lexical_sidecar::SqliteLexicalSidecar;
use super::working_set::WorkingSet;

/// The derivation the sidecar's rows come from. A sidecar written by another
/// one is emptied and built again, never read. Bump it whenever what a row,
/// a node state or the selection reads changes; the golden test of the
/// derivation fails until it is.
pub(super) const INDEX_VERSION: &str = "lexical-index-4";

/// Keeps the sidecar at the end of the store's log (DESIGN L6).
///
/// An about is built the first time it is asked about; from then on every
/// event of the log is followed for it, whoever wrote the event. Following an
/// event reads the nodes and edges it touched and applies the difference, so
/// following it twice changes nothing. A sidecar of another derivation or
/// reading, or behind a log that was replaced, starts again from nothing.
pub(super) struct LexicalMaintainer {
    sidecar: Arc<SqliteLexicalSidecar>,
    /// The position and tail this process last found the log and the
    /// sidecar agreeing on: while both stand there, the tail event is not
    /// read and decoded again.
    verified: std::sync::Mutex<Option<(u64, String)>>,
    /// An about with fewer entries is not built on its first ask.
    min_about_entries: u64,
}

/// The readings every row carries: plain and with alias terms.
const PROFILE: &str = "plain+aliased";

/// What the events after the sidecar's position touched, per about.
#[derive(Default)]
struct Touched {
    nodes: BTreeSet<String>,
    relations: BTreeSet<RelationKey>,
}

impl LexicalMaintainer {
    pub(super) fn new(sidecar: Arc<SqliteLexicalSidecar>) -> Self {
        Self {
            sidecar,
            verified: std::sync::Mutex::new(None),
            min_about_entries: 0,
        }
    }

    /// Builds only abouts with at least `entries` entries on their first ask.
    pub(super) fn with_min_about_entries(mut self, entries: u64) -> Self {
        self.min_about_entries = entries;
        self
    }

    /// Whether `about` is too small to index: fewer entries (its `records`
    /// edges) than the threshold. Counted only for an about not built yet.
    fn below_threshold(&self, reads: &dyn GraphPointReads, about: &str) -> Result<bool, String> {
        if self.min_about_entries == 0 {
            return Ok(false);
        }
        let entries = reads
            .outgoing_count(about, super::relation_key::RECORDS)
            .map_err(port)?;
        Ok(entries < self.min_about_entries)
    }

    fn remember(&self, position: u64, tail: &str) {
        if let Ok(mut verified) = self.verified.lock() {
            *verified = Some((position, tail.to_string()));
        }
    }

    fn profile(&self) -> &'static str {
        PROFILE
    }

    /// Follows the log to its end over one snapshot and builds `ensure`
    /// when it is not built yet.
    pub(super) fn catch_up(
        &self,
        reads: &dyn GraphPointReads,
        ensure: Option<&str>,
    ) -> Result<CatchUpReport, String> {
        let started = std::time::Instant::now();
        let meta = self.sidecar.meta()?;
        let last = reads.last_event_sequence().map_err(port)?;
        let mut reset = !meta.matches(INDEX_VERSION, self.profile());
        let known = self
            .verified
            .lock()
            .ok()
            .and_then(|verified| verified.clone())
            .is_some_and(|(position, tail)| position == meta.position && tail == meta.tail);
        if !reset && meta.position > 0 && !(known && meta.position == last) {
            let tail = reads
                .event(meta.position)
                .map_err(port)?
                .map(|e| tail_of(&e));
            reset = meta.position > last || tail.as_deref() != Some(meta.tail.as_str());
        }
        let position = if reset { 0 } else { meta.position };
        let mut below_threshold = false;
        let ensure = match ensure {
            Some(about) if reset || self.sidecar.stats(about)?.is_none() => {
                below_threshold = self.below_threshold(reads, about)?;
                (!below_threshold).then_some(about)
            }
            _ => None,
        };
        let mut report = CatchUpReport {
            position: last,
            reset,
            below_threshold,
            ..CatchUpReport::default()
        };
        if !reset && position == last && ensure.is_none() {
            // Already at the end of the log: current, nothing to write.
            self.remember(position, &meta.tail);
            report.committed = true;
            report.elapsed_us = started.elapsed().as_micros() as u64;
            return Ok(report);
        }
        let mut changes = Vec::new();
        // With no about built there is nothing to follow: the position moves.
        if !reset && self.sidecar.holds_an_about()? {
            for (about, touched) in self.touched(reads, position, last, &mut report)? {
                if Some(about.as_str()) == ensure {
                    continue;
                }
                changes.push(self.refresh(reads, &about, &touched, &mut report)?);
            }
        }
        if let Some(about) = ensure {
            let reader = AboutReader::new(reads, about);
            changes.push(AboutRebuild::new(&reader).run()?);
            report.abouts_rebuilt += 1;
        }
        report.rows = changes.iter().map(|change| change.rows.len() as u64).sum();
        let next = SidecarMeta {
            version: INDEX_VERSION.to_string(),
            profile: self.profile().to_string(),
            position: last,
            tail: match reads.event(last).map_err(port)? {
                Some(event) => tail_of(&event),
                None => String::new(),
            },
        };
        report.committed = self.sidecar.commit(&meta, &next, reset, &changes)?;
        if report.committed {
            self.remember(next.position, &next.tail);
        }
        report.elapsed_us = started.elapsed().as_micros() as u64;
        Ok(report)
    }

    /// What the events after `position` touched, for the abouts already built.
    fn touched(
        &self,
        reads: &dyn GraphPointReads,
        position: u64,
        last: u64,
        report: &mut CatchUpReport,
    ) -> Result<BTreeMap<String, Touched>, String> {
        let mut built = BTreeMap::<String, bool>::new();
        let mut touched = BTreeMap::<String, Touched>::new();
        for sequence in position + 1..=last {
            let Some(event) = reads.event(sequence).map_err(port)? else {
                continue;
            };
            report.events += 1;
            let is_built = match built.get(&event.root_node_id) {
                Some(is_built) => *is_built,
                None => {
                    let is_built = self.sidecar.stats(&event.root_node_id)?.is_some();
                    built.insert(event.root_node_id.clone(), is_built);
                    is_built
                }
            };
            if !is_built {
                continue;
            }
            let mutations =
                kmp_application::projection_mutations_for_context_event(&event).map_err(port)?;
            let about = touched.entry(event.root_node_id.clone()).or_default();
            for mutation in mutations {
                seed(about, mutation);
            }
        }
        Ok(touched)
    }

    fn refresh(
        &self,
        reads: &dyn GraphPointReads,
        about: &str,
        touched: &Touched,
        report: &mut CatchUpReport,
    ) -> Result<AboutChange, String> {
        let reader = AboutReader::new(reads, about);
        let stats = self
            .sidecar
            .stats(about)?
            .ok_or_else(|| format!("lexical index: about `{about}` is not built"))?;
        let refresh = AboutRefresh::new(&reader, WorkingSet::new(&self.sidecar, about));
        match refresh.run(&stats, &touched.nodes, &touched.relations)? {
            Refreshed::Changed(change) => {
                report.abouts_refreshed += 1;
                Ok(change)
            }
            Refreshed::LanguageMoved => {
                report.abouts_rebuilt += 1;
                AboutRebuild::new(&reader).run()
            }
        }
    }
}

fn seed(touched: &mut Touched, mutation: ProjectionMutation) {
    match mutation {
        ProjectionMutation::RecordNodeCard(card) => {
            touched.nodes.insert(card.node_id);
        }
        ProjectionMutation::EnsureNode(node) | ProjectionMutation::UpsertNode(node) => {
            touched.nodes.insert(node.node_id);
        }
        ProjectionMutation::UpdateNodeStatus { node_id, .. } => {
            touched.nodes.insert(node_id);
        }
        ProjectionMutation::UpsertNodeRelation(relation) => {
            touched.relations.insert(RelationKey::of(&relation));
        }
        ProjectionMutation::RemoveNodeRelation {
            source_node_id,
            target_node_id,
            relation_type,
        } => {
            touched.relations.insert(RelationKey::new(
                &source_node_id,
                &target_node_id,
                &relation_type,
            ));
        }
        ProjectionMutation::UpsertNodeDetail(detail) => {
            touched.nodes.insert(detail.node_id);
        }
    }
}

/// What witnesses the event at the sidecar's position: a log replaced under
/// the same length does not carry the same event there.
fn tail_of(event: &ContextUpdatedEvent) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        event.root_node_id, event.revision, event.content_hash
    )
}

fn port(error: kmp_domain::PortError) -> String {
    format!("lexical index: {error}")
}
