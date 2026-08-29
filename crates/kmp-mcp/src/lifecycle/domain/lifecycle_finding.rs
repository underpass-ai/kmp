use super::diagnostic_severity::DiagnosticSeverity;

/// One host-lifecycle fact ready for a human-facing diagnostic adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleFinding {
    severity: DiagnosticSeverity,
    headline: String,
    detail: Vec<String>,
}

impl LifecycleFinding {
    pub fn new(severity: DiagnosticSeverity, headline: impl Into<String>) -> Self {
        Self {
            severity,
            headline: headline.into(),
            detail: Vec::new(),
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail.push(detail.into());
        self
    }

    pub fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }

    pub fn headline(&self) -> &str {
        &self.headline
    }

    pub fn detail(&self) -> &[String] {
        &self.detail
    }
}
