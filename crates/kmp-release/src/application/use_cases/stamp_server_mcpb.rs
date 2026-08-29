use std::path::Path;

use sha2::{Digest, Sha256};

use crate::application::mappers::server_manifest_mapper::ServerManifestMapper;
use crate::application::use_cases::read_workspace_version::ReadWorkspaceVersion;
use crate::domain::mcpb_digest::McpbDigest;
use crate::domain::release_error::ReleaseError;
use crate::domain::repository_root::RepositoryRoot;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::release_file_system::ReleaseFileSystem;

pub struct StampServerMcpb<'a, F> {
    file_system: &'a F,
}

impl<'a, F> StampServerMcpb<'a, F>
where
    F: ReleaseFileSystem + CandidateFileSystem,
{
    pub fn new(file_system: &'a F) -> Self {
        Self { file_system }
    }

    pub fn execute(
        &self,
        root: &RepositoryRoot,
        archive: &Path,
        server_manifest: &Path,
    ) -> Result<(String, McpbDigest), ReleaseError> {
        let version = ReadWorkspaceVersion::new(self.file_system).execute(root)?;
        let expected = format!("kmp-mcp-{}.mcpb", version.tag());
        if archive.file_name().and_then(|name| name.to_str()) != Some(expected.as_str()) {
            return Err(ReleaseError::invalid(format!(
                "expected archive {expected}, got {}",
                archive.display()
            )));
        }
        let digest =
            McpbDigest::from_bytes(Sha256::digest(self.file_system.read_bytes(archive)?).into());
        let mut document =
            ServerManifestMapper::parse(&self.file_system.read_text(server_manifest)?)?;
        let identifier = ServerManifestMapper::stamp(&mut document, &version, &digest)?;
        self.file_system
            .write_text(server_manifest, &ServerManifestMapper::text(&document)?)?;
        Ok((identifier, digest))
    }
}
