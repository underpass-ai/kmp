use serde::{Deserialize, Serialize};

use crate::{ContextUpdatedEvent, MemoryReceiptRef, PortError};

pub const COMMAND_RECEIPT_ENTITY_KIND: &str = "command_receipt";

/// Audit data accepted with a command, indexed by that command's logical key.
/// It is never projected as a memory, label or semantic relation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCommandReceipt {
    pub reference: String,
    pub payload_json: String,
}

impl StoredCommandReceipt {
    pub fn from_event(event: &ContextUpdatedEvent) -> Result<Option<Self>, PortError> {
        let mut receipts = event
            .changes
            .iter()
            .filter(|change| change.entity_kind == COMMAND_RECEIPT_ENTITY_KIND);
        let Some(change) = receipts.next() else {
            return Ok(None);
        };
        if receipts.next().is_some() {
            return Err(PortError::InvalidState(
                "a command can carry only one receipt".into(),
            ));
        }
        let reference = MemoryReceiptRef::parse(&change.entity_id)
            .ok_or_else(|| PortError::InvalidState("invalid command receipt reference".into()))?;
        if change.operation != "RECORD"
            || reference.about() != event.root_node_id
            || Some(reference.idempotency_key()) != event.idempotency_key.as_deref()
        {
            return Err(PortError::InvalidState(
                "receipt must belong to its accepted command".into(),
            ));
        }
        let body: serde_json::Value = serde_json::from_str(&change.payload_json)
            .map_err(|error| PortError::InvalidState(format!("invalid receipt JSON: {error}")))?;
        if !body.is_object() {
            return Err(PortError::InvalidState(
                "receipt detail must be an object".into(),
            ));
        }
        Ok(Some(Self {
            reference: change.entity_id.clone(),
            payload_json: change.payload_json.clone(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ContextEventChange;

    #[test]
    fn audit_detail_must_belong_to_exactly_one_accepted_command() {
        let mut event = ContextUpdatedEvent {
            root_node_id: "project:one".into(),
            role: "memory".into(),
            revision: 1,
            content_hash: "hash".into(),
            idempotency_key: Some("key".into()),
            logical_digest: None,
            requested_by: None,
            occurred_at: std::time::UNIX_EPOCH,
            changes: vec![ContextEventChange {
                operation: "RECORD".into(),
                entity_kind: COMMAND_RECEIPT_ENTITY_KIND.into(),
                entity_id: MemoryReceiptRef::new("project:one", "key")
                    .expect("ref")
                    .reference(),
                payload_json: "{\"proof\":\"source\"}".into(),
                reason: None,
                scopes: Vec::new(),
            }],
        };
        let receipt = StoredCommandReceipt::from_event(&event)
            .expect("valid")
            .expect("receipt");
        assert_eq!(receipt.payload_json, event.changes[0].payload_json);
        for invalid in ["[]", "null", "invalid JSON"] {
            let mut changed = event.clone();
            changed.changes[0].payload_json = invalid.into();
            assert!(StoredCommandReceipt::from_event(&changed).is_err());
        }
        let mut foreign = event.clone();
        foreign.root_node_id = "project:other".into();
        assert!(StoredCommandReceipt::from_event(&foreign).is_err());
        let mut unkeyed = event.clone();
        unkeyed.idempotency_key = None;
        assert!(StoredCommandReceipt::from_event(&unkeyed).is_err());
        event.changes.push(event.changes[0].clone());
        assert!(StoredCommandReceipt::from_event(&event).is_err());
    }
}
