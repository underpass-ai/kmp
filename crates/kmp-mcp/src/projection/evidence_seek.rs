use super::rendering::temporal_axis_label;
use kmp_proto::v1beta1::{TraceEvidenceBinding, TraceMissingWitness, TraceResponse};
use serde_json::{Value, json};

pub(super) fn project(response: &TraceResponse, value: &mut Value) {
    let Some(s) = &response.seek else {
        return;
    };
    value["seek"] = json!({"from":s.from,"status":s.status,"declared_obligations_complete":s.declared_obligations_complete,
        "roles":s.roles,"missing_roles":s.missing_roles,"stop_reason":s.stop_reason,
        "discovered_nodes":s.discovered_nodes,"scanned_edges":s.scanned_edges,"work_states":s.work_states,
        "shared_states":s.shared_states,"incompatible_states":s.incompatible_states,
        "adjacency_pages":s.adjacency_pages,"coordinate_pages":s.coordinate_pages,
        "axis":temporal_axis_label(s.axis),"resolved_as_of":s.resolved_as_of.as_ref().map(|t|t.to_string()),
        "temporal_selection_resolved":s.temporal_selection_resolved,"clock_unknown_edges":s.clock_unknown_edges,
        "candidate_count":s.candidate_count,"group_count":s.group_count});
    value["candidates"] = json!(response.candidates.iter().map(|c| json!({"index":c.index,"role":c.role,
        "nodes":c.nodes,"edge_indexes":c.edge_indexes,"witness":c.witness,
        "bindings":bindings(&c.bindings),"missing":missing(&c.missing),"clock_unknown":c.clock_unknown})).collect::<Vec<_>>());
    value["groups"] = json!(response.groups.iter().map(|g| json!({"index":g.index,"candidate_indexes":g.candidate_indexes,
        "bindings":bindings(&g.bindings),"missing":missing(&g.missing),"clock_unknown":g.clock_unknown})).collect::<Vec<_>>());
}
fn bindings(items: &[TraceEvidenceBinding]) -> Vec<Value> {
    items.iter().map(|b| json!({"kind":if b.reference {"reference"} else {"label"},"key":b.key,"roles":b.roles,"values":b.values})).collect()
}
fn missing(items: &[TraceMissingWitness]) -> Vec<Value> {
    items
        .iter()
        .map(|m| json!({"role":m.role,"ref":m.r#ref,"key":m.key}))
        .collect()
}

/// A new context read, not a continuation and never an automatic wider search.
/// Only an explicit instant cut has an equivalent Goto selection here.
pub(super) fn attach_review(value: &mut Value, arguments: &Value) {
    if value.get("seek").is_none() || value["seek"]["status"] == "compatible" {
        return;
    }
    let Some(at) = arguments.get("as_of") else {
        return;
    };
    let mut read = json!({"about":arguments["about"],"at":at,"refs":[arguments["from"]],
        "include":{"relations":true,"evidence":true},"budget":{"max_bytes":10000}});
    for key in ["axis", "context_id", "purpose"] {
        if let Some(v) = arguments.get(key) {
            read[key] = v.clone();
        }
    }
    value["seek"]["review_context"] = json!({"tool":"kmp_goto","arguments":read});
}
