use std::path::{Path, PathBuf};

use crate::lifecycle::application::use_cases::survey_memories::SurveyMemories;
use crate::lifecycle::domain::piece::Piece;
use crate::lifecycle::domain::piece_kind::PieceKind;
use crate::lifecycle::domain::store_leases_dir::store_leases_dir;
use crate::lifecycle::domain::survey_roots::SurveyRoots;
use crate::lifecycle::ports::installation_catalog::InstallationCatalog;
use crate::lifecycle::ports::store_catalog::StoreCatalog;
use crate::lifecycle::ports::store_index::StoreIndex;

/// Use case: everything of KMP's that is actually on this machine, in the
/// order a reader should meet it — the engine that answers, the memory that
/// would be lost, then the wiring.
///
/// Where to look and what may be removed are decided here; every look at the
/// disk itself goes through the ports.
pub struct SurveyInstallation<'a> {
    installation: &'a dyn InstallationCatalog,
    stores: &'a dyn StoreCatalog,
    index: &'a dyn StoreIndex,
}

impl<'a> SurveyInstallation<'a> {
    pub fn new(
        installation: &'a dyn InstallationCatalog,
        stores: &'a dyn StoreCatalog,
        index: &'a dyn StoreIndex,
    ) -> Self {
        Self {
            installation,
            stores,
            index,
        }
    }

