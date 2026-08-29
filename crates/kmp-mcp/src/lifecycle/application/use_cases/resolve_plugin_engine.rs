use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::plugin_engine_candidate::PluginEngineCandidate;
use crate::lifecycle::domain::plugin_engine_resolution::PluginEngineResolution;
use crate::lifecycle::domain::plugin_engine_role::PluginEngineRole;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::lifecycle::ports::plugin_engine_probe::PluginEngineProbe;

pub struct ResolvePluginEngine<'a> {
    probes: &'a dyn PluginEngineProbe,
}

impl<'a> ResolvePluginEngine<'a> {
    pub fn new(probes: &'a dyn PluginEngineProbe) -> Self {
        Self { probes }
    }

    pub fn execute(
        &self,
        expected: &ReleaseVersion,
        candidates: &[PluginEngineCandidate],
    ) -> Result<PluginEngineResolution, LifecycleError> {
        let bundled = self.observed(candidates, PluginEngineRole::Bundled)?;
        if let Some((candidate, version)) = bundled.as_ref()
            && expected.represents_same_release(version)
        {
            return Ok(PluginEngineResolution::exact(
                candidate.executable().clone(),
                version.clone(),
            ));
        }
        let path = self.observed(candidates, PluginEngineRole::Path)?;
        if let Some((candidate, version)) = path.as_ref()
            && expected.represents_same_release(version)
        {
            return match bundled.as_ref() {
                Some((_, stale)) => Ok(PluginEngineResolution::replacing_stale_cache(
                    candidate.executable().clone(),
                    version.clone(),
                    stale,
                    expected,
                )),
                None => Ok(PluginEngineResolution::exact(
                    candidate.executable().clone(),
                    version.clone(),
                )),
            };
        }
        let observed = [bundled, path]
            .into_iter()
            .flatten()
            .map(|(candidate, version)| format!("{}={version}", candidate.role()))
            .collect::<Vec<_>>()
            .join(", ");
        Err(LifecycleError::HostVersionMismatch(format!(
            "no engine matches plugin {expected}; found {}. Run kmp setup.",
            if observed.is_empty() {
                "no executable candidates"
            } else {
                observed.as_str()
            }
        )))
    }

    fn observed<'b>(
        &self,
        candidates: &'b [PluginEngineCandidate],
        role: PluginEngineRole,
    ) -> Result<Option<(&'b PluginEngineCandidate, ReleaseVersion)>, LifecycleError> {
        let Some(candidate) = candidates.iter().find(|candidate| candidate.role() == role) else {
            return Ok(None);
        };
        self.probes
            .version(candidate.executable())
            .map(|version| version.map(|version| (candidate, version)))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::*;
    use crate::lifecycle::domain::engine_executable::EngineExecutable;

    struct FakeProbe {
        versions: BTreeMap<PathBuf, ReleaseVersion>,
    }

    impl PluginEngineProbe for FakeProbe {
        fn version(
            &self,
            executable: &EngineExecutable,
        ) -> Result<Option<ReleaseVersion>, LifecycleError> {
            Ok(self.versions.get(executable.as_path()).cloned())
        }
    }

    fn candidate(path: &str, role: PluginEngineRole) -> PluginEngineCandidate {
        PluginEngineCandidate::new(EngineExecutable::installed_at(PathBuf::from(path)), role)
    }

    #[test]
    fn matching_path_replaces_a_stale_plugin_cache() {
        let expected = ReleaseVersion::parse("0.5.2+host").expect("expected version");
        let probe = FakeProbe {
            versions: BTreeMap::from([
                (
                    PathBuf::from("/cache/kmp-mcp"),
                    ReleaseVersion::parse("0.4.2").expect("stale version"),
                ),
                (
                    PathBuf::from("/path/kmp-mcp"),
                    ReleaseVersion::parse("0.5.2").expect("current version"),
                ),
            ]),
        };
        let resolution = ResolvePluginEngine::new(&probe)
            .execute(
                &expected,
                &[
                    candidate("/path/kmp-mcp", PluginEngineRole::Path),
                    candidate("/cache/kmp-mcp", PluginEngineRole::Bundled),
                ],
            )
            .expect("matching PATH engine");
        assert_eq!(
            resolution.selected().as_path(),
            std::path::Path::new("/path/kmp-mcp")
        );
        assert!(
            resolution
                .warning()
                .is_some_and(|warning| warning.contains("cache engine 0.4.2"))
        );
    }

    #[test]
    fn resolver_refuses_every_mismatched_candidate() {
        let expected = ReleaseVersion::parse("0.5.2").expect("expected version");
        let probe = FakeProbe {
            versions: BTreeMap::from([(
                PathBuf::from("/path/kmp-mcp"),
                ReleaseVersion::parse("0.4.2").expect("stale version"),
            )]),
        };
        let error = ResolvePluginEngine::new(&probe)
            .execute(
                &expected,
                &[candidate("/path/kmp-mcp", PluginEngineRole::Path)],
            )
            .expect_err("mismatched candidate must fail");
        assert!(error.to_string().contains("no engine matches plugin 0.5.2"));
    }
}
