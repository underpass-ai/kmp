use std::collections::HashMap;

use serde_json::Value;

use crate::lifecycle::domain::host_runtime_status::HostRuntimeStatus;

/// The package that carries KMP's MCP connection into Pi.
const PACKAGE_NAME: &str = "pi-runtime";
/// The package's KMP extension; a filter excluding it disables KMP in Pi.
const KMP_EXTENSION: &str = "src/adapters/inbound/pi/entry/kmp.ts";
/// Source-string prefixes that mark a git or registry spec rather than a
/// local path; anything else is a candidate for `package.json` resolution.
const NON_LOCAL_PREFIXES: [&str; 5] = ["npm:", "git:", "http://", "https://", "git@"];

/// Anti-corruption mapper for Pi's `settings.json`.
///
/// Pi has no native MCP, so there is no MCP registration to read. KMP reaches
/// Pi through the `pi-runtime` package, and the authoritative evidence is
/// that package's entry under `packages`: a source string, or an object with a
/// `source` and optional resource filters. A filter that excludes the KMP
/// extension keeps the package but turns KMP off.
///
/// This mapper is pure: it never touches the filesystem. A local path source
/// may in fact be `pi-runtime` under a different directory name (or vice
/// versa); resolving that requires reading the source's `package.json`,
/// which is the adapter's job. The adapter passes the names it already read,
/// keyed by the exact source string as it appears in `settings.json`, via
/// [`Self::local_sources`] and the `local_package_names` parameter of
/// [`Self::map`].
#[derive(Clone, Copy, Debug, Default)]
pub struct PiRuntimeStatusMapper;

impl PiRuntimeStatusMapper {
    pub fn map(
        settings: Option<&str>,
        local_package_names: &HashMap<String, String>,
    ) -> HostRuntimeStatus {
        let Some(raw) = settings else {
            return HostRuntimeStatus::Missing;
        };
        let packages = match Self::parse_packages(raw) {
            Ok(packages) => packages,
            Err(status) => return status,
        };
        packages
            .iter()
            .find(|entry| {
                Self::source(entry).is_some_and(|source| {
                    Self::names_the_package(source, local_package_names.get(source))
                })
            })
            .map_or(HostRuntimeStatus::Missing, |entry| {
                if Self::excludes_kmp(entry) {
                    HostRuntimeStatus::Disabled
                } else {
                    HostRuntimeStatus::Registered
                }
            })
    }

    /// Package sources in `settings` that are not a git or registry spec, so
    /// the adapter should try to resolve each one's `package.json`. Returned
    /// exactly as they appear in `settings.json`, since [`Self::map`] looks
    /// them back up by that same string.
    pub fn local_sources(settings: Option<&str>) -> Vec<String> {
        let Some(raw) = settings else {
            return Vec::new();
        };
        let Ok(packages) = Self::parse_packages(raw) else {
            return Vec::new();
        };
        packages
            .iter()
            .filter_map(Self::source)
            .filter(|source| Self::is_local_candidate(source))
            .map(String::from)
            .collect()
    }

