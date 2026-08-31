use crate::lifecycle::domain::engine_executable::EngineExecutable;
use crate::lifecycle::domain::found_engine::FoundEngine;
use crate::lifecycle::domain::survey_roots::{SurveyRoots, engine_file_name};
use crate::lifecycle::ports::installation_catalog::InstallationCatalog;
use crate::lifecycle::ports::plugin_engine_probe::PluginEngineProbe;

/// Use case: every `kmp-mcp` this machine carries, each asked its release,
/// with the one `PATH` order actually selects marked.
///
/// `doctor` used to report only the hosts' effective engines, so a machine
/// could be fully green with an ancient copy one `PATH` entry away (#450).
/// Finding the copies is an observation; what they mean is
/// [`super::diagnose_engines`].
pub struct SurveyEngines<'a> {
    installation: &'a dyn InstallationCatalog,
    probe: &'a dyn PluginEngineProbe,
}

impl<'a> SurveyEngines<'a> {
    pub fn new(
        installation: &'a dyn InstallationCatalog,
        probe: &'a dyn PluginEngineProbe,
    ) -> Self {
        Self {
            installation,
            probe,
        }
    }

    pub fn execute(&self, roots: &SurveyRoots) -> Vec<FoundEngine> {
        let mut found = Vec::new();
        let mut path_has_chosen = false;
        for directory in roots.engine_directories() {
            let executable = directory.join(engine_file_name());
            if !self.installation.is_file(&executable) {
                continue;
            }
            let executable = EngineExecutable::installed_at(executable);
            // The first engine in a directory PATH itself carries is the one
            // a bare `kmp-mcp` runs; everything after it is shadowed.
            let selected_by_path = !path_has_chosen && roots.is_on_path(&directory);
            path_has_chosen |= selected_by_path;
            let version = self.probe.version(&executable).ok().flatten();
            found.push(FoundEngine::new(executable, version, selected_by_path));
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::lifecycle::adapters::native_installation_catalog::NativeInstallationCatalog;
    use crate::lifecycle::domain::lifecycle_error::LifecycleError;
    use crate::lifecycle::domain::release_version::ReleaseVersion;
    use crate::lifecycle::ports::plugin_engine_probe::PluginEngineProbe;

    struct FakeProbe {
        versions: BTreeMap<PathBuf, &'static str>,
    }

    impl PluginEngineProbe for FakeProbe {
        fn version(
            &self,
            executable: &EngineExecutable,
        ) -> Result<Option<ReleaseVersion>, LifecycleError> {
            self.versions
                .get(executable.as_path())
                .map(|raw| ReleaseVersion::parse(raw))
                .transpose()
        }
    }

    fn engine_at(directory: &Path) -> PathBuf {
        std::fs::create_dir_all(directory).expect("engine directory");
        let executable = directory.join(engine_file_name());
        std::fs::write(&executable, b"an engine").expect("engine");
        executable
    }

    #[test]
    fn path_order_decides_which_engine_a_bare_kmp_mcp_runs() {
        // The machine of #450: rustup's env ordering puts ~/.cargo/bin first,
        // so a twenty-releases-old engine is the one that answers.
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let cargo_bin = base.join("home/.cargo/bin");
        let local_bin = base.join("home/.local/bin");
        let stale = engine_at(&cargo_bin);
        let current = engine_at(&local_bin);

        let roots = SurveyRoots {
            home: base.join("home"),
            data_home: base.join("home/.local/share"),
            working_dir: base.join("project"),
            path_entries: vec![cargo_bin, local_bin],
        };
        let probe = FakeProbe {
            versions: BTreeMap::from([(stale.clone(), "0.1.13"), (current.clone(), "0.6.1")]),
        };

        let found = SurveyEngines::new(&NativeInstallationCatalog, &probe).execute(&roots);

        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(found[0].executable().as_path(), stale);
        assert!(
            found[0].selected_by_path(),
            "the first PATH entry that holds an engine is the one that runs"
        );
        assert_eq!(found[0].described_version(), "0.1.13");
        assert_eq!(found[1].executable().as_path(), current);
        assert!(
            !found[1].selected_by_path(),
            "everything after it is shadowed"
        );
    }

    #[test]
    fn an_engine_no_path_entry_reaches_is_found_and_selected_by_nothing() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let conventional = engine_at(&base.join("home/.local/bin"));

        let roots = SurveyRoots {
            home: base.join("home"),
            data_home: base.join("home/.local/share"),
            working_dir: base.join("project"),
            path_entries: Vec::new(),
        };
        let probe = FakeProbe {
            versions: BTreeMap::from([(conventional.clone(), "0.6.1")]),
        };

        let found = SurveyEngines::new(&NativeInstallationCatalog, &probe).execute(&roots);

        assert_eq!(found.len(), 1, "{found:?}");
        assert!(!found[0].selected_by_path());
    }

    #[test]
    fn an_engine_that_will_not_say_its_release_is_still_named() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        engine_at(&base.join("home/.local/bin"));

        let roots = SurveyRoots {
            home: base.join("home"),
            data_home: base.join("home/.local/share"),
            working_dir: base.join("project"),
            path_entries: Vec::new(),
        };
        let probe = FakeProbe {
            versions: BTreeMap::new(),
        };

        let found = SurveyEngines::new(&NativeInstallationCatalog, &probe).execute(&roots);

        assert_eq!(found.len(), 1, "silence is not absence: {found:?}");
        assert_eq!(found[0].described_version(), "unknown version");
        assert!(found[0].version().is_none());
    }

    #[test]
    fn one_directory_reached_twice_is_one_engine() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let local_bin = base.join("home/.local/bin");
        engine_at(&local_bin);

        let roots = SurveyRoots {
            home: base.join("home"),
            data_home: base.join("home/.local/share"),
            working_dir: base.join("project"),
            // The conventional directory is also on PATH, as it usually is.
            path_entries: vec![local_bin.clone(), local_bin],
        };
        let probe = FakeProbe {
            versions: BTreeMap::new(),
        };

        let found = SurveyEngines::new(&NativeInstallationCatalog, &probe).execute(&roots);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].selected_by_path());
    }
}
