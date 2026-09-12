use super::trace_temporal_admission::TraceTemporalAdmission;
use crate::{
    EvidencePathResult, NodeBodyDescriptor, NodeCard, NodeCardStatus, NodeDetailProjection,
    NodeProjection, PortError, RelationDirection, TemporalCoordinate, TraceBodyAdmission,
    TraceBodyDelivery, TraceBodyOptions, TraceBodyState, TraceCompactSummary, TraceManifestDigest,
    TraceProofObject, TraceProofResult, TraceSearchRequest, TraceSearchResult,
    TraceExpansionRefusal, TraceSnapshotReader, node_card_policy, trace_body_admission,
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
    let mut result = fetch(
        reader,
        &request.about,
        &entries,
        admission,
        &request.body,
        &trace_seed(request, search),
    )?;
    if result.refusal.is_some() {
        return Ok(result);
    }
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
    options: &TraceBodyOptions,
) -> Result<TraceProofResult, PortError> {
    let entries: BTreeSet<String> = search
        .viable_candidates()
        .iter()
        .flat_map(|&i| search.candidates[i as usize].nodes.iter().cloned())
        .collect();
    let mut result = fetch(reader, about, &entries, admission, options, &seek_seed(about, search))?;
    if result.refusal.is_some() {
        return Ok(result);
    }
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

/// A group is complete when its canonical proof was fetched. A body this
/// response deferred, did not request or replaced with a card was not
/// fetched, so it cannot close a group — a card is orientation, never proof.
fn complete(refs: &BTreeSet<String>, proof: &TraceProofResult) -> bool {
    !refs.is_empty()
        && proof.stop.is_none()
        && proof.incomplete_entries.iter().all(|r| !refs.contains(r))
        && proof
            .clock_unknown_entries
            .iter()
            .all(|r| !refs.contains(r))
        && proof
            .objects
            .iter()
            .filter(|object| refs.contains(&object.node.node_id))
            .all(|object| object.body_state == TraceBodyState::Loaded)
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
    options: &TraceBodyOptions,
    seed: &str,
) -> Result<TraceProofResult, PortError> {
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
    let ids: Vec<String> = nodes.keys().cloned().collect();
    let mut missing_bodies = BTreeSet::new();
    let mut omitted = BTreeSet::new();

    if !options.is_active() {
        // The legacy read, unchanged: every present body is loaded, no
        // descriptor is consulted and no manifest is computed. This is the
        // path the no-option oracle froze.
        let bodies = load_bodies(reader, &ids)?;
        for ((id, node), body) in nodes.into_iter().zip(bodies) {
            if let Some(body) = &body {
                result.body_bytes += body.detail.len() as u64;
            } else {
                missing_bodies.insert(id);
            }
            result.objects.push(TraceProofObject {
                coordinates: coordinates.remove(&node.node_id).unwrap_or_default(),
                node,
                descriptor: None,
                body_state: TraceBodyState::Loaded,
                body,
                card: None,
            });
        }
    } else {
        // Descriptors first: identity and size of every selected body, without
        // reading one. Everything below decides delivery from these.
        let descriptors = load_descriptors(reader, &ids)?;
        let manifest = manifest_id(seed, &ids, &nodes, &descriptors, &coordinates, &result, &missing);
        // Both refusals are decided here, before a card or a body is read.
        let refusal = options
            .expect_selection
            .as_ref()
            .filter(|expected| **expected != manifest)
            .map(|expected| TraceExpansionRefusal::SelectionChanged {
                expected: expected.clone(),
                actual: manifest.clone(),
            })
            .or_else(|| {
                let selected: BTreeSet<&String> = ids.iter().collect();
                let unknown: Vec<String> = options
                    .refs
                    .iter()
                    .flatten()
                    .filter(|reference| !selected.contains(reference))
                    .cloned()
                    .collect();
                // The whole batch is refused. Delivering only the refs that
                // happened to be inside would answer a different question and
                // hide the mistake in a successful-looking response.
                (!unknown.is_empty()).then(|| TraceExpansionRefusal::UnknownRefs(unknown))
            });
        if let Some(refusal) = refusal {
            return Ok(TraceProofResult {
                manifest_id: Some(manifest),
                refusal: Some(refusal),
                stop: admission.budget.stop,
                ..TraceProofResult::default()
            });
        }
        let cut = admission.window().card_cut_nanos();
        let cards = load_cards(reader, &ids, options.compact.as_deref())?;
        let presentations: Vec<Option<crate::NodeCardPresentation>> = ids
            .iter()
            .zip(&cards)
            .map(|(id, card)| {
                options.compact.as_ref().map(|_| {
                    node_card_policy::presentation(card.as_ref(), descriptors.get(id), cut)
                })
            })
            .collect();
        let reusable: BTreeSet<String> = ids
            .iter()
            .zip(&presentations)
            .filter(|(_, presented)| {
                presented
                    .as_ref()
                    .is_some_and(|p| p.status == NodeCardStatus::Valid)
            })
            .map(|(id, _)| id.clone())
            .collect();
        let admitted = trace_body_admission::admit(&ids, &descriptors, options, &reusable);
        // Only admitted ids reach the body port. Nothing deferred, unrequested
        // or served by a card is read.
        let loaded = load_bodies(reader, &admitted.load)?;
        let mut bodies: BTreeMap<String, NodeDetailProjection> = BTreeMap::new();
        for (id, body) in admitted.load.iter().zip(loaded) {
            let Some(body) = body else {
                return Err(PortError::InvalidState(format!(
                    "`{id}` had a body descriptor and no body; this projection is inconsistent"
                )));
            };
            verify_against_descriptor(&body, descriptors.get(id))?;
            bodies.insert(id.clone(), body);
        }
        let mut summary = options.compact.as_ref().map(|language| TraceCompactSummary {
            language: language.clone(),
            ..TraceCompactSummary::default()
        });
        for ((id, node), presented) in nodes.into_iter().zip(presentations) {
            let state = admitted.state(&id);
            let body = bodies.remove(&id);
            if let Some(body) = &body {
                result.body_bytes += body.detail.len() as u64;
            }
            match state {
                TraceBodyState::Missing => {
                    missing_bodies.insert(id.clone());
                }
                state if state.is_omitted() => {
                    omitted.insert(id.clone());
                }
                _ => {}
            }
            if let (Some(summary), Some(presented)) = (summary.as_mut(), presented.as_ref()) {
                match presented.status {
                    NodeCardStatus::Valid => {
                        summary.valid += 1;
                        summary.card_bytes += presented.text_bytes();
                        summary.body_bytes_omitted +=
                            descriptors.get(&id).map_or(0, |d| d.body_bytes);
                    }
                    NodeCardStatus::Stale => summary.stale += 1,
                    NodeCardStatus::Absent => summary.absent += 1,
                    NodeCardStatus::AfterCut => summary.after_cut += 1,
                }
            }
            result.objects.push(TraceProofObject {
                coordinates: coordinates.remove(&node.node_id).unwrap_or_default(),
                node,
                descriptor: descriptors.get(&id).cloned(),
                body_state: state,
                body,
                card: presented,
            });
        }
        result.compact = summary;
        result.manifest_id = Some(manifest);
        result.delivery = Some(delivery(&admitted));
    }

    // A body the store lacks and a body this response withheld are different
    // facts with the same consequence for completeness: the declared proof was
    // not fetched. Only the first is `missing_bodies`.
    let undelivered: BTreeSet<String> = missing_bodies.union(&omitted).cloned().collect();
    incomplete.extend(entries.intersection(&undelivered).cloned());
    for edge in &result.supports {
        if undelivered.contains(&edge.source_node_id) {
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

/// Bodies in requested order, with the slot check that keeps a batch honest.
fn load_bodies<R: TraceSnapshotReader>(
    reader: &R,
    ids: &[String],
) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let bodies = reader.bodies(ids)?;
    if bodies.len() != ids.len()
        || bodies
            .iter()
            .zip(ids)
            .any(|(body, id)| body.as_ref().is_some_and(|b| b.node_id != *id))
    {
        return Err(PortError::InvalidState(
            "trace body batch does not preserve requested slots".into(),
        ));
    }
    Ok(bodies)
}

/// Descriptors for the selected manifest, from the same snapshot.
///
/// A ref with no descriptor has no stored body. A backend that cannot answer
/// at all fails the read: an unsupported projection is never absent evidence,
/// and never a licence to load the whole body instead.
fn load_descriptors<R: TraceSnapshotReader>(
    reader: &R,
    ids: &[String],
) -> Result<BTreeMap<String, NodeBodyDescriptor>, PortError> {
    if ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let descriptors = reader.descriptors(ids)?;
    if descriptors.len() != ids.len()
        || descriptors
            .iter()
            .zip(ids)
            .any(|(descriptor, id)| descriptor.as_ref().is_some_and(|d| d.node_id != *id))
    {
        return Err(PortError::InvalidState(
            "trace descriptor batch does not preserve requested slots".into(),
        ));
    }
    Ok(ids
        .iter()
        .cloned()
        .zip(descriptors)
        .filter_map(|(id, descriptor)| descriptor.map(|descriptor| (id, descriptor)))
        .collect())
}

fn load_cards<R: TraceSnapshotReader>(
    reader: &R,
    ids: &[String],
    language: Option<&str>,
) -> Result<Vec<Option<NodeCard>>, PortError> {
    let Some(language) = language.filter(|_| !ids.is_empty()) else {
        return Ok(vec![None; ids.len()]);
    };
    let cards = reader.cards(ids, language)?;
    if cards.len() != ids.len()
        || cards
            .iter()
            .zip(ids)
            .any(|(card, id)| card.as_ref().is_some_and(|card| card.node_id != *id))
    {
        return Err(PortError::InvalidState(
            "trace card batch does not preserve requested slots".into(),
        ));
    }
    Ok(cards)
}

/// A loaded body must be the body its descriptor described. Anything else is
/// a store that moved under one snapshot, and it fails rather than returning
/// text nobody can place.
fn verify_against_descriptor(
    body: &NodeDetailProjection,
    descriptor: Option<&NodeBodyDescriptor>,
) -> Result<(), PortError> {
    let Some(descriptor) = descriptor else {
        return Err(PortError::InvalidState(format!(
            "`{}` was loaded without a body descriptor", body.node_id
        )));
    };
    if body.revision != descriptor.revision
        || body.content_hash != descriptor.content_hash
        || body.detail.len() as u64 != descriptor.body_bytes
    {
        return Err(PortError::InvalidState(format!(
            "`{}` does not match the descriptor this read admitted: descriptor revision {} \
             ({} bytes), body revision {} ({} bytes)",
            body.node_id,
            descriptor.revision,
            descriptor.body_bytes,
            body.revision,
            body.detail.len()
        )));
    }
    Ok(())
}

fn delivery(admitted: &TraceBodyAdmission) -> TraceBodyDelivery {
    let count = |wanted: TraceBodyState| {
        admitted
            .states
            .values()
            .filter(|state| **state == wanted)
            .count() as u32
    };
    TraceBodyDelivery {
        loaded: count(TraceBodyState::Loaded),
        deferred_budget: count(TraceBodyState::DeferredBudget),
        not_requested: count(TraceBodyState::NotRequested),
        compact: count(TraceBodyState::Compact),
        missing: count(TraceBodyState::Missing),
        admitted_record_bytes: admitted.admitted_record_bytes,
        selected_body_bytes: admitted.selected_body_bytes,
        next_deferred_ref: admitted.next_deferred.as_ref().map(|(id, _)| id.clone()),
        rerun_record_bytes: admitted.next_rerun_record_bytes(),
        named_record_bytes: admitted.next_named_record_bytes(),
    }
}

/// The identity of this selected proof table, hashed before any body loads.
///
/// Covers the bound query through `seed`, then every selected node with its
/// metadata and clocks, every body descriptor including the absent ones, every
/// support declaration, and the refs this selection could not resolve.
fn manifest_id(
    seed: &str,
    ids: &[String],
    nodes: &BTreeMap<String, NodeProjection>,
    descriptors: &BTreeMap<String, NodeBodyDescriptor>,
    coordinates: &BTreeMap<String, Vec<TemporalCoordinate>>,
    result: &TraceProofResult,
    missing: &BTreeSet<String>,
) -> String {
    let mut digest = TraceManifestDigest::new("kmp.trace.manifest.v1");
    digest.text(seed);
    digest.number(ids.len() as u64);
    for id in ids {
        digest.text(id);
        if let Some(node) = nodes.get(id) {
            digest.node(node);
        }
        digest.descriptor(descriptors.get(id));
        digest.coordinates(coordinates.get(id).map_or(&[][..], Vec::as_slice));
    }
    digest.number(result.supports.len() as u64);
    for edge in &result.supports {
        digest.relation(edge);
    }
    digest.texts(missing.iter().map(String::as_str));
    digest.finish()
}

/// The part of the manifest that belongs to the bounded target search: the
/// query as bound, the selected routes and the material selection, including
/// the fingerprint that already covers candidates this response hides.
fn trace_seed(request: &TraceSearchRequest, search: &TraceSearchResult) -> String {
    let mut digest = TraceManifestDigest::new("kmp.trace.selection.target.v1");
    digest.text(&request.about);
    digest.text(&request.from);
    digest.texts(request.targets.iter().map(String::as_str));
    digest.text(&format!("{:?}", request.direction));
    digest.texts(request.relations.iter().map(String::as_str));
    digest.number(request.paths_per_target as u64);
    digest.number(request.follow.len() as u64);
    for step in &request.follow {
        digest.text(step.relation.as_str());
        digest.text(&format!("{:?}", step.direction));
    }
    digest.text(&format!("{:?}", request.temporal));
    digest.number(search.relations.len() as u64);
    for edge in &search.relations {
        digest.relation(edge);
    }
    digest.number(search.routes.len() as u64);
    for route in &search.routes {
        digest.text(&route.target);
        digest.number(route.edge_indexes.len() as u64);
        for index in &route.edge_indexes {
            digest.number(u64::from(*index));
        }
    }
    match &search.material {
        Some(material) => {
            digest.flag(true);
            digest.texts(material.material_refs.iter().map(String::as_str));
            digest.number(u64::from(material.evaluated));
            digest.number(u64::from(material.pruned_by_width));
            digest.number(u64::from(material.benefit));
            for index in &material.selected_candidates {
                digest.number(u64::from(*index));
            }
            for index in &material.covered_groups {
                digest.number(u64::from(*index));
            }
            for index in &material.incomplete_groups {
                digest.number(u64::from(*index));
            }
        }
        None => digest.flag(false),
    }
    for index in &search.clock_unknown_edges {
        digest.number(u64::from(*index));
    }
    digest.finish()
}

/// The same, for the seek mode: the discovered candidates and groups are the
/// selection, and the caller named no destinations.
fn seek_seed(about: &str, search: &EvidencePathResult) -> String {
    let mut digest = TraceManifestDigest::new("kmp.trace.selection.seek.v1");
    digest.text(about);
    digest.text(&search.from);
    digest.number(search.candidates.len() as u64);
    for candidate in &search.candidates {
        digest.texts(candidate.nodes.iter().map(String::as_str));
        digest.flag(candidate.clock_unknown);
    }
    digest.number(search.groups.len() as u64);
    for group in &search.groups {
        digest.flag(group.clock_unknown);
        for index in &group.candidate_indexes {
            digest.number(u64::from(*index));
        }
    }
    digest.number(search.relations.len() as u64);
    for edge in &search.relations {
        digest.relation(edge);
    }
    digest.finish()
}
