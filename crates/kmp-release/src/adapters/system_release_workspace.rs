use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::Value;

use crate::domain::branch_name::BranchName;
use crate::domain::candidate_input_digest::CandidateInputDigest;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::source_commit::SourceCommit;
use crate::domain::workflow_run_id::WorkflowRunId;
use crate::ports::release_workspace::ReleaseWorkspace;

/// The Cargo package and binary name of the engine a release synchronizes its
/// guide against.
const ENGINE_PACKAGE: &str = "kmp-mcp";

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

    /// Runs a build for the artifact report it prints on stdout while its
    /// progress and diagnostics still reach the operator on stderr.
    fn reported_build(&self, arguments: &[&str]) -> Result<Vec<u8>, ReleaseError> {
        let output = Command::new("cargo")
            .args(arguments)
            .current_dir(self.root.as_path())
            .stdin(Stdio::inherit())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .output()
            .map_err(|error| ReleaseError::invalid(format!("cannot execute cargo: {error}")))?;
        if !output.status.success() {
            return Err(ReleaseError::invalid(format!(
                "cargo {} exited with {}",
                arguments.join(" "),
                output.status
            )));
        }
        Ok(output.stdout)
    }

    /// The executable Cargo reports for the engine package.
    ///
    /// Cargo emits one `compiler-artifact` line per unit, including units it
    /// found fresh, so this answers the same path on a resumed run as on a
    /// cold one.
    fn engine_from_build_report(report: &[u8]) -> Result<PathBuf, ReleaseError> {
        let report = String::from_utf8_lossy(report);
        let engine = report
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|message| message["reason"] == "compiler-artifact")
            .filter(|message| message["target"]["name"] == ENGINE_PACKAGE)
            .filter(|message| {
                message["target"]["kind"]
                    .as_array()
                    .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"))
            })
            .filter_map(|message| message["executable"].as_str().map(PathBuf::from))
            .next_back();
        engine.ok_or_else(|| {
            ReleaseError::invalid(format!(
                "the build reported no `{ENGINE_PACKAGE}` executable; nothing was produced for \
                 guide sync to inspect"
            ))
        })
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

    fn build_engine(&self) -> Result<PathBuf, ReleaseError> {
        let report = self.reported_build(&[
            "build",
            "--locked",
            "-p",
            ENGINE_PACKAGE,
            // Artifact messages on stdout name the executable Cargo wrote,
            // wherever it decided to write it; `render-diagnostics` keeps the
            // compiler's own output on stderr where the operator reads it.
            "--message-format=json-render-diagnostics",
        ])?;
        Self::engine_from_build_report(&report)
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

    fn commit_tracked(&self, message: &str) -> Result<bool, ReleaseError> {
        // Tracked changes only: a release chain commits what the release
        // steps wrote, never whatever else the working directory holds.
        self.checked_output("git", &["add", "--update"])?;
        if self
            .git_text(&["diff", "--cached", "--name-only"])?
            .is_empty()
        {
            return Ok(false);
        }
        self.inherited("git", &["commit", "-m", message])
            .map(|()| true)
    }

    fn push_current_branch(&self) -> Result<(), ReleaseError> {
        let branch = self.current_branch()?;
        self.inherited(
            "git",
            &["push", "--set-upstream", "origin", branch.as_str()],
        )
    }

    fn advance_branch(
        &self,
        branch: &BranchName,
        commit: &SourceCommit,
    ) -> Result<(), ReleaseError> {
        // No force and no checkout: the branch is protected, its gate has to
        // speak, and a fast-forward is the only move a release may make.
        let refspec = format!("{}:refs/heads/{branch}", commit.as_str());
        self.inherited("git", &["push", "origin", &refspec])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shapes taken from real `cargo build --message-format=json` output.
    fn artifact(name: &str, kind: &str, executable: Option<&str>) -> String {
        let executable = match executable {
            Some(path) => format!("\"{path}\""),
            None => "null".to_string(),
        };
        format!(
            "{{\"reason\":\"compiler-artifact\",\"target\":{{\"kind\":[\"{kind}\"],\
             \"crate_types\":[\"{kind}\"],\"name\":\"{name}\",\"edition\":\"2021\"}},\
             \"executable\":{executable},\"fresh\":false}}"
        )
    }

    #[test]
    fn the_engine_comes_from_the_default_target_directory() {
        let report = artifact("kmp-mcp", "bin", Some("/checkout/target/debug/kmp-mcp"));

        let engine = SystemReleaseWorkspace::engine_from_build_report(report.as_bytes());

        assert_eq!(
            engine.expect("engine"),
            PathBuf::from("/checkout/target/debug/kmp-mcp")
        );
    }

    #[test]
    fn the_engine_comes_from_a_configured_target_directory() {
        // A shared cache outside the checkout: the path #723 guessed wrong.
        let report = artifact("kmp-mcp", "bin", Some("/shared-cache/debug/kmp-mcp"));

        let engine = SystemReleaseWorkspace::engine_from_build_report(report.as_bytes());

        assert_eq!(
            engine.expect("engine"),
            PathBuf::from("/shared-cache/debug/kmp-mcp")
        );
    }

    #[test]
    fn a_resumed_build_still_answers_where_the_engine_is() {
        // Cargo reports fresh units too, so a rerun after an interrupted
        // release resolves the same engine instead of finding nothing.
        let report = artifact("kmp-mcp", "bin", Some("/shared-cache/debug/kmp-mcp"))
            .replace("\"fresh\":false", "\"fresh\":true");

        let engine = SystemReleaseWorkspace::engine_from_build_report(report.as_bytes());

        assert_eq!(
            engine.expect("engine"),
            PathBuf::from("/shared-cache/debug/kmp-mcp")
        );
    }

    #[test]
    fn other_packages_and_libraries_are_not_mistaken_for_the_engine() {
        let report = [
            artifact("kmp-domain", "lib", None),
            artifact(
                "kmp-release",
                "bin",
                Some("/shared-cache/debug/kmp-release"),
            ),
            artifact("kmp-mcp", "lib", None),
            artifact("kmp-mcp", "bin", Some("/shared-cache/debug/kmp-mcp")),
            "{\"reason\":\"build-finished\",\"success\":true}".to_string(),
        ]
        .join("\n");

        let engine = SystemReleaseWorkspace::engine_from_build_report(report.as_bytes());

        assert_eq!(
            engine.expect("engine"),
            PathBuf::from("/shared-cache/debug/kmp-mcp")
        );
    }

    #[test]
    fn a_build_that_produced_no_engine_is_refused() {
        let report = [
            artifact("kmp-mcp", "lib", None),
            "{\"reason\":\"build-finished\",\"success\":true}".to_string(),
        ]
        .join("\n");

        let engine = SystemReleaseWorkspace::engine_from_build_report(report.as_bytes());

        let message = engine.expect_err("a build without an engine cannot be reported as one");
        assert!(
            format!("{message:?}").contains("no `kmp-mcp` executable"),
            "the refusal has to name what is missing: {message:?}"
        );
    }

    #[test]
    fn an_empty_report_is_refused_rather_than_guessed() {
        let engine = SystemReleaseWorkspace::engine_from_build_report(b"");

        assert!(
            engine.is_err(),
            "silence from the build is not a target directory to fall back on"
        );
    }
}
