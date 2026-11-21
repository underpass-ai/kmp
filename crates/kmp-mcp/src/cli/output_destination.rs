//! Where a verb writes, decided from the file itself and not from how the
//! path was spelled or how the store was chosen.
//!
//! `.kmp/memory.jsonl` is committed memory whichever way it is named: with no
//! file, as `.kmp/memory.jsonl`, as an absolute path, through a symlinked
//! directory, or with the store pinned by `KMP_MCP_DATA_DIR` (as a host that
//! manages its own store does). Comparing the spelling let every form but the
//! exact absolute one skip the guard that keeps a teammate's events. Every
//! verb that takes an output file (`export`, `document --out`) resolves it
//! here, so none of them writes over committed memory by another name.

use std::path::{Component, Path, PathBuf};

/// One output file, resolved once.
#[derive(Debug)]
pub(super) struct OutputDestination {
    /// The path as the operator gave it (or the project head when none was):
    /// reported back unchanged, so a caller reads what it asked for.
    pub(super) given: PathBuf,
    /// The file the bytes land in: the symlink's target when the last
    /// component is a link to committed memory, so the link survives.
    pub(super) write_to: PathBuf,
    /// The store's own committed file: the catch-up, the guide exclusion and
    /// `--repair-pending` apply.
    pub(super) is_project_head: bool,
    /// Any `.kmp/memory.jsonl`, this store's or not: only a bundle that
    /// keeps its events and leaves out the local abouts may land there.
    pub(super) is_committed: bool,
}

impl OutputDestination {
    pub(super) fn resolve(given: PathBuf, project_head: Option<&Path>) -> Self {
        let real = real_location(&given);
        let is_project_head = project_head.is_some_and(|head| real_location(head) == real);
        let is_committed = is_project_head || is_committed_bundle(&real);
        let write_to = if is_committed && is_symlink(&given) {
            real
        } else {
            given.clone()
        };
        Self {
            given,
            write_to,
            is_project_head,
            is_committed,
        }
    }
}

fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
}

/// `<dir>/.kmp/memory.jsonl`, the committed-memory convention.
fn is_committed_bundle(real: &Path) -> bool {
    let mut parts = Path::new(kmp_embedded::PROJECT_BUNDLE_PATH)
        .components()
        .rev();
    let mut real_parts = real.components().rev();
    parts.all(|part| real_parts.next() == Some(part))
}

/// The absolute location a path names, symlinks followed. A file that does
/// not exist yet is located through its nearest existing ancestor, so a first
/// save that will create `.kmp/` still resolves to the same place as the head.
fn real_location(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(dir) => dir.join(path),
            Err(_) => path.to_path_buf(),
        }
    };
    let mut existing = absolute.as_path();
    let mut rest = Vec::new();
    loop {
        if let Ok(real) = std::fs::canonicalize(existing) {
            return rest.iter().rev().fold(
                real,
                |location: PathBuf, part: &Component<'_>| match part {
                    Component::ParentDir => {
                        location.parent().map(Path::to_path_buf).unwrap_or(location)
                    }
                    Component::CurDir => location,
                    other => location.join(other.as_os_str()),
                },
            );
        }
        match (existing.parent(), existing.components().next_back()) {
            (Some(parent), Some(last)) => {
                rest.push(last);
                existing = parent;
            }
            _ => return absolute,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_file_is_recognised_however_it_is_named() {
        let project = tempfile::tempdir().expect("project");
        let root = std::fs::canonicalize(project.path()).expect("real root");
        let head = root.join(".kmp/memory.jsonl");

        // Absent: a first save resolves to the head as well.
        let absent = OutputDestination::resolve(root.join("./.kmp/memory.jsonl"), Some(&head));
        assert!(absent.is_project_head && absent.is_committed, "{absent:?}");

        std::fs::create_dir_all(root.join(".kmp")).expect("bundle dir");
        std::fs::write(&head, "x").expect("bundle");
        let dotted = OutputDestination::resolve(root.join("sub/../.kmp/memory.jsonl"), Some(&head));
        assert!(dotted.is_project_head, "{dotted:?}");

        let elsewhere = OutputDestination::resolve(root.join("backup.jsonl"), Some(&head));
        assert!(!elsewhere.is_project_head && !elsewhere.is_committed);

        let pinned = OutputDestination::resolve(head.clone(), None);
        assert!(!pinned.is_project_head && pinned.is_committed, "{pinned:?}");
        assert_eq!(pinned.write_to, head);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_the_committed_file_is_the_committed_file_and_survives() {
        let project = tempfile::tempdir().expect("project");
        let root = std::fs::canonicalize(project.path()).expect("real root");
        let head = root.join(".kmp/memory.jsonl");
        std::fs::create_dir_all(root.join(".kmp")).expect("bundle dir");
        std::fs::write(&head, "x").expect("bundle");
        std::os::unix::fs::symlink(&head, root.join("link.jsonl")).expect("file link");
        std::os::unix::fs::symlink(root.join(".kmp"), root.join("alias")).expect("dir link");

        let link = OutputDestination::resolve(root.join("link.jsonl"), Some(&head));
        assert!(link.is_project_head, "{link:?}");
        assert_eq!(link.write_to, head);
        assert_eq!(link.given, root.join("link.jsonl"));

        let alias = OutputDestination::resolve(root.join("alias/memory.jsonl"), Some(&head));
        assert!(alias.is_project_head, "{alias:?}");
        assert_eq!(alias.write_to, root.join("alias/memory.jsonl"));
    }
}
