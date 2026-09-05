use crate::domain::release_version::ReleaseVersion;

/// The one lexical-bridge table every release publishes, unversioned in its
/// name because it is the same bytes for every platform and most releases
/// republish it unchanged; `kmp-mcp setup` decides by digest, not by name.
pub const LEXICAL_BRIDGE_ASSET: &str = "kmp-lexical-bridge.kmpb";

/// The exact files a release publishes: five engines, one MCPB, four plugin
/// packages and one lexical-bridge table, each with its `.sha256` beside it.
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
        names.extend([
            LEXICAL_BRIDGE_ASSET.to_string(),
            format!("{LEXICAL_BRIDGE_ASSET}.sha256"),
        ]);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `kmp-mcp setup` downloads this exact name from every release; the
    /// publication waits for exactly this many files.
    #[test]
    fn a_release_publishes_twenty_two_files_including_the_lexical_bridge() {
        let version = ReleaseVersion::parse("0.13.0").expect("version");
        let assets = CandidateAssetSet::for_version(&version);

        assert_eq!(assets.all().len(), 22);
        assert!(
            assets
                .all()
                .iter()
                .any(|name| name == "kmp-lexical-bridge.kmpb")
        );
        assert!(
            assets
                .all()
                .iter()
                .any(|name| name == "kmp-lexical-bridge.kmpb.sha256")
        );
        assert_eq!(assets.payloads().count(), 11);
    }
}