    fn parse_packages(raw: &str) -> Result<Vec<Value>, HostRuntimeStatus> {
        let body: Value = serde_json::from_str(raw).map_err(|error| {
            HostRuntimeStatus::Failed(format!("Pi settings.json is not valid JSON: {error}"))
        })?;
        Ok(body
            .get("packages")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    fn source(entry: &Value) -> Option<&str> {
        match entry {
            Value::String(source) => Some(source),
            Value::Object(fields) => fields.get("source").and_then(Value::as_str),
            _ => None,
        }
    }

    fn is_local_candidate(source: &str) -> bool {
        let source = source.split_once('#').map_or(source, |(base, _)| base);
        !NON_LOCAL_PREFIXES
            .iter()
            .any(|prefix| source.starts_with(prefix))
    }

    /// Whether a package source is `pi-runtime`. When `resolved_name` (the
    /// adapter's reading of that source's `package.json`, if any) is exactly
    /// `pi-runtime`, this is a match regardless of the source's own text.
    /// Otherwise this falls back to the name-boundary rule: a local path, a
    /// git URL or a registry spec, with or without a trailing `@version`,
    /// and for git the forms Pi's `parseGitUrl` accepts: a `#ref` suffix and
    /// a `.git` repository name.
    fn names_the_package(source: &str, resolved_name: Option<&String>) -> bool {
        if resolved_name.is_some_and(|name| name == PACKAGE_NAME) {
            return true;
        }
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
        PiRuntimeStatusMapper::map(settings, &HashMap::new())
    }

    fn map_with_names(
        settings: Option<&str>,
        local_package_names: &[(&str, &str)],
    ) -> HostRuntimeStatus {
        let names = local_package_names
            .iter()
            .map(|(source, name)| (source.to_string(), name.to_string()))
            .collect();
        PiRuntimeStatusMapper::map(settings, &names)
    }

    #[test]
    fn absent_settings_is_missing() {
        assert_eq!(map(None), HostRuntimeStatus::Missing);
    }

    #[test]
    fn underpass_package_string_is_registered() {
        assert_eq!(
            map(Some(r#"{"packages":["/home/u/Documents/ai/pi-runtime"]}"#)),
            HostRuntimeStatus::Registered
        );
    }

    #[test]
    fn underpass_package_excluding_the_kmp_extension_is_disabled() {
        let settings = r#"{"packages":[{"source":"git:github.com/underpass-ai/pi-runtime@v0.1.0","extensions":["!src/adapters/inbound/pi/entry/kmp.ts"]}]}"#;
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
            map(Some(r#"{"packages":["../../Documents/ai/pi-runtime"]}"#)),
            HostRuntimeStatus::Registered
        );
    }

    #[test]
    fn registry_and_versioned_sources_are_recognized() {
        for source in [
            "npm:pi-runtime",
            "npm:pi-runtime@0.1.0",
            "npm:@underpass-ai/pi-runtime@0.1.0",
            "git:github.com/underpass-ai/pi-runtime/",
            "pi-runtime",
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
            "git:github.com/underpass-ai/pi-runtime.git",
            "https://github.com/underpass-ai/pi-runtime.git",
            "git@github.com:underpass-ai/pi-runtime.git",
            "git:github.com/underpass-ai/pi-runtime#v0.1.0",
            "https://github.com/underpass-ai/pi-runtime.git#main",
            "git:github.com/underpass-ai/pi-runtime.git@v0.1.0",
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
            "https://github.com/other/not-pi-runtime.git#main",
            "git:github.com/underpass-ai/pi-runtime-extras.git",
        ] {
            let settings = format!(r#"{{"packages":["{source}"]}}"#);
            assert_eq!(map(Some(&settings)), HostRuntimeStatus::Missing, "{source}");
        }
    }

    #[test]
    fn a_package_merely_ending_in_the_name_is_not_it() {
        assert_eq!(
            map(Some(r#"{"packages":["npm:not-pi-runtime"]}"#)),
            HostRuntimeStatus::Missing
        );
    }

    #[test]
    fn other_filters_keep_the_kmp_extension_enabled() {
        let settings =
            r#"{"packages":[{"source":"../pi-runtime","extensions":["!src/other.ts"]}]}"#;
        assert_eq!(map(Some(settings)), HostRuntimeStatus::Registered);
    }

    #[test]
    fn settings_without_packages_is_missing() {
        assert_eq!(map(Some(r#"{"theme":"dark"}"#)), HostRuntimeStatus::Missing);
    }

    /// A local dir under any name is `pi-runtime` when the adapter read that
    /// name from its `package.json`.
    #[test]
    fn a_local_dir_with_a_matching_package_json_name_is_registered() {
        let settings = r#"{"packages":["../../Documents/ai/runtime-checkout"]}"#;
        assert_eq!(
            map_with_names(
                Some(settings),
                &[("../../Documents/ai/runtime-checkout", "pi-runtime")]
            ),
            HostRuntimeStatus::Registered
        );
    }

    /// Without a resolved name (no `package.json`, or the adapter could not
    /// read one), the directory basename rule still applies.
    #[test]
    fn a_local_dir_named_pi_runtime_without_a_package_json_is_registered() {
        let settings = r#"{"packages":["../../Documents/ai/pi-runtime"]}"#;
        assert_eq!(
            map_with_names(Some(settings), &[]),
            HostRuntimeStatus::Registered
        );
    }

    /// A `package.json` naming a different package overrides neither a
    /// matching basename (there is none here) nor produces a false match.
    #[test]
    fn a_local_dir_with_a_different_package_json_name_is_missing() {
        let settings = r#"{"packages":["../../Documents/ai/some-other-checkout"]}"#;
        assert_eq!(
            map_with_names(
                Some(settings),
                &[("../../Documents/ai/some-other-checkout", "not-pi-runtime")]
            ),
            HostRuntimeStatus::Missing
        );
    }

    #[test]
    fn local_sources_excludes_registry_and_git_specs() {
        let settings = r#"{"packages":[
            "../../Documents/ai/pi-runtime",
            "npm:pi-runtime",
            "git:github.com/underpass-ai/pi-runtime.git",
            "https://github.com/underpass-ai/pi-runtime.git",
            "git@github.com:underpass-ai/pi-runtime.git",
            {"source":"../other-local-dir"}
        ]}"#;
        assert_eq!(
            PiRuntimeStatusMapper::local_sources(Some(settings)),
            vec![
                "../../Documents/ai/pi-runtime".to_string(),
                "../other-local-dir".to_string()
            ]
        );
    }

    #[test]
    fn local_sources_is_empty_without_settings_or_on_parse_failure() {
        assert_eq!(
            PiRuntimeStatusMapper::local_sources(None),
            Vec::<String>::new()
        );
        assert_eq!(
            PiRuntimeStatusMapper::local_sources(Some("{not json")),
            Vec::<String>::new()
        );
    }
}
