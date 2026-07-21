use std::fs;
use std::path::{Path, PathBuf};

use rehydration_domain::PortError;

/// Explicit data directory override (ADR-012 rule 1).
pub const DATA_DIR_ENV: &str = "REHYDRATION_MCP_DATA_DIR";

const PROJECT_DIR_NAME: &str = ".kernel";

/// Where the data directory came from — logged at startup so the winning
/// resolution rule is always visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedDataDir {
    /// `REHYDRATION_MCP_DATA_DIR` was set.
    Explicit(PathBuf),
    /// `<project-root>/.kernel/`, project root found by walking up to `.git`.
    Project(PathBuf),
    /// Per-user fallback under the platform data dir.
    UserDefault(PathBuf),
}

impl ResolvedDataDir {
    pub fn path(&self) -> &Path {
        match self {
            Self::Explicit(path) | Self::Project(path) | Self::UserDefault(path) => path,
        }
    }

    pub fn rule_name(&self) -> &'static str {
        match self {
            Self::Explicit(_) => "env",
            Self::Project(_) => "project",
            Self::UserDefault(_) => "user",
        }
    }
}

/// ADR-012 resolution: env override > project `.kernel/` > per-user default.
/// Pure function for testability; `resolve_data_dir_from_env` feeds it from
/// the process environment.
pub fn resolve_data_dir(
    env_override: Option<&str>,
    working_dir: &Path,
    user_data_home: &Path,
) -> ResolvedDataDir {
    resolve_with_project_marker(env_override, working_dir, user_data_home, |candidate| {
        candidate.join(".git").exists()
    })
}

fn resolve_with_project_marker(
    env_override: Option<&str>,
    working_dir: &Path,
    user_data_home: &Path,
    is_project_root: impl Fn(&Path) -> bool,
) -> ResolvedDataDir {
    if let Some(explicit) = env_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return ResolvedDataDir::Explicit(PathBuf::from(explicit));
    }

    let mut current = Some(working_dir);
    while let Some(candidate) = current {
        if is_project_root(candidate) {
            return ResolvedDataDir::Project(candidate.join(PROJECT_DIR_NAME));
        }
        current = candidate.parent();
    }

    ResolvedDataDir::UserDefault(user_data_home.join("rehydration-kernel").join("default"))
}

/// Resolves from the process environment and prepares the directory: creates
/// it and, for project-scoped dirs, drops a self-ignoring `.gitignore` so
/// local memory never enters version control by accident.
pub fn resolve_data_dir_from_env() -> Result<ResolvedDataDir, PortError> {
    let env_override = std::env::var(DATA_DIR_ENV).ok();
    let working_dir = std::env::current_dir().map_err(|error| {
        PortError::Unavailable(format!(
            "embedded kernel could not resolve the working directory: {error}"
        ))
    })?;
    let user_data_home = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .ok()
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join(".local").join("share"))
        })
        .ok_or_else(|| {
            PortError::Unavailable(
                "embedded kernel could not resolve a user data directory \
                 (neither XDG_DATA_HOME nor HOME is set)"
                    .to_string(),
            )
        })?;

    let resolved = resolve_data_dir(env_override.as_deref(), &working_dir, &user_data_home);
    prepare_data_dir(&resolved)?;
    Ok(resolved)
}

fn prepare_data_dir(resolved: &ResolvedDataDir) -> Result<(), PortError> {
    fs::create_dir_all(resolved.path()).map_err(|error| {
        PortError::Unavailable(format!(
            "embedded kernel could not create data dir `{}`: {error}",
            resolved.path().display()
        ))
    })?;
    if let ResolvedDataDir::Project(path) = resolved {
        let gitignore = path.join(".gitignore");
        if !gitignore.exists() {
            fs::write(&gitignore, "*\n").map_err(|error| {
                PortError::Unavailable(format!(
                    "embedded kernel could not write `{}`: {error}",
                    gitignore.display()
                ))
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_override_wins_over_everything() {
        let resolved = resolve_data_dir(
            Some("/explicit/dir"),
            Path::new("/some/project"),
            Path::new("/home/u/.local/share"),
        );
        assert_eq!(
            resolved,
            ResolvedDataDir::Explicit(PathBuf::from("/explicit/dir"))
        );
        assert_eq!(resolved.rule_name(), "env");
    }

    #[test]
    fn blank_env_override_is_ignored() {
        let resolved = resolve_with_project_marker(
            Some("  "),
            Path::new("/anywhere"),
            Path::new("/data"),
            |_| false,
        );
        assert_eq!(resolved.rule_name(), "user");
    }

    #[test]
    fn project_root_is_found_by_walking_up_to_git() {
        let temp = tempfile::tempdir().expect("tempdir");
        let nested = temp.path().join("workspace").join("src");
        std::fs::create_dir_all(&nested).expect("nested dirs");
        std::fs::create_dir_all(temp.path().join("workspace").join(".git")).expect("git dir");

        let resolved = resolve_data_dir(None, &nested, Path::new("/data"));
        assert_eq!(
            resolved,
            ResolvedDataDir::Project(temp.path().join("workspace").join(".kernel"))
        );
    }

    #[test]
    fn no_project_falls_back_to_user_data_dir() {
        let resolved = resolve_with_project_marker(
            None,
            Path::new("/anywhere/nested"),
            Path::new("/home/u/.local/share"),
            |_| false,
        );
        assert_eq!(
            resolved,
            ResolvedDataDir::UserDefault(PathBuf::from(
                "/home/u/.local/share/rehydration-kernel/default"
            ))
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
    }
}
