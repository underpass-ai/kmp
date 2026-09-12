use crate::{NodeCardStamp, NodeCardStatus};

/// What a compact read may show for one node.
///
/// `text` is `Some` only for [`NodeCardStatus::Valid`]. A stale card, a card
/// authored after a historical cut, and a missing card all arrive here with
/// `text: None` and, when a card exists at all, its [`NodeCardStamp`]. The
/// reader is told the state and given the revision it needs to expand or
/// regenerate; it is never handed prose that describes another body version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeCardPresentation {
    pub status: NodeCardStatus,
    pub text: Option<String>,
    pub stored: Option<NodeCardStamp>,
}

impl NodeCardPresentation {
    pub fn absent() -> Self {
        Self {
            status: NodeCardStatus::Absent,
            text: None,
            stored: None,
        }
    }

    /// Byte length of what this presentation actually renders as prose.
    pub fn text_bytes(&self) -> u64 {
        self.text.as_ref().map_or(0, |text| text.len() as u64)
    }
}
