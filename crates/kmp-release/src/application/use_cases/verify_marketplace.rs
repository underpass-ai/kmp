use std::path::Path;

use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use crate::application::dto::claude_marketplace_dto::ClaudeMarketplaceDto;
use crate::application::dto::codex_marketplace_dto::CodexMarketplaceDto;
use crate::application::dto::plugin_manifest_dto::PluginManifestDto;
use crate::domain::plugin_tree_digest::PluginTreeDigest;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::source_commit::SourceCommit;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::marketplace_repository::MarketplaceRepository;
use crate::ports::release_file_system::ReleaseFileSystem;

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
        let claude: ClaudeMarketplaceDto =
            self.read_json(&root.join(".claude-plugin/marketplace.json"))?;
        let codex: CodexMarketplaceDto =
            self.read_json(&root.join(".agents/plugins/marketplace.json"))?;
        let claude_plugin = Self::one_claude_plugin(&claude)?;
        let codex_plugin = Self::one_codex_plugin(&codex)?;
        let release_ref = version.tag();
        if claude_plugin.source.source != "git-subdir"
            || claude_plugin.source.url != repository_url
            || claude_plugin.source.path != "plugins/kmp"
            || claude_plugin.source.reference != release_ref
        {
            return Err(ReleaseError::invalid(format!(
                "Claude marketplace must resolve {repository_url}/plugins/kmp through clonable immutable tag {release_ref}"
            )));
        }
        if codex_plugin.source.source != "local" || codex_plugin.source.path != "./plugins/kmp" {
            return Err(ReleaseError::invalid(
                "Codex marketplace must resolve the reviewed ./plugins/kmp snapshot",
            ));
        }
        Self::verify_description("Claude marketplace kmp entry", &claude_plugin.description)?;
        let plugin_root = root.join("plugins/kmp");
        for relative in [".claude-plugin/plugin.json", ".codex-plugin/plugin.json"] {
            let manifest: PluginManifestDto = self.read_json(&plugin_root.join(relative))?;
            if manifest
                .version
                .split_once('+')
                .map_or(manifest.version.as_str(), |pair| pair.0)
                != version.as_str()
            {
                return Err(ReleaseError::invalid(format!(
                    "{relative} is {}, not {version}",
                    manifest.version
                )));
            }
        }
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
        let source_tree = self.tree_digest(&plugin_root)?;
        let result = match remote {
            Some(commit) => {
                if commit != expected {
                    return Err(ReleaseError::invalid(format!(
                        "remote annotated tag {release_ref} peels to {commit}, not expected commit {expected}"
                    )));
                }
                self.repository
                    .clone_reference(repository_url, &release_ref, &checkout)?;
                let cloned_tree = self.tree_digest(&checkout.join("plugins/kmp"))?;
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

    fn one_claude_plugin(
        catalog: &ClaudeMarketplaceDto,
    ) -> Result<
        &crate::application::dto::claude_marketplace_plugin_dto::ClaudeMarketplacePluginDto,
        ReleaseError,
    > {
        let matches = catalog
            .plugins
            .iter()
            .filter(|plugin| plugin.name == "kmp")
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(ReleaseError::invalid(format!(
                "Claude marketplace must contain exactly one kmp entry, found {}",
                matches.len()
            )));
        }
        Ok(matches[0])
    }

    fn one_codex_plugin(
        catalog: &CodexMarketplaceDto,
    ) -> Result<
        &crate::application::dto::codex_marketplace_plugin_dto::CodexMarketplacePluginDto,
        ReleaseError,
    > {
        let matches = catalog
            .plugins
            .iter()
            .filter(|plugin| plugin.name == "kmp")
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(ReleaseError::invalid(format!(
                "Codex marketplace must contain exactly one kmp entry, found {}",
                matches.len()
            )));
        }
        Ok(matches[0])
    }

    fn verify_description(source: &str, description: &str) -> Result<(), ReleaseError> {
        let lower = description.to_lowercase();
        if !description.contains("ChronoLoom") {
            return Err(ReleaseError::invalid(format!(
                "{source} must describe the ChronoLoom view"
            )));
        }
        if ["ten tools", "10 tools", "ten moves", "10 moves"]
            .iter()
            .any(|retired| lower.contains(retired))
        {
            return Err(ReleaseError::invalid(format!(
                "{source} advertises a retired whole-surface count"
            )));
        }
        Ok(())
    }

    fn tree_digest(&self, root: &Path) -> Result<PluginTreeDigest, ReleaseError> {
        let mut entries = self.file_system.walk_files(root)?;
        entries.sort();
        let mut digest = Sha256::new();
        for path in entries {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| {
                    ReleaseError::invalid(format!("cannot relativize {}: {error}", path.display()))
                })?
                .to_string_lossy()
                .replace('\\', "/");
            let relative = relative.as_bytes();
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

    fn read_json<T: DeserializeOwned>(&self, path: &Path) -> Result<T, ReleaseError> {
        serde_json::from_str(&self.file_system.read_text(path)?).map_err(|error| {
            ReleaseError::invalid(format!("cannot read {}: {error}", path.display()))
        })
    }
}
