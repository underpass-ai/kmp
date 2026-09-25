use super::jev_verdict::JevVerdict;

/// Jev's second look at an accepted item with the agent's own why and
/// evidence: the support it found, the type it would choose, the direction
/// it reads, and which of those raised the doubt.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ApplyDoubt {
    pub item_id: String,
    pub support: f64,
    pub best: JevVerdict,
    pub direction: Option<f64>,
    pub reasons: Vec<&'static str>,
}
