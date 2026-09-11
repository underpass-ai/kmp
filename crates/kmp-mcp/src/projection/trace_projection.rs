use serde_json::{Value, json};

use kmp_proto::v1beta1::TraceResponse;

use super::rendering::*;

pub(crate) fn trace_from_response(response: TraceResponse) -> Value {
    let mut value = json!({
        "summary": response.summary,
        "trace": response.trace.iter().map(memory_relation_json).collect::<Vec<_>>(),
        "page": response
            .page
            .as_ref()
            .map(page_info_json)
            .unwrap_or_else(empty_page_info_json),
        "quality": optional_quality_json(response.quality.as_ref()),
        "warnings": response.warnings
    });
    super::evidence_seek::project(&response, &mut value);
    if let Some(proof) = response.proof {
        value["gaps"] = json!(
            response
                .gaps
                .iter()
                .map(|g| json!({"ref":g.r#ref,"reasons":g.reasons}))
                .collect::<Vec<_>>()
        );
        value["proof"] = json!({"missing_refs_count":proof.missing_refs_count,"missing_bodies_count":proof.missing_bodies_count,
            "incomplete_entries_count":proof.incomplete_entries_count,"clock_unknown_entries_count":proof.clock_unknown_entries_count,
            "complete_groups":proof.complete_groups,"incomplete_groups":proof.incomplete_groups,
            "stop_reason":proof.stop_reason,"body_bytes":proof.body_bytes});
        value["objects"] = json!(response.objects.iter().map(|o| {
            let object = o.object.as_ref().expect("trace proof object");
                    let mut value = json!({"ref":object.r#ref,"kind":object.kind,"text":object.text,"metadata":object.metadata,
                        "source":object.source,"has_body":o.has_body,"status":o.status,
                        "coordinates":o.coordinates.iter().map(temporal_coordinate_json).collect::<Vec<_>>(),
                        "content_hash":o.has_body.then_some(&o.content_hash),"revision":o.has_body.then_some(o.revision)});
                    let fields = value.as_object_mut().expect("proof object");
                    insert_optional_timestamp(fields, "time", o.source_time);
                    if let Some(clocks) = &o.support_clocks {
                        let mut stamps = serde_json::Map::new();
                        insert_optional_timestamp(&mut stamps, "observed_at", clocks.observed_at);
                        insert_optional_timestamp(&mut stamps, "ingested_at", clocks.ingested_at);
                        fields.insert("support_clocks".into(), Value::Object(stamps));
                    }
                    value
        }).collect::<Vec<_>>());
        value["supports"] = json!(
            response
                .supports
                .iter()
                .map(memory_relation_json)
                .collect::<Vec<_>>()
        );
    }
    if let Some(search) = response.search {
        value["search"] = json!({"stop_reason": search.stop_reason, "discovered_nodes": search.discovered_nodes,
            "scanned_edges": search.scanned_edges, "expanded_nodes": search.expanded_nodes,
            "from": search.from, "paths_per_target": search.paths_per_target,
            "considered_states": search.considered_states, "incomplete_targets": search.incomplete_targets,
            "follow": search.follow.iter().map(|step| json!({"rel":step.rel,"direction":step.direction})).collect::<Vec<_>>(),
            "leaves": search.leaves, "unreached_targets": search.unreached_targets, "direction": search.direction,
            "axis": temporal_axis_label(search.axis), "coordinate_rows": search.coordinate_rows,
            "temporal_selection_resolved": search.temporal_selection_resolved,
            "clock_unknown_edges": search.clock_unknown_edges,
            "resolved_as_of": search.resolved_as_of.as_ref().map(|at| at.to_string())});
        if let Some(routing) = search.routing {
            value["search"]["routing"] = json!({"order":if routing.focused {"dimension_focus_pages_v2"} else {"breadth_first"},
                "evaluated_entries":routing.evaluated_entries,"preferred_entries":routing.preferred_entries,
                "dimensional_rejections":routing.dimensional_rejections,"priority_pops":routing.priority_pops,
                "exploration_pops":routing.exploration_pops,"preferred_route_entries":routing.preferred_route_entries,
                "adjacency_pages":routing.adjacency_pages,"coordinate_pages":routing.coordinate_pages,"resumed_states":routing.resumed_states});
        }
        if let Some(material) = search.material {
            value["search"]["material"] = json!({
                "candidate_count":material.candidate_count,"selected_candidates":material.selected_candidates,
                "material_nodes":material.material_nodes,"benefit":material.benefit,
                "covered_groups":material.covered_groups,"incomplete_groups":material.incomplete_groups,
                "evaluated":material.evaluated,"pruned_by_width":material.pruned_by_width,
                "candidate_fingerprint":material.candidate_fingerprint
            });
        }
        value["routes"] = json!(
            response
                .routes
                .iter()
                .map(|r| json!({"target": r.target, "edge_indexes": r.edge_indexes}))
                .collect::<Vec<_>>()
        );
    }
    value
}
