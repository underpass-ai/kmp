use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::blocking::Client;
use serde_json::Value;

use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::plugin_notice::ports::latest_release_source::LatestReleaseSource;

/// The switch that turns the version check off.
pub const VERSION_CHECK_ENV: &str = "KMP_VERSION_CHECK";

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

    /// The release a test injects in place of GitHub's (a debug or
    /// `eval-hooks` build only; a release binary has no such hook).
    fn injected() -> Option<Option<ReleaseVersion>> {
        crate::environment::eval_hooks::latest_version()
            .map(|value| ReleaseVersion::parse(&value).ok())
    }

    /// `KMP_VERSION_CHECK` off: no call to GitHub. It reads the on/off words
    /// every switch reads, case-insensitively; an unknown value never gets
    /// here, because the verb refuses it at startup.
    fn check_disabled() -> bool {
        crate::serving::environment::optional_env_string(VERSION_CHECK_ENV)
            .as_deref()
            .and_then(crate::environment::on_off::parse_on_off)
            == Some(false)
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
        if Self::check_disabled() {
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
