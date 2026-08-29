use std::path::Path;

use sha2::{Digest, Sha256};

use crate::application::dto::plugin_archive_entry_dto::PluginArchiveEntryDto;
use crate::application::dto::plugin_package_receipt_dto::PluginPackageReceiptDto;
use crate::application::mappers::plugin_manifest_package_mapper::PluginManifestPackageMapper;
use crate::application::use_cases::read_workspace_version::ReadWorkspaceVersion;
use crate::domain::mcpb_digest::McpbDigest;
use crate::domain::plugin_package_kind::PluginPackageKind;
use crate::domain::plugin_package_target::PluginPackageTarget;
use crate::domain::plugin_package_version::PluginPackageVersion;
use crate::domain::release_archive_path::ReleaseArchivePath;
use crate::domain::release_error::ReleaseError;
use crate::domain::repository_root::RepositoryRoot;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::plugin_archive_writer::PluginArchiveWriter;
use crate::ports::release_binary_version_reader::ReleaseBinaryVersionReader;
use crate::ports::release_file_system::ReleaseFileSystem;
use crate::ports::release_repository::ReleaseRepository;

pub struct PackagePlugin<'a, F, R, B, A> {
    file_system: &'a F,
    repository: &'a R,
    binaries: &'a B,
    archives: &'a A,
}

impl<'a, F, R, B, A> PackagePlugin<'a, F, R, B, A>
where
    F: ReleaseFileSystem + CandidateFileSystem,
    R: ReleaseRepository,
    B: ReleaseBinaryVersionReader,
    A: PluginArchiveWriter,
{
    pub fn new(file_system: &'a F, repository: &'a R, binaries: &'a B, archives: &'a A) -> Self {
        Self {
            file_system,
            repository,
            binaries,
            archives,
        }
    }

    pub fn execute(
        &self,
        root: &RepositoryRoot,
        binary: &Path,
        output: &Path,
        kind: PluginPackageKind,
    ) -> Result<PluginPackageReceiptDto, ReleaseError> {
        let workspace_version = ReadWorkspaceVersion::new(self.file_system).execute(root)?;
        let binary_version = self.binaries.read_version(binary)?;
        if binary_version != workspace_version {
            return Err(ReleaseError::invalid(format!(
                "plugin binary is {binary_version}, workspace is {workspace_version}"
            )));
        }
        let package_version = match kind {
            PluginPackageKind::Release => PluginPackageVersion::release(&workspace_version),
            PluginPackageKind::Development => PluginPackageVersion::development(
                &workspace_version,
                &self.repository.head_commit(root)?,
            ),
        };
        let plugin_root = root.join("plugins/kmp");
        let mut entries = Vec::new();
        for source in self.file_system.walk_files(&plugin_root)? {
            let relative = source.strip_prefix(&plugin_root).map_err(|error| {
                ReleaseError::invalid(format!("plugin path escaped its root: {error}"))
            })?;
            if relative.starts_with("bin") {
                continue;
            }
            let mut content = self.file_system.read_bytes(&source)?;
            if matches!(
                relative.to_string_lossy().as_ref(),
                ".codex-plugin/plugin.json" | ".claude-plugin/plugin.json"
            ) {
                content = PluginManifestPackageMapper::stamp(&content, &package_version)?;
            }
            entries.push(PluginArchiveEntryDto {
                destination: ReleaseArchivePath::parse(format!(
                    "kmp/{}",
                    relative.to_string_lossy().replace('\\', "/")
                ))?,
                content,
                executable: self.file_system.is_executable(&source)?,
            });
        }
        let binary_name = if binary
            .extension()
            .is_some_and(|extension| extension == "exe")
        {
            "kmp/bin/kmp-mcp.exe"
        } else {
            "kmp/bin/kmp-mcp"
        };
        entries.push(PluginArchiveEntryDto {
            destination: ReleaseArchivePath::parse(binary_name)?,
            content: self.file_system.read_bytes(binary)?,
            executable: true,
        });

        self.file_system.remove_dir_all_if_present(output)?;
        self.file_system.create_dir_all(output)?;
        let target = PluginPackageTarget::current();
        let archive = output.join(format!(
            "kmp-plugin-{}-{}.tar.gz",
            package_version,
            target.suffix()
        ));
        self.archives.write(&archive, &entries)?;
        let digest =
            McpbDigest::from_bytes(Sha256::digest(self.file_system.read_bytes(&archive)?).into());
        let archive_name = archive
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| ReleaseError::invalid("plugin archive has no portable file name"))?;
        let checksum = archive.with_file_name(format!("{archive_name}.sha256"));
        self.file_system
            .write_text(&checksum, &format!("{digest}  {archive_name}\n"))?;
        Ok(PluginPackageReceiptDto {
            archive,
            digest,
            version: package_version,
        })
    }
}
