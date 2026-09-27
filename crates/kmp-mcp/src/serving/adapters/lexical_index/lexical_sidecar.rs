use std::path::{Path, PathBuf};
use std::sync::Arc;

use kmp_domain::PortError;
use kmp_embedded::EmbeddedKernelStore;
use kmp_proto_mapping::v1beta1::{LexicalObservation, LexicalShadowWitness};

use super::catch_up_report::CatchUpReport;
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
}

pub(crate) fn lexical_index_path(data_dir: &Path) -> PathBuf {
    data_dir.join(LEXICAL_INDEX_FILE)
}

impl LexicalSidecar {
    /// Opens the sidecar beside the store when `mode` asks for it.
    pub(crate) fn open(data_dir: &Path, mode: LexicalIndexMode) -> Self {
        if !mode.is_open() {
            return Self::disabled();
        }
        match SqliteLexicalSidecar::open(&lexical_index_path(data_dir)) {
            Ok(sidecar) => {
                let sidecar = Arc::new(sidecar);
                Self {
                    maintainer: Some(Arc::new(LexicalMaintainer::new(Arc::clone(&sidecar)))),
                    sidecar: Some(sidecar),
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

    /// Where an ask's ranker leaves what it measured, while the sidecar is on.
    pub(crate) fn witness(&self) -> Option<Arc<LexicalShadowWitness>> {
        self.sidecar.as_ref().map(|_| Arc::default())
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
