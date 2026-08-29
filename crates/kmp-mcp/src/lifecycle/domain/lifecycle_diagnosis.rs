use super::diagnostic_severity::DiagnosticSeverity;
use super::lifecycle_finding::LifecycleFinding;

/// Complete read-only diagnosis of native KMP host convergence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleDiagnosis {
    findings: Vec<LifecycleFinding>,
}

impl LifecycleDiagnosis {
    pub fn from_findings(findings: Vec<LifecycleFinding>) -> Self {
        Self { findings }
    }

    pub fn findings(&self) -> &[LifecycleFinding] {
        &self.findings
    }

    pub fn has_failure(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.severity() == DiagnosticSeverity::Fail)
    }
}
