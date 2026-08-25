//! `kmp-mcp uninstall` — the inverse `/kmp:setup` never had.
//!
//! There was no supported way to remove an installation, or one store inside
//! it, and the last instruction left to a user was `rm -rf` against paths they
//! had to work out themselves. On one machine an install already spans two
//! engine copies, two stores on two formats, a committed bundle, a Claude Code
//! plugin cache and a Codex prompt directory — a list nobody can enumerate
//! from memory, which is the whole reason a verb has to.
//!
//! Two rules shape everything here.
//!
//! **Nothing is removed until it has been shown.** The dry run is the
//! default, and it prints exactly what `--apply` would take: path, kind, size.
//! A destructive command whose first run destroys is one people learn to fear
//! and then avoid.
//!
//! **Memory is saved before it is removed.** Every store is exported into the
//! working directory first and the file is named out loud, so the last thing
//! an uninstall does before deleting memory is hand it back. A copy made at
//! the moment of removal beats a bundle committed at some earlier point,
//! because only one of the two is certainly current. If the export cannot be
//! made — a live session holding the store, a directory that will not take a
//! file — the store stays. `--purge` is how someone says they want it gone
//! without a copy.

use std::path::{Path, PathBuf};

/// What a piece of an installation is, which decides how it is treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceKind {
    /// A `kmp-mcp` executable.
    Engine,
    /// A data directory holding memory.
    Store,
    /// A committed event-log bundle. Removing memory does not remove this:
    /// it lives in the repository and belongs to whoever committed it.
    Bundle,
    /// Plugin files a host reads.
    HostFiles,
    /// A registration inside a host's own configuration file.
    HostWiring,
}

