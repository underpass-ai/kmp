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
            let _ = writeln!(out, "               kept — {reason}");
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

    fn survey(roots: &SurveyRoots) -> Vec<Piece> {
        let stores = FilesystemStoreCatalog::new(&roots.data_home);
        let index = JsonlStoreIndex::new(&roots.data_home);
        SurveyInstallation::new(&NativeInstallationCatalog, &stores, &index).execute(roots)
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
}