    pub fn execute(&self, roots: &SurveyRoots) -> Vec<Piece> {
        let mut pieces = Vec::new();

        for directory in engine_directories(roots) {
            let engine = directory.join(engine_file_name());
            if !self.installation.is_file(&engine) {
                continue;
            }
            // An engine on `PATH` but outside this home may be a package
            // manager's, or another user's. It is worth naming — a second
            // copy is how a live session ends up older than the merged fix
            // (#80) — and it is not this verb's to delete.
            let ours = engine.starts_with(&roots.home);
            let size = self.installation.size_of(&engine).human();
            pieces.push(Piece {
                kind: PieceKind::Engine,
                detail: if ours {
                    size
                } else {
                    format!("{size} — outside your home; remove it yourself if you meant to")
                },
                path: engine,
                bundled_events: None,
                ours_to_remove: ours,
            });
        }

        for store in self.stores(roots) {
            let bundled_events = bundle_beside(&store)
                .as_deref()
                .and_then(|bundle| self.installation.bundle_event_count(bundle));
            pieces.push(Piece {
                kind: PieceKind::Store,
                detail: self.described_store(&store),
                path: store,
                bundled_events,
                ours_to_remove: true,
            });
        }

        for bundle in self
            .stores(roots)
            .iter()
            .filter_map(|store| bundle_beside(store))
        {
            if self.installation.is_file(&bundle) {
                pieces.push(Piece {
                    kind: PieceKind::Bundle,
                    detail: format!(
                        "{} — committed memory; left alone unless you ask",
                        self.installation.size_of(&bundle).human()
                    ),
                    path: bundle,
                    bundled_events: None,
                    ours_to_remove: false,
                });
            }
        }

        // KMP's own note of where memory has been. Small, and leaving it
        // behind means a fresh install starts with a list of stores that are
        // gone.
        let index = self.index.location();
        if self.installation.is_file(&index) {
            pieces.push(Piece {
                kind: PieceKind::HostFiles,
                detail: format!(
                    "{} — the note of which memories exist here",
                    self.installation.size_of(&index).human()
                ),
                path: index,
                bundled_events: None,
                ours_to_remove: true,
            });
        }

        let leases = store_leases_dir(&roots.data_home);
        if self.installation.is_directory(&leases) {
            pieces.push(Piece {
                kind: PieceKind::HostFiles,
                detail: format!(
                    "{} — machine-local locks that keep active stores safe",
                    self.installation.size_of(&leases).human()
                ),
                path: leases,
                bundled_events: None,
                ours_to_remove: true,
            });
        }

        let claude_plugin = roots.home.join(".claude/plugins/cache/underpass/kmp");
        if self.installation.is_directory(&claude_plugin) {
            pieces.push(Piece {
                kind: PieceKind::HostFiles,
                detail: format!(
                    "Claude Code plugin — {}",
                    self.described_versions(&claude_plugin)
                ),
                path: claude_plugin,
                bundled_events: None,
                ours_to_remove: true,
            });
        }

        let codex_prompts = roots.home.join(".codex/prompts");
        for prompt in self.kmp_prompts(&codex_prompts) {
            pieces.push(Piece {
                kind: PieceKind::HostFiles,
                detail: format!(
                    "Codex /kmp- prompt — {}",
                    self.installation.size_of(&prompt).human()
                ),
                path: prompt,
                bundled_events: None,
                ours_to_remove: true,
            });
        }

        // Registrations live inside files that are not ours. This verb names
        // them and the command that removes them; it does not edit a user's
        // configuration on their behalf, because a botched edit there costs
        // more than the uninstall saves.
        let claude_config = roots.home.join(".claude.json");
        for (needle, server_id) in [("\"kmp\"", "kmp"), ("\"kernel-memory\"", "kernel-memory")] {
            if self.installation.file_mentions(&claude_config, needle) {
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
            if self.installation.file_mentions(&codex_config, &header) {
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

    /// Every store this machine has, not only the one this directory
    /// resolves to. The per-user default and the project store are both easy
    /// to forget, and forgetting one is how an uninstall leaves memory
    /// behind.
    fn stores(&self, roots: &SurveyRoots) -> Vec<PathBuf> {
        // The same enumeration `info` shows, so the dry run cannot promise
        // to remove a set the operator was never shown — including the
        // stores no resolution rule reaches, which are the strongest
        // candidates for removal and the ones nothing would otherwise
        // mention.
        let mut found: Vec<PathBuf> = SurveyMemories::new(self.stores, self.index)
            .execute()
            .into_iter()
            .map(|memory| memory.path)
            .collect();

        // Plus the one under this directory, which may never have been
        // opened.
        let project = roots.working_dir.join(".kernel");
        if self.installation.is_file(&project.join("FORMAT_VERSION")) {
            found.push(project);
        }
        found.sort();
        found.dedup();
        found
    }

    fn described_store(&self, store: &Path) -> String {
        let format = self
            .installation
            .store_stamp(store)
            .unwrap_or_else(|| "?".to_string());
        format!(
            "{} · store format {format}",
            self.installation.size_of(store).human()
        )
    }

    fn described_versions(&self, directory: &Path) -> String {
        let versions = self.installation.entry_names(directory);
        let size = self.installation.size_of(directory).human();
        if versions.is_empty() {
            size
        } else {
            format!("{} ({})", size, versions.join(", "))
        }
    }

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
}

fn engine_file_name() -> &'static str {
    if cfg!(windows) {
        "kmp-mcp.exe"
    } else {
        "kmp-mcp"
    }
}

fn engine_directories(roots: &SurveyRoots) -> Vec<PathBuf> {
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

/// Only a project store has a bundle. An explicit data dir or the per-user
/// default belongs to no repository, so there is no conventional place a
/// copy of it would be — the same reason `export` refuses to guess one.
fn bundle_beside(store: &Path) -> Option<PathBuf> {
    if store.file_name()? != std::ffi::OsStr::new(".kernel") {
        return None;
    }
    store
        .parent()
        .map(|root| root.join(kmp_embedded::PROJECT_BUNDLE_PATH))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::lifecycle::adapters::filesystem_store_catalog::FilesystemStoreCatalog;
    use crate::lifecycle::adapters::jsonl_store_index::JsonlStoreIndex;
    use crate::lifecycle::adapters::native_installation_catalog::NativeInstallationCatalog;
    use crate::lifecycle::application::use_cases::remove_piece::RemovePiece;

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

    fn bundle_at(path: &Path, events: u64) {
        std::fs::create_dir_all(path.parent().expect("bundle parent")).expect("bundle dir");
        std::fs::write(path, format!("{{\"event_count\":{events}}}\n")).expect("bundle");
    }

    fn survey(roots: &SurveyRoots) -> Vec<Piece> {
        let stores = FilesystemStoreCatalog::new(&roots.data_home);
        let index = JsonlStoreIndex::new(&roots.data_home);
        SurveyInstallation::new(&NativeInstallationCatalog, &stores, &index).execute(roots)
    }

    fn remove(piece: &Piece) -> Result<(), String> {
        RemovePiece::new(&NativeInstallationCatalog).execute(piece)
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
        assert!(bundle.refusal().is_some(), "and it is not ours to delete");
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
        assert!(store.refusal().is_none());
    }
}
