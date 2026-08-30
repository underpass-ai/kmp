use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use crate::domain::branch_name::BranchName;
use crate::domain::candidate_input_digest::CandidateInputDigest;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::source_commit::SourceCommit;
use crate::domain::workflow_run_id::WorkflowRunId;
use crate::ports::release_workspace::ReleaseWorkspace;

pub struct SystemReleaseWorkspace {
    root: RepositoryRoot,
}

impl SystemReleaseWorkspace {
    pub fn new(root: RepositoryRoot) -> Self {
        Self { root }
    }

    fn output(&self, program: &str, arguments: &[&str]) -> Result<Output, ReleaseError> {
        Command::new(program)
            .args(arguments)
            .current_dir(self.root.as_path())
            .output()
            .map_err(|error| ReleaseError::invalid(format!("cannot execute {program}: {error}")))
    }

    fn checked_output(&self, program: &str, arguments: &[&str]) -> Result<Vec<u8>, ReleaseError> {
        let output = self.output(program, arguments)?;
        if !output.status.success() {
            return Err(ReleaseError::invalid(format!(
                "{} {} failed: {}",
                program,
                arguments.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(output.stdout)
    }

    fn inherited(&self, program: &str, arguments: &[&str]) -> Result<(), ReleaseError> {
        let status = Command::new(program)
            .args(arguments)
            .current_dir(self.root.as_path())
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|error| ReleaseError::invalid(format!("cannot execute {program}: {error}")))?;
        if status.success() {
            Ok(())
        } else {
            Err(ReleaseError::invalid(format!(
                "{} {} exited with {status}",
                program,
                arguments.join(" ")
            )))
        }
    }

    /// Runs a gate script for its verdict rather than its console output, so a
    /// failure can be collected into a readiness report instead of scrolling
    /// past.
    fn reported(&self, script: &str) -> Result<(), ReleaseError> {
        let output = self.output("bash", &[script])?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        } else {
            stderr
        };
        Err(ReleaseError::invalid(format!("{script} failed: {detail}")))
    }

    fn git_text(&self, arguments: &[&str]) -> Result<String, ReleaseError> {
        self.checked_output("git", arguments)
            .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_string())
    }
}

impl ReleaseWorkspace for SystemReleaseWorkspace {
    fn refresh_lockfile(&self) -> Result<(), ReleaseError> {
        self.checked_output("cargo", &["metadata", "--format-version", "1"])
            .map(|_| ())
    }

    fn build_engine(&self) -> Result<(), ReleaseError> {
        self.inherited("cargo", &["build", "--locked", "-p", "kmp-mcp"])
    }

    fn show_version_diff(&self) -> Result<(), ReleaseError> {
        self.inherited(
            "git",
            &[
                "--no-pager",
                "diff",
                "--stat",
                "--",
                "CHANGELOG.md",
                "Cargo.toml",
                "Cargo.lock",
                "distribution/charts/kmp/Chart.yaml",
                "plugins/kmp/.claude-plugin/plugin.json",
                "plugins/kmp/.codex-plugin/plugin.json",
                "plugins/kmp/guide/guide.requests.json",
                "plugins/kmp/guide/memory.jsonl",
                "server.json",
                "distribution/mcpb/manifest.json",
            ],
        )
    }

    fn require_clean(&self) -> Result<(), ReleaseError> {
        let status = self.git_text(&["status", "--porcelain"])?;
        if status.is_empty() {
            Ok(())
        } else {
            Err(ReleaseError::invalid(format!(
                "working tree is dirty; commit or stash before continuing:\n{status}"
            )))
        }
    }

    fn current_branch(&self) -> Result<BranchName, ReleaseError> {
        BranchName::parse(self.git_text(&["rev-parse", "--abbrev-ref", "HEAD"])?)
    }

    fn head_commit(&self) -> Result<SourceCommit, ReleaseError> {
        SourceCommit::parse(self.git_text(&["rev-parse", "HEAD"])?)
    }

    fn upstream_commit(&self) -> Result<Option<SourceCommit>, ReleaseError> {
        let output = self.output("git", &["rev-parse", "--verify", "@{upstream}"])?;
        if !output.status.success() {
            return Ok(None);
        }
        SourceCommit::parse(String::from_utf8_lossy(&output.stdout).trim().to_string()).map(Some)
    }

    fn verify_registry(&self) -> Result<(), ReleaseError> {
        self.inherited("bash", &["scripts/ci/mcp-registry.sh"])
    }

    fn verify_vendored_contract(&self) -> Result<(), ReleaseError> {
        self.reported("scripts/ci/check-vendored-contract.sh")
    }

    fn verify_publish_chain(&self) -> Result<(), ReleaseError> {
        self.reported("scripts/ci/check-publish-chain.sh")
    }

    fn changed_files_since(&self, commit: &SourceCommit) -> Result<Vec<PathBuf>, ReleaseError> {
        let paths = self.git_text(&["diff", "--name-only", commit.as_str(), "--"])?;
        Ok(paths.lines().map(PathBuf::from).collect())
    }

    fn tag_exists(&self, version: &ReleaseVersion) -> Result<bool, ReleaseError> {
        self.output(
            "git",
            &[
                "rev-parse",
                "-q",
                "--verify",
                &format!("refs/tags/{}", version.tag()),
            ],
        )
        .map(|output| output.status.success())
    }

    fn create_and_push_tag(
        &self,
        version: &ReleaseVersion,
        run_id: &WorkflowRunId,
        input: &CandidateInputDigest,
    ) -> Result<(), ReleaseError> {
        let tag = version.tag();
        let candidate_run = format!("candidate-run: {run_id}");
        let candidate_inputs = format!("candidate-inputs: {input}");
        self.inherited(
            "git",
            &[
                "tag",
                "-a",
                &tag,
                "-m",
                &format!("Release {tag}"),
                "-m",
                &candidate_run,
                "-m",
                &candidate_inputs,
            ],
        )?;
        self.inherited("git", &["push", "origin", &tag])
    }
}
