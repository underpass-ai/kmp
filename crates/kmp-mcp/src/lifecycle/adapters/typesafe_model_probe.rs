use kmp_embedded::ResolvedDataDir;

use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;
use crate::serving::adapters::pinned_model_check;
use crate::serving::model_check::ModelCheck;

/// Whether the TypeSafe model the selected store pins is still served:
/// `doctor` asks TypeSafe's model list (`GET /v1/models`, the key and no
/// store text), so a deprecated pin warns before judgements fail. Nothing
/// without `typesafe.json`; a service it cannot reach is "not checked",
/// never a failure.
pub(crate) fn typesafe_model_finding(resolved: &ResolvedDataDir) -> Option<LifecycleFinding> {
    pinned_model_check(resolved.path()).map(finding)
}

fn finding(check: ModelCheck) -> LifecycleFinding {
    match check {
        ModelCheck::Served { model } => LifecycleFinding::new(
            DiagnosticSeverity::Ok,
            format!("TypeSafe model {model}: served"),
        ),
        ModelCheck::NotServed { model, served } => LifecycleFinding::new(
            DiagnosticSeverity::Warn,
            format!(
                "TypeSafe model {model}: not served any more; pin a served version in typesafe.json"
            ),
        )
        .with_detail(format!("served: {}", served.join(", "))),
        ModelCheck::NotChecked { model, reason } => LifecycleFinding::new(
            DiagnosticSeverity::Ok,
            format!("TypeSafe model {model}: not checked ({reason})"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_pin_the_service_no_longer_lists_warns() {
        let served = finding(ModelCheck::Served {
            model: "jev-1.14.0".into(),
        });
        assert_eq!(served.severity(), DiagnosticSeverity::Ok);
        let gone = finding(ModelCheck::NotServed {
            model: "jev-1.13.0".into(),
            served: vec!["jev-1.14.0".into()],
        });
        assert_eq!(gone.severity(), DiagnosticSeverity::Warn);
        let unknown = finding(ModelCheck::NotChecked {
            model: "jev-1.14.0".into(),
            reason: "TypeSafe unreachable".into(),
        });
        assert_eq!(unknown.severity(), DiagnosticSeverity::Ok);
    }
}
