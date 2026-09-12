use crate::NodeCardStamp;

/// A reader-authored compact view of one node body, in one language.
///
/// A card is derived, never canonical. It names the exact body version it was
/// written from — revision and content hash — so a later reader can tell
/// whether it still describes what the store holds, without trusting the
/// prose. Nothing here participates in ranking, path selection or proof;
/// deleting every card changes no answer except the cards themselves.
///
/// `card_revision` is the card's own compare-and-set token, separate from the
/// node's revision. Rewriting a card never advances the node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeCard {
    pub node_id: String,
    /// Language tag the card is written in. Part of the identity: a Spanish
    /// and an English card for one node coexist and neither shadows the other.
    pub language: String,
    pub text: String,
    /// Revision of the node body this card was authored from.
    pub source_revision: u64,
    /// The public token of that body version, kept for provenance. It is not
    /// what validity is decided on: it is derived from the event and the node
    /// identity, so a write can change the text without changing it.
    pub source_content_hash: String,
    /// Digest over the exact stored record of that body version. This is the
    /// identity a card is bound to, and the one a reuse check compares.
    pub source_record_digest: String,
    /// Canonical UTF-8 bytes of the body this card stood in for, recorded at
    /// authorship so a reader can see what the card saves without loading it.
    pub source_body_bytes: u64,
    /// Writer name recorded at authorship.
    pub authored_by: String,
    /// RFC3339 instant stamped by the kernel, not by the caller. A historical
    /// read compares against this so a card cannot appear before it existed.
    pub authored_at: String,
    pub card_revision: u64,
}

impl NodeCard {
    /// Everything a read may disclose about this card when it may not show
    /// the prose.
    pub fn stamp(&self) -> NodeCardStamp {
        NodeCardStamp {
            source_revision: self.source_revision,
            source_content_hash: self.source_content_hash.clone(),
            source_record_digest: self.source_record_digest.clone(),
            source_body_bytes: self.source_body_bytes,
            card_revision: self.card_revision,
            authored_by: self.authored_by.clone(),
            authored_at: self.authored_at.clone(),
            text_bytes: self.text.len() as u64,
        }
    }
}
