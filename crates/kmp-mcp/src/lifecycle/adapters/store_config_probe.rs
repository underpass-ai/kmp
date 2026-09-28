use kmp_embedded::ResolvedDataDir;

use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;
use crate::serving::adapters::inspect_store_config;
use crate::serving::store_config_entry::StoreConfigEntry;
use crate::serving::store_config_state::StoreConfigState;

/// Which of the selected store's optional files a session opening it would
/// apply: one finding per present file, `on` with the settings that took
/// effect or why not, then one line naming the files that are absent.
///
/// The verdicts are the server's own store config acknowledgement, read the
/// same way a session reads them at start; nothing here parses a file
/// (#887). A file present and not applied is a warning: the operator wrote
/// it for a reason and it is doing nothing.
pub(crate) fn store_config_findings(resolved: &ResolvedDataDir) -> Vec<LifecycleFinding> {
    findings(&inspect_store_config(resolved.path()))
}

fn findings(entries: &[StoreConfigEntry]) -> Vec<LifecycleFinding> {
    let mut findings = Vec::new();
    let mut absent = Vec::new();
    for entry in entries {
        match &entry.state {
            // A file the operator named elsewhere and turned off (the
            // cassette, a bridge path) has no place beside the store.
            StoreConfigState::Absent if entry.path.is_some() => absent.push(entry.name.as_str()),
            StoreConfigState::Absent => {}
            StoreConfigState::On => findings.push(on(entry, DiagnosticSeverity::Ok, "on")),
            StoreConfigState::OnWithWarning(reason) => findings.push(
                on(entry, DiagnosticSeverity::Warn, "on, with its defaults").with_detail(reason),
            ),
            StoreConfigState::Rejected(reason) => findings.push(LifecycleFinding::new(
                DiagnosticSeverity::Warn,
                format!("{}: rejected: {reason}", entry.name),
            )),
        }
    }
    if findings.is_empty() {
        findings.push(LifecycleFinding::new(
            DiagnosticSeverity::Ok,
            "no optional files beside the store: every setting is its default",
        ));
    }
    if !absent.is_empty() {
        findings.push(LifecycleFinding::new(
            DiagnosticSeverity::Ok,
            format!("off (absent): {}", absent.join(", ")),
        ));
    }
    findings
}

fn on(entry: &StoreConfigEntry, severity: DiagnosticSeverity, state: &str) -> LifecycleFinding {
    let mut headline = format!("{}: {state}", entry.name);
    for setting in &entry.effective {
        headline.push_str(" · ");
        headline.push_str(setting);
    }
    LifecycleFinding::new(severity, headline)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn entry(name: &str, state: StoreConfigState, effective: &[&str]) -> StoreConfigEntry {
        StoreConfigEntry {
            name: name.to_string(),
            path: Some(PathBuf::from("/store").join(name)),
            state,
            effective: effective
                .iter()
                .map(|setting| setting.to_string())
                .collect(),
        }
    }

    fn headlines(findings: &[LifecycleFinding]) -> Vec<&str> {
        findings.iter().map(LifecycleFinding::headline).collect()
    }

    #[test]
    fn every_known_file_is_on_rejected_or_named_absent() {
        let findings = findings(&[
            entry("typesafe.json", StoreConfigState::On, &[]),
            entry(
                "rerank.json",
                StoreConfigState::On,
                &["pool 40", "margin 0.3"],
            ),
            entry(
                "ask-judge.json",
                StoreConfigState::Rejected("invalid ask judge configuration".into()),
                &[],
            ),
            entry("curate.json", StoreConfigState::Absent, &[]),
            entry("wake-focus.json", StoreConfigState::Absent, &[]),
            StoreConfigEntry {
                name: "typesafe-cassette".into(),
                path: None,
                state: StoreConfigState::Absent,
                effective: Vec::new(),
            },
        ]);
        assert_eq!(
            headlines(&findings),
            [
                "typesafe.json: on",
                "rerank.json: on · pool 40 · margin 0.3",
                "ask-judge.json: rejected: invalid ask judge configuration",
                "off (absent): curate.json, wake-focus.json",
            ]
        );
        assert_eq!(findings[2].severity(), DiagnosticSeverity::Warn);
        assert_eq!(findings[1].severity(), DiagnosticSeverity::Ok);
    }

    #[test]
    fn a_file_read_with_its_defaults_warns_and_says_why() {
        let findings = findings(&[entry(
            "write-relations.json",
            StoreConfigState::OnWithWarning("unknown lifecycle `jevv`".into()),
            &["lifecycle off"],
        )]);
        assert_eq!(findings[0].severity(), DiagnosticSeverity::Warn);
        assert_eq!(
            findings[0].headline(),
            "write-relations.json: on, with its defaults · lifecycle off"
        );
        assert_eq!(findings[0].detail(), ["unknown lifecycle `jevv`"]);
    }

    #[test]
    fn a_bare_store_says_every_setting_is_its_default() {
        let findings = findings(&[entry("rerank.json", StoreConfigState::Absent, &[])]);
        assert_eq!(
            headlines(&findings),
            [
                "no optional files beside the store: every setting is its default",
                "off (absent): rerank.json",
            ]
        );
    }
}
