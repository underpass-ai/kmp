use std::path::Path;

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
}
