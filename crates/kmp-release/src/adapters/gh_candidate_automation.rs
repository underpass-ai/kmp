use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::Duration;

use crate::application::mappers::github_run_mapper::GithubRunMapper;
use crate::domain::branch_name::BranchName;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::source_commit::SourceCommit;
use crate::domain::workflow_run_id::WorkflowRunId;
use crate::ports::candidate_automation::CandidateAutomation;

pub struct GhCandidateAutomation {
    root: RepositoryRoot,
}

impl GhCandidateAutomation {
    pub fn new(root: RepositoryRoot) -> Self {
        Self { root }
    }

    fn output(&self, arguments: &[&str]) -> Result<Output, ReleaseError> {
        Command::new("gh")
            .args(arguments)
            .current_dir(self.root.as_path())
            .output()
            .map_err(|error| ReleaseError::invalid(format!("cannot execute gh: {error}")))
    }

    fn checked_text(&self, arguments: &[&str]) -> Result<String, ReleaseError> {
        let output = self.output(arguments)?;
        if !output.status.success() {
            return Err(ReleaseError::invalid(format!(
                "gh {} failed: {}",
                arguments.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn inherited(&self, arguments: &[&str]) -> Result<(), ReleaseError> {
        let status = Command::new("gh")
            .args(arguments)
            .current_dir(self.root.as_path())
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|error| ReleaseError::invalid(format!("cannot execute gh: {error}")))?;
        if status.success() {
            Ok(())
        } else {
            Err(ReleaseError::invalid(format!(
                "gh {} exited with {status}",
                arguments.join(" ")
            )))
        }
    }

    fn listed_runs(
        &self,
        arguments: &[&str],
    ) -> Result<Vec<crate::application::dto::github_run_dto::GithubRunDto>, ReleaseError> {
        GithubRunMapper::map_many(&self.checked_text(arguments)?)
    }
}

impl CandidateAutomation for GhCandidateAutomation {
    fn dispatch(
        &self,
        branch: &BranchName,
        commit: &SourceCommit,
    ) -> Result<WorkflowRunId, ReleaseError> {
        let known = self
            .listed_runs(&[
                "run",
                "list",
                "--workflow",
                "release.yml",
                "--event",
                "workflow_dispatch",
                "--limit",
                "100",
                "--json",
                "databaseId",
            ])?
            .into_iter()
            .map(|run| WorkflowRunId::parse(run.database_id.to_string()))
            .collect::<Result<BTreeSet<_>, _>>()?;

        self.inherited(&["workflow", "run", "release.yml", "--ref", branch.as_str()])?;

        for _ in 0..60 {
            let runs = self.listed_runs(&[
                "run",
                "list",
                "--workflow",
                "release.yml",
                "--event",
                "workflow_dispatch",
                "--branch",
                branch.as_str(),
                "--limit",
                "20",
                "--json",
                "databaseId,headSha",
            ])?;
            for run in runs {
                let id = WorkflowRunId::parse(run.database_id.to_string())?;
                if run.head_sha == commit.as_str() && !known.contains(&id) {
                    return Ok(id);
                }
            }
            thread::sleep(Duration::from_secs(2));
        }
        Err(ReleaseError::invalid(format!(
            "release workflow dispatch did not appear for {commit}"
        )))
    }

    fn watch(&self, run_id: &WorkflowRunId) -> Result<(), ReleaseError> {
        self.inherited(&["run", "watch", run_id.as_str(), "--exit-status"])
    }

    fn download(
        &self,
        run_id: &WorkflowRunId,
        version: &ReleaseVersion,
        destination: &Path,
    ) -> Result<(), ReleaseError> {
        let artifact = format!("kmp-release-candidate-{version}");
        self.inherited(&[
            "run",
            "download",
            run_id.as_str(),
            "--name",
            &artifact,
            "--dir",
            &destination.to_string_lossy(),
        ])
    }

    fn successful_runs(&self) -> Result<Vec<WorkflowRunId>, ReleaseError> {
        self.listed_runs(&[
            "run",
            "list",
            "--workflow",
            "release.yml",
            "--event",
            "workflow_dispatch",
            "--status",
            "success",
            "--limit",
            "50",
            "--json",
            "databaseId",
        ])?
        .into_iter()
        .map(|run| WorkflowRunId::parse(run.database_id.to_string()))
        .collect()
    }
}
