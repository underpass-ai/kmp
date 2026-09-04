/// Two facts that both still stand and that a declared `contradicts` edge
/// joins. Shown, never resolved: the disagreement is the information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tension {
    ref_id: String,
    other: String,
    /// The scope both stand in, when they share one; empty otherwise.
    scope_id: String,
    why: String,
    evidence: String,
}

impl Tension {
    pub fn new(
        ref_id: impl Into<String>,
        other: impl Into<String>,
        scope_id: impl Into<String>,
        why: impl Into<String>,
        evidence: impl Into<String>,
    ) -> Self {
        Self {
            ref_id: ref_id.into(),
            other: other.into(),
            scope_id: scope_id.into(),
            why: why.into(),
            evidence: evidence.into(),
        }
    }

    pub fn ref_id(&self) -> &str {
        &self.ref_id
    }

    pub fn other(&self) -> &str {
        &self.other
    }

    pub fn scope_id(&self) -> &str {
        &self.scope_id
    }

    pub fn why(&self) -> &str {
        &self.why
    }

    pub fn evidence(&self) -> &str {
        &self.evidence
    }
}
