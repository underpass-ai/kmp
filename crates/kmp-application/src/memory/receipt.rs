//! Immutable command detail. Audit records use the event log and never become
//! graph memories, dimensional memberships or semantic relations.

use kmp_domain::{COMMAND_RECEIPT_ENTITY_KIND, IdempotentOutcome, MemoryReceiptRef};
use serde_json::json;

use crate::ApplicationError;
use crate::commands::UpdateContextChange;
use crate::queries::{GetNodeDetailResult, GraphNodeView, NodeDetailView};

use super::{
    InspectMemoryQuery, InspectMemoryResult, MemoryData, MemoryIngestCommand, MemoryIngestOutcome,
};

pub(super) fn receipt_change(
    command: &MemoryIngestCommand,
    canonical: &MemoryData,
    outcome: &MemoryIngestOutcome,
) -> Result<Option<UpdateContextChange>, ApplicationError> {
    let Some(context) = &command.receipt_context else {
        return Ok(None);
    };
    if !context.is_object() {
        return Err(ApplicationError::Validation(
            "receipt_context must be an object".into(),
        ));
    }
    let reference = MemoryReceiptRef::new(&command.about, &command.idempotency_key)
        .map_err(|error| ApplicationError::Validation(error.to_string()))?;
    Ok(Some(UpdateContextChange {
        operation: "RECORD".into(),
        entity_kind: COMMAND_RECEIPT_ENTITY_KIND.into(),
        entity_id: reference.reference(),
        payload_json: json!({
            "version": 1,
            "about": command.about,
            "idempotency_key": command.idempotency_key,
            "canonical_memory": canonical,
            "provenance": command.provenance,
            "writer": context,
            "accepted": outcome.accepted,
            "clocks": outcome.clocks,
            "warnings": outcome.warnings,
            "created_dimensions": outcome.created_dimensions,
            "resembling_labels": outcome.resembling_labels
        })
        .to_string(),
        reason: String::new(),
        scopes: Vec::new(),
    }))
}

pub(super) fn inspect_receipt(
    query: InspectMemoryQuery,
    accepted: IdempotentOutcome,
) -> Result<InspectMemoryResult, ApplicationError> {
    let receipt = accepted
        .receipt
        .filter(|receipt| receipt.reference == query.ref_id)
        .ok_or_else(|| {
            ApplicationError::NotFound(format!("receipt not found: {}", query.ref_id))
        })?;
    let payload: serde_json::Value =
        serde_json::from_str(&receipt.payload_json).map_err(|error| {
            ApplicationError::Ports(kmp_domain::PortError::InvalidState(format!(
                "invalid stored receipt: {error}"
            )))
        })?;
    let detail = json!({
        "receipt": payload,
        "command": {"revision": accepted.revision, "content_hash": accepted.content_hash,
                    "logical_digest": accepted.logical_digest}
    })
    .to_string();
    Ok(InspectMemoryResult {
        detail: GetNodeDetailResult {
            node: GraphNodeView {
                node_id: query.ref_id.clone(),
                node_kind: "write_receipt".into(),
                title: "Accepted write receipt".into(),
                summary:
                    "Immutable accepted-command detail; not the current state of its memories."
                        .into(),
                status: "accepted".into(),
                labels: Vec::new(),
                properties: Default::default(),
            },
            detail: Some(NodeDetailView {
                node_id: query.ref_id,
                detail,
                content_hash: accepted.content_hash,
                revision: accepted.revision,
            }),
        },
        incoming: Vec::new(),
        outgoing: Vec::new(),
        evidence: Vec::new(),
        raw_coordinates: Vec::new(),
        include_details: query.include_details,
        include_raw: query.include_raw,
    })
}