impl PieceKind {
    fn label(self) -> &'static str {
        match self {
            Self::Engine => "engine",
            Self::Store => "memory",
            Self::Bundle => "bundle",
            Self::HostFiles => "host files",
            Self::HostWiring => "host wiring",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Piece {
    pub kind: PieceKind,
    pub path: PathBuf,
    /// What the reader needs to judge it: a size, a store format, an event
    /// count, or the command that removes a registration this verb will not
    /// edit on someone's behalf.
    pub detail: String,
    /// Whether this piece's memory has a copy somewhere else. `None` for
    /// anything that is not a store.
    pub bundled_events: Option<u64>,
    /// Whether this verb may remove it at all. A binary outside the home it
    /// surveyed may belong to a package manager or to another user, and a
    /// registration lives inside a file that is not ours.
    pub ours_to_remove: bool,
}

/// Where to look. Taken as arguments rather than read from the environment so
/// the survey can be exercised against a temporary tree.
pub struct Roots {
    pub home: PathBuf,
    pub data_home: PathBuf,
    pub working_dir: PathBuf,
    pub path_entries: Vec<PathBuf>,
}

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
    if !installed.is_empty() {
        pieces.push(Piece {
            kind: PieceKind::HostFiles,
            detail: format!("Codex — {} /kmp- prompts", installed.len()),
            path: codex_prompts,
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

/// Whether this piece may be removed at all, and why not when it may not.
///
/// A store is never refused here: its memory is saved first, and only a failed
/// save keeps it. What is refused is what was never this verb's to delete.
pub fn refusal(piece: &Piece) -> Option<String> {
    if piece.ours_to_remove {
        return None;
    }
    Some(match piece.kind {
        PieceKind::HostWiring => "inside a file that is not ours".to_string(),
        PieceKind::Bundle => "committed memory belongs to the repository".to_string(),
        _ => "outside the home this surveyed".to_string(),
    })
}

/// Where a store's memory is handed back before the store goes.
///
/// Into the working directory, because that is where the person running the
/// command is standing and a file they cannot find is not a copy. The name
/// carries the store it came from, so two rescues in one run do not overwrite
/// each other.
pub fn rescue_path(piece: &Piece, workspace: &Path) -> Option<PathBuf> {
    if piece.kind != PieceKind::Store {
        return None;
    }
    Some(workspace.join(format!("kmp-memory-{}.jsonl", store_label(&piece.path))))
}

fn store_label(store: &Path) -> String {
    let name = store
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("store");
    // A project store is always called `.kernel`, so the directory above it is
    // the name a person would recognise.
    if name == ".kernel" {
        return store
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string();
    }
    name.to_string()
}

/// The report. Same shape as the doctor: one line per piece, the fix attached
/// to the problem, a verdict in plain words.
pub fn report(pieces: &[Piece], workspace: &Path, purge: bool, applying: bool) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}\n",
        crate::banner::large_with(if applying {
            "  uninstall — removing what is listed below"
        } else {
            "  uninstall — a dry run; nothing has been removed"
        })
    );

    if pieces.is_empty() {
        let _ = writeln!(out, "Nothing of KMP's is on this machine.");
        return out;
    }

    let _ = writeln!(out, "{}", crate::banner::head("Found"));
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
        let _ = writeln!(out, "{}", crate::banner::head("Getting it back"));
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

fn engine_file_name() -> &'static str {
    if cfg!(windows) {
        "kmp-mcp.exe"
    } else {
        "kmp-mcp"
    }
}

fn engine_directories(roots: &Roots) -> Vec<PathBuf> {
    let mut directories = vec![
        roots.home.join(".local/bin"),
        roots.home.join(".cargo/bin"),
        roots.data_home.join("kmp/bin"),
    ];
    directories.extend(roots.path_entries.iter().cloned());
    directories.sort();
    directories.dedup();
    directories
}

/// Every store this machine has, not only the one this directory resolves to.
/// The per-user default and the project store are both easy to forget, and
/// forgetting one is how an uninstall leaves memory behind.
fn stores(roots: &Roots) -> Vec<PathBuf> {
    // The same enumeration `info` shows, so the dry run cannot promise to
    // remove a set the operator was never shown — including the stores no
    // resolution rule reaches, which are the strongest candidates for removal
    // and the ones nothing would otherwise mention.
    let index = roots
        .data_home
        .join("kmp")
        .join(crate::memories::INDEX_FILE);
    let mut found: Vec<PathBuf> =
        crate::memories::list(&roots.data_home, &crate::memories::read_index(&index))
            .into_iter()
            .map(|memory| memory.path)
            .collect();

    // Plus the one under this directory, which may never have been opened.
    let project = roots.working_dir.join(".kernel");
    if project.join("FORMAT_VERSION").is_file() {
        found.push(project);
    }
    found.sort();
    found.dedup();
    found
}

/// Only a project store has a bundle. An explicit data dir or the per-user
/// default belongs to no repository, so there is no conventional place a copy
/// of it would be — the same reason `export` refuses to guess one.
fn bundle_beside(store: &Path) -> Option<PathBuf> {
    if store.file_name()? != std::ffi::OsStr::new(".kernel") {
        return None;
    }
    store
        .parent()
        .map(|root| root.join(kmp_embedded::PROJECT_BUNDLE_PATH))
}

/// The header of a bundle says how many events it holds, and reading it never
/// opens the store.
fn bundle_event_count(bundle: &Path) -> Option<u64> {
    let contents = std::fs::read_to_string(bundle).ok()?;
    let header = contents.lines().next()?;
    serde_json::from_str::<serde_json::Value>(header)
        .ok()?
        .get("event_count")?
        .as_u64()
}

fn kmp_prompts(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("kmp-") && name.ends_with(".md"))
        })
        .collect()
}

fn file_mentions(path: &Path, needle: &str) -> bool {
    std::fs::read_to_string(path).is_ok_and(|contents| contents.contains(needle))
}

