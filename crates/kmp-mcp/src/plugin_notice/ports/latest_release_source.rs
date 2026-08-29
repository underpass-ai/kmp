use crate::lifecycle::domain::release_version::ReleaseVersion;

/// Fail-open outbound port: no release means no session-start notice.
pub trait LatestReleaseSource {
    fn latest(&self) -> Option<ReleaseVersion>;
}
