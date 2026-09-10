//! Boundary projection of bounded domain dependency groups onto stored proof.
use std::collections::BTreeSet;

use kmp_domain::{KmpBundle, ProofDependencyGroup, compare_temporal_coordinates};
use kmp_proto::v1beta1::{ProofDependencyGroup as ProtoGroup, TemporalEntry};

use super::bundle_views::{
    answer_evidence_from_bundle, bundle_memory_metadata, proto_coordinate_from_domain,
};
use super::temporal_admission::{TemporalAdmission, coordinates_by_ref};

pub(super) fn select(
    bundle: &KmpBundle,
    admission: &TemporalAdmission,
    entries: &[TemporalEntry],
) -> (Vec<ProtoGroup>, BTreeSet<String>, Vec<TemporalEntry>) {
    let scoped = bundle
        .relationships()
        .iter()
        .filter(|edge| edge.relationship_type() == "contains_entry")
        .map(|edge| edge.target_node_id().to_string())
        .collect::<BTreeSet<_>>();
    let claims = answer_evidence_from_bundle(bundle)
        .into_iter()
        .filter(|item| {
            item.id
                .strip_prefix("entry:")
                .is_some_and(|id| scoped.contains(id))
        })
        .collect::<Vec<_>>();
    let eligible = claims
        .iter()
        .filter_map(|claim| claim.id.strip_prefix("entry:"))
        .filter(|id| admission.admits_entry(id))
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    // Exclude future relation assertions before even counting their endpoints.
    // Missing/outside-selection entry identities remain private to the domain
    // selector; the response contains only anonymous unavailable counts.
    let relationships = bundle
        .relationships()
        .iter()
        .filter(|edge| admission.admits_dependency_relation(edge))
        .cloned()
        .collect::<Vec<_>>();
    let groups = entries
        .iter()
        .filter(|entry| eligible.contains(&entry.r#ref))
        .map(|entry| ProofDependencyGroup::select(&entry.r#ref, &scoped, &eligible, &relationships))
        .map(|group| ProtoGroup {
            seed_ref: group.seed_ref().to_string(),
            member_refs: group.member_refs().to_vec(),
            unavailable_in_selection: group.unavailable_in_selection() as u32,
            omitted_by_limit: group.omitted_by_limit() as u32,
            max_hops: ProofDependencyGroup::MAX_HOPS as u32,
            max_members: ProofDependencyGroup::MAX_MEMBERS as u32,
        })
        .collect::<Vec<_>>();
    let members = groups
        .iter()
        .flat_map(|group| group.member_refs.iter().cloned())
        .collect::<BTreeSet<_>>();
    let seeds = entries
        .iter()
        .map(|entry| entry.r#ref.as_str())
        .collect::<BTreeSet<_>>();
    let coordinates = coordinates_by_ref(bundle);
    let extra_entries = claims
        .into_iter()
        .filter_map(|claim| {
            let id = claim.id.strip_prefix("entry:")?;
            if seeds.contains(id) || !members.contains(id) {
                return None;
            }
            let node = std::iter::once(bundle.root_node())
                .chain(bundle.neighbor_nodes())
                .find(|node| node.node_id() == id)?;
            let mut entry_coordinates = coordinates.get(id)?.clone();
            entry_coordinates.sort_by(compare_temporal_coordinates);
            entry_coordinates.dedup();
            Some(TemporalEntry {
                r#ref: id.to_string(),
                kind: node
                    .properties()
                    .get("entry_kind")
                    .cloned()
                    .unwrap_or_else(|| node.node_kind().to_string()),
                text: claim.text,
                coordinates: entry_coordinates
                    .iter()
                    .map(proto_coordinate_from_domain)
                    .collect(),
                metadata: bundle_memory_metadata(bundle, id),
            })
        })
        .collect();
    (groups, members, extra_entries)
}
