use serde_json::Value;

use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::version_preparation::VersionPreparation;
use crate::ports::release_file_system::ReleaseFileSystem;

/// Prepares every versioned release surface as one validated write set.
pub struct PrepareReleaseVersion<'a, F> {
    file_system: &'a F,
}

impl<'a, F: ReleaseFileSystem> PrepareReleaseVersion<'a, F> {
    pub fn new(file_system: &'a F) -> Self {
        Self { file_system }
    }

    pub fn execute(
        &self,
        root: &RepositoryRoot,
        version: &ReleaseVersion,
    ) -> Result<VersionPreparation, ReleaseError> {
        let cargo_path = root.join("Cargo.toml");
        let (cargo, dependency_count) =
            Self::cargo_manifest(&self.file_system.read_text(&cargo_path)?, version)?;
        let chart_path = root.join("distribution/charts/kmp/Chart.yaml");
        let chart = Self::chart(&self.file_system.read_text(&chart_path)?, version)?;

        let manifest_paths = [
            root.join("plugins/kmp/.claude-plugin/plugin.json"),
            root.join("plugins/kmp/.codex-plugin/plugin.json"),
        ];
        let manifests = manifest_paths
            .iter()
            .map(|path| {
                self.file_system
                    .read_text(path)
                    .and_then(|text| Self::plugin_manifest(&text, version))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let server_path = root.join("server.json");
        let (server, hash_reset) =
            Self::server_manifest(&self.file_system.read_text(&server_path)?, version)?;
        let mcpb_path = root.join("distribution/mcpb/manifest.json");
        let mcpb = Self::json_version(
            &self.file_system.read_text(&mcpb_path)?,
            version,
            "MCPB manifest",
        )?;

        // No file changes until every input has parsed and validated.
        self.file_system.write_text(&cargo_path, &cargo)?;
        self.file_system.write_text(&chart_path, &chart)?;
        for (path, content) in manifest_paths.iter().zip(manifests) {
            self.file_system.write_text(path, &content)?;
        }
        self.file_system.write_text(&server_path, &server)?;
        self.file_system.write_text(&mcpb_path, &mcpb)?;
        Ok(VersionPreparation::new(dependency_count, hash_reset))
    }

    fn cargo_manifest(
        text: &str,
        version: &ReleaseVersion,
    ) -> Result<(String, usize), ReleaseError> {
        let mut in_workspace_package = false;
        let mut workspace_replaced = false;
        let mut dependency_count = 0;
        let mut output = Vec::new();
        for line in text.lines() {
            if line.starts_with('[') {
                in_workspace_package = line == "[workspace.package]";
            }
            if in_workspace_package && !workspace_replaced && line.starts_with("version = ") {
                output.push(format!("version = \"{version}\""));
                workspace_replaced = true;
            } else if line.starts_with("kmp-")
                && line.contains("path = \"crates/")
                && line.contains("version = \"")
            {
                output.push(Self::replace_quoted_value(
                    line,
                    "version = \"",
                    version.as_str(),
                )?);
                dependency_count += 1;
            } else {
                output.push(line.to_string());
            }
        }
        if !workspace_replaced || dependency_count == 0 {
            return Err(ReleaseError::invalid(
                "Cargo.toml omitted the workspace version or internal dependency pins",
            ));
        }
        Ok((format!("{}\n", output.join("\n")), dependency_count))
    }

    fn chart(text: &str, version: &ReleaseVersion) -> Result<String, ReleaseError> {
        let mut chart = false;
        let mut app = false;
        let lines = text
            .lines()
            .map(|line| {
                if !chart && line.starts_with("version:") {
                    chart = true;
                    format!("version: {version}")
                } else if !app && line.starts_with("appVersion:") {
                    app = true;
                    format!("appVersion: \"{version}\"")
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>();
        if !chart || !app {
            return Err(ReleaseError::invalid(
                "Chart.yaml omitted version or appVersion",
            ));
        }
        Ok(format!("{}\n", lines.join("\n")))
    }

    fn plugin_manifest(text: &str, version: &ReleaseVersion) -> Result<String, ReleaseError> {
        let body: Value = serde_json::from_str(text).map_err(|error| {
            ReleaseError::invalid(format!("plugin manifest is invalid: {error}"))
        })?;
        if body["version"].as_str().is_none() {
            return Err(ReleaseError::invalid("plugin manifest omitted version"));
        }
        Self::replace_json_version_line(text, version, "plugin manifest")
    }

    fn replace_json_version_line(
        text: &str,
        version: &ReleaseVersion,
        label: &str,
    ) -> Result<String, ReleaseError> {
        let mut replaced = false;
        let lines = text
            .lines()
            .map(|line| {
                if !replaced && line.trim_start().starts_with("\"version\"") {
                    replaced = true;
                    let indentation = &line[..line.len() - line.trim_start().len()];
                    let comma = if line.trim_end().ends_with(',') {
                        ","
                    } else {
                        ""
                    };
                    format!("{indentation}\"version\": \"{version}\"{comma}")
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>();
        if !replaced {
            return Err(ReleaseError::invalid(format!("{label} omitted version")));
        }
        Ok(format!("{}\n", lines.join("\n")))
    }

    fn server_manifest(
        text: &str,
        version: &ReleaseVersion,
    ) -> Result<(String, bool), ReleaseError> {
        let mut body: Value = serde_json::from_str(text)
            .map_err(|error| ReleaseError::invalid(format!("server.json is invalid: {error}")))?;
        let previous = body["version"].as_str().unwrap_or_default().to_string();
        body["version"] = Value::String(version.to_string());
        let packages = body["packages"]
            .as_array_mut()
            .ok_or_else(|| ReleaseError::invalid("server.json packages must be an array"))?;
        let mut cargo_count = 0;
        let mut mcpb_count = 0;
        let mut reset = false;
        for package in packages {
            match package["registryType"].as_str() {
                Some("cargo") => {
                    package["version"] = Value::String(version.to_string());
                    cargo_count += 1;
                }
                Some("mcpb") => {
                    let identifier = format!(
                        "https://github.com/underpass-ai/kmp/releases/download/{}/kmp-mcp-{}.mcpb",
                        version.tag(),
                        version.tag()
                    );
                    if previous != version.as_str()
                        || package["identifier"].as_str() != Some(identifier.as_str())
                    {
                        package["fileSha256"] = Value::String("0".repeat(64));
                        reset = true;
                    }
                    package["identifier"] = Value::String(identifier);
                    mcpb_count += 1;
                }
                _ => {}
            }
        }
        if cargo_count != 1 || mcpb_count != 1 {
            return Err(ReleaseError::invalid(
                "server.json must contain exactly one cargo and one MCPB package",
            ));
        }
        let encoded = serde_json::to_string_pretty(&body).map_err(|error| {
            ReleaseError::invalid(format!("cannot encode server.json: {error}"))
        })?;
        Ok((format!("{encoded}\n"), reset))
    }

    fn json_version(
        text: &str,
        version: &ReleaseVersion,
        label: &str,
    ) -> Result<String, ReleaseError> {
        let body: Value = serde_json::from_str(text)
            .map_err(|error| ReleaseError::invalid(format!("{label} is invalid: {error}")))?;
        if body["version"].as_str().is_none() {
            return Err(ReleaseError::invalid(format!("{label} omitted version")));
        }
        Self::replace_json_version_line(text, version, label)
    }

    fn replace_quoted_value(line: &str, marker: &str, value: &str) -> Result<String, ReleaseError> {
        let start = line
            .find(marker)
            .map(|position| position + marker.len())
            .ok_or_else(|| ReleaseError::invalid(format!("line omitted `{marker}`")))?;
        let end = line[start..]
            .find('"')
            .map(|position| start + position)
            .ok_or_else(|| ReleaseError::invalid("version value is not quoted"))?;
        Ok(format!("{}{}{}", &line[..start], value, &line[end..]))
    }
}
