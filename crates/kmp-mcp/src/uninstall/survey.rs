//! Enumerating an installation, once, so nobody has to from memory.
//!
//! One concept: turning roots into the list of pieces on this machine. On one
//! machine that already spans two engine copies, two stores, a committed
//! bundle, a plugin cache and a shared prompt directory — the list nobody can
//! recite, which is the whole reason a verb has to.

use std::path::Path;

use crate::uninstall::description::{describe_size, describe_store, describe_versions};
use crate::uninstall::discovery::{
    bundle_beside, bundle_event_count, engine_directories, engine_file_name, file_mentions,
    kmp_prompts, stores,
};
use crate::uninstall::piece::Piece;
use crate::uninstall::piece_kind::PieceKind;
use crate::uninstall::roots::Roots;
use crate::uninstall::store_lease::store_leases_dir;

/// Everything of KMP's that is actually on this machine, in the order a
/// reader should meet it: the engine that answers, the memory that would be
/// lost, then the wiring.
pub fn survey(roots: &Roots) -> Vec<Piece> {
    let mut pieces = Vec::new();

    for directory in engine_directories(roots) {
        let engine = directory.join(engine_file_name());
        if !engine.is_file() {
            continue;
        }
        // An engine on `PATH` but outside this home may be a package
        // manager's, or another user's. It is worth naming — a second copy is
        // how a live session ends up older than the merged fix (#80) — and it
        // is not this verb's to delete.
        let ours = engine.starts_with(&roots.home);
        pieces.push(Piece {
            kind: PieceKind::Engine,
            detail: if ours {
                describe_size(&engine)
            } else {
                format!(
                    "{} — outside your home; remove it yourself if you meant to",
                    describe_size(&engine)
                )
            },
            path: engine,
            bundled_events: None,
            ours_to_remove: ours,
        });
    }

    for store in stores(roots) {
        let bundled_events = bundle_beside(&store)
            .as_deref()
            .and_then(bundle_event_count);
        pieces.push(Piece {
            kind: PieceKind::Store,
            detail: describe_store(&store),
            path: store,
            bundled_events,
            ours_to_remove: true,
        });
    }

    for bundle in stores(roots)
        .iter()
        .filter_map(|store| bundle_beside(store))
    {
        if bundle.is_file() {
            pieces.push(Piece {
                kind: PieceKind::Bundle,
                detail: format!(
                    "{} — committed memory; left alone unless you ask",
                    describe_size(&bundle)
                ),
                path: bundle,
                bundled_events: None,
                ours_to_remove: false,
            });
        }
    }

    // KMP's own note of where memory has been. Small, and leaving it behind
    // means a fresh install starts with a list of stores that are gone.
    let index = roots
        .data_home
        .join("kmp")
        .join(crate::memories::INDEX_FILE);
    if index.is_file() {
        pieces.push(Piece {
            kind: PieceKind::HostFiles,
            detail: format!(
                "{} — the note of which memories exist here",
                describe_size(&index)
            ),
            path: index,
            bundled_events: None,
            ours_to_remove: true,
        });
    }

    let leases = store_leases_dir(&roots.data_home);
    if leases.is_dir() {
        pieces.push(Piece {
            kind: PieceKind::HostFiles,
            detail: format!(
                "{} — machine-local locks that keep active stores safe",
                describe_size(&leases)
            ),
            path: leases,
            bundled_events: None,
            ours_to_remove: true,
        });
    }

    let claude_plugin = roots.home.join(".claude/plugins/cache/underpass/kmp");
    if claude_plugin.is_dir() {
        pieces.push(Piece {
            kind: PieceKind::HostFiles,
            detail: format!("Claude Code plugin — {}", describe_versions(&claude_plugin)),
            path: claude_plugin,
            bundled_events: None,
            ours_to_remove: true,
        });
    }

    let codex_prompts = roots.home.join(".codex/prompts");
    let installed = kmp_prompts(&codex_prompts);
    for prompt in installed {
        pieces.push(Piece {
            kind: PieceKind::HostFiles,
            detail: format!("Codex /kmp- prompt — {}", describe_size(&prompt)),
            path: prompt,
            bundled_events: None,
            ours_to_remove: true,
        });
    }

    // Registrations live inside files that are not ours. This verb names them
    // and the command that removes them; it does not edit a user's
    // configuration on their behalf, because a botched edit there costs more
    // than the uninstall saves.
    let claude_config = roots.home.join(".claude.json");
    for (needle, server_id) in [("\"kmp\"", "kmp"), ("\"kernel-memory\"", "kernel-memory")] {
        if file_mentions(&claude_config, needle) {
            pieces.push(Piece {
                kind: PieceKind::HostWiring,
                detail: format!("remove with:  claude mcp remove {server_id}"),
                path: claude_config.clone(),
                bundled_events: None,
                ours_to_remove: false,
            });
        }
    }
    let codex_config = roots.home.join(".codex/config.toml");
    for server_id in ["kmp", "kernel-memory"] {
        let header = format!("[mcp_servers.{server_id}]");
        if file_mentions(&codex_config, &header) {
            pieces.push(Piece {
                kind: PieceKind::HostWiring,
                detail: format!("delete the {header} block"),
                path: codex_config.clone(),
                bundled_events: None,
                ours_to_remove: false,
            });
        }
    }

    pieces
}

