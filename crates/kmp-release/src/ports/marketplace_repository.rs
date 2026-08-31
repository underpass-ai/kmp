use std::path::{Path, PathBuf};

use crate::domain::release_error::ReleaseError;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::source_commit::SourceCommit;

pub trait MarketplaceRepository {
    fn local_annotated_tag_commit(
        &self,
        root: &RepositoryRoot,
        tag: &str,
    ) -> Result<Option<SourceCommit>, ReleaseError>;
    fn remote_annotated_tag_commit(
        &self,
        repository: &str,
        tag: &str,
    ) -> Result<Option<SourceCommit>, ReleaseError>;
    fn remote_branch_commit(
        &self,
        repository: &str,
        branch: &str,
    ) -> Result<Option<SourceCommit>, ReleaseError>;
    fn clone_reference(
        &self,
        repository: &str,
        reference: &str,
        destination: &Path,
    ) -> Result<(), ReleaseError>;
    /// The files git tracks under `relative`, as paths relative to `root`.
    ///
    /// The plugin a catalog publishes is the tree git carries, so that is
    /// what a parity claim has to compare. A working directory is not that
    /// tree: it also holds whatever the build and the installed product leave
    /// behind, and asking the filesystem instead of git made the comparison
    /// fail on exactly the machines that use KMP (#448).
    fn tracked_files_under(
        &self,
        root: &Path,
        relative: &str,
    ) -> Result<Vec<PathBuf>, ReleaseError>;
}
