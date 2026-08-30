use serde_json::Value;

use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::version_source::VersionSource;
use crate::ports::release_file_system::ReleaseFileSystem;

/// Reads back every surface `version prepare` writes, so a release can be told
/// which one was left behind instead of discovering it one failing step at a
/// time. It reports what it found; deciding what that means belongs to the
/// caller.
pub struct CollectVersionSources<'a, F> {
    file_system: &'a F,
}

impl<'a, F: ReleaseFileSystem> CollectVersionSources<'a, F> {
    pub fn new(file_system: &'a F) -> Self {
        Self { file_system }
    }

    pub fn execute(
        &self,
        root: &RepositoryRoot,
        version: &ReleaseVersion,
    ) -> Result<Vec<VersionSource>, ReleaseError> {
        let plain = version.as_str();
        let mut sources = Vec::new();

        let cargo = self.file_system.read_text(&root.join("Cargo.toml"))?;
        sources.push(VersionSource::new(
            "Cargo.toml [workspace.package] version",
            plain,
            Self::workspace_version(&cargo),
        ));
        sources.push(VersionSource::new(
            "Cargo.toml internal dependency pins",
            plain,
            Self::dependency_pins(&cargo),
        ));

        let chart = self
            .file_system
            .read_text(&root.join("distribution/charts/kmp/Chart.yaml"))?;
        sources.push(VersionSource::new(
            "Chart.yaml version",
            plain,
            Self::yaml_value(&chart, "version:"),
        ));
        sources.push(VersionSource::new(
            "Chart.yaml appVersion",
            plain,
            Self::yaml_value(&chart, "appVersion:"),
        ));

        for (label, relative) in [
            (
                "plugins/kmp/.claude-plugin/plugin.json version",
                "plugins/kmp/.claude-plugin/plugin.json",
            ),
            (
                "plugins/kmp/.codex-plugin/plugin.json version",
                "plugins/kmp/.codex-plugin/plugin.json",
            ),
            (
                "distribution/mcpb/manifest.json version",
                "distribution/mcpb/manifest.json",
            ),
        ] {
            let body = self.read_json(root, relative)?;
            sources.push(VersionSource::new(
                label,
                plain,
                Self::build_metadata_stripped(Self::string_at(&body, &["version"])),
            ));
        }

        let server = self.read_json(root, "server.json")?;
        sources.push(VersionSource::new(
            "server.json version",
            plain,
            Self::string_at(&server, &["version"]),
        ));
        sources.push(VersionSource::new(
            "server.json cargo package version",
            plain,
            Self::registry_field(&server, "cargo", "version"),
        ));
        sources.push(VersionSource::new(
            "server.json MCPB identifier",
            format!(
                "https://github.com/underpass-ai/kmp/releases/download/{tag}/kmp-mcp-{tag}.mcpb",
                tag = version.tag()
            ),
            Self::registry_field(&server, "mcpb", "identifier"),
        ));

        let guide = self
            .file_system
            .read_text(&root.join("plugins/kmp/guide/memory.jsonl"))?;
        sources.push(VersionSource::new(
            "guide envelope kernel_version",
            plain,
            Self::guide_kernel_version(&guide),
        ));

        let catalog = self.read_json(root, ".claude-plugin/marketplace.json")?;
        sources.push(VersionSource::new(
            ".claude-plugin/marketplace.json ref",
            version.tag(),
            Self::catalog_reference(&catalog),
        ));

        Ok(sources)
    }

    fn read_json(&self, root: &RepositoryRoot, relative: &str) -> Result<Value, ReleaseError> {
        let path = root.join(relative);
        serde_json::from_str(&self.file_system.read_text(&path)?)
            .map_err(|error| ReleaseError::invalid(format!("{relative} is invalid: {error}")))
    }

    fn workspace_version(cargo: &str) -> String {
        let mut in_workspace_package = false;
        for raw in cargo.lines() {
            let line = raw.trim();
            if line.starts_with('[') {
                in_workspace_package = line == "[workspace.package]";
                continue;
            }
            if in_workspace_package && let Some(value) = line.strip_prefix("version = ") {
                return value.trim_matches('"').to_string();
            }
        }
        "absent".to_string()
    }

    /// Every `kmp-*` pin collapses to one reported value while they agree; a
    /// half-bumped set reports the versions it actually found.
    fn dependency_pins(cargo: &str) -> String {
        let mut found = Vec::new();
        for line in cargo.lines() {
            if line.starts_with("kmp-")
                && line.contains("path = \"crates/")
                && let Some(value) = Self::quoted_after(line, "version = \"")
                && !found.contains(&value)
            {
                found.push(value);
            }
        }
        if found.is_empty() {
            "absent".to_string()
        } else {
            found.join(", ")
        }
    }

    fn yaml_value(chart: &str, key: &str) -> String {
        chart
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .map(|value| value.trim().trim_matches('"').to_string())
            .unwrap_or_else(|| "absent".to_string())
    }

    fn guide_kernel_version(bundle: &str) -> String {
        bundle
            .lines()
            .next()
            .and_then(|header| serde_json::from_str::<Value>(header).ok())
            .map(|header| Self::string_at(&header, &["kernel_version"]))
            .unwrap_or_else(|| "absent".to_string())
    }

    fn catalog_reference(catalog: &Value) -> String {
        catalog["plugins"]
            .as_array()
            .and_then(|plugins| {
                plugins
                    .iter()
                    .find(|plugin| plugin["name"] == "kmp")
                    .map(|plugin| Self::string_at(plugin, &["source", "ref"]))
            })
            .unwrap_or_else(|| "absent".to_string())
    }

    fn registry_field(server: &Value, registry: &str, field: &str) -> String {
        server["packages"]
            .as_array()
            .and_then(|packages| {
                packages
                    .iter()
                    .find(|package| package["registryType"] == registry)
                    .map(|package| Self::string_at(package, &[field]))
            })
            .unwrap_or_else(|| "absent".to_string())
    }

    fn string_at(body: &Value, path: &[&str]) -> String {
        let mut cursor = body;
        for key in path {
            cursor = &cursor[key];
        }
        cursor
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| "absent".to_string())
    }

    /// Plugin manifests may carry SemVer build metadata that the release does
    /// not own; only the release core has to agree.
    fn build_metadata_stripped(value: String) -> String {
        value
            .split_once('+')
            .map_or(value.clone(), |pair| pair.0.to_string())
    }

    fn quoted_after(line: &str, marker: &str) -> Option<String> {
        let start = line.find(marker)? + marker.len();
        let end = line[start..].find('"')? + start;
        Some(line[start..end].to_string())
    }
}
