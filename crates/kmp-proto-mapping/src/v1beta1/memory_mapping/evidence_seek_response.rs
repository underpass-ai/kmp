//! Complete selection fingerprint, then positional pages of links, paths, groups.
use super::{
    bundle_views::memory_relation_from_bundle_relationship,
    read_selection_fingerprint::ReadSelectionFingerprint,
    scalars::{proto_temporal_axis, timestamp_from_sort_or_rfc3339},
};
use kmp_application::TracePageRequest;
use kmp_domain::{
    BundleRelationship, EvidencePathBinding, EvidencePathBindings, EvidencePathRequest,
    EvidencePathResult, EvidencePathStatus,
};
use kmp_proto::v1beta1::{
    PageInfo, TraceEvidenceBinding, TraceEvidenceCandidate, TraceEvidenceGroup,
    TraceEvidenceSelection, TraceMissingWitness, TraceResponse,
};
use std::collections::BTreeMap;

pub fn evidence_seek_response_from_result(
    result: EvidencePathResult,
    request: &EvidencePathRequest,
    seek: &kmp_proto::v1beta1::TraceSeekOptions,
    page: TracePageRequest,
) -> TraceResponse {
    let status = match result.status {
        EvidencePathStatus::Compatible => "compatible",
        EvidencePathStatus::Ambiguous => "ambiguous",
        EvidencePathStatus::ReviewRequired => "review_required",
        EvidencePathStatus::MissingObligation => "missing_obligation",
        EvidencePathStatus::IncompatibleObligations => "incompatible_obligations",
        EvidencePathStatus::Partial => "partial",
    };
    let constraints = constraints(request);
    let mut response = TraceResponse {
        summary: format!("Declared evidence obligations: {status}; {} compatible groups. This does not establish answer truth or completeness.",result.groups.len()),
        trace: result.relations.iter().map(|r| memory_relation_from_bundle_relationship(&BundleRelationship::from_projection(r))).collect(),
        candidates: result.candidates.iter().enumerate().map(|(index,c)| TraceEvidenceCandidate {
            index: index as u32, role: request.roles[c.role].name.clone(), nodes: c.nodes.clone(), edge_indexes: c.edge_indexes.clone(),
            witness: c.nodes[c.context_hops as usize + seek.roles[c.role].via.len() + 1].clone(),
            context_hops: c.context_hops,
            bindings: bindings(&c.bindings,&constraints), missing: missing(&c.bindings), clock_unknown: c.clock_unknown
        }).collect(),
        groups: result.groups.iter().enumerate().map(|(index,g)| TraceEvidenceGroup { index: index as u32,
            candidate_indexes: g.candidate_indexes.clone(), bindings: bindings(&g.bindings,&constraints), missing: missing(&g.bindings), clock_unknown: g.clock_unknown }).collect(),
        seek: Some(TraceEvidenceSelection {
            context_discovery: result.context_discovery,
            from: result.from.clone(), status: status.into(), declared_obligations_complete: result.known_complete(),
            roles: request.roles.iter().map(|r|r.name.clone()).collect(), missing_roles: result.missing_roles.clone(),
            stop_reason: result.stop.as_str().into(), discovered_nodes: result.discovered_nodes, scanned_edges: result.scanned_edges,
            work_states: result.work_states, shared_states: result.shared_states, incompatible_states: result.incompatible_states,
            adjacency_pages: result.adjacency_pages, coordinate_pages: result.coordinate_pages,
            axis: proto_temporal_axis(request.temporal.axis().unwrap_or_default()) as i32,
            resolved_as_of: timestamp_from_sort_or_rfc3339(result.resolved_as_of.as_deref()),
            temporal_selection_resolved: result.temporal_selection_resolved, clock_unknown_edges: result.clock_unknown_edges,
            candidate_count: result.candidates.len() as u32, group_count: result.groups.len() as u32
        }),
        warnings: vec!["Seek checks only declared relation paths and witness constraints in one same-about snapshot. Missing labels and relation clocks remain unknown. Shared labels do not prove identity; source bodies and lifecycle truth are not inferred. Join all pages before auditing global edge/candidate indexes. A work cutoff is partial, never evidence of absence.".into()],
        ..Default::default()
    };
    if result.context_discovery {
        response.warnings.push("Context discovery retains all minimum-hop prefixes per reachable relation origin, with ties; longer prefixes are omitted. Prefix links are navigation leads, never a composed claim about the seed. Review original arrows, rationale, evidence and source bodies before deciding relevance. No task obligations are inferred from prose.".into());
    }
    response.selection_fingerprint = ReadSelectionFingerprint::trace_search(&response);
    let total = response.trace.len() + response.candidates.len() + response.groups.len();
    let offset = page.offset().min(total);
    let mut skip = offset;
    let mut remaining = page.entries_or_default();
    slice(&mut response.trace, &mut skip, &mut remaining);
    slice(&mut response.candidates, &mut skip, &mut remaining);
    slice(&mut response.groups, &mut skip, &mut remaining);
    let returned = page.entries_or_default() - remaining;
    let end = offset + returned;
    response.page = Some(PageInfo {
        returned: returned as u32,
        total: total as u32,
        has_more: end < total,
        next_cursor: if end < total {
            end.to_string()
        } else {
            String::new()
        },
    });
    response
}

fn slice<T: Clone>(items: &mut Vec<T>, skip: &mut usize, remaining: &mut usize) {
    let start = (*skip).min(items.len());
    *skip -= start;
    let count = (*remaining).min(items.len() - start);
    *remaining -= count;
    *items = items[start..start + count].to_vec();
}

fn constraints(request: &EvidencePathRequest) -> BTreeMap<String, TraceEvidenceBinding> {
    let mut result = BTreeMap::<String, TraceEvidenceBinding>::new();
    for role in &request.roles {
        for binding in &role.bindings {
            let (key, reference) = match binding {
                EvidencePathBinding::Label { key, .. } => (key.clone(), false),
                EvidencePathBinding::Reference { .. } => (String::new(), true),
            };
            let entry =
                result
                    .entry(binding.name().into())
                    .or_insert_with(|| TraceEvidenceBinding {
                        key,
                        reference,
                        ..Default::default()
                    });
            if !entry.roles.contains(&role.name) {
                entry.roles.push(role.name.clone());
            }
        }
    }
    result
}

fn bindings(
    bindings: &EvidencePathBindings,
    constraints: &BTreeMap<String, TraceEvidenceBinding>,
) -> Vec<TraceEvidenceBinding> {
    bindings
        .domains
        .iter()
        .filter_map(|(name, values)| {
            constraints.get(name).map(|spec| TraceEvidenceBinding {
                values: values.iter().cloned().collect(),
                ..spec.clone()
            })
        })
        .collect()
}
fn missing(bindings: &EvidencePathBindings) -> Vec<TraceMissingWitness> {
    bindings
        .missing
        .iter()
        .map(|m| TraceMissingWitness {
            role: m.role.clone(),
            r#ref: m.reference.clone(),
            key: m.key.clone(),
        })
        .collect()
}
