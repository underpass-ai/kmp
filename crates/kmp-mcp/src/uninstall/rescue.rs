//! Saving memory before it is removed, and refusing when it cannot be.
//!
//! One concept: the promise that the last thing an uninstall does before
//! deleting memory is hand it back. A copy made at the moment of removal beats
//! a bundle committed at some earlier point, because only one of the two is
//! certainly current.

use std::path::{Path, PathBuf};

use crate::uninstall::description::store_label;
use crate::uninstall::piece::Piece;
use crate::uninstall::piece_kind::PieceKind;
use sha2::{Digest, Sha256};

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
    // A readable basename is not an identity: two checkouts can both be
    // called `api`, and arbitrary stores commonly end in `memory`. Keep the
    // label for the human and bind the rescue to the store's full path so a
    // later export in the same uninstall cannot truncate an earlier one.
    let digest = Sha256::digest(piece.path.to_string_lossy().as_bytes());
    let path_id = format!("{digest:x}");
    Some(workspace.join(format!(
        "kmp-memory-{}-{}.jsonl",
        store_label(&piece.path),
        &path_id[..12]
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uninstall::survey::survey;
    use crate::uninstall::test_support::{roots, store_at};

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
        let name = rescue
            .file_name()
            .and_then(|name| name.to_str())
            .expect("rescue file name");
        assert!(name.starts_with("kmp-memory-project-"), "{name}");
        assert!(name.ends_with(".jsonl"), "{name}");
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
    fn registered_stores_with_the_same_basename_get_distinct_rescues() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let first = base.join("checkouts/work/api/memory");
        let second = base.join("checkouts/personal/api/memory");
        store_at(&first, "2");
        store_at(&second, "2");
        let roots = roots(base);
        crate::memories::remember(&roots.data_home, &first);
        crate::memories::remember(&roots.data_home, &second);

        let workspace = base.join("project");
        let rescues = survey(&roots)
            .iter()
            .filter_map(|piece| rescue_path(piece, &workspace))
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
