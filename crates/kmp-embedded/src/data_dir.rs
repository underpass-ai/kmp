use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use kmp_adapter_embedded::validate_store_layout;
use kmp_domain::PortError;

use crate::memory_selection::{self, SelectedMemory};
use crate::memory_selection_refusal::SelectionRefusal;

/// Explicit data directory override (ADR-012 rule 1).
pub const DATA_DIR_ENV: &str = "KMP_MCP_DATA_DIR";

const PROJECT_DIR_NAME: &str = ".kernel";

/// Where a project keeps the committed copy of its memory, relative to the
/// project root.
///
/// The store itself (`.kernel/`) is machine state and is auto-gitignored. A
/// bundle is the event log in one text file, which is a different thing: it
/// belongs to the repository the same way a migration or a fixture does, so
/// memory branches, reviews and reverts with the code that produced it.
///
/// The path is a convention rather than a setting so that `export` and
/// `import` with no argument mean the same thing in every checkout, and so a
/// reviewer knows where to look.
pub const PROJECT_BUNDLE_PATH: &str = ".kmp/memory.jsonl";

/// Where the data directory came from — logged at startup so the winning
/// resolution rule is always visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedDataDir {
    /// `KMP_MCP_DATA_DIR` was set.
    Explicit(PathBuf),
    /// The operator saved this selection in the user config file.
    Saved(PathBuf),
    /// `<project-root>/.kernel/`, project root found by walking up to `.git`.
    Project(PathBuf),
    /// Per-user fallback under the platform data dir.
    UserDefault(PathBuf),
    /// A project store was present but could not be opened, so the live
    /// session selected the user store and must surface that the repository
    /// bundle beside the rejected store is no longer maintained.
    UserFallback {
        path: PathBuf,
        orphaned_bundle: OrphanedProjectBundle,
    },
}

/// The durability contract lost when an unopenable project store falls back
/// to the shared user store. Selection owns this fact because it is the only
/// layer that knows both paths and the rejected layout reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanedProjectBundle {
    pub bundle_path: PathBuf,
    pub project_store_path: PathBuf,
    pub selected_store_path: PathBuf,
    pub reason: String,
}

impl ResolvedDataDir {
    pub fn path(&self) -> &Path {
        match self {
            Self::Explicit(path)
            | Self::Saved(path)
            | Self::Project(path)
            | Self::UserDefault(path) => path,
            Self::UserFallback { path, .. } => path,
        }
    }

    pub fn rule_name(&self) -> &'static str {
        match self {
            Self::Explicit(_) => "env",
            Self::Saved(_) => "saved",
            Self::Project(_) => "project",
            Self::UserDefault(_) => "user",
            Self::UserFallback { .. } => "user fallback",
        }
    }

    /// The rule in the words a person reads, naming what actually decided.
    pub fn rule_sentence(&self) -> &'static str {
        match self {
            Self::Explicit(_) => {
                "the KMP_MCP_DATA_DIR environment variable, which overrides everything for this \
                 process"
            }
            Self::Saved(_) => "the selection saved in the user config file",
            Self::Project(_) => "the nearest project root above the working directory",
            Self::UserDefault(_) => "the per-user default, because nothing more specific applied",
            Self::UserFallback { .. } => {
                "the per-user default, because the project store beside a committed bundle could \
                 not be opened"
            }
        }
    }

    pub fn orphaned_bundle(&self) -> Option<&OrphanedProjectBundle> {
        match self {
            Self::UserFallback {
                orphaned_bundle, ..
            } => Some(orphaned_bundle),
            Self::Explicit(_) | Self::Saved(_) | Self::Project(_) | Self::UserDefault(_) => None,
        }
    }
}

/// Resolution order: the environment override, then the saved selection,
/// then the project `.kernel/`, then the per-user default.
///
/// `KMP_MCP_DATA_DIR` stays first because it is the most explicit and most
/// local thing anyone can say, and every test, baseline and reproduction
/// script depends on that override meaning exactly one process. A saved
/// selection then beats automatic discovery, which is the whole point of
/// saving one: a workspace with no project marker must reach the memory the
/// operator chose, not whatever the per-user default happens to hold.
///
/// Pure function for testability; `resolve_data_dir_from_env` feeds it from
/// the process environment and the user config file.
pub fn resolve_data_dir(
    env_override: Option<&str>,
    saved: Option<&SelectedMemory>,
    working_dir: &Path,
    user_data_home: &Path,
) -> ResolvedDataDir {
    resolve_with_project_marker(
        env_override,
        saved,
        working_dir,
        user_data_home,
        |candidate| candidate.join(".git").exists(),
    )
}