fn describe_size(path: &Path) -> String {
    let bytes = directory_size(path);
    if bytes >= 1_048_576 {
        format!("{:.1}M", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1_024 {
        format!("{}K", bytes / 1_024)
    } else {
        format!("{bytes}B")
    }
}

fn describe_store(store: &Path) -> String {
    let format = std::fs::read_to_string(store.join("FORMAT_VERSION"))
        .map(|text| text.trim().to_string())
        .unwrap_or_else(|_| "?".to_string());
    format!("{} · store format {format}", describe_size(store))
}

fn describe_versions(directory: &Path) -> String {
    let mut versions = std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();
    versions.sort();
    if versions.is_empty() {
        describe_size(directory)
    } else {
        format!("{} ({})", describe_size(directory), versions.join(", "))
    }
}

fn directory_size(path: &Path) -> u64 {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    // A symlink is not walked into. Its target may be anywhere, including an
    // ancestor of this walk, and a size that recurses forever is not a size.
    if metadata.is_symlink() {
        return 0;
    }
    std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| directory_size(&entry.path()))
        .sum()
}

/// Removes one piece, or says why it did not.
///
/// It removes exactly the path it was given and never walks outward: no
/// sibling store, no parent directory, nothing that was not printed in the
/// dry run the operator read before saying `--apply`.
pub fn remove(piece: &Piece) -> Result<(), String> {
    if let Some(reason) = refusal(piece) {
        return Err(reason);
    }
    let path = &piece.path;
    let result = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    result.map_err(|error| format!("could not remove `{}`: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots(base: &Path) -> Roots {
        Roots {
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

    fn bundle_at(path: &Path, events: u64) {
        std::fs::create_dir_all(path.parent().expect("bundle parent")).expect("bundle dir");
        std::fs::write(path, format!("{{\"event_count\":{events}}}\n")).expect("bundle");
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
    fn a_store_is_saved_into_the_working_directory_before_it_goes() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        store_at(&base.join("project/.kernel"), "1");

        let store = survey(&roots(base))
            .into_iter()
            .find(|piece| piece.kind == PieceKind::Store)
            .expect("the project store");

        // Never refused for want of a copy: the copy is made here.
        assert!(refusal(&store).is_none());
        let rescue = rescue_path(&store, &base.join("project")).expect("stores are saved");
        assert_eq!(
            rescue.file_name().and_then(|name| name.to_str()),
            Some("kmp-memory-project.jsonl"),
            "the file has to name the memory it holds"
        );
    }

    #[test]
    fn two_stores_do_not_overwrite_each_others_rescue() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        store_at(&base.join("home/.local/share/kmp/default"), "2");
        store_at(&base.join("project/.kernel"), "1");

        let workspace = base.join("project");
        let names = survey(&roots(base))
            .iter()
            .filter_map(|piece| rescue_path(piece, &workspace))
            .map(|path| path.display().to_string())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(names.len(), 2, "{names:?}");
    }

    #[test]
    fn the_dry_run_says_where_each_memory_will_be_saved() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        store_at(&base.join("project/.kernel"), "1");

        let workspace = base.join("project");
        let pieces = survey(&roots(base));
        let saving = report(&pieces, &workspace, false, false);
        assert!(saving.contains("saved first to"), "{saving}");
        assert!(saving.contains("kmp-memory-project.jsonl"), "{saving}");

        // ...and that --purge is the way to say no copy is wanted.
        let purged = report(&pieces, &workspace, true, false);
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

    #[test]
    fn a_dry_run_says_it_removed_nothing() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        store_at(&base.join("project/.kernel"), "1");

        let pieces = survey(&roots(base));
        let dry = report(&pieces, &base.join("project"), false, false);
        assert!(dry.contains("nothing has been removed"));
        assert!(base.join("project/.kernel").exists());
    }

    #[test]
    fn the_report_says_how_to_get_the_memory_back() {
        // A copy nobody knows how to restore is a file, not a backup.
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        store_at(&base.join("project/.kernel"), "1");

        let saving = report(&survey(&roots(base)), &base.join("project"), false, true);
        assert!(saving.contains("kmp-mcp import"), "{saving}");
        assert!(
            saving.contains("empty store"),
            "and the one rule that trips people up: {saving}"
        );
    }

    #[test]
    fn an_empty_machine_says_so_rather_than_printing_a_blank_list() {
        let base = tempfile::tempdir().expect("temp");
        let empty = report(&survey(&roots(base.path())), base.path(), false, false);
        assert!(empty.contains("Nothing of KMP's is on this machine."));
    }
}
