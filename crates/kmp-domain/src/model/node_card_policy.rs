//! The rules of a reader-authored card, as pure functions over a snapshot.
//!
//! Admission and presentation live here rather than in the adapter because
//! they are the contract: which body version a card may claim, and when a
//! stored card may be shown. The adapter owns the transaction that supplies
//! the snapshot and persists the result; it owns none of the decisions.
//!
//! Both functions work from a [`NodeBodyDescriptor`], never from body text.
//! That is what lets a valid card be reused without reading a single canonical
//! record, and it is why validity is decided on the record digest rather than
//! on the public content hash, which a write can leave unchanged.

use crate::{
    AuthorNodeCard, NodeBodyDescriptor, NodeCard, NodeCardExpectation, NodeCardPresentation,
    NodeCardRejection, NodeCardStatus, NodeProjection, temporal_instant_nanos,
};

/// The about a node declares it belongs to.
const MEMORY_ABOUT: &str = "memory_about";
/// The label every memory entry carries.
const ENTRY_LABEL: &str = "entry";
/// The kinds a stored evidence source is written under.
const EVIDENCE_KINDS: [&str; 2] = ["memory_evidence", "evidence"];

/// A card record may not itself become an unbounded read. A compact response
/// that loads a megabyte of prose per node has replaced one problem with the
/// same problem.
pub const MAX_CARD_BYTES: usize = 4_096;

/// Whether a stored card stands for exactly the body this descriptor names.
///
/// Revision alone is not enough and the public hash alone is not enough. The
/// record digest is computed over the stored bytes, so it moves whenever the
/// text does.
pub fn describes(card: &NodeCard, descriptor: &NodeBodyDescriptor) -> bool {
    card.source_revision == descriptor.revision
        && card.source_record_digest == descriptor.record_digest
}

/// Decides whether one card write may land, against the node, its body
/// descriptor and the card already stored — all read inside the writer's own
/// transaction.
///
/// The returned card is what the adapter must persist verbatim, including the
/// `card_revision` this function chose. Nothing else may invent one.
pub fn admit(
    command: &AuthorNodeCard,
    node: Option<&NodeProjection>,
    descriptor: Option<&NodeBodyDescriptor>,
    existing: Option<&NodeCard>,
    cut_nanos: Option<i128>,
) -> Result<NodeCard, NodeCardRejection> {
    // The same admission the proof materializer uses: an entry of this about,
    // or one of its own evidence sources. A shared source is usually the
    // largest record on a path, so refusing to card it would leave exactly the
    // body that costs most without a compact form.
    let condensable = node.is_some_and(|node| {
        node.properties.get(MEMORY_ABOUT).map(String::as_str) == Some(command.about.as_str())
            && (node.labels.iter().any(|label| label == ENTRY_LABEL)
                || EVIDENCE_KINDS.contains(&node.node_kind.as_str()))
    });
    if !condensable {
        return Err(NodeCardRejection::UnknownRef {
            node_id: command.node_id.clone(),
            about: command.about.clone(),
        });
    }
    let Some(descriptor) = descriptor else {
        return Err(NodeCardRejection::NoBody {
            node_id: command.node_id.clone(),
        });
    };
    if descriptor.revision != command.source_revision
        || descriptor.record_digest != command.source_record_digest
    {
        return Err(NodeCardRejection::SourceMoved {
            declared_revision: command.source_revision,
            actual_revision: descriptor.revision,
            declared_record_digest: command.source_record_digest.clone(),
            actual_record_digest: descriptor.record_digest.clone(),
        });
    }
    // The cut gates authorship too. A card stamped into the past would be
    // shown by a historical read that could not have seen it, and a matching
    // source version does not make that stamp true.
    if let Some(cut) = cut_nanos
        && temporal_instant_nanos(&command.authored_at).is_none_or(|at| at > cut)
    {
        return Err(NodeCardRejection::AuthoredAfterCut {
            authored_at: command.authored_at.clone(),
        });
    }
    if command.text.trim().is_empty() {
        return Err(NodeCardRejection::EmptyCard);
    }
    if command.text.len() > MAX_CARD_BYTES {
        return Err(NodeCardRejection::CardTooLarge {
            card_bytes: command.text.len(),
            limit: MAX_CARD_BYTES,
        });
    }
    if command.text.len() as u64 >= descriptor.body_bytes {
        return Err(NodeCardRejection::NotCompact {
            card_bytes: command.text.len(),
            body_bytes: descriptor.body_bytes,
        });
    }
    match (command.expect, existing) {
        (NodeCardExpectation::Absent, Some(stored)) => {
            return Err(NodeCardRejection::CardAlreadyExists {
                actual_card_revision: stored.card_revision,
            });
        }
        (NodeCardExpectation::CardRevision(declared), None) => {
            return Err(NodeCardRejection::CardAbsent {
                declared_card_revision: declared,
            });
        }
        (NodeCardExpectation::CardRevision(declared), Some(stored))
            if declared != stored.card_revision =>
        {
            return Err(NodeCardRejection::CardMoved {
                declared_card_revision: declared,
                actual_card_revision: stored.card_revision,
            });
        }
        _ => {}
    }
    Ok(NodeCard {
        node_id: command.node_id.clone(),
        language: command.language.clone(),
        text: command.text.clone(),
        source_revision: descriptor.revision,
        source_content_hash: descriptor.content_hash.clone(),
        source_record_digest: descriptor.record_digest.clone(),
        source_body_bytes: descriptor.body_bytes,
        authored_by: command.authored_by.clone(),
        authored_at: command.authored_at.clone(),
        card_revision: existing.map_or(1, |stored| stored.card_revision + 1),
    })
}

/// Decides what a compact read may show for one node.
///
/// Two independent gates, both required. The primary one is the **selected
/// source version**: a card may stand for a body only when it describes the
/// descriptor this read selected. The second applies only under a historical
/// cut: a card authored after the instant the reader stands at did not exist
/// then, and its prose must not appear in that answer. Neither gate implies
/// the other — a stale card passes the date test, and a card written a moment
/// ago passes the version test.
///
/// An `authored_at` this kernel cannot parse fails the cut, never passes it.
pub fn presentation(
    card: Option<&NodeCard>,
    descriptor: Option<&NodeBodyDescriptor>,
    cut_nanos: Option<i128>,
) -> NodeCardPresentation {
    let Some(card) = card else {
        return NodeCardPresentation::absent();
    };
    let stamp = Some(card.stamp());
    if let Some(cut) = cut_nanos {
        let authored = temporal_instant_nanos(&card.authored_at);
        if authored.is_none_or(|authored| authored > cut) {
            return NodeCardPresentation {
                status: NodeCardStatus::AfterCut,
                text: None,
                stored: stamp,
            };
        }
    }
    if descriptor.is_some_and(|descriptor| describes(card, descriptor)) {
        NodeCardPresentation {
            status: NodeCardStatus::Valid,
            text: Some(card.text.clone()),
            stored: stamp,
        }
    } else {
        NodeCardPresentation {
            status: NodeCardStatus::Stale,
            text: None,
            stored: stamp,
        }
    }
}

#[cfg(test)]
#[path = "node_card_policy_tests.rs"]
mod tests;
