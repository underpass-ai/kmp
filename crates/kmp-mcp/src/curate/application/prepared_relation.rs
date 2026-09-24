use crate::curate::domain::pair_origin::PairOrigin;

/// An accepted item resolved against its review, ready for the writer:
/// direction, type, the agent's text, and the relate proposal a link across
/// abouts must carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedRelation {
    pub item_id: String,
    pub from: String,
    pub to: String,
    pub rel: String,
    pub why: String,
    pub evidence: String,
    pub confidence: Option<String>,
    pub proposal: Option<Vec<String>>,
    pub origin: PairOrigin,
}
