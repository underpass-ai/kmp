//! What the operator reads before agreeing to lose anything.
//!
//! One concept: rendering a survey. Nothing is removed until it has been
//! shown, so this is the dry run's entire output and the applied run's record
//! of what it took.

use std::path::Path;

use crate::uninstall::piece::Piece;
use crate::uninstall::piece_kind::PieceKind;
use crate::uninstall::rescue::{refusal, rescue_path};

/// The report. Same shape as the doctor: one line per piece, the fix attached
/// to the problem, a verdict in plain words.
pub fn report(
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
        if let Some(reason) = refusal(piece) {
            let _ = writeln!(out, "               kept — {reason}");
        } else if let Some(rescue) = rescue_path(piece, workspace) {
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
    use super::*;
    use crate::uninstall::removal::remove;
    use crate::uninstall::survey::survey;
    use crate::uninstall::test_support::{bundle_at, roots, store_at};

    #[test]
    fn the_dry_run_says_where_each_memory_will_be_saved() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        store_at(&base.join("project/.kernel"), "1");

        let workspace = base.join("project");
        let pieces = survey(&roots(base));
        let saving = report(
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
        let purged = report(&pieces, &workspace, true, false, crate::style::Style::Plain);
        assert!(purged.contains("NOT saved"), "{purged}");
    }

    #[test]
    fn the_committed_bundle_is_listed_and_left_alone() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        store_at(&base.join("project/.kernel"), "1");
        bundle_at(&base.join("project/.kmp/memory.jsonl"), 21);

        let bundle = survey(&roots(base))
            .into_iter()
            .find(|piece| piece.kind == PieceKind::Bundle)
            .expect("the bundle is part of the picture");
        assert!(bundle.detail.contains("left alone"));
        assert!(refusal(&bundle).is_some(), "and it is not ours to delete");
    }

    #[test]
    fn host_wiring_is_named_with_the_command_and_never_edited() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        std::fs::create_dir_all(base.join("home/.codex")).expect("codex dir");
        std::fs::write(
            base.join("home/.claude.json"),
            "{\"mcpServers\":{\"kmp\":{}}}",
        )
        .expect("claude config");
        std::fs::write(
            base.join("home/.codex/config.toml"),
            "[mcp_servers.kmp]\ncommand = \"kmp-mcp\"\n",
        )
        .expect("codex config");

        let wiring: Vec<_> = survey(&roots(base))
            .into_iter()
            .filter(|piece| piece.kind == PieceKind::HostWiring)
            .collect();
        assert_eq!(wiring.len(), 2, "{wiring:?}");
        assert!(
            wiring
                .iter()
                .any(|piece| piece.detail.contains("claude mcp remove"))
        );
        // A botched edit of someone's configuration costs more than the
        // uninstall saves, so this verb says what to do and does not do it.
        for piece in &wiring {
            assert!(remove(piece).is_err());
            assert!(piece.path.exists());
        }
    }

    #[test]
    fn a_dry_run_says_it_removed_nothing() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        store_at(&base.join("project/.kernel"), "1");

        let pieces = survey(&roots(base));
        let dry = report(
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

        let saving = report(
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
        let empty = report(
            &survey(&roots(base.path())),
            base.path(),
            false,
            false,
            crate::style::Style::Plain,
        );
        assert!(empty.contains("Nothing of KMP's is on this machine."));
    }
}
