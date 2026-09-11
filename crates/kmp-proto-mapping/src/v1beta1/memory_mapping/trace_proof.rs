use super::bundle_views::{
    memory_relation_from_bundle_relationship, persisted_memory_metadata, persisted_memory_source,
    persisted_support_clocks, proto_coordinate_from_domain,
};
use kmp_domain::{BundleRelationship, TraceProofResult};
use kmp_proto::v1beta1::{InspectedObject, TraceProofObject, TraceProofStatus, TraceResponse};

pub(super) fn project(response: &mut TraceResponse, proof: TraceProofResult) {
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
    response.gaps = gaps
        .into_iter()
        .map(|(r#ref, reasons)| kmp_proto::v1beta1::TraceProofGap { r#ref, reasons })
        .collect();
    response.objects = proof
        .objects
        .into_iter()
        .map(|o| TraceProofObject {
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
            has_body: o.body.is_some(),
            content_hash: o
                .body
                .as_ref()
                .map(|b| b.content_hash.clone())
                .unwrap_or_default(),
            revision: o.body.as_ref().map_or(0, |b| b.revision),
        })
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
