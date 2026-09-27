use std::path::{Path, PathBuf};
use std::sync::Arc;

use kmp_domain::{GraphNeighborhoodReader, PortError};
use kmp_embedded::EmbeddedKernelStore;
use kmp_proto_mapping::v1beta1::{LexicalObservation, LexicalShadowWitness};

use super::catch_up_report::CatchUpReport;
use super::index_limits::IndexLimits;
use super::indexed_parts::IndexedParts;
use super::indexed_plan::IndexedPlan;
pub(crate) use super::indexed_read::IndexedRead;
use super::lexical_maintainer::LexicalMaintainer;
use super::shadow_comparison::ShadowComparison;
use super::shadow_report::ShadowReport;
pub(crate) use super::shadow_scope::ShadowScope;
use super::sqlite_lexical_sidecar::SqliteLexicalSidecar;
use crate::serving::lexical_index_mode::LexicalIndexMode;

/// The file the sidecar lives in, beside the store and outside `store/`,
/// whose format gate refuses files it does not know.
pub(crate) const LEXICAL_INDEX_FILE: &str = "lexical-index.sqlite3";

/// The depth an ask reads an about at by default, which is what the
/// sidecar indexes.
pub(crate) const LEXICAL_ASK_DEPTH: u8 = super::about_rebuild::ASK_DEPTH;

/// The lexical index as the embedded backend holds it (DESIGN L6): in shadow,
/// followed after every write and before every ask, compared with every ask,
/// never answering one. Every failure is logged and leaves the ask and the
/// write exactly as they would be without it.
pub(crate) struct LexicalSidecar {
    sidecar: Option<Arc<SqliteLexicalSidecar>>,
    maintainer: Option<Arc<LexicalMaintainer>>,
    mode: LexicalIndexMode,
    limits: IndexLimits,
    /// MaxScore against the floor (P14).
    prune: bool,
}

pub(crate) fn lexical_index_path(data_dir: &Path) -> PathBuf {
    data_dir.join(LEXICAL_INDEX_FILE)
}

impl LexicalSidecar {
    /// Opens the sidecar beside the store when `mode` asks for it. Only `on`
    /// keeps small abouts unindexed; `shadow` and `verify` measure every about.
    pub(crate) fn open(data_dir: &Path, mode: LexicalIndexMode, limits: IndexLimits) -> Self {
        let limits = if mode == LexicalIndexMode::On {
            limits
        } else {
            limits.every_about()
        };
        if !mode.is_open() {
            return Self::disabled();
        }
        match SqliteLexicalSidecar::open(&lexical_index_path(data_dir)) {
            Ok(sidecar) => {
                let sidecar = Arc::new(sidecar);
                Self {
                    maintainer: Some(Arc::new(
                        LexicalMaintainer::new(Arc::clone(&sidecar))
                            .with_min_about_entries(limits.min_about_entries),
                    )),
                    sidecar: Some(sidecar),
                    mode,
                    limits,
                    prune: crate::serving::environment::lexical_maxscore(),
                }
            }
            Err(error) => {
                tracing::warn!(target: "kmp_mcp::lexical_index", %error, "lexical index disabled");
                Self::disabled()
            }
        }
    }

    pub(crate) fn disabled() -> Self {
        Self {
            sidecar: None,
            maintainer: None,
            mode: LexicalIndexMode::Off,
            limits: IndexLimits::DEFAULT,
            prune: false,
        }
    }

    /// Follows the store's log to its end, building `ensure` if it is not
    /// built. `None` when the sidecar is off or could not follow.
    pub(crate) async fn catch_up(
        &self,
        store: &EmbeddedKernelStore,
        ensure: Option<&str>,
    ) -> Option<CatchUpReport> {
        self.maintainer.as_ref()?;
        let mut outcome = self.follow(store, ensure).await;
        // Another call or process moved the sidecar first: what it wrote is
        // read, and what is left to follow is followed, once.
        if matches!(&outcome, Ok(report) if !report.committed) {
            outcome = self.follow(store, ensure).await;
        }
        match outcome {
            Ok(report) => {
                tracing::debug!(
                    target: "kmp_mcp::lexical_index",
                    event = "kmp_lexical_catch_up",
                    position = report.position,
                    events = report.events,
                    abouts_refreshed = report.abouts_refreshed,
                    abouts_rebuilt = report.abouts_rebuilt,
                    rows = report.rows,
                    reset = report.reset,
                    committed = report.committed,
                    elapsed_us = report.elapsed_us,
                    "lexical index followed the log"
                );
                Some(report)
            }
            Err(error) => {
                tracing::warn!(target: "kmp_mcp::lexical_index", %error, "lexical index could not follow the log");
                None
            }
        }
    }

    async fn follow(
        &self,
        store: &EmbeddedKernelStore,
        ensure: Option<&str>,
    ) -> Result<CatchUpReport, PortError> {
        let Some(maintainer) = self.maintainer.as_ref().map(Arc::clone) else {
            return Err(PortError::Unavailable("lexical index is off".into()));
        };
        let ensure = ensure.map(str::to_string);
        store
            .read_points(move |reads| {
                maintainer
                    .catch_up(reads, ensure.as_deref())
                    .map_err(PortError::Unavailable)
            })
            .await
    }

    /// Where an ask's ranker leaves what it measured, while the sidecar is
    /// in shadow.
    pub(crate) fn witness(&self) -> Option<Arc<LexicalShadowWitness>> {
        self.sidecar
            .as_ref()
            .filter(|_| self.mode.shadows())
            .map(|_| Arc::default())
    }

    /// What the process does with the sidecar.
    pub(crate) fn mode(&self) -> LexicalIndexMode {
        self.mode
    }

