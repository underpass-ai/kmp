use std::fmt;
use std::str::FromStr;

use serde::Serialize;

use super::lifecycle_error::LifecycleError;

/// A validated KMP release identity, without a leading `v`.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ReleaseVersion(String);

impl ReleaseVersion {
    pub fn parse(raw: &str) -> Result<Self, LifecycleError> {
        let value = raw.strip_prefix('v').unwrap_or(raw).trim();
        let mut core_and_pre = value.splitn(2, '+');
        let version = core_and_pre.next().unwrap_or_default();
        let metadata = core_and_pre.next();
        let mut core_and_pre = version.splitn(2, '-');
        let core = core_and_pre.next().unwrap_or_default();
        let pre = core_and_pre.next();
        let core_is_valid = core.split('.').count() == 3
            && core
                .split('.')
                .all(|piece| !piece.is_empty() && piece.bytes().all(|byte| byte.is_ascii_digit()));
        let suffix_is_valid = |suffix: Option<&str>| {
            suffix.is_none_or(|suffix| {
                !suffix.is_empty()
                    && suffix
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
            })
        };
        if !core_is_valid || !suffix_is_valid(pre) || !suffix_is_valid(metadata) {
            return Err(LifecycleError::InvalidReleaseVersion(raw.to_string()));
        }
        Ok(Self(value.to_string()))
    }

    pub fn current() -> Self {
        Self(env!("CARGO_PKG_VERSION").to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn tag(&self) -> String {
        format!("v{}", self.0)
    }

    pub fn engine_version(&self) -> &str {
        self.0.split('+').next().unwrap_or(&self.0)
    }

    pub fn represents_same_release(&self, other: &Self) -> bool {
        self.engine_version() == other.engine_version()
    }

    pub fn is_newer_than(&self, other: &Self) -> bool {
        self.core_numbers() > other.core_numbers()
    }

    fn core_numbers(&self) -> (u64, u64, u64) {
        let mut numbers = self
            .engine_version()
            .split('-')
            .next()
            .unwrap_or_default()
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or_default());
        (
            numbers.next().unwrap_or_default(),
            numbers.next().unwrap_or_default(),
            numbers.next().unwrap_or_default(),
        )
    }
}

impl fmt::Display for ReleaseVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for ReleaseVersion {
    type Err = LifecycleError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_not_a_free_form_string() {
        assert_eq!(
            ReleaseVersion::parse("v0.5.1").expect("version").as_str(),
            "0.5.1"
        );
        assert!(ReleaseVersion::parse("main").is_err());
        assert!(ReleaseVersion::parse("0.5").is_err());
    }

    #[test]
    fn host_build_metadata_still_represents_the_release_engine() {
        let release = ReleaseVersion::parse("0.5.1").expect("release");
        let host = ReleaseVersion::parse("0.5.1+codex.42").expect("host");
        assert!(release.represents_same_release(&host));
    }

    #[test]
    fn release_order_is_numeric_instead_of_lexicographic() {
        let newer = ReleaseVersion::parse("0.10.0").expect("newer");
        let older = ReleaseVersion::parse("0.9.9").expect("older");
        assert!(newer.is_newer_than(&older));
        assert!(!older.is_newer_than(&newer));
    }
}
