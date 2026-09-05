use std::path::Path;

use crate::lifecycle::application::use_cases::survey_engines::SurveyEngines;
use crate::lifecycle::application::use_cases::survey_holds::SurveyHolds;
use crate::lifecycle::domain::piece::Piece;
use crate::lifecycle::domain::piece_hold::PieceHold;
use crate::lifecycle::domain::piece_kind::PieceKind;
use crate::lifecycle::domain::survey_roots::SurveyRoots;
use crate::lifecycle::ports::installation_catalog::InstallationCatalog;
use crate::lifecycle::ports::plugin_engine_probe::PluginEngineProbe;
use crate::lifecycle::ports::process_liveness::ProcessLiveness;

/// Use case: the engines on this machine, as things an uninstall may act on.
///
/// [`SurveyEngines`] answers where the copies are and what release each one
/// is. Two further questions decide what a reader can do about one, and both
/// are decisions rather than observations: whether it is ours to delete, and
/// whether a host is being served from it right now.
pub struct SurveyEnginePieces<'a> {
    installation: &'a dyn InstallationCatalog,
    probe: &'a dyn PluginEngineProbe,
    liveness: &'a dyn ProcessLiveness,
}

impl<'a> SurveyEnginePieces<'a> {
    pub fn new(
        installation: &'a dyn InstallationCatalog,
        probe: &'a dyn PluginEngineProbe,
        liveness: &'a dyn ProcessLiveness,
    ) -> Self {
        Self {
            installation,
            probe,
            liveness,
        }
    }

    pub fn execute(&self, roots: &SurveyRoots, claude_plugin: &Path) -> Vec<Piece> {
        SurveyEngines::new(self.installation, self.probe)
            .execute(roots)
            .into_iter()
            .map(|engine| {
                let path = engine.executable().as_path().to_path_buf();
                // An engine on `PATH` but outside this home may be a package
                // manager's, or another user's. It is worth naming — a second
                // copy is how a live session ends up older than the merged
                // fix (#80) — and it is not this verb's to delete.
                let ours = path.starts_with(&roots.home);
                // The size never said which copy was ancient, which is the
                // only thing that makes a second engine worth acting on
                // (#450).
                let size = self.installation.size_of(&path).human();
                let identity = format!("{} · {size}", engine.described_version());
                // An engine inside a plugin version is served from there for
                // as long as the session that opened it lives. Saying so in
                // the dry run is the difference between a reader who restarts
                // a host and one who reads a clean list and is refused at
                // apply (#520).
                let held_by = self.hold_on_version_of(&path, claude_plugin);
                Piece {
                    kind: PieceKind::Engine,
                    detail: if ours {
                        identity
                    } else {
                        format!(
                            "{identity} — outside your home; remove it yourself if you meant to"
                        )
                    },
                    path,
                    bundled_events: None,
                    ours_to_remove: ours,
                    held_by,
                }
            })
            .collect()
    }

