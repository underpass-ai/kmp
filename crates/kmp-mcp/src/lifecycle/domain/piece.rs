use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::piece_kind::PieceKind;

/// One thing the survey found, told what to decide about itself: whether it
/// may be removed, and where its memory is handed back first.
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

impl Piece {
    /// Whether this piece may be removed at all, and why not when it may not.
    ///
    /// A store is never refused here: its memory is saved first, and only a
    /// failed save keeps it. What is refused is what was never this verb's
    /// to delete.
    pub fn refusal(&self) -> Option<String> {
        if self.ours_to_remove {
            return None;
        }
        Some(match self.kind {
            PieceKind::HostWiring => "inside a file that is not ours".to_string(),
            PieceKind::Bundle => "committed memory belongs to the repository".to_string(),
            _ => "outside the home this surveyed".to_string(),
        })
    }

    /// Where this store's memory is handed back before the store goes.
    ///
    /// Into the working directory, because that is where the person running
    /// the command is standing and a file they cannot find is not a copy. The
    /// name carries the store it came from, so two rescues in one run do not
    /// overwrite each other.
    pub fn rescue_path(&self, workspace: &Path) -> Option<PathBuf> {
        if self.kind != PieceKind::Store {
            return None;
        }
        // A readable basename is not an identity: two checkouts can both be
        // called `api`, and arbitrary stores commonly end in `memory`. Keep
        // the label for the human and bind the rescue to the store's full
        // path so a later export in the same uninstall cannot truncate an
        // earlier one.
        let digest = Sha256::digest(self.path.to_string_lossy().as_bytes());
        let path_id = format!("{digest:x}");
        Some(workspace.join(format!(
            "kmp-memory-{}-{}.jsonl",
            store_label(&self.path),
            &path_id[..12]
        )))
    }
}

fn store_label(store: &Path) -> String {
    let name = store
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("store");
    // A project store is always called `.kernel`, so the directory above it
    // is the name a person would recognise.
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

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::Piece;
    use crate::lifecycle::domain::piece_kind::PieceKind;

    fn store_piece(path: &Path) -> Piece {
        Piece {
            kind: PieceKind::Store,
            path: path.to_path_buf(),
            detail: String::new(),
            bundled_events: None,
            ours_to_remove: true,
        }
    }

    #[test]
    fn a_store_is_saved_into_the_working_directory_before_it_goes() {
        let store = store_piece(Path::new("/checkout/project/.kernel"));

        // Never refused for want of a copy: the copy is made at removal time.
        assert!(store.refusal().is_none());
        let rescue = store
            .rescue_path(Path::new("/checkout/project"))
            .expect("stores are saved");
        let name = rescue
            .file_name()
            .and_then(|name| name.to_str())
            .expect("rescue file name");
        assert!(name.starts_with("kmp-memory-project-"), "{name}");
        assert!(name.ends_with(".jsonl"), "{name}");
    }

    #[test]
    fn two_stores_do_not_overwrite_each_others_rescue() {
        let workspace = PathBuf::from("/checkout/project");
        let names = [
            store_piece(Path::new("/home/user/.local/share/kmp/default")),
            store_piece(Path::new("/checkout/project/.kernel")),
        ]
        .iter()
        .filter_map(|piece| piece.rescue_path(&workspace))
        .map(|path| path.display().to_string())
        .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(names.len(), 2, "{names:?}");
    }

    #[test]
    fn registered_stores_with_the_same_basename_get_distinct_rescues() {
        // A readable basename is not an identity: two checkouts can both be
        // called `api`, and arbitrary stores commonly end in `memory`.
        let workspace = PathBuf::from("/checkout/project");
        let rescues = [
            store_piece(Path::new("/checkouts/work/api/memory")),
            store_piece(Path::new("/checkouts/personal/api/memory")),
        ]
        .iter()
        .filter_map(|piece| piece.rescue_path(&workspace))
        .collect::<Vec<_>>();

        assert_eq!(rescues.len(), 2, "{rescues:?}");
        assert_ne!(rescues[0], rescues[1], "{rescues:?}");
        assert!(rescues.iter().all(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with("kmp-memory-memory-") && name.ends_with(".jsonl")
                })
        }));
    }
}
