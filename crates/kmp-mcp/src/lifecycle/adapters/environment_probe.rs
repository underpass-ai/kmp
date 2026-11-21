//! What `doctor` and `info` say about the environment switches: each one the
//! environment sets, with its value or why it is refused.

use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;

/// The switches the environment sets. A refused value is a failure: it
/// stops the server and every verb that opens a store, so `doctor` must not
/// call the machine usable while one is set (E18).
pub(crate) fn environment_findings() -> Vec<LifecycleFinding> {
    let mut findings = crate::environment::switches_set()
        .into_iter()
        .map(|(switch, value)| match value {
            Ok(value) => {
                let shown = if switch.is_secret() {
                    "set".to_string()
                } else {
                    value
                };
                let mut finding = LifecycleFinding::new(
                    DiagnosticSeverity::Ok,
                    format!("{}={shown}", switch.name()),
                )
                .with_detail(switch.effect().to_string());
                if switch.is_hook() {
                    finding = finding.with_detail("a test and evaluation hook of this build");
                }
                finding
            }
            Err(refusal) => LifecycleFinding::new(
                DiagnosticSeverity::Fail,
                format!("{} holds a value kmp-mcp refuses", switch.name()),
            )
            .with_detail(refusal.to_string()),
        })
        .collect::<Vec<_>>();
    if findings.is_empty() {
        findings.push(LifecycleFinding::new(
            DiagnosticSeverity::Ok,
            "no KMP switch is set; every one is at its default",
        ));
    }
    findings
}

/// `KMP_*` names set in the environment that this build does not read.
/// Only `doctor` names them: a typo there is the next thing to fix.
pub(crate) fn unread_switch_findings() -> Vec<LifecycleFinding> {
    crate::environment::unread_names()
        .into_iter()
        .map(|name| {
            LifecycleFinding::new(
                DiagnosticSeverity::Warn,
                format!("{name} is set, and this build reads no such switch"),
            )
            .with_detail(
                "a typo, a switch removed at 1.0 (KMP_MCP_ENGINE, KMP_LEXICAL_MAXSCORE), a test \
                 hook a release build does not carry, or another binary's variable",
            )
        })
        .collect()
}
