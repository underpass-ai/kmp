//! Seed the installed guide into an embedded store the first time a guide
//! read finds it missing.
//!
//! Setup installs assets and never writes memory, and that stays true: what
//! writes here is the guide read itself, on the store it was asked about,
//! through the same `kmp_ingest` the explicit `guide sync` uses. One attempt
//! per session: a store that still has no guide afterwards gets the explicit
//! repair, with every place that was searched.
use std::sync::atomic::Ordering;

use super::KernelMcpServer;
use crate::guide::GuideAssetLocator;
use crate::lifecycle::domain::release_version::ReleaseVersion;

/// What the one seeding attempt of this session did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GuideSeedOutcome {
    /// Both guide abouts were written from the assets at `root`.
    Seeded { root: String },
    /// No matching assets, or the write failed; `searched` says where it
    /// looked and why each place did not serve.
    Unavailable { searched: Vec<String> },
    /// A remote or fixture backend keeps the explicit sync: a shared kernel
    /// is nobody's to seed from one client's plugin tree.
    NotEmbedded,
}

impl GuideSeedOutcome {
    pub(crate) fn seeded(&self) -> bool {
        matches!(self, Self::Seeded { .. })
    }

    /// The lines a repair message carries about this attempt.
    pub(crate) fn searched(&self) -> Vec<String> {
        match self {
            Self::Seeded { root } => vec![format!("seeded from {root}")],
            Self::Unavailable { searched } => searched.clone(),
            Self::NotEmbedded => vec!["not attempted: this backend is not embedded".to_string()],
        }
    }
}

impl KernelMcpServer {
    /// Seed the guide once for this session and say what happened. A second
    /// call reports the first attempt instead of searching again.
    pub(crate) async fn seed_installed_guide(&self) -> GuideSeedOutcome {
        if let Some(outcome) = self.guide_seed.get() {
            return outcome.clone();
        }
        if self.embedded_engine.is_none() {
            return self.record_guide_seed(GuideSeedOutcome::NotEmbedded);
        }
        if self.guide_seed_attempted.swap(true, Ordering::SeqCst) {
            // Another call of this session is seeding right now; it will
            // record the outcome. This caller retries its read either way.
            return GuideSeedOutcome::Unavailable {
                searched: vec!["a seed is already running in this session".to_string()],
            };
        }
        let version = ReleaseVersion::current();
        let mut search = GuideAssetLocator::from_environment(&version).locate(&version);
        let Some(root) = search.root.take() else {
            return self.record_guide_seed(GuideSeedOutcome::Unavailable {
                searched: search.searched,
            });
        };
        let outcome = match crate::guide::load_assets(&root) {
            // Boxed: the ingest goes through this server's own dispatch, which
            // is what called here, and an async cycle needs one indirection.
            Ok(requests) => {
                match Box::pin(crate::guide::ingest_through(self, &requests, "guide")).await {
                    Ok(()) => GuideSeedOutcome::Seeded {
                        root: root.display().to_string(),
                    },
                    Err(error) => {
                        search
                            .searched
                            .push(format!("seeding from {} failed: {error}", root.display()));
                        GuideSeedOutcome::Unavailable {
                            searched: search.searched,
                        }
                    }
                }
            }
            Err(error) => {
                search
                    .searched
                    .push(format!("{} could not be loaded: {error}", root.display()));
                GuideSeedOutcome::Unavailable {
                    searched: search.searched,
                }
            }
        };
        if let GuideSeedOutcome::Seeded { root } = &outcome {
            tracing::info!(root = %root, "seeded the installed guide into this store");
            eprintln!("kmp-mcp: installed the guide into this store from {root}");
        }
        self.record_guide_seed(outcome)
    }

    fn record_guide_seed(&self, outcome: GuideSeedOutcome) -> GuideSeedOutcome {
        let _ = self.guide_seed.set(outcome.clone());
        outcome
    }
}
