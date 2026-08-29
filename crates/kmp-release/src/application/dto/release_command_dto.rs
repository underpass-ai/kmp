use std::path::PathBuf;

use crate::domain::calendar_date::CalendarDate;
use crate::domain::candidate_input_digest::CandidateInputDigest;
use crate::domain::plugin_package_kind::PluginPackageKind;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::source_commit::SourceCommit;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ReleaseCommandDto {
    PrepareChangelog {
        version: ReleaseVersion,
        date: CalendarDate,
        path: PathBuf,
    },
    CheckChangelog {
        version: ReleaseVersion,
        path: PathBuf,
    },
    SyncReadme {
        source: PathBuf,
        targets: Vec<PathBuf>,
    },
    CandidateInputs {
        github_output: Option<PathBuf>,
    },
    AssembleCandidate {
        version: ReleaseVersion,
        binaries: PathBuf,
        plugins: PathBuf,
        mcpb: PathBuf,
        output: PathBuf,
    },
    VerifyCandidate {
        version: ReleaseVersion,
        directory: PathBuf,
        input_digest: Option<CandidateInputDigest>,
        run_id: Option<String>,
    },
    SyncGuide {
        version: ReleaseVersion,
        root: RepositoryRoot,
        binary: PathBuf,
    },
    WriteGuideAssets {
        root: RepositoryRoot,
        binary: PathBuf,
    },
    ApplyGuideAssets {
        root: RepositoryRoot,
        binary: PathBuf,
    },
    PackageMcpb {
        version: ReleaseVersion,
        root: RepositoryRoot,
        input: PathBuf,
        output: PathBuf,
    },
    StampServerMcpb {
        root: RepositoryRoot,
        archive: PathBuf,
        server_manifest: PathBuf,
    },
    PackagePlugin {
        root: RepositoryRoot,
        binary: PathBuf,
        output: PathBuf,
        kind: PluginPackageKind,
    },
    VerifyMarketplace {
        version: ReleaseVersion,
        root: RepositoryRoot,
        repository: String,
        expected_commit: Option<SourceCommit>,
        allow_unpublished_tag: bool,
    },
    CurrentVersion {
        root: RepositoryRoot,
    },
    PrepareVersion {
        version: ReleaseVersion,
        root: RepositoryRoot,
    },
}

impl ReleaseCommandDto {
    pub fn default_root() -> Result<RepositoryRoot, crate::domain::release_error::ReleaseError> {
        RepositoryRoot::discover()
    }
}
