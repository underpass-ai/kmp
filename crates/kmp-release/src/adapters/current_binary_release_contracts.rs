use std::path::{Path, PathBuf};
use std::process::Command;

use crate::domain::candidate_input_digest::CandidateInputDigest;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::source_commit::SourceCommit;
use crate::domain::workflow_run_id::WorkflowRunId;
use crate::ports::release_contracts::ReleaseContracts;

pub struct CurrentBinaryReleaseContracts {
    binary: PathBuf,
    root: RepositoryRoot,
}

impl CurrentBinaryReleaseContracts {
    pub fn new(root: RepositoryRoot) -> Result<Self, ReleaseError> {
        let binary = std::env::current_exe().map_err(|error| {
            ReleaseError::invalid(format!(
                "cannot resolve current kmp-release binary: {error}"
            ))
        })?;
        Ok(Self { binary, root })
    }

    fn execute(&self, arguments: &[String], announce: bool) -> Result<String, ReleaseError> {
        let output = Command::new(&self.binary)
            .args(arguments)
            .current_dir(self.root.as_path())
            .output()
            .map_err(|error| {
                ReleaseError::invalid(format!("cannot execute kmp-release contract: {error}"))
            })?;
        if !output.status.success() {
            return Err(ReleaseError::invalid(format!(
                "kmp-release {} failed: {}",
                arguments.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if announce && !stdout.is_empty() {
            println!("{stdout}");
        }
        Ok(stdout)
    }

    fn announced(&self, arguments: &[&str]) -> Result<(), ReleaseError> {
        self.execute(
            &arguments
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            true,
        )
        .map(|_| ())
    }
}

impl ReleaseContracts for CurrentBinaryReleaseContracts {
    fn sync_readmes(&self) -> Result<(), ReleaseError> {
        self.announced(&["readme", "sync"])
    }

    fn prepare_changelog(&self, version: &ReleaseVersion) -> Result<(), ReleaseError> {
        self.announced(&["changelog", "prepare", version.as_str()])
    }

    fn check_changelog(&self, version: &ReleaseVersion) -> Result<(), ReleaseError> {
        self.announced(&["changelog", "check", version.as_str()])
    }

    fn prepare_version(&self, version: &ReleaseVersion) -> Result<(), ReleaseError> {
        self.announced(&["version", "prepare", version.as_str()])
    }

    fn workspace_version(&self) -> Result<ReleaseVersion, ReleaseError> {
        let value = self.execute(&["version".to_string(), "current".to_string()], false)?;
        ReleaseVersion::parse(value)
    }

    fn sync_guide(&self, version: &ReleaseVersion, binary: &Path) -> Result<(), ReleaseError> {
        self.execute(
            &[
                "guide".to_string(),
                "sync".to_string(),
                version.to_string(),
                "--binary".to_string(),
                binary.to_string_lossy().to_string(),
            ],
            true,
        )
        .map(|_| ())
    }

    fn stamp_mcpb(&self, archive: &Path) -> Result<(), ReleaseError> {
        self.execute(
            &[
                "mcpb".to_string(),
                "stamp".to_string(),
                archive.to_string_lossy().to_string(),
            ],
            true,
        )
        .map(|_| ())
    }

    fn candidate_inputs(&self) -> Result<CandidateInputDigest, ReleaseError> {
        let digest = self.execute(&["candidate".to_string(), "inputs".to_string()], false)?;
        CandidateInputDigest::parse(digest)
    }

    fn verify_candidate(
        &self,
        version: &ReleaseVersion,
        directory: &Path,
        input: &CandidateInputDigest,
        run_id: &WorkflowRunId,
    ) -> Result<(), ReleaseError> {
        self.execute(
            &[
                "candidate".to_string(),
                "verify".to_string(),
                "--version".to_string(),
                version.to_string(),
                "--directory".to_string(),
                directory.to_string_lossy().to_string(),
                "--input-sha256".to_string(),
                input.to_string(),
                "--run-id".to_string(),
                run_id.to_string(),
            ],
            true,
        )
        .map(|_| ())
    }

    fn verify_marketplace(
        &self,
        version: &ReleaseVersion,
        expected_commit: &SourceCommit,
    ) -> Result<(), ReleaseError> {
        self.execute(
            &[
                "marketplace".to_string(),
                "verify".to_string(),
                version.to_string(),
                "--expected-commit".to_string(),
                expected_commit.to_string(),
                "--allow-unpublished-tag".to_string(),
            ],
            true,
        )
        .map(|_| ())
    }
}