fn resolve_with_project_marker(
    env_override: Option<&str>,
    saved: Option<&SelectedMemory>,
    working_dir: &Path,
    user_data_home: &Path,
    is_project_root: impl Fn(&Path) -> bool,
) -> ResolvedDataDir {
    if let Some(explicit) = env_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let path = PathBuf::from(explicit);
        return ResolvedDataDir::Explicit(if path.is_absolute() {
            path
        } else {
            working_dir.join(path)
        });
    }

    if let Some(saved) = saved {
        return ResolvedDataDir::Saved(saved.path().to_path_buf());
    }

    let mut current = Some(working_dir);
    while let Some(candidate) = current {
        if is_project_root(candidate) {
            return ResolvedDataDir::Project(candidate.join(PROJECT_DIR_NAME));
        }
        current = candidate.parent();
    }

    ResolvedDataDir::UserDefault(user_data_home.join("kmp").join("default"))
}

/// Where user-scope memory lives: the Unix data home when available, then
/// the native Windows local-data directories.
///
/// Exposed because it is also where anything that wants to enumerate the
/// machine's memories has to look, and a second copy of this rule would be a
/// second answer to the same question.
pub fn user_data_home() -> Option<PathBuf> {
    user_data_home_from(|name| std::env::var_os(name))
}

fn user_data_home_from(mut read: impl FnMut(&str) -> Option<OsString>) -> Option<PathBuf> {
    let path = |value: Option<OsString>| {
        value
            .map(PathBuf::from)
            .filter(|candidate| !candidate.as_os_str().is_empty())
    };

    path(read("XDG_DATA_HOME"))
        .or_else(|| path(read("HOME")).map(|home| home.join(".local").join("share")))
        .or_else(|| path(read("LOCALAPPDATA")))
        .or_else(|| path(read("APPDATA")))
        .or_else(|| path(read("USERPROFILE")).map(|home| home.join("AppData").join("Local")))
}

/// The conventional bundle path for the project `data_dir` belongs to.
///
/// Only a project-scoped store has one: an explicit `KMP_MCP_DATA_DIR` or the
/// per-user default has no repository to be committed to, and guessing one
/// would put memory somewhere the operator did not choose.
pub fn project_bundle_path(resolved: &ResolvedDataDir) -> Option<PathBuf> {
    match resolved {
        ResolvedDataDir::Project(path) => path
            .parent()
            .map(|project_root| project_root.join(PROJECT_BUNDLE_PATH)),
        ResolvedDataDir::Explicit(_)
        | ResolvedDataDir::Saved(_)
        | ResolvedDataDir::UserDefault(_)
        | ResolvedDataDir::UserFallback { .. } => None,
    }
}

/// Resolves from the process environment and prepares the directory. Every
/// data directory gets the same safety skeleton, regardless of whether it was
/// discovered from a project, supplied explicitly, or created by migration.
pub fn resolve_data_dir_from_env() -> Result<ResolvedDataDir, PortError> {
    let resolved = locate_data_dir_from_env()?;
    prepare_data_dir(&resolved)?;
    Ok(resolved)
}

/// Resolves from the process environment and touches nothing.
///
/// Reporting where memory *would* live must not bring it into being:
/// `kmp-mcp info` and `kmp-mcp doctor` run wherever a user happens to be
/// standing, and a diagnostic that leaves a `.kernel/` behind in an unrelated
/// repository has answered a question by changing the answer.
pub fn locate_data_dir_from_env() -> Result<ResolvedDataDir, PortError> {
    let env_override = std::env::var(DATA_DIR_ENV).ok();
    let working_dir = std::env::current_dir().map_err(|error| {
        PortError::Unavailable(format!(
            "embedded kernel could not resolve the working directory: {error}"
        ))
    })?;
    let user_data_home = user_data_home().ok_or_else(|| {
        PortError::Unavailable(
            "embedded kernel could not resolve a user data directory \
             (none of XDG_DATA_HOME, HOME, LOCALAPPDATA, APPDATA, or USERPROFILE is set)"
                .to_string(),
        )
    })?;
    reject_unexpanded_home_override(env_override.as_deref())?;
    // A saved selection that cannot be read is an error, never a silent
    // fall-through: landing on a different store without saying so is the
    // defect this selection exists to end.
    let saved = memory_selection::saved_selection().map_err(|error| {
        PortError::InvalidState(format!(
            "the saved user memory selection is unusable: {error}"
        ))
    })?;

    let resolved = resolve_data_dir(
        env_override.as_deref(),
        saved.as_ref(),
        &working_dir,
        &user_data_home,
    );
    Ok(fallback_from_unopenable_project(resolved, &user_data_home))
}

