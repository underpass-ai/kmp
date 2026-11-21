use crate::lifecycle::application::use_cases::diagnose_engines::diagnose_engines;
use crate::lifecycle::application::use_cases::survey_engines::SurveyEngines;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;
use crate::lifecycle::domain::release_version::ReleaseVersion;

use super::native_installation_catalog::NativeInstallationCatalog;
use super::native_plugin_engine_probe::NativePluginEngineProbe;

/// Every `kmp-mcp` this machine carries, judged against the one running.
pub(crate) fn engines_findings() -> Vec<LifecycleFinding> {
    let roots = super::native_survey_roots::native_survey_roots();
    let found =
        SurveyEngines::new(&NativeInstallationCatalog, &NativePluginEngineProbe).execute(&roots);
    diagnose_engines(&found, &ReleaseVersion::current())
}
