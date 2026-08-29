use reqwest::blocking::Client;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::lifecycle::domain::engine_artifact::EngineArtifact;
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
        let base = format!(
            "https://github.com/underpass-ai/kmp/releases/download/{}/{}",
            version.tag(),
            asset
        );
        let bytes = self.get_bytes(&base)?;
        let checksum = String::from_utf8(self.get_bytes(&format!("{base}.sha256"))?)
            .map_err(|error| LifecycleError::Network(error.to_string()))?;
        let published = checksum
            .split_whitespace()
            .next()
            .ok_or_else(|| LifecycleError::Network(format!("checksum for {asset} is empty")))?;
        let actual = format!("{:x}", Sha256::digest(&bytes));
        if published != actual {
            return Err(LifecycleError::Network(format!(
                "checksum mismatch for {asset}: published {published}, downloaded {actual}"
            )));
        }
        Ok(EngineArtifact::verified(version.clone(), bytes))
    }
}
