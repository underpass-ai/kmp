use std::collections::BTreeMap;

/// What Jev chose among the options offered, with the full distribution and
/// its confidence. A suggestion, never a decision.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JevVerdict {
    pub choice: String,
    pub probabilities: BTreeMap<String, f64>,
    pub confidence: f64,
}
