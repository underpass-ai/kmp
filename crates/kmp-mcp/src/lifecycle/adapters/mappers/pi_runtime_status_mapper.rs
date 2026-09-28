use serde_json::Value;

use crate::lifecycle::domain::host_runtime_status::HostRuntimeStatus;

/// The package that carries KMP's MCP connection into Pi.
const PACKAGE_NAME: &str = "underpass-pi";
/// The package's KMP extension; a filter excluding it disables KMP in Pi.
const KMP_EXTENSION: &str = "src/adapters/inbound/pi/entry/kmp.ts";

/// Anti-corruption mapper for Pi's `settings.json`.
///
/// Pi has no native MCP, so there is no MCP registration to read. KMP reaches
/// Pi through the `underpass-pi` package, and the authoritative evidence is
/// that package's entry under `packages`: a source string, or an object with a
/// `source` and optional resource filters. A filter that excludes the KMP
/// extension keeps the package but turns KMP off.
#[derive(Clone, Copy, Debug, Default)]
pub struct PiRuntimeStatusMapper;

impl PiRuntimeStatusMapper {
    pub fn map(settings: Option<&str>) -> HostRuntimeStatus {
        let Some(raw) = settings else {
            return HostRuntimeStatus::Missing;
        };
        let body: Value = match serde_json::from_str(raw) {
            Ok(body) => body,
            Err(error) => {
                return HostRuntimeStatus::Failed(format!(
                    "Pi settings.json is not valid JSON: {error}"
                ));
            }
        };
        let packages = body
            .get("packages")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        packages
            .iter()
            .find(|entry| Self::source(entry).is_some_and(Self::names_the_package))
            .map_or(HostRuntimeStatus::Missing, |entry| {
                if Self::excludes_kmp(entry) {
                    HostRuntimeStatus::Disabled
                } else {
                    HostRuntimeStatus::Registered
                }
            })
    }

    fn source(entry: &Value) -> Option<&str> {
        match entry {
            Value::String(source) => Some(source),
            Value::Object(fields) => fields.get("source").and_then(Value::as_str),
            _ => None,
        }
    }

    /// Whether a package source is `underpass-pi`: a local path, a git URL or
    /// a registry spec, with or without a trailing `@version`, and for git
    /// the forms Pi's `parseGitUrl` accepts: a `#ref` suffix and a `.git`
    /// repository name.
    fn names_the_package(source: &str) -> bool {
        let source = source.split_once('#').map_or(source, |(base, _)| base);
        let source = source.trim_end_matches(['/', '\\']);
        let unversioned = match source.rsplit_once('@') {
            Some((base, version))
                if !base.is_empty() && !base.ends_with([':', '/']) && !version.contains('/') =>
            {
                base
            }
            _ => source,
        };
        let unversioned = unversioned.strip_suffix(".git").unwrap_or(unversioned);
        unversioned
            .strip_suffix(PACKAGE_NAME)
            .is_some_and(|prefix| prefix.is_empty() || prefix.ends_with(['/', '\\', ':']))
    }

    fn excludes_kmp(entry: &Value) -> bool {
        let excluded = format!("!{KMP_EXTENSION}");
        entry
            .get("extensions")
            .and_then(Value::as_array)
            .is_some_and(|filters| {
                filters
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|filter| filter == excluded)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(settings: Option<&str>) -> HostRuntimeStatus {
        PiRuntimeStatusMapper::map(settings)
    }

    #[test]
    fn absent_settings_is_missing() {
        assert_eq!(map(None), HostRuntimeStatus::Missing);
    }

    #[test]
    fn underpass_package_string_is_registered() {
        assert_eq!(
            map(Some(
                r#"{"packages":["/home/u/Documents/ai/underpass-pi"]}"#
            )),
            HostRuntimeStatus::Registered
        );
    }

    #[test]
    fn underpass_package_excluding_the_kmp_extension_is_disabled() {
        let settings = r#"{"packages":[{"source":"git:github.com/underpass-ai/underpass-pi@v0.1.0","extensions":["!src/adapters/inbound/pi/entry/kmp.ts"]}]}"#;
        assert_eq!(map(Some(settings)), HostRuntimeStatus::Disabled);
    }

    #[test]
    fn unreadable_settings_is_failed() {
        assert!(matches!(
            map(Some("{not json")),
            HostRuntimeStatus::Failed(detail) if detail.contains("settings.json")
        ));
    }

    #[test]
    fn foreign_packages_only_is_missing() {
        assert_eq!(
            map(Some(r#"{"packages":["npm:pi-mcp-adapter"]}"#)),
            HostRuntimeStatus::Missing
        );
    }

    /// The shape `pi install <dir>` writes: a path relative to the agent home.
    #[test]
    fn a_relative_local_path_is_registered() {
        assert_eq!(
            map(Some(r#"{"packages":["../../Documents/ai/underpass-pi"]}"#)),
            HostRuntimeStatus::Registered
        );
    }

    #[test]
    fn registry_and_versioned_sources_are_recognized() {
        for source in [
            "npm:underpass-pi",
            "npm:underpass-pi@0.1.0",
            "npm:@underpass-ai/underpass-pi@0.1.0",
            "git:github.com/underpass-ai/underpass-pi/",
            "underpass-pi",
        ] {
            let settings = format!(r#"{{"packages":["{source}"]}}"#);
            assert_eq!(
                map(Some(&settings)),
                HostRuntimeStatus::Registered,
                "{source}"
            );
        }
    }

    #[test]
    fn git_sources_in_the_forms_pi_parses_are_recognized() {
        for source in [
            "git:github.com/underpass-ai/underpass-pi.git",
            "https://github.com/underpass-ai/underpass-pi.git",
            "git@github.com:underpass-ai/underpass-pi.git",
            "git:github.com/underpass-ai/underpass-pi#v0.1.0",
            "https://github.com/underpass-ai/underpass-pi.git#main",
            "git:github.com/underpass-ai/underpass-pi.git@v0.1.0",
        ] {
            let settings = format!(r#"{{"packages":["{source}"]}}"#);
            assert_eq!(
                map(Some(&settings)),
                HostRuntimeStatus::Registered,
                "{source}"
            );
        }
    }

    #[test]
    fn a_ref_or_git_suffix_does_not_widen_the_name() {
        for source in [
            "https://github.com/other/not-underpass-pi.git#main",
            "git:github.com/underpass-ai/underpass-pi-extras.git",
        ] {
            let settings = format!(r#"{{"packages":["{source}"]}}"#);
            assert_eq!(map(Some(&settings)), HostRuntimeStatus::Missing, "{source}");
        }
    }

    #[test]
    fn a_package_merely_ending_in_the_name_is_not_it() {
        assert_eq!(
            map(Some(r#"{"packages":["npm:not-underpass-pi"]}"#)),
            HostRuntimeStatus::Missing
        );
    }

    #[test]
    fn other_filters_keep_the_kmp_extension_enabled() {
        let settings =
            r#"{"packages":[{"source":"../underpass-pi","extensions":["!src/other.ts"]}]}"#;
        assert_eq!(map(Some(settings)), HostRuntimeStatus::Registered);
    }

    #[test]
    fn settings_without_packages_is_missing() {
        assert_eq!(map(Some(r#"{"theme":"dark"}"#)), HostRuntimeStatus::Missing);
    }
}
