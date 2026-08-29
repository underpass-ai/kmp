use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::ports::release_file_system::ReleaseFileSystem;

pub struct ReadWorkspaceVersion<'a, F> {
    file_system: &'a F,
}

impl<'a, F: ReleaseFileSystem> ReadWorkspaceVersion<'a, F> {
    pub fn new(file_system: &'a F) -> Self {
        Self { file_system }
    }

    pub fn execute(&self, root: &RepositoryRoot) -> Result<ReleaseVersion, ReleaseError> {
        let cargo = self.file_system.read_text(&root.join("Cargo.toml"))?;
        let mut in_workspace_package = false;
        for raw in cargo.lines() {
            let line = raw.trim();
            if line.starts_with('[') {
                in_workspace_package = line == "[workspace.package]";
                continue;
            }
            if in_workspace_package && let Some(value) = line.strip_prefix("version = ") {
                return ReleaseVersion::parse(value.trim_matches('"'));
            }
        }
        Err(ReleaseError::invalid(
            "Cargo.toml has no [workspace.package] version",
        ))
    }
}
