use super::SourceKind;

/// Provenance metadata: who produced this fact and when it was observed.
///
/// Attached optionally to nodes and relationships to support auditability
/// and trust assessment. When present, the rendering pipeline surfaces it
/// in the LLM-facing text so models can weigh source quality.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    source_kind: SourceKind,
    source_agent: Option<String>,
    observed_at: Option<String>,
}

impl Provenance {
    pub fn new(source_kind: SourceKind) -> Self {
        Self {
            source_kind,
            source_agent: None,
            observed_at: None,
        }
    }

    pub fn with_source_agent(mut self, agent: impl Into<String>) -> Self {
        self.source_agent = Some(agent.into());
        self
    }

    pub fn with_observed_at(mut self, observed_at: impl Into<String>) -> Self {
        self.observed_at = Some(observed_at.into());
        self
    }

    pub fn source_kind(&self) -> &SourceKind {
        &self.source_kind
    }

    pub fn source_agent(&self) -> Option<&str> {
        self.source_agent.as_deref()
    }

    pub fn observed_at(&self) -> Option<&str> {
        self.observed_at.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_provenance() {
        let p = Provenance::new(SourceKind::Agent);
        assert_eq!(*p.source_kind(), SourceKind::Agent);
        assert!(p.source_agent().is_none());
        assert!(p.observed_at().is_none());
    }

    #[test]
    fn full_provenance() {
        let p = Provenance::new(SourceKind::Human)
            .with_source_agent("operator-1")
            .with_observed_at("2026-03-25T10:00:00Z");
        assert_eq!(*p.source_kind(), SourceKind::Human);
        assert_eq!(p.source_agent(), Some("operator-1"));
        assert_eq!(p.observed_at(), Some("2026-03-25T10:00:00Z"));
    }
}
