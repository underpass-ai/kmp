use crate::application::use_cases::check_changelog::CheckChangelog;
use crate::application::use_cases::check_marketplace_contracts::CheckMarketplaceContracts;
use crate::application::use_cases::collect_version_sources::CollectVersionSources;
use crate::domain::plugin_repository::PluginRepository;
use crate::domain::readiness_check::ReadinessCheck;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_readiness::ReleaseReadiness;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::ports::release_contracts::ReleaseContracts;
use crate::ports::release_file_system::ReleaseFileSystem;
use crate::ports::release_workspace::ReleaseWorkspace;

/// Answers "is this tree ready to release X.Y.Z?" without building anything.
///
/// Every check here is knowable from the tree, and every one of them is also
/// enforced later by the step that needs it. Running them together, and running
/// all of them even after the first failure, is what keeps a tree with two
/// problems from costing two candidate builds.
pub struct CheckReleaseReadiness<'a, F, C, W> {
    file_system: &'a F,
    contracts: &'a C,
    workspace: &'a W,
    root: &'a RepositoryRoot,
}

impl<'a, F, C, W> CheckReleaseReadiness<'a, F, C, W>
where
    F: ReleaseFileSystem,
    C: ReleaseContracts,
    W: ReleaseWorkspace,
{
    pub fn new(
        file_system: &'a F,
        contracts: &'a C,
        workspace: &'a W,
        root: &'a RepositoryRoot,
    ) -> Self {
        Self {
            file_system,
            contracts,
            workspace,
            root,
        }
    }

    pub fn execute(&self, version: &ReleaseVersion) -> ReleaseReadiness {
        ReleaseReadiness::new(
            version.clone(),
            vec![
                self.changelog(version),
                self.version_sources(version),
                self.marketplace_catalogs(version),
                self.working_tree(),
                self.pushed_branch(),
                self.gate(
                    "vendored contract",
                    self.workspace.verify_vendored_contract(),
                ),
                self.gate("publish chain", self.workspace.verify_publish_chain()),
                self.candidate_inputs(),
            ],
        )
    }

    fn changelog(&self, version: &ReleaseVersion) -> ReadinessCheck {
        let path = self.root.join("CHANGELOG.md");
        match CheckChangelog::new(self.file_system).execute(&path, version) {
            Ok(()) => ReadinessCheck::passed("changelog", format!("[{version}] is written")),
            Err(error) => ReadinessCheck::failed("changelog", error.to_string()),
        }
    }

    fn version_sources(&self, version: &ReleaseVersion) -> ReadinessCheck {
        let sources = match CollectVersionSources::new(self.file_system).execute(self.root, version)
        {
            Ok(sources) => sources,
            Err(error) => return ReadinessCheck::failed("version sources", error.to_string()),
        };
        let total = sources.len();
        let stale = sources
            .iter()
            .filter(|source| !source.agrees())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if stale.is_empty() {
            ReadinessCheck::passed(
                "version sources",
                format!("all {total} sources read {version}"),
            )
        } else {
            ReadinessCheck::failed("version sources", stale.join("\n"))
        }
    }

    fn marketplace_catalogs(&self, version: &ReleaseVersion) -> ReadinessCheck {
        match CheckMarketplaceContracts::new(self.file_system).execute(
            self.root,
            version,
            PluginRepository::URL,
        ) {
            Ok(()) => ReadinessCheck::passed(
                "marketplace catalogs",
                format!("Claude pins {} and Codex pins ./plugins/kmp", version.tag()),
            ),
            Err(error) => ReadinessCheck::failed("marketplace catalogs", error.to_string()),
        }
    }

    fn working_tree(&self) -> ReadinessCheck {
        match self.workspace.require_clean() {
            Ok(()) => ReadinessCheck::passed("working tree", "clean"),
            Err(error) => ReadinessCheck::failed("working tree", error.to_string()),
        }
    }

    fn pushed_branch(&self) -> ReadinessCheck {
        let branch = match self.workspace.current_branch() {
            Ok(branch) => branch,
            Err(error) => return ReadinessCheck::failed("pushed branch", error.to_string()),
        };
        let head = match self.workspace.head_commit() {
            Ok(head) => head,
            Err(error) => return ReadinessCheck::failed("pushed branch", error.to_string()),
        };
        match self.workspace.upstream_commit() {
            Ok(Some(upstream)) if upstream == head => {
                ReadinessCheck::passed("pushed branch", format!("{branch} is at {head}"))
            }
            Ok(Some(upstream)) => ReadinessCheck::failed(
                "pushed branch",
                format!("{branch} is at {head} but its upstream is at {upstream}; push it"),
            ),
            Ok(None) => ReadinessCheck::failed(
                "pushed branch",
                format!("{branch} has no upstream; push it before building a candidate"),
            ),
            Err(error) => ReadinessCheck::failed("pushed branch", error.to_string()),
        }
    }

    fn gate(&self, name: &str, outcome: Result<(), ReleaseError>) -> ReadinessCheck {
        match outcome {
            Ok(()) => ReadinessCheck::passed(name, "passes"),
            Err(error) => ReadinessCheck::failed(name, error.to_string()),
        }
    }

    fn candidate_inputs(&self) -> ReadinessCheck {
        match self.contracts.candidate_inputs() {
            Ok(digest) => ReadinessCheck::passed(
                "candidate inputs",
                format!("{digest}; a candidate must be built from this digest"),
            ),
            Err(error) => ReadinessCheck::failed("candidate inputs", error.to_string()),
        }
    }
}
