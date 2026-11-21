use std::path::PathBuf;

use crate::lifecycle::domain::survey_roots::SurveyRoots;

/// The roots of this machine, read from this process's environment: what
/// `doctor` and `uninstall` both survey, so they look in the same places.
pub fn native_survey_roots() -> SurveyRoots {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let data_home =
        kmp_embedded::user_data_home().unwrap_or_else(|| home.join(".local").join("share"));
    SurveyRoots {
        codex_home: codex_home_from_environment(),
        home,
        data_home,
        working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        path_entries: std::env::var_os("PATH")
            .map(|value| std::env::split_paths(&value).collect())
            .unwrap_or_default(),
    }
}

/// Where Codex keeps its configuration and plugin cache: `CODEX_HOME`, else
/// `~/.codex`, else `.codex` under the working directory.
pub fn codex_home_from_environment() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".codex"))
        })
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".codex")
        })
}
