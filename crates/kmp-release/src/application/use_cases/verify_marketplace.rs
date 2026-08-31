use std::path::Path;

use sha2::{Digest, Sha256};

use crate::application::use_cases::check_marketplace_contracts::CheckMarketplaceContracts;
use crate::domain::plugin_tree_digest::PluginTreeDigest;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::source_commit::SourceCommit;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::marketplace_repository::MarketplaceRepository;
use crate::ports::release_file_system::ReleaseFileSystem;

/// The one path in the repository both marketplaces map to.
const PLUGIN_TREE: &str = "plugins/kmp";

/// Proves that the tag both catalogs advertise resolves to the reviewed commit
/// and that Codex and Claude Code install byte-identical plugin trees. The
/// answers that need no network are settled first, by
/// [`CheckMarketplaceContracts`], so a stale catalog fails before a clone.
pub struct VerifyMarketplace<'a, F, R> {
    file_system: &'a F,
    repository: &'a R,
}

impl<'a, F, R> VerifyMarketplace<'a, F, R>
where
    F: ReleaseFileSystem + CandidateFileSystem,
    R: MarketplaceRepository,
{
    pub fn new(file_system: &'a F, repository: &'a R) -> Self {
        Self {
            file_system,
            repository,
        }
    }

    pub fn execute(
        &self,
        root: &RepositoryRoot,
        version: &ReleaseVersion,
        repository_url: &str,
        expected_commit: Option<&SourceCommit>,
        allow_unpublished_tag: bool,
    ) -> Result<SourceCommit, ReleaseError> {
        CheckMarketplaceContracts::new(self.file_system).execute(root, version, repository_url)?;
        let release_ref = version.tag();
        let expected = match expected_commit {
            Some(commit) => commit.clone(),
            None => self
                .repository
                .local_annotated_tag_commit(root, &release_ref)?
                .ok_or_else(|| {
                    ReleaseError::invalid(format!(
                        "local annotated tag {release_ref} does not exist"
                    ))
                })?,
        };
        let remote = self
            .repository
            .remote_annotated_tag_commit(repository_url, &release_ref)?;
        let checkout = root.join(format!("tmp/marketplace-clone-{}", std::process::id()));
        self.file_system.remove_dir_all_if_present(&checkout)?;
        let source_tree = self.tree_digest(root.as_path(), PLUGIN_TREE)?;
        let result = match remote {
            Some(commit) => {
                if commit != expected {
                    return Err(ReleaseError::invalid(format!(
                        "remote annotated tag {release_ref} peels to {commit}, not expected commit {expected}"
                    )));
                }
                self.repository
                    .clone_reference(repository_url, &release_ref, &checkout)?;
                let cloned_tree = self.tree_digest(&checkout, PLUGIN_TREE)?;
                if cloned_tree != source_tree {
                    return Err(ReleaseError::invalid(
                        "Claude and Codex marketplace mappings do not resolve the exact same plugin tree",
                    ));
                }
                Ok(commit)
            }
            None if allow_unpublished_tag => {
                let main = self
                    .repository
                    .remote_branch_commit(repository_url, "main")?
                    .ok_or_else(|| ReleaseError::invalid("remote main does not exist"))?;
                if main != expected {
                    return Err(ReleaseError::invalid(format!(
                        "unpublished tag {release_ref} would not name remote main: expected {expected}, found {main}"
                    )));
                }
                Ok(expected)
            }
            None => Err(ReleaseError::invalid(format!(
                "remote annotated tag {release_ref} does not exist"
            ))),
        };
        self.file_system.remove_dir_all_if_present(&checkout)?;
        result
    }

    /// The digest of the plugin tree git carries under `relative`, read from
    /// `repository` on disk.
    ///
    /// Enumeration comes from git and content from the filesystem, on purpose.
    /// Git decides *what* the plugin is — that is the artifact the catalogs
    /// publish and the clone brings back — while the filesystem still decides
    /// what those files currently say, so a real uncommitted edit to a tracked
    /// file remains a real difference. What is no longer a difference is
    /// everything git never carried: the gitignored engine the plugin installs
    /// on every machine that uses KMP made this comparison fail on a release
    /// that was in fact consistent (#448).
    fn tree_digest(
        &self,
        repository: &Path,
        relative: &str,
    ) -> Result<PluginTreeDigest, ReleaseError> {
        let mut entries = self.repository.tracked_files_under(repository, relative)?;
        entries.sort();
        let mut digest = Sha256::new();
        for tracked in entries {
            let relative = tracked
                .strip_prefix(relative)
                .map_err(|error| {
                    ReleaseError::invalid(format!(
                        "cannot relativize {}: {error}",
                        tracked.display()
                    ))
                })?
                .to_string_lossy()
                .replace('\\', "/");
            let relative = relative.as_bytes();
            let path = repository.join(&tracked);
            let content = self.file_system.read_bytes(&path)?;
            digest.update(
                u64::try_from(relative.len())
                    .map_err(|_| ReleaseError::invalid("plugin path is too large"))?
                    .to_be_bytes(),
            );
            digest.update(relative);
            digest.update(if self.file_system.is_executable(&path)? {
                b"x"
            } else {
                b"-"
            });
            digest.update(
                u64::try_from(content.len())
                    .map_err(|_| ReleaseError::invalid("plugin file is too large"))?
                    .to_be_bytes(),
            );
            digest.update(content);
        }
        Ok(PluginTreeDigest::from_bytes(digest.finalize().into()))
    }
}
