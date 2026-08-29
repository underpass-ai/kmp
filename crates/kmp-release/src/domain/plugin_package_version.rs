use std::fmt::{Display, Formatter};

use crate::domain::release_version::ReleaseVersion;
use crate::domain::source_commit::SourceCommit;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PluginPackageVersion(String);

impl PluginPackageVersion {
    pub fn release(version: &ReleaseVersion) -> Self {
        Self(version.to_string())
    }

    pub fn development(version: &ReleaseVersion, commit: &SourceCommit) -> Self {
        let short = &commit.as_str()[..12];
        Self(format!("{version}+{short}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for PluginPackageVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}