    /// Reads the part of `query`'s about the postings of its words reach,
    /// and the whole about's statistics to rank it against (DESIGN L6, P13).
    /// `Ok(Err(why))` when the ask is not one the index can hold; it then
    /// reads the about. `followed` is the catch-up that preceded the ask:
    /// the read must stand where it left the sidecar. A `deeper` ask is
    /// held only while nothing lies past the indexed depth.
    pub(crate) async fn indexed_read(
        &self,
        store: &EmbeddedKernelStore,
        service: &kmp_embedded::EmbeddedMemoryService,
        query: &kmp_application::memory::AskMemoryQuery,
        followed: Option<&CatchUpReport>,
        bridge: &kmp_proto_mapping::v1beta1::LexicalBridge,
        deeper: bool,
    ) -> Result<Result<IndexedRead, &'static str>, String> {
        let Some(sidecar) = self.sidecar.as_ref().map(Arc::clone) else {
            return Ok(Err("the index is closed"));
        };
        if followed.is_some_and(|report| report.below_threshold) {
            return Ok(Err("the about is below the index's size threshold"));
        }
        let Some(position) = followed
            .filter(|report| report.committed)
            .map(|report| report.position)
        else {
            return Ok(Err("the index did not follow the log"));
        };
        let about = query.about.clone();
        let question = query.question.clone();
        let started = std::time::Instant::now();
        let planned = {
            let sidecar = Arc::clone(&sidecar);
            let about = about.clone();
            let bridge = bridge.clone();
            // `verify` measures every ask the index can hold, however many
            // candidates it reaches; `on` answers only those it saves on.
            let bounded = (self.mode == LexicalIndexMode::On).then_some(self.limits);
            let prune = self.prune;
            tokio::task::spawn_blocking(move || {
                IndexedPlan::read(&sidecar, &about, &question, &bridge, bounded, deeper, prune)
            })
            .await
            .map_err(|error| error.to_string())??
        };
        let plan = match planned {
            Ok(plan) => plan,
            Err(why) => return Ok(Err(why)),
        };
        let candidates = plan.candidates.clone();
        let plan_us = started.elapsed().as_micros() as u64;
        let parts = store
            .read_points(move |reads| {
                if reads.last_event_sequence()? != position {
                    return Ok(None);
                }
                IndexedParts::new(reads, &sidecar, &about)
                    .read(&candidates)
                    .map_err(PortError::Unavailable)
            })
            .await
            .map_err(|error| error.to_string())?;
        let Some(mut parts) = parts else {
            return Ok(Err("the store moved"));
        };
        parts.read_revision = store
            .graph_read_revision()
            .await
            .map_err(|error| error.to_string())?;
        let result = service
            .ask_from_parts(query, parts, kmp_application::RenderDemand::Skip)
            .await
            .map_err(|error| error.to_string())?;
        Ok(Ok(IndexedRead {
            result,
            indexed: plan.indexed,
            reached: plan.reached,
            candidates: plan.candidates.len(),
            documents: plan.documents,
            plan_us,
            parts_us: started.elapsed().as_micros() as u64 - plan_us,
        }))
    }

    /// Leaves MaxScore on or off for this sidecar, whatever the environment
    /// says (tests that compare both in one process).
    #[cfg(test)]
    pub(crate) fn with_maxscore(mut self, prune: bool) -> Self {
        self.prune = prune;
        self
    }

    /// Compares the sidecar with what an ask measured and logs the result.
    /// `read` says how the ask's read relates to what the sidecar indexes; `before`
    /// is the catch-up that preceded the ask. An about found to differ is
    /// forgotten, so the next ask builds it again.
    pub(crate) async fn shadow(
        &self,
        store: &EmbeddedKernelStore,
        about: &str,
        observation: Option<LexicalObservation>,
        read: ShadowScope,
        before: Option<CatchUpReport>,
    ) -> Option<ShadowReport> {
        let sidecar = Arc::clone(self.sidecar.as_ref()?);
        let report = match (read, observation, before) {
            (ShadowScope::Other, _, _) => ShadowReport::not_comparable("not the indexed read"),
            (_, None, _) => ShadowReport::not_comparable("no ranking"),
            (_, _, None) => ShadowReport::not_comparable("sidecar not followed"),
            (_, _, Some(before)) if !before.committed => {
                ShadowReport::not_comparable("another process moved the sidecar")
            }
            (read, Some(observation), Some(before)) => {
                let last = store.read_points(|reads| reads.last_event_sequence()).await;
                if last.ok() != Some(before.position) {
                    ShadowReport::not_comparable("the store moved")
                } else {
                    let about = about.to_string();
                    let compared = tokio::task::spawn_blocking(move || {
                        let comparison = ShadowComparison::new(&sidecar);
                        let report = if comparison.reads_past(&about, read.deeper())? {
                            ShadowReport::not_comparable("the ask reads past the index")
                        } else if matches!(read, ShadowScope::Selection { .. }) {
                            comparison.compare_selection(&about, &observation)?
                        } else {
                            comparison.compare(&about, &observation)?
                        };
                        if report.differences() > 0 {
                            sidecar.forget(&about)?;
                        }
                        Ok::<_, String>(report)
                    })
                    .await
                    .map_err(|error| error.to_string())
                    .and_then(|compared| compared);
                    match compared {
                        Ok(report) => report,
                        Err(error) => {
                            tracing::warn!(target: "kmp_mcp::lexical_index", %error, "lexical index could not be compared");
                            return None;
                        }
                    }
                }
            }
        };
        report.emit(about);
        Some(report)
    }
}
