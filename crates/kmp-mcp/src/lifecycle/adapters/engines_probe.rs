use std::path::PathBuf;

use crate::lifecycle::application::use_cases::diagnose_engines::diagnose_engines;
use crate::lifecycle::application::use_cases::survey_engines::SurveyEngines;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::lifecycle::domain::survey_roots::SurveyRoots;

use super::native_installation_catalog::NativeInstallationCatalog;
use super::native_plugin_engine_probe::NativePluginEngineProbe;

/// Every `kmp-mcp` this machine carries, judged against the one running.
pub(crate) fn engines_findings() -> Vec<LifecycleFinding> {
    let roots = native_roots();
    let found =
        SurveyEngines::new(&NativeInstallationCatalog, &NativePluginEngineProbe).execute(&roots);
    diagnose_engines(&found, &ReleaseVersion::current())
}

/// The doctor looks at this machine, so the roots come from this environment.
fn native_roots() -> SurveyRoots {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let data_home = kmp_embedded::user_data_home().unwrap_or_else(|| home.join(".local/share"));
    SurveyRoots {
        home,
        data_home,
        working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        path_entries: std::env::var_os("PATH")
            .map(|value| std::env::split_paths(&value).collect())
            .unwrap_or_default(),
    }
}
