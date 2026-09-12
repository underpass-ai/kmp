/// What a card read found for one requested node.
///
/// These say nothing about whether the evidence behind the node is
/// sufficient. `complete` and `incomplete` belong to trace proof groups and
/// are deliberately not reused here: a `valid` card over a node whose support
/// is missing is still a `valid` card, and an `absent` card removes no node
/// from any answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeCardStatus {
    /// A card exists and the body it declares is the body the store holds.
    Valid,
    /// A card exists and the body has moved since it was authored. The text
    /// is still returned, with both revisions, so the reader decides.
    Stale,
    /// No card is stored for this node and language.
    Absent,
    /// A card is stored but was authored after the historical cut this read
    /// stands at. The state is named; the text is never returned.
    AfterCut,
}

impl NodeCardStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            NodeCardStatus::Valid => "valid",
            NodeCardStatus::Stale => "stale",
            NodeCardStatus::Absent => "absent",
            NodeCardStatus::AfterCut => "after_cut",
        }
    }
}
