use super::{
    read_selection_fingerprint::ReadSelectionFingerprint,
    scalars::{ProtoMappingResult, invalid_argument},
};
use kmp_domain::{TraceMaterialResult, TraceMaterialSelection, TraceProofRequirement};
use kmp_proto::v1beta1::{
    TraceMaterialSelectionOptions, TraceMaterialSelectionResult, TraceResponse, TraceRoute,
};
use std::collections::BTreeMap;

pub(super) fn request(
    options: TraceMaterialSelectionOptions,
) -> ProtoMappingResult<TraceMaterialSelection> {
    if options.max_material_nodes == 0 {
        return Err(invalid_argument(
            "search.select.max_material_nodes is required and must be positive",
        ));
    }
    Ok(TraceMaterialSelection {
        max_nodes: options.max_material_nodes,
        max_paths: if options.max_paths == 0 {
            4
        } else {
            options.max_paths
        },
        groups: options
            .groups
            .into_iter()
            .map(|g| TraceProofRequirement {
                weight: if g.weight == 0 { 1 } else { g.weight },
                alternatives: g
                    .alternatives
                    .into_iter()
                    .map(|a| a.refs.into_iter().collect())
                    .collect(),
            })
            .collect(),
    })
}

/// Keep the full candidate digest while returning only selected complete paths.
pub(super) fn project(response: &mut TraceResponse, material: TraceMaterialResult) {
    let candidate_fingerprint = ReadSelectionFingerprint::trace_search(response);
    let candidate_count = response.routes.len() as u32;
    let original_routes = std::mem::take(&mut response.routes);
    let original_edges = std::mem::take(&mut response.trace);
    let mut indexes = BTreeMap::new();
    for &index in &material.selected_candidates {
        let route = &original_routes[index as usize];
        let edge_indexes = route
            .edge_indexes
            .iter()
            .map(|&old| {
                *indexes.entry(old).or_insert_with(|| {
                    response.trace.push(original_edges[old as usize].clone());
                    response.trace.len() as u32 - 1
                })
            })
            .collect();
        response.routes.push(TraceRoute {
            target: route.target.clone(),
            edge_indexes,
        });
    }
    let selected_count = material.selected_candidates.len();
    let group_count = material.covered_groups.len() + material.incomplete_groups.len();
    response.summary = format!(
        "{} Selected {selected_count} of {candidate_count} candidate paths, covering {} of {group_count} caller-declared groups.",
        response.summary,
        material.covered_groups.len()
    );
    let search = response
        .search
        .as_mut()
        .expect("bounded trace has search metadata");
    search.clock_unknown_edges = search
        .clock_unknown_edges
        .iter()
        .filter_map(|old| indexes.get(old).copied())
        .collect();
    search.material = Some(TraceMaterialSelectionResult {
        candidate_count,
        selected_candidates: material.selected_candidates,
        material_nodes: material.material_refs.len() as u32,
        benefit: material.benefit,
        covered_groups: material.covered_groups,
        incomplete_groups: material.incomplete_groups,
        evaluated: material.evaluated,
        pruned_by_width: material.pruned_by_width,
        candidate_fingerprint,
    });
}

#[cfg(test)]
#[path = "trace_material_tests.rs"]
mod tests;
