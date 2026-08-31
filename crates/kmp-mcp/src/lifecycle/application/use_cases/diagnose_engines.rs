use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::found_engine::FoundEngine;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;
use crate::lifecycle::domain::release_version::ReleaseVersion;

/// What the engines on this machine mean, next to the one that is running.
///
/// One line per copy, so the reader sees the whole picture rather than a
/// verdict about it, and a warning when a copy is a different release — with
/// the `PATH` question answered, because a stale engine nothing resolves to
/// is a leftover while a stale engine `PATH` selects is a live hazard.
pub fn diagnose_engines(found: &[FoundEngine], target: &ReleaseVersion) -> Vec<LifecycleFinding> {
    if found.is_empty() {
        return vec![
            LifecycleFinding::new(DiagnosticSeverity::Warn, "no kmp-mcp is on PATH")
                .with_detail("this engine answers, but nothing else on this machine would"),
        ];
    }

    found
        .iter()
        .map(|engine| {
            let path = engine.executable().as_path().display().to_string();
            let selected = if engine.selected_by_path() {
                "PATH selects this one"
            } else {
                "not selected by PATH"
            };
            if engine.matches(target) {
                LifecycleFinding::new(
                    DiagnosticSeverity::Ok,
                    format!("{} — {}", engine.described_version(), selected),
                )
                .with_detail(path)
            } else {
                let finding = LifecycleFinding::new(
                    DiagnosticSeverity::Warn,
                    format!(
                        "{} — not this engine's {target}; {selected}",
                        engine.described_version()
                    ),
                )
                .with_detail(path);
                if engine.selected_by_path() {
                    finding.with_detail(
                        "a bare `kmp-mcp` runs this one; put the current engine's directory \
                         earlier on PATH, or remove it with `kmp-mcp uninstall`",
                    )
                } else {
                    finding.with_detail("remove it with `kmp-mcp uninstall` if you meant to")
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::lifecycle::domain::engine_executable::EngineExecutable;

    fn engine(path: &str, version: Option<&str>, selected: bool) -> FoundEngine {
        FoundEngine::new(
            EngineExecutable::installed_at(PathBuf::from(path)),
            version.map(|raw| ReleaseVersion::parse(raw).expect("version")),
            selected,
        )
    }

    fn target() -> ReleaseVersion {
        ReleaseVersion::parse("0.6.1").expect("target")
    }

    #[test]
    fn every_engine_at_this_release_is_ok() {
        let findings = diagnose_engines(
            &[
                engine("/home/x/.local/bin/kmp-mcp", Some("0.6.1"), true),
                engine("/home/x/.local/share/kmp/bin/kmp-mcp", Some("0.6.1"), false),
            ],
            &target(),
        );

        assert_eq!(findings.len(), 2);
        assert!(
            findings
                .iter()
                .all(|finding| finding.severity() == DiagnosticSeverity::Ok)
        );
        assert!(findings[0].headline().contains("PATH selects this one"));
    }

    #[test]
    fn a_stale_engine_that_path_selects_names_the_hazard_and_the_fix() {
        // The machine of #450: doctor fully green with 0.1.13 one PATH entry
        // away, against format-2 stores.
        let findings = diagnose_engines(
            &[
                engine("/home/x/.cargo/bin/kmp-mcp", Some("0.1.13"), true),
                engine("/home/x/.local/bin/kmp-mcp", Some("0.6.1"), false),
            ],
            &target(),
        );

        assert_eq!(findings[0].severity(), DiagnosticSeverity::Warn);
        assert!(findings[0].headline().contains("0.1.13"), "{findings:?}");
        assert!(findings[0].headline().contains("PATH selects this one"));
        assert!(
            findings[0]
                .detail()
                .iter()
                .any(|line| line.contains("a bare `kmp-mcp` runs this one"))
        );
        assert_eq!(findings[1].severity(), DiagnosticSeverity::Ok);
    }

    #[test]
    fn a_stale_engine_path_does_not_reach_is_a_leftover_not_a_hazard() {
        let findings = diagnose_engines(
            &[engine("/opt/old/kmp-mcp", Some("0.1.13"), false)],
            &target(),
        );

        assert_eq!(findings[0].severity(), DiagnosticSeverity::Warn);
        assert!(findings[0].headline().contains("not selected by PATH"));
        assert!(
            findings[0]
                .detail()
                .iter()
                .any(|line| line.contains("if you meant to"))
        );
    }

    #[test]
    fn an_engine_that_will_not_say_its_release_is_a_warning_not_a_silence() {
        let findings = diagnose_engines(&[engine("/opt/x/kmp-mcp", None, true)], &target());
        assert_eq!(findings[0].severity(), DiagnosticSeverity::Warn);
        assert!(findings[0].headline().contains("unknown version"));
    }

    #[test]
    fn finding_nothing_at_all_is_worth_saying() {
        let findings = diagnose_engines(&[], &target());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity(), DiagnosticSeverity::Warn);
    }
}
