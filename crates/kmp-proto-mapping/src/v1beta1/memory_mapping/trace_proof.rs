use super::bundle_views::{
    memory_relation_from_bundle_relationship, persisted_memory_metadata, persisted_memory_source,
    persisted_support_clocks, proto_coordinate_from_domain,
};
use kmp_domain::{
    BundleRelationship, NodeCardPresentation, TraceBodyState, TraceExpansionRefusal,
    TraceProofResult,
};
use kmp_proto::v1beta1::{
    InspectedObject, NodeBodyDescriptor, NodeCardView, TraceBodyDelivery, TraceCompactSummary,
    TraceExpansionRefusal as ProtoRefusal, TraceProofObject, TraceProofStatus, TraceResponse,
};

pub(super) fn project(response: &mut TraceResponse, proof: TraceProofResult) {
    if let Some(refusal) = proof.refusal {
        // Nothing of this snapshot is projected: no object, no support, no
        // canonical text and no card.
        let warning = match &refusal {
            TraceExpansionRefusal::SelectionChanged { .. } =>
                "read_selection_changed: the selected proof table moved since the manifest this \
                 call declared. No canonical or card text was returned and nothing may be joined \
                 by ref across the two. Repeat the original read without proof_refs or \
                 expect_selection to obtain the current manifest.",
            TraceExpansionRefusal::UnknownRefs(_) =>
                "unknown_expansion_refs: search.proof_refs named refs outside the selected proof \
                 table. The whole batch was refused rather than widening the selection or \
                 delivering only part of it; expand refs this selection actually contains.",
        };
        response.expansion_refusal = Some(match refusal {
            TraceExpansionRefusal::SelectionChanged { expected, actual } => ProtoRefusal {
                code: "read_selection_changed".into(),
                expected,
                actual,
                unknown_refs: vec![],
            },
            TraceExpansionRefusal::UnknownRefs(unknown_refs) => ProtoRefusal {
                code: "unknown_expansion_refs".into(),
                expected: String::new(),
                actual: String::new(),
                unknown_refs,
            },
        });
        response.proof = Some(TraceProofStatus {
            manifest_id: proof.manifest_id.unwrap_or_default(),
            stop_reason: proof
                .stop
                .map_or("sources_enumerated", |stop| stop.as_str())
                .into(),
            ..TraceProofStatus::default()
        });
        response.warnings.push(warning.into());
        return;
    }
    let mut gaps = std::collections::BTreeMap::<String, Vec<String>>::new();
    for (refs, reason) in [
        (&proof.missing_refs, "missing_ref"),
        (&proof.missing_bodies, "missing_body"),
        (&proof.incomplete_entries, "incomplete_entry"),
        (&proof.clock_unknown_entries, "clock_unknown"),
    ] {
        for reference in refs {
            gaps.entry(reference.clone())
                .or_default()
                .push(reason.into());
        }
    }
    // A body the store holds and this response withheld is its own reason,
    // per ref, never folded into missing_body. Without this a deferred or
    // carded shared source appears only through the entries it supports.
    for object in &proof.objects {
        let reason = match object.body_state {
            TraceBodyState::DeferredBudget => "deferred_budget",
            TraceBodyState::NotRequested => "not_requested",
            TraceBodyState::Compact => "compact",
            TraceBodyState::Loaded | TraceBodyState::Missing => continue,
        };
        gaps.entry(object.node.node_id.clone())
            .or_default()
            .push(reason.into());
    }
    response.gaps = gaps
        .into_iter()
        .map(|(r#ref, reasons)| kmp_proto::v1beta1::TraceProofGap { r#ref, reasons })
        .collect();
    response.objects = proof
        .objects
        .into_iter()
        .map(|o| {
            let state = o.body_state;
            let required = o.required_record_bytes().unwrap_or_default();
            let stored_body = o.body.is_some() || o.descriptor.is_some();
            let descriptor = o.descriptor;
            let card = o.card;
            TraceProofObject {
            status: o.node.status,
            coordinates: o
                .coordinates
                .iter()
                .map(proto_coordinate_from_domain)
                .collect(),
            source_time: super::scalars::timestamp_from_sort_or_rfc3339(
                o.node.properties.get("payload_time").map(String::as_str),
            ),
            support_clocks: persisted_support_clocks(&o.node.properties),
            object: Some(InspectedObject {
                r#ref: o.node.node_id,
                kind: o.node.node_kind,
                text: o.body.as_ref().map_or(o.node.summary, |b| b.detail.clone()),
                metadata: persisted_memory_metadata(&o.node.properties),
                source: persisted_memory_source(&o.node.properties)
                    .unwrap_or_default()
                    .into(),
            }),
            // Presence in the store, which a descriptor also answers when
            // the body was withheld. Delivery is body_state, never this.
            has_body: stored_body,
            content_hash: o
                .body
                .as_ref()
                .map(|b| b.content_hash.clone())
                .or_else(|| descriptor.as_ref().map(|d| d.content_hash.clone()))
                .unwrap_or_default(),
            revision: o
                .body
                .as_ref()
                .map(|b| b.revision)
                .or_else(|| descriptor.as_ref().map(|d| d.revision))
                .unwrap_or_default(),
            required_record_bytes: required,
            body_state: state.as_str().into(),
            descriptor: descriptor.map(|d| NodeBodyDescriptor {
                r#ref: d.node_id,
                revision: d.revision,
                content_hash: d.content_hash,
                record_bytes: d.record_bytes,
                body_bytes: d.body_bytes,
                record_digest: d.record_digest,
            }),
            card: card.map(card_view),
        }})
        .collect();
    response.supports = proof
        .supports
        .iter()
        .map(|e| memory_relation_from_bundle_relationship(&BundleRelationship::from_projection(e)))
        .collect();
    response.proof = Some(TraceProofStatus {
        missing_refs_count: proof.missing_refs.len() as u32,
        missing_bodies_count: proof.missing_bodies.len() as u32,
        incomplete_entries_count: proof.incomplete_entries.len() as u32,
        clock_unknown_entries_count: proof.clock_unknown_entries.len() as u32,
        complete_groups: proof.complete_groups,
        incomplete_groups: proof.incomplete_groups,
        stop_reason: proof
            .stop
            .map_or("sources_enumerated", |s| s.as_str())
            .into(),
        body_bytes: proof.body_bytes,
        manifest_id: proof.manifest_id.unwrap_or_default(),
        delivery: proof.delivery.map(|d| TraceBodyDelivery {
            loaded: d.loaded,
            deferred_budget: d.deferred_budget,
            not_requested: d.not_requested,
            compact: d.compact,
            missing: d.missing,
            admitted_record_bytes: d.admitted_record_bytes,
            selected_body_bytes: d.selected_body_bytes,
            next_deferred_ref: d.next_deferred_ref.unwrap_or_default(),
            rerun_record_bytes: d.rerun_record_bytes.unwrap_or_default(),
            named_record_bytes: d.named_record_bytes.unwrap_or_default(),
        }),
        compact: proof.compact.map(|c| TraceCompactSummary {
            language: c.language,
            valid: c.valid,
            stale: c.stale,
            absent: c.absent,
            after_cut: c.after_cut,
            card_bytes: c.card_bytes,
            body_bytes_omitted: c.body_bytes_omitted,
        }),
    });
    response.warnings.push("Proof objects and supports are shared tables. Join all pages before assessing complete_groups: this reports fetched declared sources for dated selected routes, not semantic sufficiency, current lifecycle state or truth. Undated attachments remain uncertain. body_bytes counts canonical UTF-8 bodies; budget.max_bytes bounds each response, not storage allocations.".into());
}

pub(super) fn page<T>(items: &mut Vec<T>, skip: &mut usize, remaining: &mut usize) {
    let start = (*skip).min(items.len());
    *skip -= start;
    let count = (*remaining).min(items.len() - start);
    *remaining -= count;
    items.drain(start + count..);
    items.drain(..start);
}

#[cfg(test)]
#[path = "trace_proof_tests.rs"]
mod tests;

/// A card as the wire carries it. `text` survives only for a valid card: the
/// domain already cleared it for every other state, and this mapping has no
/// branch that could put it back.
fn card_view(presented: NodeCardPresentation) -> NodeCardView {
    let stamp = presented.stored;
    NodeCardView {
        status: presented.status.as_str().into(),
        text: presented.text.unwrap_or_default(),
        source_revision: stamp.as_ref().map_or(0, |s| s.source_revision),
        source_content_hash: stamp
            .as_ref()
            .map(|s| s.source_content_hash.clone())
            .unwrap_or_default(),
        source_record_digest: stamp
            .as_ref()
            .map(|s| s.source_record_digest.clone())
            .unwrap_or_default(),
        source_body_bytes: stamp.as_ref().map_or(0, |s| s.source_body_bytes),
        card_revision: stamp.as_ref().map_or(0, |s| s.card_revision),
        authored_by: stamp
            .as_ref()
            .map(|s| s.authored_by.clone())
            .unwrap_or_default(),
        authored_at: stamp
            .as_ref()
            .map(|s| s.authored_at.clone())
            .unwrap_or_default(),
        text_bytes: stamp.as_ref().map_or(0, |s| s.text_bytes),
    }
}
