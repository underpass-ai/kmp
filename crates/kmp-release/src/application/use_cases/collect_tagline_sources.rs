use serde_json::Value;

use crate::domain::release_error::ReleaseError;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::tagline_source::TaglineSource;
use crate::ports::release_file_system::ReleaseFileSystem;

/// Reads back every file a storefront takes the product's one-line
/// description from, against the sentence the workspace manifest declares.
/// Each storefront — crates.io, the MCP Registry, the MCPB installer, both
/// plugin marketplaces, the ChatGPT app directory — reads exactly one of
/// them, so this is the list of places the product can quietly become
/// several products. It reports what it found; the readiness check decides.
pub struct CollectTaglineSources<'a, F> {
    file_system: &'a F,
}

impl<'a, F: ReleaseFileSystem> CollectTaglineSources<'a, F> {
    pub fn new(file_system: &'a F) -> Self {
        Self { file_system }
    }

    /// The tagline is whatever `[workspace.package] description` says.
    pub fn execute(&self, root: &RepositoryRoot) -> Result<Vec<TaglineSource>, ReleaseError> {
        let cargo = self.file_system.read_text(&root.join("Cargo.toml"))?;
        let tagline = Self::workspace_description(&cargo).ok_or_else(|| {
            ReleaseError::invalid(
                "Cargo.toml [workspace.package] has no description; it is the tagline every \
                 storefront opens with",
            )
        })?;
        let mut sources = Vec::new();

        let crate_manifest = self
            .file_system
            .read_text(&root.join("crates/kmp-mcp/Cargo.toml"))?;
        sources.push(TaglineSource::new(
            "crates/kmp-mcp/Cargo.toml description (crates.io)",
            &tagline,
            Self::package_description(&crate_manifest).unwrap_or_default(),
        ));

        for (label, relative, path) in [
            (
                "server.json description (MCP Registry)",
                "server.json",
                &["description"][..],
            ),
            (
                "distribution/mcpb/manifest.json description (MCPB installer)",
                "distribution/mcpb/manifest.json",
                &["description"][..],
            ),
            (
                "plugins/kmp/.claude-plugin/plugin.json description (Claude Code)",
                "plugins/kmp/.claude-plugin/plugin.json",
                &["description"][..],
            ),
            (
                "plugins/kmp/.codex-plugin/plugin.json description (Codex)",
                "plugins/kmp/.codex-plugin/plugin.json",
                &["description"][..],
            ),
            (
                "plugins/kmp/.codex-plugin/plugin.json interface.shortDescription (Codex)",
                "plugins/kmp/.codex-plugin/plugin.json",
                &["interface", "shortDescription"][..],
            ),
            (
                ".claude-plugin/marketplace.json plugin description (Claude marketplace)",
                ".claude-plugin/marketplace.json",
                &["plugins", "0", "description"][..],
            ),
            (
                "chatgpt-app-submission.json app_info.subtitle (ChatGPT apps)",
                "chatgpt-app-submission.json",
                &["app_info", "subtitle"][..],
            ),
        ] {
            let body = self.read_json(root, relative)?;
            sources.push(TaglineSource::new(
                label,
                &tagline,
                Self::string_at(&body, path),
            ));
        }
        Ok(sources)
    }

    fn read_json(&self, root: &RepositoryRoot, relative: &str) -> Result<Value, ReleaseError> {
        let path = root.join(relative);
        serde_json::from_str(&self.file_system.read_text(&path)?)
            .map_err(|error| ReleaseError::invalid(format!("{relative} is invalid: {error}")))
    }

    fn workspace_description(cargo: &str) -> Option<String> {
        Self::description_in_table(cargo, "[workspace.package]")
    }

    fn package_description(cargo: &str) -> Option<String> {
        Self::description_in_table(cargo, "[package]")
    }

    fn description_in_table(cargo: &str, table: &str) -> Option<String> {
        let mut inside = false;
        for raw in cargo.lines() {
            let line = raw.trim();
            if line.starts_with('[') {
                inside = line == table;
                continue;
            }
            if inside && let Some(value) = line.strip_prefix("description = ") {
                return Some(value.trim_matches('"').to_string());
            }
        }
        None
    }

    fn string_at(body: &Value, path: &[&str]) -> String {
        let mut current = body;
        for key in path {
            current = match key.parse::<usize>() {
                Ok(index) => &current[index],
                Err(_) => &current[*key],
            };
        }
        current.as_str().unwrap_or_default().to_string()
    }
}
