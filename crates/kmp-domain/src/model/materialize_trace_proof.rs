use super::trace_temporal_admission::TraceTemporalAdmission;
use crate::{
    EvidencePathResult, NodeProjection, PortError, RelationDirection, TraceProofObject,
    TraceProofResult, TraceSearchRequest, TraceSearchResult, TraceSnapshotReader,
};
use std::collections::{BTreeMap, BTreeSet};

/// Selected routes remain separate for coverage; objects are fetched jointly.
pub(super) fn trace<R: TraceSnapshotReader>(
    reader: &R,
    request: &TraceSearchRequest,
    search: &TraceSearchResult,
    admission: &mut TraceTemporalAdmission<'_, R>,
) -> Result<TraceProofResult, PortError> {
    let selected: Vec<usize> = search.material.as_ref().map_or_else(
        || (0..search.routes.len()).collect(),
        |m| m.selected_candidates.iter().map(|&i| i as usize).collect(),
    );
    let mut paths = Vec::new();
    for index in selected {
        let route = &search.routes[index];
        let mut refs = BTreeSet::from([search.from.clone()]);
        for &index in &route.edge_indexes {
            let edge = &search.relations[index as usize];
            refs.extend([edge.source_node_id.clone(), edge.target_node_id.clone()]);
        }
        let dated = route
            .edge_indexes
            .iter()
            .all(|i| !search.clock_unknown_edges.contains(i));
        paths.push((refs, dated));
    }
    let entries = paths.iter().flat_map(|(p, _)| p.iter().cloned()).collect();
    let mut result = fetch(reader, &request.about, &entries, admission)?;
    let available: BTreeSet<_> = paths
        .iter()
        .filter(|(refs, dated)| *dated && complete(refs, &result))
        .flat_map(|(refs, _)| refs.iter().cloned())
        .collect();
    let requirements = request.select.as_ref().map_or_else(
        || {
            request
                .targets
                .iter()
                .map(|t| crate::TraceProofRequirement {
                    alternatives: vec![BTreeSet::from([t.clone()])],
                    weight: 1,
                })
                .collect()
        },
        |p| p.requirements(&request.targets),
    );
    for (index, group) in requirements.iter().enumerate() {
        // Group indices belong to the caller, never selected candidate positions.
        let complete = result.stop.is_none()
            && group
                .alternatives
                .iter()
                .any(|a| !a.is_empty() && a.is_subset(&available));
        group_result(&mut result, index, complete);
    }
    Ok(result)
}

pub(super) fn evidence<R: TraceSnapshotReader>(
    reader: &R,
    about: &str,
    search: &EvidencePathResult,
    admission: &mut TraceTemporalAdmission<'_, R>,
) -> Result<TraceProofResult, PortError> {
    let entries: BTreeSet<String> = search
        .viable_candidates()
        .iter()
        .flat_map(|&i| search.candidates[i as usize].nodes.iter().cloned())
        .collect();
    let mut result = fetch(reader, about, &entries, admission)?;
    for (index, group) in search.groups.iter().enumerate() {
        let refs = group
            .candidate_indexes
            .iter()
            .flat_map(|&i| search.candidates[i as usize].nodes.iter().cloned())
            .collect();
        let known = !group.clock_unknown
            && group.bindings.missing.is_empty()
            && group.candidate_indexes.iter().all(|&i| {
                let c = &search.candidates[i as usize];
                !c.clock_unknown && c.bindings.missing.is_empty()
            });
        let fetched = known && complete(&refs, &result);
        group_result(&mut result, index, fetched);
    }
    Ok(result)
}

fn complete(refs: &BTreeSet<String>, proof: &TraceProofResult) -> bool {
    !refs.is_empty()
        && proof.stop.is_none()
        && proof.incomplete_entries.iter().all(|r| !refs.contains(r))
        && proof
            .clock_unknown_entries
            .iter()
            .all(|r| !refs.contains(r))
}

fn group_result(result: &mut TraceProofResult, index: usize, complete: bool) {
    if complete {
        result.complete_groups.push(index as u32);
    } else {
        result.incomplete_groups.push(index as u32);
    }
}

