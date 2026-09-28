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
    /// What the file said that was not understood, when anything was.
    #[serde(skip)]
    warning: Option<String>,
}

impl WriteRelationsConfig {
    pub(super) const FILE: &'static str = "write-relations.json";

    /// `None` without the file. A file that cannot be read as this
    /// configuration still opts in, with the defaults: the opt-in is the
    /// file, as it always was. What was not understood is kept as the
    /// warning, so the acknowledgement can say so (#886).
    pub(super) fn load(data_dir: &Path) -> Option<Self> {
        let bytes = std::fs::read(data_dir.join(Self::FILE)).ok()?;
        let mut config = serde_json::from_slice::<Self>(&bytes).unwrap_or_else(|error| Self {
            warning: Some(format!("read with the defaults: {error}")),
            ..Self::default()
        });
        if let Some(name) = config.lifecycle.as_deref()
            && LifecycleMode::named(name).is_none()
        {
            config.warning = Some(format!(
                "unknown lifecycle `{name}` (off, rule or jev): lifecycle proposals are off"
            ));
        }
        Some(config)
    }

    /// What the file said that was not understood, if anything.
    pub(super) fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
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

    fn warning_for(body: &str) -> Option<String> {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(dir.path().join(WriteRelationsConfig::FILE), body).expect("write");
        WriteRelationsConfig::load(dir.path())
            .expect("the file opts in")
            .warning()
            .map(str::to_string)
    }

    #[test]
    fn a_file_that_is_understood_carries_no_warning() {
        assert_eq!(warning_for("{}"), None);
        assert_eq!(warning_for(r#"{"lifecycle":"rule"}"#), None);
    }

    #[test]
    fn invalid_json_or_a_misspelt_key_opts_in_with_a_warning_naming_the_error() {
        let invalid = warning_for(r#"{"lifecycle":"#).expect("warning");
        assert!(invalid.starts_with("read with the defaults: "), "{invalid}");
        let misspelt = warning_for(r#"{"lifecyle":"jev"}"#).expect("warning");
        assert!(misspelt.contains("unknown field `lifecyle`"), "{misspelt}");
    }

    #[test]
    fn an_unknown_lifecycle_name_is_reported_instead_of_falling_back_silently() {
        let warning = warning_for(r#"{"lifecycle":"jevv"}"#).expect("warning");
        assert!(warning.contains("unknown lifecycle `jevv`"), "{warning}");
    }
}
