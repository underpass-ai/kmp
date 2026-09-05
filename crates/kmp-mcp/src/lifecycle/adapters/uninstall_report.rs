use std::path::Path;

use crate::lifecycle::domain::piece::Piece;
use crate::lifecycle::domain::piece_kind::PieceKind;

/// The uninstall report. Same shape as the doctor: one line per piece, the
/// fix attached to the problem, a verdict in plain words. Presentation only —
/// every decision it prints was made by the domain.
pub fn uninstall_report(
    pieces: &[Piece],
    workspace: &Path,
    purge: bool,
    applying: bool,
    style: crate::style::Style,
) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}\n",
        crate::banner::large_with(
            style,
            if applying {
                "  uninstall — apply requested; preflighting what is listed"
            } else {
                "  uninstall — a dry run; nothing has been removed"
            }
        )
    );

    if pieces.is_empty() {
        let _ = writeln!(out, "Nothing of KMP's is on this machine.");
        return out;
    }

    let _ = writeln!(out, "{}", crate::banner::head_styled(style, "Found"));
    for piece in pieces {
        let _ = writeln!(out, "  {:<12} {}", piece.kind.label(), piece.path.display());
        let _ = writeln!(out, "               {}", piece.detail);
        if let Some(reason) = piece.refusal() {
            // `held` and `kept` are different news. One is a restart away,
            // the other is never happening here, and a reader planning what
            // to do next cannot act on a word that covers both (#520).
            let verdict = if piece.is_held() { "held" } else { "kept" };
            let _ = writeln!(out, "               {verdict} — {reason}");
        } else if let Some(rescue) = piece.rescue_path(workspace) {
            let _ = writeln!(
                out,
                "               {} {}",
                if purge {
                    "NOT saved — --purge"
                } else {
                    "saved first to"
                },
                if purge {
                    String::new()
                } else {
                    rescue.display().to_string()
                }
            );
        }
    }
    out.push('\n');

    // Ahead of the restore note: a hold is what stops the reader today.
    // The memory rescue matters when something is actually being removed.
    if pieces.iter().any(Piece::is_held) {
        let _ = writeln!(
            out,
            "{}",
            crate::banner::head_styled(style, "Held right now")
        );
        let _ = writeln!(
            out,
            "  A host that started before an update keeps serving the engine it opened,\n  \
             so the file is still in use however current the installation looks.\n\n  \
             Restart the host named above and run this again. Nothing here ends a\n  \
             process for you.\n"
        );
    }
    if pieces.iter().any(|piece| piece.kind == PieceKind::Store) && !purge {
        let _ = writeln!(
            out,
            "{}",
            crate::banner::head_styled(style, "Getting it back")
        );
        let _ = writeln!(
            out,
            "  Each saved file is the whole event log, in order. To bring one back:\n\n    \
             KMP_MCP_DATA_DIR=<a new directory> kmp-mcp import <the saved file>\n\n  \
             Import needs an empty store — it restores, it does not merge — so point it\n  \
             somewhere new rather than at a store that already holds events.\n"
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::uninstall_report;
    use crate::lifecycle::adapters::filesystem_store_catalog::FilesystemStoreCatalog;
    use crate::lifecycle::adapters::jsonl_store_index::JsonlStoreIndex;
    use crate::lifecycle::adapters::native_installation_catalog::NativeInstallationCatalog;
    use crate::lifecycle::application::use_cases::survey_installation::SurveyInstallation;
    use crate::lifecycle::domain::piece::Piece;
    use crate::lifecycle::domain::survey_roots::SurveyRoots;

    fn roots(base: &Path) -> SurveyRoots {
        SurveyRoots {
            home: base.join("home"),
            data_home: base.join("home/.local/share"),
            working_dir: base.join("project"),
            path_entries: Vec::new(),
        }
    }

    fn store_at(path: &Path, format: &str) {
        std::fs::create_dir_all(path.join("store")).expect("store dir");
        std::fs::write(path.join("FORMAT_VERSION"), format).expect("format stamp");
        std::fs::write(path.join("store/kernel.sqlite3"), vec![0u8; 2_048]).expect("store file");
    }

    struct NothingRunning;

    impl crate::lifecycle::ports::process_liveness::ProcessLiveness for NothingRunning {
        fn is_running(&self, _: u32) -> bool {
            false
        }
    }

    struct NoEngines;

    impl crate::lifecycle::ports::plugin_engine_probe::PluginEngineProbe for NoEngines {
        fn version(
            &self,
            _: &crate::lifecycle::domain::engine_executable::EngineExecutable,
        ) -> Result<
            Option<crate::lifecycle::domain::release_version::ReleaseVersion>,
            crate::lifecycle::domain::lifecycle_error::LifecycleError,
        > {
            Ok(None)
        }
    }

    fn survey(roots: &SurveyRoots) -> Vec<Piece> {
        let stores = FilesystemStoreCatalog::new(&roots.data_home);
        let index = JsonlStoreIndex::new(&roots.data_home);
        SurveyInstallation::new(
            &NativeInstallationCatalog,
            &NoEngines,
            &stores,
            &index,
            &NothingRunning,
        )
        .execute(roots)
    }

    #[test]
    fn the_dry_run_says_where_each_memory_will_be_saved() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        store_at(&base.join("project/.kernel"), "1");

        let workspace = base.join("project");
        let pieces = survey(&roots(base));
        let saving = uninstall_report(
            &pieces,
            &workspace,
            false,
            false,
            crate::style::Style::Plain,
        );
        assert!(saving.contains("saved first to"), "{saving}");
        assert!(saving.contains("kmp-memory-project-"), "{saving}");
        assert!(saving.contains(".jsonl"), "{saving}");

        // ...and that --purge is the way to say no copy is wanted.
        let purged = uninstall_report(&pieces, &workspace, true, false, crate::style::Style::Plain);
        assert!(purged.contains("NOT saved"), "{purged}");
    }

    #[test]
    fn a_dry_run_says_it_removed_nothing() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        store_at(&base.join("project/.kernel"), "1");

        let pieces = survey(&roots(base));
        let dry = uninstall_report(
            &pieces,
            &base.join("project"),
            false,
            false,
            crate::style::Style::Plain,
        );
        assert!(dry.contains("nothing has been removed"));
        assert!(base.join("project/.kernel").exists());
    }

    #[test]
    fn the_report_says_how_to_get_the_memory_back() {
        // A copy nobody knows how to restore is a file, not a backup.
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        store_at(&base.join("project/.kernel"), "1");

        let saving = uninstall_report(
            &survey(&roots(base)),
            &base.join("project"),
            false,
            true,
            crate::style::Style::Plain,
        );
        assert!(saving.contains("kmp-mcp import"), "{saving}");
        assert!(
            saving.contains("empty store"),
            "and the one rule that trips people up: {saving}"
        );
    }

    #[test]
    fn an_empty_machine_says_so_rather_than_printing_a_blank_list() {
        let base = tempfile::tempdir().expect("temp");
        let empty = uninstall_report(
            &survey(&roots(base.path())),
            base.path(),
            false,
            false,
            crate::style::Style::Plain,
        );
        assert!(empty.contains("Nothing of KMP's is on this machine."));
    }

    #[test]
    fn a_held_piece_reads_as_held_and_says_what_would_free_it() {
        let base = tempfile::tempdir().expect("temp");
        let engine = base
            .path()
            .join("home/.claude/plugins/cache/underpass/kmp/0.11.0/bin");
        std::fs::create_dir_all(&engine).expect("engine dir");
        let engine = engine.join("kmp-mcp");
        std::fs::write(&engine, vec![0u8; 16]).expect("engine");

        let held = Piece {
            kind: crate::lifecycle::domain::piece_kind::PieceKind::Engine,
            path: engine,
            detail: "0.11.0 · 15.9M".to_string(),
            bundled_events: None,
            ours_to_remove: true,
            held_by: Some(crate::lifecycle::domain::piece_hold::PieceHold::new(
                "claude", 868_043,
            )),
        };

        let report = uninstall_report(
            &[held],
            base.path(),
            false,
            false,
            crate::style::Style::Plain,
        );

        assert!(report.contains("held — claude (pid 868043)"), "{report}");
        assert!(
            !report.contains("kept —"),
            "a hold is not a refusal: {report}"
        );
        assert!(report.contains("Held right now"), "{report}");
        assert!(
            report.contains("\n  Restart the host named above and run this again."),
            "the paragraph keeps the two-space margin of every other block: {report}"
        );
        assert!(
            report.contains("Nothing here ends a\n  process for you."),
            "the reader must know uninstall will not kill it for them: {report}"
        );
    }

    #[test]
    fn a_piece_that_is_simply_not_ours_still_reads_as_kept() {
        let base = tempfile::tempdir().expect("temp");
        let foreign = Piece {
            kind: crate::lifecycle::domain::piece_kind::PieceKind::HostWiring,
            path: base.path().join("home/.codex/config.toml"),
            detail: "delete the [mcp_servers.kmp] block".to_string(),
            bundled_events: None,
            ours_to_remove: false,
            held_by: None,
        };

        let report = uninstall_report(
            &[foreign],
            base.path(),
            false,
            false,
            crate::style::Style::Plain,
        );

        assert!(
            report.contains("kept — inside a file that is not ours"),
            "{report}"
        );
        assert!(!report.contains("Held right now"), "{report}");
    }
}
