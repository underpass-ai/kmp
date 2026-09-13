//! Boundary projection of bounded domain dependency groups onto stored proof.
use std::collections::BTreeSet;

use kmp_domain::{
    KmpBundle, ProofDependencyGroup, TemporalProofPlan, TemporalTraversalResult,
    compare_temporal_coordinates,
};
use kmp_proto::v1beta1::{ProofDependencyGroup as ProtoGroup, TemporalEntry};

use super::bundle_views::{bundle_memory_metadata, proto_coordinate_from_domain};
use super::temporal_admission::{TemporalAdmission, coordinates_by_ref};

pub(super) fn select(
    bundle: &KmpBundle,
    admission: &TemporalAdmission,
    entries: &[TemporalEntry],
    traversal: &TemporalTraversalResult,
) -> (Vec<ProtoGroup>, BTreeSet<String>, Vec<TemporalEntry>) {
    let plan = TemporalProofPlan::select(bundle, traversal, true)
        .expect("temporal membership coordinates were validated by traversal");
    let groups = plan
        .groups()
        .iter()
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
    let extra_entries = std::iter::once(bundle.root_node())
        .chain(bundle.neighbor_nodes())
        .filter_map(|node| {
            let id = node.node_id();
            if seeds.contains(id) || !members.contains(id) {
                return None;
            }
            let mut entry_coordinates = coordinates.get(id)?.clone();
            entry_coordinates.retain(|coordinate| admission.admits_coordinate(coordinate));
            entry_coordinates.sort_by(compare_temporal_coordinates);
            entry_coordinates.dedup();
            Some(TemporalEntry {
                r#ref: id.to_string(),
                kind: node
                    .properties()
                    .get("entry_kind")
                    .cloned()
                    .unwrap_or_else(|| node.node_kind().to_string()),
                text: node.summary().to_string(),
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
