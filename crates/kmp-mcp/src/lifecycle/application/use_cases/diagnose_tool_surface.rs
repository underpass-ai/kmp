use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;

pub fn diagnose_tool_surface(observed: &[String], declared: &[String]) -> LifecycleFinding {
    let observed_names = observed
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let declared_names = declared
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let missing = declared_names
        .difference(&observed_names)
        .cloned()
        .collect::<Vec<_>>();
    let unexpected = observed_names
        .difference(&declared_names)
        .cloned()
        .collect::<Vec<_>>();
    if missing.is_empty() && unexpected.is_empty() {
        LifecycleFinding::new(
            DiagnosticSeverity::Ok,
            format!("{} declared tools answered", observed.len()),
        )
        .with_detail(observed.join(" "))
    } else {
        let mut finding = LifecycleFinding::new(
            DiagnosticSeverity::Fail,
            "the MCP tool surface differs from its protocol",
        );
        if !missing.is_empty() {
            finding = finding.with_detail(format!("missing: {}", missing.join(" ")));
        }
        if !unexpected.is_empty() {
            finding = finding.with_detail(format!("unexpected: {}", unexpected.join(" ")));
        }
        finding
    }
}

#[cfg(test)]
mod tests {
    use super::diagnose_tool_surface;
    use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;

    #[test]
    fn the_tool_finding_reports_drift_by_name_not_by_count() {
        let observed = vec!["kmp_wake".to_string(), "kmp_surprise".to_string()];
        let declared = vec!["kmp_wake".to_string(), "kmp_ask".to_string()];
        let finding = diagnose_tool_surface(&observed, &declared);
        assert_eq!(finding.severity(), DiagnosticSeverity::Fail);
        assert!(
            finding
                .detail()
                .iter()
                .any(|line| line == "missing: kmp_ask")
        );
        assert!(
            finding
                .detail()
                .iter()
                .any(|line| line == "unexpected: kmp_surprise")
        );
    }
}
