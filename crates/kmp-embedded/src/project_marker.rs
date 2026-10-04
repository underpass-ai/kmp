//! What makes a directory the root of a project, read from the directory
//! itself: a `.git` directory, a git worktree's `.git` file, or a store that
//! is already there.

use std::fs;
use std::path::{Path, PathBuf};

/// The store directory inside a project root.
pub(crate) const PROJECT_DIR_NAME: &str = ".kernel";

/// What makes a directory the root of a project, as seen from inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProjectMarker {
    /// A checkout (`.git` directory) or a directory that already holds a
    /// store: the store is `.kernel/` right there.
    Root,
    /// A git worktree whose main checkout is elsewhere.
    Worktree { main_checkout: PathBuf },
}

/// Reads the markers in one directory. A `.git` directory or an existing
/// store makes it a root; a `.git` file is a worktree when its common git
/// directory is the `.git` of an existing main checkout. Anything else that
/// is a `.git` file — a submodule, a worktree of a bare repository, a file
/// git itself could not follow — keeps the directory as its own root, which
/// is what git also treats as the checkout.
pub(crate) fn project_marker(candidate: &Path) -> Option<ProjectMarker> {
    let git = candidate.join(".git");
    if git.is_dir() {
        return Some(ProjectMarker::Root);
    }
    if git.is_file() {
        return Some(
            main_checkout_of_worktree(candidate, &git)
                .map(|main_checkout| ProjectMarker::Worktree { main_checkout })
                .unwrap_or(ProjectMarker::Root),
        );
    }
    candidate
        .join(PROJECT_DIR_NAME)
        .join("FORMAT_VERSION")
        .is_file()
        .then_some(ProjectMarker::Root)
}

/// `<worktree>/.git` says `gitdir: <main>/.git/worktrees/<name>`, and that
/// directory's `commondir` names `<main>/.git`. Both may be relative.
fn main_checkout_of_worktree(worktree: &Path, git_file: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(git_file).ok()?;
    let git_dir = text.lines().find_map(|line| line.strip_prefix("gitdir:"))?;
    let git_dir = worktree.join(git_dir.trim());
    let common = fs::read_to_string(git_dir.join("commondir")).ok()?;
    let common = fs::canonicalize(git_dir.join(common.trim())).ok()?;
    if common.file_name()? != ".git" || !common.is_dir() {
        return None;
    }
    let main_checkout = common.parent()?.to_path_buf();
    let same = fs::canonicalize(worktree).ok().as_deref() == Some(main_checkout.as_path());
    (!same).then_some(main_checkout)
}
