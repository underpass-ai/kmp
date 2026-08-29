use std::path::PathBuf;
use std::process::Command;

use crate::domain::release_error::ReleaseError;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::source_commit::SourceCommit;
use crate::ports::marketplace_repository::MarketplaceRepository;
use crate::ports::release_repository::ReleaseRepository;

#[derive(Debug, Clone, Copy, Default)]
pub struct GitCli;

impl GitCli {
    fn output(root: &RepositoryRoot, arguments: &[&str]) -> Result<Vec<u8>, ReleaseError> {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(root.as_path())
            .output()
            .map_err(|error| ReleaseError::invalid(format!("could not execute git: {error}")))?;
        if !output.status.success() {
            return Err(ReleaseError::invalid(format!(
                "git {} failed: {}",
                arguments.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(output.stdout)
    }

    fn remote_output(arguments: &[&str]) -> Result<std::process::Output, ReleaseError> {
        Command::new("git")
            .args(arguments)
            .output()
            .map_err(|error| ReleaseError::invalid(format!("could not execute git: {error}")))
    }
}

impl ReleaseRepository for GitCli {
    fn tracked_files(&self, root: &RepositoryRoot) -> Result<Vec<PathBuf>, ReleaseError> {
        let output = Self::output(root, &["ls-files", "-z"])?;
        output
            .split(|byte| *byte == 0)
            .filter(|raw| !raw.is_empty())
            .map(|raw| {
                std::str::from_utf8(raw)
                    .map(PathBuf::from)
                    .map_err(|error| {
                        ReleaseError::invalid(format!("git returned a non-UTF-8 path: {error}"))
                    })
            })
            .collect()
    }

    fn head_commit(&self, root: &RepositoryRoot) -> Result<SourceCommit, ReleaseError> {
        let output = Self::output(root, &["rev-parse", "HEAD"])?;
        SourceCommit::parse(String::from_utf8_lossy(&output).trim().to_string())
    }
}

impl MarketplaceRepository for GitCli {
    fn local_annotated_tag_commit(
        &self,
        root: &RepositoryRoot,
        tag: &str,
    ) -> Result<Option<SourceCommit>, ReleaseError> {
        let reference = format!("refs/tags/{tag}");
        let kind = Command::new("git")
            .args(["cat-file", "-t", &reference])
            .current_dir(root.as_path())
            .output()
            .map_err(|error| ReleaseError::invalid(format!("could not execute git: {error}")))?;
        if !kind.status.success() {
            return Ok(None);
        }
        if String::from_utf8_lossy(&kind.stdout).trim() != "tag" {
            return Err(ReleaseError::invalid(format!(
                "local release tag {tag} must be annotated"
            )));
        }
        let peeled = format!("{reference}^{{commit}}");
        let output = Self::output(root, &["rev-parse", "--verify", &peeled])?;
        SourceCommit::parse(String::from_utf8_lossy(&output).trim().to_string()).map(Some)
    }

    fn remote_annotated_tag_commit(
        &self,
        repository: &str,
        tag: &str,
    ) -> Result<Option<SourceCommit>, ReleaseError> {
        let tag_ref = format!("refs/tags/{tag}");
        let peeled_ref = format!("{tag_ref}^{{}}");
        let output =
            Self::remote_output(&["ls-remote", "--tags", repository, &tag_ref, &peeled_ref])?;
        if !output.status.success() {
            return Err(ReleaseError::invalid(format!(
                "could not resolve remote tag {tag}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let mut tag_object = None;
        let mut peeled = None;
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let mut fields = line.split_whitespace();
            let commit = fields.next().unwrap_or_default();
            match fields.next().unwrap_or_default() {
                reference if reference == tag_ref => tag_object = Some(commit.to_string()),
                reference if reference == peeled_ref => peeled = Some(commit.to_string()),
                _ => {}
            }
        }
        match (tag_object, peeled) {
            (None, None) => Ok(None),
            (Some(_), Some(commit)) => SourceCommit::parse(commit).map(Some),
            (Some(_), None) => Err(ReleaseError::invalid(format!(
                "remote release tag {tag} must be annotated so its peeled commit is auditable"
            ))),
            (None, Some(_)) => Err(ReleaseError::invalid(format!(
                "remote release tag {tag} returned a peeled commit without its tag object"
            ))),
        }
    }

    fn remote_branch_commit(
        &self,
        repository: &str,
        branch: &str,
    ) -> Result<Option<SourceCommit>, ReleaseError> {
        let reference = format!("refs/heads/{branch}");
        let output = Self::remote_output(&["ls-remote", repository, &reference])?;
        if !output.status.success() {
            return Err(ReleaseError::invalid(format!(
                "could not resolve remote branch {branch}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let commit = String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .next()
            .map(str::to_string);
        commit.map(SourceCommit::parse).transpose()
    }

    fn clone_reference(
        &self,
        repository: &str,
        reference: &str,
        destination: &std::path::Path,
    ) -> Result<(), ReleaseError> {
        let destination = destination.to_string_lossy();
        let output = Self::remote_output(&[
            "clone",
            "--depth",
            "1",
            "--branch",
            reference,
            repository,
            &destination,
        ])?;
        if !output.status.success() {
            return Err(ReleaseError::invalid(format!(
                "Claude-compatible `git clone --branch {reference}` failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }
}
