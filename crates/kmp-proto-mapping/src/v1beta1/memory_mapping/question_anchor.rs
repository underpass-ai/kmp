use super::anchor_strength::AnchorStrength;

/// One identifier a question names, as a search term the ranker indexes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct QuestionAnchor {
    /// As the question wrote it, without edge punctuation: what `missing`
    /// reports in the reader's words.
    pub(super) written: String,
    /// The whole search term it names (`c6.24`, `188`, `0.7.0`).
    pub(super) term: String,
    pub(super) strength: AnchorStrength,
    /// Named after `excluding`, `without`, `sin` in its clause: never
    /// required, never the principal anchor.
    pub(super) negated: bool,
}

impl QuestionAnchor {
    /// Whether the question requires this anchor to be found.
    pub(super) fn is_required(&self) -> bool {
        self.strength == AnchorStrength::Hard && !self.negated
    }
}
