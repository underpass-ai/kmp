use std::path::PathBuf;

use crate::domain::release_error::ReleaseError;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::source_commit::SourceCommit;

pub trait ReleaseRepository {
    fn tracked_files(&self, root: &RepositoryRoot) -> Result<Vec<PathBuf>, ReleaseError>;
    fn head_commit(&self, root: &RepositoryRoot) -> Result<SourceCommit, ReleaseError>;
}
