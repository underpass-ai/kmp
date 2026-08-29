use crate::domain::release_version::ReleaseVersion;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CandidateAssetSet {
    names: Vec<String>,
}

impl CandidateAssetSet {
    pub fn for_version(version: &ReleaseVersion) -> Self {
        const TARGETS: [&str; 5] = [
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
            "x86_64-apple-darwin",
            "aarch64-apple-darwin",
            "x86_64-pc-windows-msvc",
        ];
        const PLUGIN_LABELS: [&str; 4] = [
            "linux-x86_64",
            "linux-arm64",
            "macos-arm64",
            "windows-x86_64",
        ];
        let mut names = Vec::new();
        for target in TARGETS {
            let suffix = if target.ends_with("windows-msvc") {
                ".exe"
            } else {
                ""
            };
            let name = format!("kmp-mcp-v{version}-{target}{suffix}");
            names.extend([name.clone(), format!("{name}.sha256")]);
        }
        let mcpb = format!("kmp-mcp-v{version}.mcpb");
        names.extend([mcpb.clone(), format!("{mcpb}.sha256")]);
        for label in PLUGIN_LABELS {
            let name = format!("kmp-plugin-{version}-{label}.tar.gz");
            names.extend([name.clone(), format!("{name}.sha256")]);
        }
        names.sort();
        Self { names }
    }

    pub fn all(&self) -> &[String] {
        &self.names
    }

    pub fn payloads(&self) -> impl Iterator<Item = &str> {
        self.names
            .iter()
            .filter(|name| !name.ends_with(".sha256"))
            .map(String::as_str)
    }

    pub fn matches(&self, actual: &[String]) -> bool {
        self.names == actual
    }
}