/// Build the one-piece plan for `uninstall --store`.
///
/// The selector is deliberately absolute and resolves to one canonical store
/// identity. The report therefore names the exact path protected by the lease
/// even when the caller reached it through a symlink or a `.` component.
pub fn selected_store(path: &Path) -> Result<Piece, String> {
    if !path.is_absolute() {
        return Err(format!(
            "--store requires an absolute path; `{}` is relative",
            path.display()
        ));
    }
    let path = std::fs::canonicalize(path).map_err(|error| {
        format!(
            "could not resolve selected store `{}`: {error}",
            path.display()
        )
    })?;
    if !path.is_dir() || !path.join("FORMAT_VERSION").is_file() {
        return Err(format!(
            "`{}` is not a KMP store: expected a directory containing FORMAT_VERSION",
            path.display()
        ));
    }
    Ok(Piece {
        kind: PieceKind::Store,
        detail: describe_store(&path),
        bundled_events: bundle_beside(&path).as_deref().and_then(bundle_event_count),
        path,
        ours_to_remove: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uninstall::removal::remove;
    use crate::uninstall::rescue::refusal;
    use crate::uninstall::test_support::{roots, store_at};

    #[test]
    fn selective_uninstall_requires_one_real_absolute_store() {
        let base = tempfile::tempdir().expect("temp");
        let store = base.path().join("memory");
        store_at(&store, "2");

        assert!(selected_store(Path::new("memory")).is_err());
        assert!(selected_store(&base.path().join("missing")).is_err());

        let selected = selected_store(&store).expect("the exact store is selected");
        assert_eq!(selected.kind, PieceKind::Store);
        assert_eq!(
            selected.path,
            std::fs::canonicalize(store).expect("canonical store")
        );
    }

    #[test]
    fn the_survey_finds_every_store_not_only_the_one_this_directory_resolves_to() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        store_at(&base.join("home/.local/share/kmp/default"), "2");
        store_at(&base.join("project/.kernel"), "1");

        let stores: Vec<_> = survey(&roots(base))
            .into_iter()
            .filter(|piece| piece.kind == PieceKind::Store)
            .collect();

        // Forgetting one is how an uninstall leaves memory behind.
        assert_eq!(stores.len(), 2, "{stores:?}");
        assert!(
            stores
                .iter()
                .any(|piece| piece.detail.contains("store format 2"))
        );
        assert!(
            stores
                .iter()
                .any(|piece| piece.detail.contains("store format 1"))
        );
    }

    #[test]
    fn uninstall_removes_only_kmp_prompts_from_the_shared_codex_directory() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let prompts = base.join("home/.codex/prompts");
        std::fs::create_dir_all(prompts.join("kmp-not-a-prompt.md")).expect("prompt dirs");
        for name in [
            "kmp-audit.md",
            "kmp-wake.md",
            "mi-revision-de-codigo.md",
            "notas-personales.md",
            "plantilla-de-pr.md",
        ] {
            std::fs::write(prompts.join(name), name).expect("prompt");
        }
        std::fs::write(
            prompts.join("kmp-not-a-prompt.md/owned-by-user.md"),
            "leave me alone",
        )
        .expect("user-owned nested prompt");

        let installed = survey(&roots(base))
            .into_iter()
            .filter(|piece| piece.kind == PieceKind::HostFiles && piece.detail.starts_with("Codex"))
            .collect::<Vec<_>>();
        assert_eq!(
            installed
                .iter()
                .map(|piece| piece.path.file_name().expect("prompt name"))
                .collect::<Vec<_>>(),
            ["kmp-audit.md", "kmp-wake.md"]
        );

        for piece in &installed {
            remove(piece).expect("remove KMP prompt");
        }

        assert!(prompts.is_dir(), "the shared Codex directory must remain");
        assert!(!prompts.join("kmp-audit.md").exists());
        assert!(!prompts.join("kmp-wake.md").exists());
        for name in [
            "mi-revision-de-codigo.md",
            "notas-personales.md",
            "plantilla-de-pr.md",
            "kmp-not-a-prompt.md/owned-by-user.md",
        ] {
            assert!(prompts.join(name).exists(), "preserve {name}");
        }
    }

    #[test]
    fn former_host_registration_is_still_found_for_the_migration_release() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        std::fs::create_dir_all(base.join("home/.codex")).expect("codex dir");
        std::fs::write(
            base.join("home/.claude.json"),
            "{\"mcpServers\":{\"kernel-memory\":{}}}",
        )
        .expect("claude config");
        std::fs::write(
            base.join("home/.codex/config.toml"),
            "[mcp_servers.kernel-memory]\ncommand = \"kmp-mcp\"\n",
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
                .all(|piece| piece.detail.contains("kernel-memory"))
        );
    }

    #[test]
    fn an_engine_outside_the_surveyed_home_is_named_and_not_touched() {
        // A second engine on PATH is worth seeing — that is how a live
        // session ends up older than the merged fix — but it may be a package
        // manager's, and deleting somebody else's binary is not an uninstall.
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let elsewhere = base.join("usr/local/bin");
        std::fs::create_dir_all(&elsewhere).expect("bin dir");
        std::fs::write(elsewhere.join("kmp-mcp"), b"binary").expect("engine");
        std::fs::create_dir_all(base.join("home/.local/bin")).expect("home bin");
        std::fs::write(base.join("home/.local/bin/kmp-mcp"), b"binary").expect("engine");

        let mut roots = roots(base);
        roots.path_entries = vec![elsewhere.clone()];
        let engines: Vec<_> = survey(&roots)
            .into_iter()
            .filter(|piece| piece.kind == PieceKind::Engine)
            .collect();

        assert_eq!(engines.len(), 2, "both are worth naming: {engines:?}");
        let outside = engines
            .iter()
            .find(|piece| piece.path.starts_with(&elsewhere))
            .expect("the one outside home");
        assert!(!outside.ours_to_remove);
        assert!(outside.detail.contains("outside your home"));
        assert!(remove(outside).is_err(), "even --purge does not reach it");
        assert!(outside.path.exists());

        let ours = engines
            .iter()
            .find(|piece| piece.path.starts_with(base.join("home")))
            .expect("the one in this home");
        assert!(ours.ours_to_remove);
    }

    #[test]
    fn the_per_user_store_has_no_bundle_to_look_for() {
        // Only a project store has a conventional place a copy would be. The
        // old check looked for `<data-home>/kmp/.kmp/memory.jsonl`, which is
        // nowhere, and reached the right answer for the wrong reason.
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        store_at(&base.join("home/.local/share/kmp/default"), "2");

        let store = survey(&roots(base))
            .into_iter()
            .find(|piece| piece.kind == PieceKind::Store)
            .expect("the user store");
        assert_eq!(store.bundled_events, None);
        // It still goes — the copy is made at removal time, not looked for.
        assert!(refusal(&store).is_none());
    }
}
