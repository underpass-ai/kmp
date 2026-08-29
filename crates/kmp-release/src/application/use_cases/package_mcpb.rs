use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::application::dto::mcpb_archive_entry_dto::McpbArchiveEntryDto;
use crate::application::dto::mcpb_package_receipt_dto::McpbPackageReceiptDto;
use crate::domain::mcpb_digest::McpbDigest;
use crate::domain::mcpb_target::McpbTarget;
use crate::domain::release_archive_path::ReleaseArchivePath;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::release_archive_writer::ReleaseArchiveWriter;
use crate::ports::release_file_system::ReleaseFileSystem;

pub struct PackageMcpb<'a, F, A> {
    file_system: &'a F,
    archives: &'a A,
}

impl<'a, F, A> PackageMcpb<'a, F, A>
where
    F: ReleaseFileSystem + CandidateFileSystem,
    A: ReleaseArchiveWriter,
{
    pub fn new(file_system: &'a F, archives: &'a A) -> Self {
        Self {
            file_system,
            archives,
        }
    }

    pub fn execute(
        &self,
        root: &RepositoryRoot,
        version: &ReleaseVersion,
        input: &Path,
        output: &Path,
    ) -> Result<McpbPackageReceiptDto, ReleaseError> {
        let manifest = root.join("distribution/mcpb/manifest.json");
        let manifest_body: Value = serde_json::from_str(&self.file_system.read_text(&manifest)?)
            .map_err(|error| ReleaseError::invalid(format!("MCPB manifest is invalid: {error}")))?;
        if manifest_body["version"].as_str() != Some(version.as_str()) {
            return Err(ReleaseError::invalid(format!(
                "MCPB manifest version does not match requested {version}"
            )));
        }

        let mut entries = vec![
            self.entry(root.join("LICENSE"), "LICENSE", false)?,
            self.entry(root.join("NOTICE"), "NOTICE", false)?,
            self.entry(
                root.join("THIRD_PARTY_NOTICES.md"),
                "THIRD_PARTY_NOTICES.md",
                false,
            )?,
            self.entry(manifest, "manifest.json", false)?,
            self.entry(
                root.join("distribution/mcpb/server/kmp-mcp"),
                "server/kmp-mcp",
                true,
            )?,
        ];
        for target in McpbTarget::all() {
            let source = input.join(target.input_name(version));
            entries.push(self.entry(source.clone(), target.archive_name(), true)?);
            if target == McpbTarget::WindowsX86_64 {
                entries.push(self.entry(source, "server/kmp-mcp.exe", true)?);
            }
        }
        for entry in &entries {
            let _ = self.file_system.file_size(&entry.source)?;
        }

        self.file_system.create_dir_all(output)?;
        let archive = output.join(format!("kmp-mcp-{}.mcpb", version.tag()));
        self.archives.write(&archive, &entries)?;
        let digest =
            McpbDigest::from_bytes(Sha256::digest(self.file_system.read_bytes(&archive)?).into());
        let checksum = archive.with_extension("mcpb.sha256");
        let name = archive
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| ReleaseError::invalid("MCPB archive has no portable file name"))?;
        self.file_system
            .write_text(&checksum, &format!("{digest}  {name}\n"))?;
        Ok(McpbPackageReceiptDto { archive, digest })
    }

    fn entry(
        &self,
        source: PathBuf,
        destination: impl Into<String>,
        executable: bool,
    ) -> Result<McpbArchiveEntryDto, ReleaseError> {
        Ok(McpbArchiveEntryDto {
            source,
            destination: ReleaseArchivePath::parse(destination)?,
            executable,
        })
    }
}