fn fallback_from_unopenable_project(
    resolved: ResolvedDataDir,
    user_data_home: &Path,
) -> ResolvedDataDir {
    let ResolvedDataDir::Project(project_store_path) = resolved else {
        return resolved;
    };
    let Some(project_root) = project_store_path.parent() else {
        return ResolvedDataDir::Project(project_store_path);
    };
    let bundle_path = project_root.join(PROJECT_BUNDLE_PATH);
    if !bundle_path.is_file() {
        return ResolvedDataDir::Project(project_store_path);
    }
    let Err(error) = validate_store_layout(&project_store_path) else {
        return ResolvedDataDir::Project(project_store_path);
    };
    let selected_store_path = user_data_home.join("kmp").join("default");
    ResolvedDataDir::UserFallback {
        path: selected_store_path.clone(),
        orphaned_bundle: OrphanedProjectBundle {
            bundle_path,
            project_store_path,
            selected_store_path,
            reason: error.to_string(),
        },
    }
}

fn reject_unexpanded_home_override(env_override: Option<&str>) -> Result<(), PortError> {
    let Some(explicit) = env_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    let path = Path::new(explicit);
    let starts_with_tilde = path
        .components()
        .next()
        .is_some_and(|component| component.as_os_str() == "~");
    if !starts_with_tilde {
        return Ok(());
    }

    Err(PortError::InvalidState(
        SelectionRefusal::unexpanded(format!("{DATA_DIR_ENV} value"), explicit).to_string(),
    ))
}

fn prepare_data_dir(resolved: &ResolvedDataDir) -> Result<(), PortError> {
    ensure_data_dir_skeleton(resolved.path())
}

