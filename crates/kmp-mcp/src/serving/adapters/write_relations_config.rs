use std::path::Path;

use serde::Deserialize;

use crate::curate::domain::lifecycle_mode::LifecycleMode;

/// `write-relations.json`: relations proposed after each write, beside
/// `typesafe.json`. `{}` turns them on. `lifecycle` (`off` by default,
/// `rule` or `jev`) also proposes, in every focused review of the store,
/// the current facts a new one may replace: same principal anchor and entry
/// kind, read first by Jev under `jev` (DESIGN L4 4e).
#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct WriteRelationsConfig {
    #[serde(default)]
    lifecycle: Option<String>,
}

impl WriteRelationsConfig {
    pub(super) const FILE: &'static str = "write-relations.json";

    /// `None` without the file. A file that cannot be read as this
    /// configuration still opts in, with the defaults: the opt-in is the
    /// file, as it always was.
    pub(super) fn load(data_dir: &Path) -> Option<Self> {
        let bytes = std::fs::read(data_dir.join(Self::FILE)).ok()?;
        Some(serde_json::from_slice(&bytes).unwrap_or_default())
    }

    /// The lifecycle mode the file names; off when it names none it knows.
    pub(super) fn lifecycle(&self) -> LifecycleMode {
        self.lifecycle
            .as_deref()
            .and_then(LifecycleMode::named)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_opts_in_and_lifecycle_proposals_are_their_own_switch() {
        let dir = tempfile::tempdir().expect("dir");
        let write = |body: &str| {
            std::fs::write(dir.path().join(WriteRelationsConfig::FILE), body).expect("write");
            WriteRelationsConfig::load(dir.path())
        };
        assert_eq!(WriteRelationsConfig::load(dir.path()), None);
        let mode = |body: &str| write(body).map(|config| config.lifecycle());
        assert_eq!(mode("{}"), Some(LifecycleMode::Off));
        assert_eq!(mode(r#"{"lifecycle":"jev"}"#), Some(LifecycleMode::Jev));
        assert_eq!(mode(r#"{"lifecycle":"rule"}"#), Some(LifecycleMode::Rule));
        assert_eq!(mode(r#"{"lifecycle":"always"}"#), Some(LifecycleMode::Off));
        assert_eq!(mode("not json"), Some(LifecycleMode::Off));
    }
}
