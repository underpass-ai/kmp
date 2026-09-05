use reqwest::blocking::Client;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::lifecycle::domain::engine_artifact::EngineArtifact;
use crate::lifecycle::domain::lexical_bridge_artifact::LexicalBridgeArtifact;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::lifecycle::ports::release_repository::ReleaseRepository;

/// GitHub Releases adapter for immutable, checksummed KMP engines.
pub struct GithubReleaseRepository {
    client: Client,
}

impl GithubReleaseRepository {
    pub fn new() -> Result<Self, LifecycleError> {
        let client = Client::builder()
            .user_agent("kmp-lifecycle")
            .build()
            .map_err(|error| LifecycleError::Network(error.to_string()))?;
        Ok(Self { client })
    }

    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, LifecycleError> {
        self.client
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .and_then(reqwest::blocking::Response::bytes)
            .map(|bytes| bytes.to_vec())
            .map_err(|error| LifecycleError::Network(format!("could not download {url}: {error}")))
    }

    /// A response the caller must be able to tell apart from a network
    /// fault: a release that publishes no table is not a broken release.
    fn get_optional_bytes(&self, url: &str) -> Result<Option<Vec<u8>>, LifecycleError> {
        let response =
            self.client.get(url).send().map_err(|error| {
                LifecycleError::Network(format!("could not reach {url}: {error}"))
            })?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        response
            .error_for_status()
            .and_then(reqwest::blocking::Response::bytes)
            .map(|bytes| Some(bytes.to_vec()))
            .map_err(|error| LifecycleError::Network(format!("could not download {url}: {error}")))
    }

    fn download_url(version: &ReleaseVersion, asset: &str) -> String {
        format!(
            "https://github.com/underpass-ai/kmp/releases/download/{}/{}",
            version.tag(),
            asset
        )
    }

    /// The published digest beside an asset, with the trailing filename that
    /// `sha256sum` writes stripped off.
    fn published_checksum(raw: Vec<u8>, asset: &str) -> Result<String, LifecycleError> {
        String::from_utf8(raw)
            .map_err(|error| LifecycleError::Network(error.to_string()))?
            .split_whitespace()
            .next()
            .map(str::to_string)
            .ok_or_else(|| LifecycleError::Network(format!("checksum for {asset} is empty")))
    }

    fn require_digest(bytes: &[u8], published: &str, asset: &str) -> Result<(), LifecycleError> {
        let actual = format!("{:x}", Sha256::digest(bytes));
        if published == actual {
            return Ok(());
        }
        Err(LifecycleError::Network(format!(
            "checksum mismatch for {asset}: published {published}, downloaded {actual}"
        )))
    }

    fn asset_name(version: &ReleaseVersion) -> Result<String, LifecycleError> {
        let target = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
            ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
            ("macos", "aarch64") => "aarch64-apple-darwin",
            ("macos", "x86_64") => "x86_64-apple-darwin",
            (os, arch) => {
                return Err(LifecycleError::UnsupportedPlatform(format!(
                    "release {} has no declared lifecycle engine for {os}-{arch}; install it with cargo",
                    version
                )));
            }
        };
        Ok(format!("kmp-mcp-v{}-{target}", version.engine_version()))
    }
}

/// The table is one artifact for every platform, and every release
/// republishes it unchanged, so a client decides by digest rather than by
/// version. Changing this name changes where existing installations look.
const LEXICAL_BRIDGE_ASSET: &str = "kmp-lexical-bridge.kmpb";

impl ReleaseRepository for GithubReleaseRepository {
    fn latest(&self) -> Result<ReleaseVersion, LifecycleError> {
        let response = self
            .client
            .get("https://api.github.com/repos/underpass-ai/kmp/releases/latest")
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|error| LifecycleError::Network(error.to_string()))?;
        let body: Value = response
            .json()
            .map_err(|error| LifecycleError::Network(error.to_string()))?;
        let tag = body["tag_name"].as_str().ok_or_else(|| {
            LifecycleError::Network("latest GitHub release omitted tag_name".to_string())
        })?;
        ReleaseVersion::parse(tag)
    }

    fn engine(&self, version: &ReleaseVersion) -> Result<EngineArtifact, LifecycleError> {
        let asset = Self::asset_name(version)?;
        let base = Self::download_url(version, &asset);
        let bytes = self.get_bytes(&base)?;
        let published =
            Self::published_checksum(self.get_bytes(&format!("{base}.sha256"))?, &asset)?;
        Self::require_digest(&bytes, &published, &asset)?;
        Ok(EngineArtifact::verified(version.clone(), bytes))
    }

    fn lexical_bridge_checksum(
        &self,
        version: &ReleaseVersion,
    ) -> Result<Option<String>, LifecycleError> {
        let url = Self::download_url(version, &format!("{LEXICAL_BRIDGE_ASSET}.sha256"));
        match self.get_optional_bytes(&url)? {
            Some(raw) => Self::published_checksum(raw, LEXICAL_BRIDGE_ASSET).map(Some),
            None => Ok(None),
        }
    }

    fn lexical_bridge(
        &self,
        version: &ReleaseVersion,
        published: &str,
    ) -> Result<LexicalBridgeArtifact, LifecycleError> {
        let bytes = self.get_bytes(&Self::download_url(version, LEXICAL_BRIDGE_ASSET))?;
        Self::require_digest(&bytes, published, LEXICAL_BRIDGE_ASSET)?;
        Ok(LexicalBridgeArtifact::verified(
            bytes,
            published.to_string(),
            format!("{LEXICAL_BRIDGE_ASSET} from {}", version.tag()),
        ))
    }
}
