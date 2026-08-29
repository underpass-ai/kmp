use crate::lifecycle::domain::release_version::ReleaseVersion;

/// Domain decision for a non-mutating session-start notice.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PluginNotice {
    Quiet,
    Misaligned {
        engine: ReleaseVersion,
        plugin: ReleaseVersion,
    },
    UpdateAvailable {
        installed: ReleaseVersion,
        latest: ReleaseVersion,
    },
}

impl PluginNotice {
    pub fn misaligned(engine: ReleaseVersion, plugin: ReleaseVersion) -> Self {
        Self::Misaligned { engine, plugin }
    }

    pub fn from_latest(installed: ReleaseVersion, latest: Option<ReleaseVersion>) -> Self {
        match latest {
            Some(latest) if latest.is_newer_than(&installed) => {
                Self::UpdateAvailable { installed, latest }
            }
            _ => Self::Quiet,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_numerically_newer_release_is_actionable() {
        let notice = PluginNotice::from_latest(
            ReleaseVersion::parse("0.9.0").expect("installed"),
            Some(ReleaseVersion::parse("0.10.0").expect("latest")),
        );
        assert!(matches!(notice, PluginNotice::UpdateAvailable { .. }));
    }
}