fn fetch<R: TraceSnapshotReader>(
    reader: &R,
    about: &str,
    entries: &BTreeSet<String>,
    admission: &mut TraceTemporalAdmission<'_, R>,
) -> Result<TraceProofResult, PortError> {
    admission.budget.proof_reads = true;
    let mut result = TraceProofResult::default();
    let mut nodes = BTreeMap::<String, NodeProjection>::new();
    let mut missing = BTreeSet::new();
    let mut incomplete = BTreeSet::new();
    let mut unknown = BTreeSet::new();
    let mut sources = BTreeMap::<String, bool>::new();
    let mut coordinates = BTreeMap::new();
    for entry in entries {
        if admission.budget.stop.is_some() {
            incomplete.insert(entry.clone());
            continue;
        }
        match admission.budget.node(entry)? {
            Some(node)
                if node.properties.get("memory_about").map(String::as_str) == Some(about)
                    && node.labels.iter().any(|l| l == "entry") =>
            {
                nodes.insert(entry.clone(), node);
            }
            _ => {
                if admission.budget.stop.is_none() {
                    missing.insert(entry.clone());
                }
                incomplete.insert(entry.clone());
                continue;
            }
        }
        match admission.proof_coordinates(entry)? {
            Some(values) => {
                coordinates.insert(entry.clone(), values);
            }
            None => {
                incomplete.insert(entry.clone());
            }
        }
        let mut after = None;
        let mut has_source = false;
        loop {
            let Some(page) = admission.budget.page(
                entry,
                RelationDirection::Incoming,
                after,
                Some("supports"),
            )?
            else {
                incomplete.insert(entry.clone());
                break;
            };
            for edge in page.edges {
                if edge.target_node_id != *entry || edge.relation_type != "supports" {
                    return Err(PortError::InvalidState(
                        "proof support adjacency returned the wrong endpoint or type".into(),
                    ));
                }
                if !admission
                    .window()
                    .admits_dependency_relation(&edge.explanation)
                {
                    continue;
                }
                let reference = &edge.source_node_id;
                if !sources.contains_key(reference) && !missing.contains(reference) {
                    match admission.budget.node(reference)? {
                        Some(node)
                            if node.properties.get("memory_about").map(String::as_str)
                                == Some(about)
                                && matches!(
                                    node.node_kind.as_str(),
                                    "memory_evidence" | "evidence"
                                ) =>
                        {
                            sources.insert(reference.clone(), true);
                            nodes.insert(reference.clone(), node);
                        }
                        Some(node)
                            if node.properties.get("memory_about").map(String::as_str)
                                != Some(about) =>
                        {
                            // Do not expose foreign source bodies or relation explanations.
                            sources.insert(reference.clone(), false);
                            missing.insert(reference.clone());
                        }
                        _ => {
                            if admission.budget.stop.is_none() {
                                missing.insert(reference.clone());
                            }
                        }
                    }
                }
                if sources.get(reference) != Some(&true) {
                    incomplete.insert(entry.clone());
                    // Preserve the physical declaration for missing/wrong-kind local refs.
                    if sources.get(reference) != Some(&false) {
                        result.supports.push(edge);
                    }
                    continue;
                }
                has_source = true;
                if !admission.window().is_frontier()
                    && !admission.window().relation_clock_known(&edge.explanation)
                {
                    unknown.insert(entry.clone());
                }
                result.supports.push(edge);
            }
            if page.exhausted {
                break;
            }
            after = page.next;
        }
        if !has_source {
            incomplete.insert(entry.clone());
        }
    }
    let ids: Vec<_> = nodes.keys().cloned().collect();
    let bodies = if ids.is_empty() {
        vec![]
    } else {
        reader.bodies(&ids)?
    };
    if bodies.len() != ids.len()
        || bodies
            .iter()
            .zip(&ids)
            .any(|(body, id)| body.as_ref().is_some_and(|b| b.node_id != *id))
    {
        return Err(PortError::InvalidState(
            "trace body batch does not preserve requested slots".into(),
        ));
    }
    let mut missing_bodies = BTreeSet::new();
    for ((id, node), body) in nodes.into_iter().zip(bodies) {
        if let Some(body) = &body {
            result.body_bytes += body.detail.len() as u64;
        } else {
            missing_bodies.insert(id);
        }
        result.objects.push(TraceProofObject {
            coordinates: coordinates.remove(&node.node_id).unwrap_or_default(),
            node,
            body,
        });
    }
    incomplete.extend(entries.intersection(&missing_bodies).cloned());
    for edge in &result.supports {
        if missing_bodies.contains(&edge.source_node_id) {
            incomplete.insert(edge.target_node_id.clone());
        }
    }
    result.missing_refs = missing.into_iter().collect();
    result.missing_bodies = missing_bodies.into_iter().collect();
    result.incomplete_entries = incomplete.into_iter().collect();
    result.clock_unknown_entries = unknown.into_iter().collect();
    result.stop = admission.budget.stop;
    Ok(result)
}