    /// The hold on the plugin version this engine lives in, if any.
    ///
    /// The version directory is the one directly under the plugin root, so
    /// the marker is found from `<root>/<version>/bin/kmp-mcp` without
    /// assuming how deep the executable sits.
    fn hold_on_version_of(&self, engine: &Path, plugin_root: &Path) -> Option<PieceHold> {
        let version = engine
            .ancestors()
            .find(|ancestor| ancestor.parent() == Some(plugin_root))?;
        SurveyHolds::new(self.installation, self.liveness).execute("claude", version)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use super::SurveyEnginePieces;
    use crate::lifecycle::adapters::native_installation_catalog::NativeInstallationCatalog;
    use crate::lifecycle::application::use_cases::remove_piece::RemovePiece;
    use crate::lifecycle::domain::engine_executable::EngineExecutable;
    use crate::lifecycle::domain::lifecycle_error::LifecycleError;
    use crate::lifecycle::domain::piece::Piece;
    use crate::lifecycle::domain::release_version::ReleaseVersion;
    use crate::lifecycle::domain::survey_roots::SurveyRoots;
    use crate::lifecycle::ports::plugin_engine_probe::PluginEngineProbe;
    use crate::lifecycle::ports::process_liveness::ProcessLiveness;

    struct FakeProbe(BTreeMap<PathBuf, &'static str>);

    impl PluginEngineProbe for FakeProbe {
        fn version(
            &self,
            executable: &EngineExecutable,
        ) -> Result<Option<ReleaseVersion>, LifecycleError> {
            self.0
                .get(executable.as_path())
                .map(|raw| ReleaseVersion::parse(raw))
                .transpose()
        }
    }

    struct Running(Vec<u32>);

    impl ProcessLiveness for Running {
        fn is_running(&self, pid: u32) -> bool {
            self.0.contains(&pid)
        }
    }

    fn plugin_root(base: &Path) -> PathBuf {
        base.join("home/.claude/plugins/cache/underpass/kmp")
    }

    /// Claude Code puts a plugin's `bin` on `PATH` for the sessions it
    /// serves, which is how a plugin-local engine is surveyed at all.
    fn roots_serving(base: &Path, version: &str) -> SurveyRoots {
        SurveyRoots {
            home: base.join("home"),
            data_home: base.join("home/.local/share"),
            working_dir: base.join("project"),
            path_entries: vec![plugin_root(base).join(version).join("bin")],
        }
    }

    fn engine_at(base: &Path, version: &str) -> PathBuf {
        let engine = plugin_root(base).join(version).join("bin/kmp-mcp");
        std::fs::create_dir_all(engine.parent().expect("bin")).expect("engine dir");
        std::fs::write(&engine, vec![0u8; 4_096]).expect("engine");
        engine
    }

    fn opened_by(base: &Path, version: &str, pid: u32) {
        let markers = plugin_root(base).join(version).join(".in_use");
        std::fs::create_dir_all(&markers).expect("marker dir");
        std::fs::write(markers.join(pid.to_string()), b"{}").expect("marker");
    }

    fn survey(roots: &SurveyRoots, base: &Path, probe: FakeProbe, live: Vec<u32>) -> Vec<Piece> {
        SurveyEnginePieces::new(&NativeInstallationCatalog, &probe, &Running(live))
            .execute(roots, &plugin_root(base))
    }

    fn answering(engine: &Path, version: &'static str) -> FakeProbe {
        FakeProbe([(engine.to_path_buf(), version)].into_iter().collect())
    }

    #[test]
    fn an_engine_a_live_session_is_still_serving_is_held_rather_than_removable() {
        // The exact shape of #520: a host that started before an update keeps
        // the engine it opened, and the dry run used to offer it for deletion
        // without a word.
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let engine = engine_at(base, "0.11.0");
        opened_by(base, "0.11.0", 868_043);

        let pieces = survey(
            &roots_serving(base, "0.11.0"),
            base,
            answering(&engine, "0.11.0"),
            vec![868_043],
        );

        let held = pieces.first().expect("the engine is surveyed");
        assert!(held.is_held());
        assert_eq!(
            held.refusal().as_deref(),
            Some("claude (pid 868043) is still using it; restart that host, then remove it")
        );
        assert_eq!(
            RemovePiece::new(&NativeInstallationCatalog).execute(held),
            Err("claude (pid 868043) is still using it; restart that host, then remove it".into()),
            "a held engine is never removed out from under its host"
        );
        assert!(engine.exists(), "and the file is still there");
    }

    #[test]
    fn the_same_engine_is_removable_once_the_host_that_held_it_is_gone() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let engine = engine_at(base, "0.11.0");
        opened_by(base, "0.11.0", 868_043);

        // Same marker on disk; nothing is running any more.
        let pieces = survey(
            &roots_serving(base, "0.11.0"),
            base,
            answering(&engine, "0.11.0"),
            Vec::new(),
        );

        let free = pieces.first().expect("the engine is surveyed");
        assert!(!free.is_held());
        assert!(free.refusal().is_none());
    }

    #[test]
    fn an_engine_outside_the_surveyed_home_is_named_and_not_ours_to_remove() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let elsewhere = base.join("opt/bin");
        std::fs::create_dir_all(&elsewhere).expect("dir");
        std::fs::write(elsewhere.join("kmp-mcp"), vec![0u8; 32]).expect("engine");
        let roots = SurveyRoots {
            home: base.join("home"),
            data_home: base.join("home/.local/share"),
            working_dir: base.join("project"),
            path_entries: vec![elsewhere.clone()],
        };

        let piece = survey(&roots, base, FakeProbe(BTreeMap::new()), Vec::new())
            .into_iter()
            .next()
            .expect("the foreign engine is still named");

        assert!(!piece.ours_to_remove);
        assert!(
            piece.detail.contains("outside your home"),
            "{}",
            piece.detail
        );
        assert!(
            RemovePiece::new(&NativeInstallationCatalog)
                .execute(&piece)
                .is_err()
        );
        assert!(elsewhere.join("kmp-mcp").exists());
    }

    #[test]
    fn an_engine_line_says_which_release_it_is_beside_its_size() {
        // A size alone never said which copy was ancient, which is the only
        // thing that makes a second engine worth acting on.
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let engine = engine_at(base, "0.11.0");

        let piece = survey(
            &roots_serving(base, "0.11.0"),
            base,
            answering(&engine, "0.11.0"),
            Vec::new(),
        )
        .remove(0);

        assert!(piece.detail.starts_with("0.11.0 · "), "{}", piece.detail);
    }

    #[test]
    fn an_engine_that_will_not_say_its_release_is_still_listed() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        engine_at(base, "0.11.0");

        let piece = survey(
            &roots_serving(base, "0.11.0"),
            base,
            FakeProbe(BTreeMap::new()),
            Vec::new(),
        )
        .remove(0);

        assert!(
            piece.detail.starts_with("unknown version"),
            "{}",
            piece.detail
        );
    }

    #[test]
    fn an_engine_that_is_not_under_a_plugin_version_has_nothing_to_hold_it() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let bin = base.join("home/.local/bin");
        std::fs::create_dir_all(&bin).expect("dir");
        std::fs::write(bin.join("kmp-mcp"), vec![0u8; 32]).expect("engine");
        let roots = SurveyRoots {
            home: base.join("home"),
            data_home: base.join("home/.local/share"),
            working_dir: base.join("project"),
            path_entries: vec![bin],
        };

        let piece = survey(&roots, base, FakeProbe(BTreeMap::new()), vec![1]).remove(0);

        assert!(!piece.is_held());
    }
}
