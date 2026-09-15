use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::{ContextEventChange, ContextUpdatedEvent, NodeCard, PortError};

/// Authored presentation state has its own event stream, separate from the
/// source memory's revisions. A baseline preserves an existing pre-event card;
/// it does not claim to recover revisions that were already overwritten.
pub struct NodeCardEvent;

impl NodeCardEvent {
    pub const ROLE: &str = "node_cards";
    pub const ENTITY_KIND: &str = "node_card";

    pub fn record(
        about: &str,
        card: &NodeCard,
        baseline: bool,
        occurred_at: SystemTime,
    ) -> Result<ContextUpdatedEvent, PortError> {
        let payload_json = serde_json::to_string(card)
            .map_err(|error| PortError::InvalidState(format!("encode card event: {error}")))?;
        Ok(ContextUpdatedEvent {
            root_node_id: about.into(),
            role: Self::ROLE.into(),
            revision: 0,
            content_hash: format!("sha256:{:x}", Sha256::digest(payload_json.as_bytes())),
            changes: vec![ContextEventChange {
                operation: if baseline { "BASELINE" } else { "RECORD" }.into(),
                entity_kind: Self::ENTITY_KIND.into(),
                entity_id: card.node_id.clone(),
                payload_json,
                reason: None,
                scopes: Vec::new(),
            }],
            idempotency_key: None,
            logical_digest: None,
            requested_by: Some(card.authored_by.clone()),
            occurred_at,
        })
    }

    /// Decode strictly before replay: an event in this stream must never be
    /// silently ignored or projected into the canonical memory stream.
    pub fn card(event: &ContextUpdatedEvent) -> Result<Option<NodeCard>, PortError> {
        if event.role != Self::ROLE {
            if event
                .changes
                .iter()
                .any(|c| c.entity_kind == Self::ENTITY_KIND)
            {
                return Err(PortError::InvalidState(
                    "card event has the wrong stream".into(),
                ));
            }
            return Ok(None);
        }
        let invalid = || PortError::InvalidState("invalid authored card event".into());
        let [change] = event.changes.as_slice() else {
            return Err(invalid());
        };
        if change.entity_kind != Self::ENTITY_KIND
            || !matches!(change.operation.as_str(), "RECORD" | "BASELINE")
            || event.root_node_id.is_empty()
            || event.content_hash
                != format!(
                    "sha256:{:x}",
                    Sha256::digest(change.payload_json.as_bytes())
                )
        {
            return Err(invalid());
        }
        let card: NodeCard = serde_json::from_str(&change.payload_json).map_err(|_| invalid())?;
        if card.node_id != change.entity_id
            || card.card_revision == 0
            || card.language.is_empty()
            || card.text.is_empty()
            || card.text.len() > crate::node_card_policy::MAX_CARD_BYTES
            || crate::temporal_instant_nanos(&card.authored_at).is_none()
            || event.requested_by.as_deref() != Some(card.authored_by.as_str())
        {
            return Err(invalid());
        }
        Ok(Some(card))
    }
}
