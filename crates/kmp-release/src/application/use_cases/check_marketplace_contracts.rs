use std::path::Path;

use serde::de::DeserializeOwned;

use crate::application::dto::claude_marketplace_dto::ClaudeMarketplaceDto;
use crate::application::dto::claude_marketplace_plugin_dto::ClaudeMarketplacePluginDto;
use crate::application::dto::codex_marketplace_dto::CodexMarketplaceDto;
use crate::application::dto::codex_marketplace_plugin_dto::CodexMarketplacePluginDto;
use crate::application::dto::plugin_manifest_dto::PluginManifestDto;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::ports::release_file_system::ReleaseFileSystem;

/// Everything the two co-located catalogs can be held to without touching the
/// network: the shape of both entries, the tag the Claude catalog pins, the
/// product claim it advertises and the version both plugin manifests carry.
///
/// It is deliberately separate from resolving the tag. These answers are
/// knowable from the tree, so a release must not have to build a candidate to
/// learn them.
pub struct CheckMarketplaceContracts<'a, F> {
    file_system: &'a F,
}

impl<'a, F: ReleaseFileSystem> CheckMarketplaceContracts<'a, F> {
    pub fn new(file_system: &'a F) -> Self {
        Self { file_system }
    }

    pub fn execute(
        &self,
        root: &RepositoryRoot,
        version: &ReleaseVersion,
        repository_url: &str,
    ) -> Result<(), ReleaseError> {
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
                "Claude marketplace must resolve {repository_url}/plugins/kmp through clonable immutable tag {release_ref}, not {}",
                claude_plugin.source.reference
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
        Ok(())
    }

    fn one_claude_plugin(
        catalog: &ClaudeMarketplaceDto,
    ) -> Result<&ClaudeMarketplacePluginDto, ReleaseError> {
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
    ) -> Result<&CodexMarketplacePluginDto, ReleaseError> {
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

    fn read_json<T: DeserializeOwned>(&self, path: &Path) -> Result<T, ReleaseError> {
        serde_json::from_str(&self.file_system.read_text(path)?).map_err(|error| {
            ReleaseError::invalid(format!("cannot read {}: {error}", path.display()))
        })
    }
}
