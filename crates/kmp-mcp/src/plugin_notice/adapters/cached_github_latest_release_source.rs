use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::blocking::Client;
use serde_json::Value;

use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::plugin_notice::ports::latest_release_source::LatestReleaseSource;

/// GitHub adapter with a one-day, fail-open machine-local cache.
pub struct CachedGithubLatestReleaseSource {
    client: Option<Client>,
    cache: PathBuf,
}

impl CachedGithubLatestReleaseSource {
    const MAX_AGE: Duration = Duration::from_secs(86_400);

    pub fn from_environment() -> Self {
        let cache_home = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
            .unwrap_or_else(|| PathBuf::from(".cache"));
        let client = Client::builder()
            .user_agent("kmp-version-notice")
            .connect_timeout(Duration::from_secs(1))
            .timeout(Duration::from_secs(2))
            .build()
            .ok();
        Self {
            client,
            cache: cache_home.join("kmp/latest-release"),
        }
    }

    fn injected() -> Option<Option<ReleaseVersion>> {
        std::env::var("KMP_LATEST_VERSION")
            .ok()
            .map(|value| ReleaseVersion::parse(&value).ok())
    }

    fn cached(&self, now: u64) -> Option<ReleaseVersion> {
        let content = fs::read_to_string(&self.cache).ok()?;
        let mut fields = content.split_whitespace();
        let checked_at = fields.next()?.parse::<u64>().ok()?;
        if now.saturating_sub(checked_at) >= Self::MAX_AGE.as_secs() {
            return None;
        }
        ReleaseVersion::parse(fields.next()?).ok()
    }

    fn remote(&self) -> Option<ReleaseVersion> {
        let response = self
            .client
            .as_ref()?
            .get("https://api.github.com/repos/underpass-ai/kmp/releases/latest")
            .send()
            .ok()?
            .error_for_status()
            .ok()?;
        let body: Value = response.json().ok()?;
        ReleaseVersion::parse(body["tag_name"].as_str()?).ok()
    }

    fn store(&self, now: u64, version: &ReleaseVersion) {
        let Some(parent) = self.cache.parent() else {
            return;
        };
        if fs::create_dir_all(parent).is_err() {
            return;
        }
        let temporary = self.cache.with_extension("tmp");
        if fs::write(&temporary, format!("{now} {version}\n")).is_ok() {
            let _ = fs::rename(temporary, &self.cache);
        }
    }
}

impl LatestReleaseSource for CachedGithubLatestReleaseSource {
    fn latest(&self) -> Option<ReleaseVersion> {
        if let Some(injected) = Self::injected() {
            return injected;
        }
        if std::env::var("KMP_VERSION_CHECK").is_ok_and(|value| value == "off") {
            return None;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if let Some(cached) = self.cached(now) {
            return Some(cached);
        }
        let remote = self.remote()?;
        self.store(now, &remote);
        Some(remote)
    }
}
