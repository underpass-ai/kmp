use std::path::Path;

use crate::lifecycle::domain::engine_executable::EngineExecutable;
use crate::lifecycle::domain::piece::Piece;
use crate::lifecycle::domain::piece_kind::PieceKind;
use crate::lifecycle::domain::survey_roots::engine_file_name;
use crate::lifecycle::ports::installation_catalog::InstallationCatalog;
use crate::lifecycle::ports::plugin_engine_probe::PluginEngineProbe;

/// Use case: build the one-piece plan for `uninstall --engine`.
///
/// Doctor names a superseded engine and tells the reader to remove it. Until
/// this existed, the only command it could name removed the whole
/// installation, so the narrow repair and the total one were the same
/// keystrokes — the store, both plugin trees and the engine still in use
/// included (#520). This is the narrow one: one executable, named
/// absolutely, and nothing else considered.
pub struct SelectEngine<'a> {
    installation: &'a dyn InstallationCatalog,
    probe: &'a dyn PluginEngineProbe,
}

impl<'a> SelectEngine<'a> {
    pub fn new(
        installation: &'a dyn InstallationCatalog,
        probe: &'a dyn PluginEngineProbe,
    ) -> Self {
        Self {
            installation,
            probe,
        }
    }

    /// The engine at this path, or why it is not one.
    ///
    /// `home` bounds what this verb will delete for you. An engine outside it
    /// may be a package manager's or another user's, and is reported the way
    /// the survey reports it rather than silently removed.
    pub fn execute(&self, path: &Path, home: &Path) -> Result<Piece, String> {
        if !path.is_absolute() {
            return Err(format!(
                "--engine requires an absolute path; `{}` is relative",
                path.display()
            ));
        }
        let path = self.installation.canonicalize(path).map_err(|error| {
            format!(
                "could not resolve selected engine `{}`: {error}",
                path.display()
            )
        })?;
        if !self.installation.is_file(&path) {
            return Err(format!("`{}` is not a file", path.display()));
        }
        // The name is the whole check. Running an arbitrary path to ask its
        // version would execute whatever was pointed at, and a verb whose
        // next step is deletion must not be the thing that runs it first.
        if path.file_name().and_then(|name| name.to_str()) != Some(engine_file_name()) {
            return Err(format!(
                "`{}` is not a KMP engine: expected a file named `{}`",
                path.display(),
                engine_file_name()
            ));
        }

        let ours = path.starts_with(home);
        let version = self
            .probe
            .version(&EngineExecutable::installed_at(path.clone()))
            .ok()
            .flatten()
            .map_or_else(
                || "unknown version".to_string(),
                |version| version.as_str().to_string(),
            );
        let identity = format!("{version} · {}", self.installation.size_of(&path).human());
        Ok(Piece {
            kind: PieceKind::Engine,
            detail: if ours {
                identity
            } else {
                format!("{identity} — outside your home; remove it yourself if you meant to")
            },
            path,
            bundled_events: None,
            ours_to_remove: ours,
            // Filled by the caller, which knows the plugin tree this engine
            // sits in and can ask whether a host is reading it.
            held_by: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::SelectEngine;
    use crate::lifecycle::adapters::native_installation_catalog::NativeInstallationCatalog;
    use crate::lifecycle::domain::engine_executable::EngineExecutable;
    use crate::lifecycle::domain::lifecycle_error::LifecycleError;
    use crate::lifecycle::domain::piece_kind::PieceKind;
    use crate::lifecycle::domain::release_version::ReleaseVersion;
    use crate::lifecycle::ports::plugin_engine_probe::PluginEngineProbe;

    struct Answers(Option<&'static str>);

    impl PluginEngineProbe for Answers {
        fn version(&self, _: &EngineExecutable) -> Result<Option<ReleaseVersion>, LifecycleError> {
            Ok(self
                .0
                .map(|raw| ReleaseVersion::parse(raw).expect("version")))
        }
    }

    fn engine_at(home: &Path, version: &str) -> PathBuf {
        let directory = home
            .join(".claude/plugins/cache/underpass/kmp")
            .join(version)
            .join("bin");
        std::fs::create_dir_all(&directory).expect("engine dir");
        let path = directory.join(super::engine_file_name());
        std::fs::write(&path, vec![0u8; 1_024]).expect("engine file");
        path
    }

    #[test]
    fn one_engine_inside_the_home_is_selected_with_its_release_and_size() {
        let base = tempfile::tempdir().expect("temp dir");
        let home = base.path().join("home");
        let engine = engine_at(&home, "0.11.0");

        let piece = SelectEngine::new(&NativeInstallationCatalog, &Answers(Some("0.11.0")))
            .execute(&engine, &home)
            .expect("an engine");

        assert_eq!(piece.kind, PieceKind::Engine);
        assert!(piece.ours_to_remove);
        assert!(piece.detail.starts_with("0.11.0 · "), "{}", piece.detail);
        assert!(piece.refusal().is_none());
    }

    #[test]
    fn an_engine_outside_the_home_is_named_but_not_ours_to_remove() {
        let base = tempfile::tempdir().expect("temp dir");
        let elsewhere = base.path().join("usr/lib");
        let engine = engine_at(&elsewhere, "0.11.0");

        let piece = SelectEngine::new(&NativeInstallationCatalog, &Answers(None))
            .execute(&engine, &base.path().join("home"))
            .expect("an engine");

        assert!(!piece.ours_to_remove);
        assert_eq!(
            piece.refusal().as_deref(),
            Some("outside the home this surveyed")
        );
        assert!(piece.detail.contains("unknown version"), "{}", piece.detail);
    }

    #[test]
    fn a_relative_path_is_refused_before_anything_is_resolved() {
        let reason = SelectEngine::new(&NativeInstallationCatalog, &Answers(None))
            .execute(Path::new("bin/kmp-mcp"), Path::new("/home/user"))
            .expect_err("relative");

        assert!(reason.contains("absolute"), "{reason}");
    }

    #[test]
    fn a_file_that_is_not_an_engine_is_refused_rather_than_run_to_find_out() {
        let base = tempfile::tempdir().expect("temp dir");
        let decoy = base.path().join("payload.sh");
        std::fs::write(&decoy, b"#!/bin/sh\n").expect("decoy");

        let reason = SelectEngine::new(&NativeInstallationCatalog, &Answers(None))
            .execute(&decoy, base.path())
            .expect_err("not an engine");

        assert!(reason.contains("not a KMP engine"), "{reason}");
    }

    #[test]
    fn a_directory_is_refused_even_when_it_is_named_like_the_engine() {
        let base = tempfile::tempdir().expect("temp dir");
        let directory = base.path().join(super::engine_file_name());
        std::fs::create_dir_all(&directory).expect("dir");

        let reason = SelectEngine::new(&NativeInstallationCatalog, &Answers(None))
            .execute(&directory, base.path())
            .expect_err("a directory");

        assert!(reason.contains("not a file"), "{reason}");
    }

    #[test]
    fn a_path_that_is_not_there_says_so_instead_of_planning_a_removal() {
        let base = tempfile::tempdir().expect("temp dir");

        let reason = SelectEngine::new(&NativeInstallationCatalog, &Answers(None))
            .execute(&base.path().join("gone/kmp-mcp"), base.path())
            .expect_err("missing");

        assert!(reason.contains("could not resolve"), "{reason}");
    }
}
