use std::path::{Path, PathBuf};

use crate::lifecycle::domain::piece::Piece;
use crate::lifecycle::domain::piece_kind::PieceKind;
use crate::lifecycle::domain::survey_roots::SurveyRoots;
use crate::lifecycle::ports::installation_catalog::InstallationCatalog;

/// Use case: what a retired way of installing KMP left on this machine.
///
/// Separate from the installation survey because it answers a different
/// question. That one asks what serves the reader now; this one asks what
/// used to and no longer does. Flattened into one list the two are
/// indistinguishable — a plugin tree and a dead prompt are both "host files"
/// — and the reader deciding what to keep needs exactly that distinction
/// (#520).
///
/// Standalone Codex wiring is retired: the native plugin owns MCP, ships its
/// own prompts and its own scripts. Anything still sitting in the old places
/// is a leftover, and nothing running reads it.
pub struct SurveyLeftovers<'a> {
    installation: &'a dyn InstallationCatalog,
}

impl<'a> SurveyLeftovers<'a> {
    pub fn new(installation: &'a dyn InstallationCatalog) -> Self {
        Self { installation }
    }

    pub fn execute(&self, roots: &SurveyRoots) -> Vec<Piece> {
        let mut pieces = Vec::new();

        for prompt in self.kmp_prompts(&roots.home.join(".codex/prompts")) {
            pieces.push(Piece {
                kind: PieceKind::Leftover,
                // Said once per line and no more: ten identical sentences of
                // explanation bury the ten paths they are attached to.
                detail: format!(
                    "{} — Codex /kmp- prompt left by the retired standalone wiring",
                    self.installation.size_of(&prompt).human()
                ),
                path: prompt,
                bundled_events: None,
                ours_to_remove: true,
                held_by: None,
            });
        }

        // The scripts those prompts call. Nothing mentioned them: the engine
        // survey looks in this directory for a `kmp-mcp` and walks past
        // everything beside it, so a reader who removed the prompts still had
        // the shell half of a retired install and no line saying so.
        for script in self.standalone_scripts(&roots.data_home.join("kmp/bin")) {
            pieces.push(Piece {
                kind: PieceKind::Leftover,
                detail: format!(
                    "{} — standalone install script; the plugin ships its own",
                    self.installation.size_of(&script).human()
                ),
                path: script,
                bundled_events: None,
                ours_to_remove: true,
                held_by: None,
            });
        }

        pieces
    }

    /// Only the prompts KMP installed. The directory is the user's, and
    /// their own files sit beside ours.
    fn kmp_prompts(&self, directory: &Path) -> Vec<PathBuf> {
        self.installation
            .files_in(directory)
            .into_iter()
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("kmp-") && name.ends_with(".md"))
            })
            .collect()
    }

    /// The shell scripts a standalone install put beside its engine. The
    /// engine itself is not one of them; it is surveyed as an engine.
    fn standalone_scripts(&self, directory: &Path) -> Vec<PathBuf> {
        self.installation
            .files_in(directory)
            .into_iter()
            .filter(|path| path.extension().is_some_and(|extension| extension == "sh"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::SurveyLeftovers;
    use crate::lifecycle::adapters::native_installation_catalog::NativeInstallationCatalog;
    use crate::lifecycle::application::use_cases::remove_piece::RemovePiece;
    use crate::lifecycle::domain::piece_kind::PieceKind;
    use crate::lifecycle::domain::survey_roots::SurveyRoots;

    fn roots(base: &Path) -> SurveyRoots {
        SurveyRoots {
            home: base.join("home"),
            data_home: base.join("home/.local/share"),
            working_dir: base.join("project"),
            path_entries: Vec::new(),
        }
    }

    fn names(pieces: &[crate::lifecycle::domain::piece::Piece]) -> Vec<String> {
        pieces
            .iter()
            .map(|piece| {
                piece
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .expect("name")
                    .to_string()
            })
            .collect()
    }

    fn survey(base: &Path) -> Vec<crate::lifecycle::domain::piece::Piece> {
        SurveyLeftovers::new(&NativeInstallationCatalog).execute(&roots(base))
    }

    #[test]
    fn the_scripts_a_standalone_install_left_are_found_beside_its_engine() {
        // Nothing mentioned them before: the engine survey walks this
        // directory looking for `kmp-mcp` and past everything beside it.
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let bin = base.join("home/.local/share/kmp/bin");
        std::fs::create_dir_all(&bin).expect("standalone bin");
        for name in ["kmp-doctor.sh", "kmp-update.sh"] {
            std::fs::write(bin.join(name), name).expect("script");
        }
        std::fs::write(bin.join("kmp-mcp"), vec![0u8; 16]).expect("engine");

        let found = survey(base);

        assert_eq!(names(&found), ["kmp-doctor.sh", "kmp-update.sh"]);
        assert!(found.iter().all(|piece| piece.kind == PieceKind::Leftover));
        assert!(
            bin.join("kmp-mcp").exists(),
            "the engine is surveyed as an engine, not swept up as a leftover"
        );
    }

    #[test]
    fn only_kmp_prompts_are_taken_from_the_shared_codex_directory() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let prompts = base.join("home/.codex/prompts");
        std::fs::create_dir_all(prompts.join("kmp-not-a-prompt.md")).expect("prompt dirs");
        for name in [
            "kmp-doctor.md",
            "kmp-wake.md",
            "mi-revision-de-codigo.md",
            "notas-personales.md",
        ] {
            std::fs::write(prompts.join(name), name).expect("prompt");
        }

        let found = survey(base);
        assert_eq!(names(&found), ["kmp-doctor.md", "kmp-wake.md"]);

        for piece in &found {
            RemovePiece::new(&NativeInstallationCatalog)
                .execute(piece)
                .expect("remove KMP prompt");
        }
        assert!(prompts.is_dir(), "the shared Codex directory must remain");
        assert!(prompts.join("notas-personales.md").exists());
        assert!(
            prompts.join("kmp-not-a-prompt.md").is_dir(),
            "a directory named like a prompt is not a file we installed"
        );
    }

    #[test]
    fn a_prompt_says_which_wiring_left_it_rather_than_only_its_size() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let prompts = base.join("home/.codex/prompts");
        std::fs::create_dir_all(&prompts).expect("prompts");
        std::fs::write(prompts.join("kmp-doctor.md"), b"run the doctor").expect("prompt");

        let detail = survey(base).remove(0).detail;

        assert!(detail.contains("Codex /kmp- prompt"), "{detail}");
        assert!(detail.contains("retired standalone wiring"), "{detail}");
    }

    #[test]
    fn a_machine_the_standalone_wiring_never_touched_has_no_leftovers() {
        let base = tempfile::tempdir().expect("temp");
        assert!(survey(base.path()).is_empty());
    }

    #[test]
    fn a_standalone_engine_with_no_scripts_around_it_leaves_nothing_to_report() {
        // The engine still answers; only the shell around it was retired.
        let base = tempfile::tempdir().expect("temp");
        let bin = base.path().join("home/.local/share/kmp/bin");
        std::fs::create_dir_all(&bin).expect("bin");
        std::fs::write(bin.join("kmp-mcp"), vec![0u8; 16]).expect("engine");

        assert!(survey(base.path()).is_empty());
    }
}
