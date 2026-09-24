use super::jev_verdict::JevVerdict;

/// Jev's second look at an accepted item with the agent's own why and
/// evidence: the support it found and the type it would choose.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ApplyDoubt {
    pub item_id: String,
    pub support: f64,
    pub best: JevVerdict,
}