/// Creates the non-store part of a KMP data directory.
///
/// Fresh startup calls this function. The self-ignore file is deliberately
/// installed even for an explicit path: an
/// operator can put such a path inside a repository, and the store must not
/// start appearing in `git status`. Existing files are never replaced.
pub fn ensure_data_dir_skeleton(path: &Path) -> Result<(), PortError> {
    fs::create_dir_all(path).map_err(|error| {
        PortError::Unavailable(format!(
            "embedded kernel could not create data dir `{}`: {error}",
            path.display()
        ))
    })?;

    let gitignore = path.join(".gitignore");
    if !gitignore.exists() {
        fs::write(&gitignore, "*\n").map_err(|error| {
            PortError::Unavailable(format!(
                "embedded kernel could not write `{}`: {error}",
                gitignore.display()
            ))
        })?;
    }
    let logs = path.join("logs");
    fs::create_dir_all(&logs).map_err(|error| {
        PortError::Unavailable(format!(
            "embedded kernel could not create log dir `{}`: {error}",
            logs.display()
        ))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_project_store_has_a_conventional_bundle_path() {
        let project = ResolvedDataDir::Project(PathBuf::from("/repo/.kernel"));
        assert_eq!(
            project_bundle_path(&project),
            Some(PathBuf::from("/repo/.kmp/memory.jsonl")),
            "the bundle sits beside the store's project root, not inside the store"
        );

        // Neither of these belongs to a repository, and picking one for them
        // would write memory somewhere nobody chose.
        assert_eq!(
            project_bundle_path(&ResolvedDataDir::Explicit(PathBuf::from("/tmp/dir"))),
            None
        );
        assert_eq!(
            project_bundle_path(&ResolvedDataDir::UserDefault(PathBuf::from("/home/u/kmp"))),
            None
        );
    }

    fn saved(path: &str) -> SelectedMemory {
        SelectedMemory::parse(path).expect("an absolute test selection")
    }

    #[test]
    fn env_override_wins_over_everything() {
        let resolved = resolve_data_dir(
            Some("/explicit/dir"),
            Some(&saved("/saved/dir")),
            Path::new("/some/project"),
            Path::new("/home/u/.local/share"),
        );
        assert_eq!(
            resolved,
            ResolvedDataDir::Explicit(PathBuf::from("/explicit/dir"))
        );
        assert_eq!(resolved.rule_name(), "env");
        assert!(resolved.rule_sentence().contains("KMP_MCP_DATA_DIR"));
    }

    /// The whole precedence, one rule removed at a time: environment, then
    /// the saved selection, then the project, then the per-user default.
    #[test]
    fn every_rule_wins_exactly_over_the_ones_below_it() {
        let selection = saved("/saved/dir");
        let in_a_project = |env, saved| {
            resolve_with_project_marker(
                env,
                saved,
                Path::new("/workspace/project"),
                Path::new("/home/u/.local/share"),
                |candidate| candidate == Path::new("/workspace/project"),
            )
        };

        assert_eq!(
            in_a_project(Some("/explicit/dir"), Some(&selection)),
            ResolvedDataDir::Explicit(PathBuf::from("/explicit/dir"))
        );
        assert_eq!(
            in_a_project(None, Some(&selection)),
            ResolvedDataDir::Saved(PathBuf::from("/saved/dir")),
            "a saved selection beats project discovery, which is the point of saving one"
        );
        assert_eq!(
            in_a_project(None, None),
            ResolvedDataDir::Project(PathBuf::from("/workspace/project/.kernel"))
        );

        // The reproduction in #680: a workspace with no project marker and a
        // saved selection must reach the chosen memory, not the old default.
        let outside_a_project = |saved| {
            resolve_with_project_marker(
                None,
                saved,
                Path::new("/workspace/not-a-repository"),
                Path::new("/home/u/.local/share"),
                |_| false,
            )
        };
        let chosen = outside_a_project(Some(&selection));
        assert_eq!(chosen, ResolvedDataDir::Saved(PathBuf::from("/saved/dir")));
        assert_eq!(chosen.rule_name(), "saved");
        assert!(chosen.rule_sentence().contains("saved in the user config"));
        assert_eq!(
            outside_a_project(None),
            ResolvedDataDir::UserDefault(PathBuf::from("/home/u/.local/share/kmp/default"))
        );
    }

    #[test]
    fn a_relative_override_is_reported_as_the_path_that_will_actually_open() {
        let resolved = resolve_data_dir(
            Some("memory/kmp"),
            None,
            Path::new("/workspace/project"),
            Path::new("/home/u/.local/share"),
        );
        assert_eq!(
            resolved,
            ResolvedDataDir::Explicit(PathBuf::from("/workspace/project/memory/kmp"))
        );
    }

    #[test]
    fn blank_env_override_is_ignored() {
        let resolved = resolve_with_project_marker(
            Some("  "),
            None,
            Path::new("/anywhere"),
            Path::new("/data"),
            |_| false,
        );
        assert_eq!(resolved.rule_name(), "user");

        let with_selection = resolve_with_project_marker(
            Some("  "),
            Some(&saved("/saved/dir")),
            Path::new("/anywhere"),
            Path::new("/data"),
            |_| false,
        );
        assert_eq!(with_selection.rule_name(), "saved");
    }

    #[test]
    fn project_root_is_found_by_walking_up_to_git() {
        let temp = tempfile::tempdir().expect("tempdir");
        let nested = temp.path().join("workspace").join("src");
        std::fs::create_dir_all(&nested).expect("nested dirs");
        std::fs::create_dir_all(temp.path().join("workspace").join(".git")).expect("git dir");

        let resolved = resolve_data_dir(None, None, &nested, Path::new("/data"));
        assert_eq!(
            resolved,
            ResolvedDataDir::Project(temp.path().join("workspace").join(".kernel"))
        );
    }

    #[test]
    fn no_project_falls_back_to_user_data_dir() {
        let resolved = resolve_with_project_marker(
            None,
            None,
            Path::new("/anywhere/nested"),
            Path::new("/home/u/.local/share"),
            |_| false,
        );
        assert_eq!(
            resolved,
            ResolvedDataDir::UserDefault(PathBuf::from("/home/u/.local/share/kmp/default"))
        );
    }

    /// A saved selection is not a project store: it has no repository to be
    /// committed to, so guessing a bundle path for it would put memory
    /// somewhere nobody chose.
    #[test]
    fn a_saved_selection_has_no_conventional_bundle() {
        let selected = ResolvedDataDir::Saved(PathBuf::from("/saved/dir"));
        assert_eq!(project_bundle_path(&selected), None);
        assert_eq!(selected.orphaned_bundle(), None);
    }

    #[test]
    fn an_unopenable_project_store_with_a_bundle_selects_user_memory_and_keeps_the_loss() {
        let project = tempfile::tempdir().expect("project");
        let project_store = project.path().join(".kernel");
        std::fs::create_dir_all(project_store.join("store")).expect("legacy store dir");
        std::fs::write(project_store.join("FORMAT_VERSION"), "1\n").expect("legacy stamp");
        std::fs::write(project_store.join("store/retired-layout.bin"), b"legacy")
            .expect("legacy store");
        let bundle = project.path().join(PROJECT_BUNDLE_PATH);
        std::fs::create_dir_all(bundle.parent().expect("bundle parent")).expect("bundle dir");
        std::fs::write(&bundle, "maintained memory\n").expect("bundle");

        let selected = fallback_from_unopenable_project(
            ResolvedDataDir::Project(project_store.clone()),
            Path::new("/user-data"),
        );

        assert_eq!(selected.path(), Path::new("/user-data/kmp/default"));
        assert_eq!(selected.rule_name(), "user fallback");
        let orphaned = selected.orphaned_bundle().expect("orphaned bundle outcome");
        assert_eq!(orphaned.bundle_path, bundle);
        assert_eq!(orphaned.project_store_path, project_store);
        assert_eq!(orphaned.selected_store_path, selected.path());
        assert!(
            orphaned.reason.contains("format version 1"),
            "{}",
            orphaned.reason
        );
        assert_eq!(project_bundle_path(&selected), None);
    }

    #[test]
    fn an_unopenable_project_without_a_bundle_still_fails_closed_in_place() {
        let project = tempfile::tempdir().expect("project");
        let project_store = project.path().join(".kernel");
        std::fs::create_dir_all(&project_store).expect("legacy store dir");
        std::fs::write(project_store.join("FORMAT_VERSION"), "1\n").expect("legacy stamp");

        let selected = fallback_from_unopenable_project(
            ResolvedDataDir::Project(project_store.clone()),
            Path::new("/user-data"),
        );

        assert_eq!(selected, ResolvedDataDir::Project(project_store));
        assert!(selected.orphaned_bundle().is_none());
    }

    #[test]
    fn user_data_home_keeps_unix_precedence_and_supports_native_windows() {
        let unix = user_data_home_from(|name| match name {
            "XDG_DATA_HOME" => Some(OsString::from("/xdg")),
            "HOME" => Some(OsString::from("/home/user")),
            "LOCALAPPDATA" => Some(OsString::from(r"C:\Users\user\AppData\Local")),
            _ => None,
        });
        assert_eq!(unix, Some(PathBuf::from("/xdg")));

        let windows = user_data_home_from(|name| match name {
            "LOCALAPPDATA" => Some(OsString::from(r"C:\Users\user\AppData\Local")),
            "APPDATA" => Some(OsString::from(r"C:\Users\user\AppData\Roaming")),
            _ => None,
        });
        assert_eq!(windows, Some(PathBuf::from(r"C:\Users\user\AppData\Local")));

        let profile = user_data_home_from(|name| match name {
            "USERPROFILE" => Some(OsString::from(r"C:\Users\user")),
            _ => None,
        });
        assert_eq!(
            profile,
            Some(
                PathBuf::from(r"C:\Users\user")
                    .join("AppData")
                    .join("Local")
            )
        );
    }

    #[test]
    fn project_dir_preparation_writes_self_ignoring_gitignore() {
        let temp = tempfile::tempdir().expect("tempdir");
        let kernel_dir = temp.path().join(".kernel");
        let resolved = ResolvedDataDir::Project(kernel_dir.clone());

        prepare_data_dir(&resolved).expect("prepare");

        let gitignore = std::fs::read_to_string(kernel_dir.join(".gitignore")).expect("gitignore");
        assert_eq!(gitignore, "*\n");
        assert!(kernel_dir.join("logs").is_dir());
    }

    #[test]
    fn explicit_dirs_get_the_same_non_destructive_skeleton() {
        let temp = tempfile::tempdir().expect("tempdir");
        let data_dir = temp.path().join("destination");

        ensure_data_dir_skeleton(&data_dir).expect("prepare explicit destination");
        assert_eq!(
            std::fs::read_to_string(data_dir.join(".gitignore")).expect("gitignore"),
            "*\n"
        );
        assert!(data_dir.join("logs").is_dir());

        std::fs::write(data_dir.join(".gitignore"), "keep-me\n").expect("custom ignore");
        ensure_data_dir_skeleton(&data_dir).expect("prepare again");
        assert_eq!(
            std::fs::read_to_string(data_dir.join(".gitignore")).expect("custom gitignore"),
            "keep-me\n",
            "the skeleton never overwrites an operator-owned ignore file"
        );
    }
}
