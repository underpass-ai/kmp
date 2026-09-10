use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::{EntryLabels, KmpBundle, MemoryRelationType, labels_by_entry};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::{MemoryIngestCommand, NeighborhoodItem, NeighborhoodLink};

const MAX_ITEMS: usize = 5;
const TEXT_BYTES: usize = 320;
const TARGET_BYTES: usize = 2048;

/// A bounded review of explicit scope. Its fingerprint covers complete selected
/// source material, including material omitted from the compact representation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WriteNeighborhood {
    pub token: String,
    pub items: Vec<NeighborhoodItem>,
    pub links: Vec<NeighborhoodLink>,
    pub eligible: usize,
    pub omitted: usize,
    pub omitted_conflicts: usize,
    pub abouts: Vec<String>,
    pub partial: bool,
}

pub(super) fn requires_review(command: &MemoryIngestCommand) -> bool {
    command.neighborhood_review.is_some()
        && command.memory.relations.iter().any(|relation| {
            MemoryRelationType::new(&relation.rel)
                .ok()
                .is_none_or(|kind| kind.requires_writer_review())
        })
}

pub(super) fn build_neighborhood(
    command: &MemoryIngestCommand,
    bundles: &[KmpBundle],
) -> WriteNeighborhood {
    let endpoints = command
        .memory
        .relations
        .iter()
        .flat_map(|link| [&link.source_ref, &link.target_ref])
        .cloned()
        .collect::<BTreeSet<_>>();
    let labels = EntryLabels::from_coordinates(
        command
            .memory
            .entries
            .iter()
            .flat_map(|entry| entry.coordinates.iter())
            .map(|c| (c.dimension.as_str(), c.scope_id.as_str())),
    );
    let mut candidates = Vec::new();
    let mut sources = BTreeMap::new();
    for bundle in bundles {
        let owner = bundle.root_node_id().as_str();
        let catalogue = labels_by_entry(bundle);
        let related = bundle
            .relationships()
            .iter()
            .filter(|link| {
                endpoints.contains(link.source_node_id())
                    || endpoints.contains(link.target_node_id())
            })
            .filter(|link| {
                *link.explanation().semantic_class()
                    != kmp_domain::RelationSemanticClass::Structural
            })
            .flat_map(|link| [link.source_node_id(), link.target_node_id()])
            .collect::<BTreeSet<_>>();
        let scoped = |reference: &str| {
            owner == command.about
                && catalogue.get(reference).is_some_and(|other| {
                    labels.keys().any(|key| {
                        labels.values(key).is_some_and(|values| {
                            other
                                .values(key)
                                .is_some_and(|other_values| !values.is_disjoint(other_values))
                        })
                    })
                })
        };
        let conflicts = bundle
            .relationships()
            .iter()
            .filter(|link| {
                link.relationship_type() == "contradicts"
                    && [link.source_node_id(), link.target_node_id()]
                        .iter()
                        .any(|reference| endpoints.contains(*reference) || scoped(reference))
            })
            .flat_map(|link| [link.source_node_id(), link.target_node_id()])
            .collect::<BTreeSet<_>>();
        for node in bundle.neighbor_nodes() {
            let reference = node.node_id();
            let reason = if conflicts.contains(reference) {
                "explicit_conflict"
            } else if endpoints.contains(reference) {
                "proposed_endpoint"
            } else if scoped(reference) && node.node_kind() == "constraint" {
                "scoped_constraint"
            } else if related.contains(reference) {
                "direct_relation"
            } else if scoped(reference) {
                "shared_label_recent"
            } else {
                continue;
            };
            let payload = node
                .properties()
                .get("memory_payload_json")
                .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());
            let mut clocks = BTreeMap::<String, Vec<String>>::new();
            if let Some(payload) = &payload {
                for coordinate in payload["coordinates"].as_array().into_iter().flatten() {
                    for name in [
                        "occurred_at",
                        "observed_at",
                        "ingested_at",
                        "valid_from",
                        "valid_until",
                    ] {
                        if let Some(value) = coordinate[name].as_str() {
                            let values = clocks.entry(name.into()).or_default();
                            if !values.iter().any(|v| v == value) {
                                values.push(value.into());
                            }
                        }
                    }
                }
            }
            // The projection's summary is the complete canonical entry text.
            // Long text is omitted whole, never clipped before a qualification.
            candidates.push(NeighborhoodItem {
                about: owner.into(),
                reference: reference.into(),
                state: "stored".into(),
                kind: node.node_kind().into(),
                reason: reason.into(),
                text: (node.summary().len() <= TEXT_BYTES).then(|| node.summary().to_owned()),
                text_omitted: node.summary().len() > TEXT_BYTES,
                clocks,
            });
            sources.insert(
                reference.to_string(),
                json!({"text":node.summary(), "properties":node.properties()}),
            );
        }
    }
    for entry in &command.memory.entries {
        if !endpoints.contains(&entry.id) {
            continue;
        }
        candidates.push(NeighborhoodItem {
            about: command.about.clone(),
            reference: entry.id.clone(),
            state: "proposed".into(),
            kind: entry.kind.clone(),
            reason: "proposed_endpoint".into(),
            text: (entry.text.len() <= TEXT_BYTES).then(|| entry.text.clone()),
            text_omitted: entry.text.len() > TEXT_BYTES,
            clocks: {
                let mut clocks = BTreeMap::<String, Vec<String>>::new();
                for coordinate in &entry.coordinates {
                    for (axis, value) in [
                        ("occurred_at", &coordinate.occurred_at),
                        ("observed_at", &coordinate.observed_at),
                        ("valid_from", &coordinate.valid_from),
                        ("valid_until", &coordinate.valid_until),
                    ] {
                        if let Some(value) = value {
                            let values = clocks.entry(axis.into()).or_default();
                            if !values.contains(value) {
                                values.push(value.clone());
                            }
                        }
                    }
                }
                clocks
            },
        });
    }
    let priority = |item: &NeighborhoodItem| match item.reason.as_str() {
        "explicit_conflict" => 0,
        "scoped_constraint" => 1,
        "proposed_endpoint" => 2,
        "direct_relation" => 3,
        _ => 4,
    };
    candidates.sort_by(|a, b| {
        priority(a)
            .cmp(&priority(b))
            .then_with(|| {
                b.clocks
                    .get("ingested_at")
                    .cmp(&a.clocks.get("ingested_at"))
            })
            .then_with(|| a.reference.cmp(&b.reference))
            .then_with(|| a.state.cmp(&b.state))
    });
    let references = sources.keys().collect::<BTreeSet<_>>();
    let links = bundles
        .iter()
        .flat_map(|bundle| bundle.relationships())
        .filter(|link| {
            references.contains(&link.source_node_id().to_string())
                || references.contains(&link.target_node_id().to_string())
        })
        .map(|link| {
            (
                link.source_node_id(),
                link.relationship_type(),
                link.target_node_id(),
                link.explanation().to_properties(),
            )
        })
        .collect::<BTreeSet<_>>();
    let mut hash = Sha256::new();
    hash.update(b"kmp.write.neighborhood.v1\0");
    hash.update(super::ingest::logical_digest(command));
    hash.update(serde_json::to_vec(&(sources, links)).expect("literal neighborhood serializes"));
    let conflicts = candidates
        .iter()
        .filter(|item| item.reason == "explicit_conflict")
        .count();
    let mut packet = WriteNeighborhood {
        token: format!("{:x}", hash.finalize()),
        eligible: candidates.len(),
        items: candidates.into_iter().take(MAX_ITEMS).collect(),
        omitted: 0,
        omitted_conflicts: 0,
        partial: false,
        links: Vec::new(),
        abouts: bundles
            .iter()
            .map(|bundle| bundle.root_node_id().as_str().to_owned())
            .chain(std::iter::once(command.about.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    };
    loop {
        packet.links = bundles
            .iter()
            .flat_map(|bundle| bundle.relationships())
            .filter(|link| {
                *link.explanation().semantic_class()
                    != kmp_domain::RelationSemanticClass::Structural
            })
            .filter_map(|link| {
                Some(NeighborhoodLink {
                    from: packet.items.iter().position(|item| {
                        item.state == "stored" && item.reference == link.source_node_id()
                    })?,
                    rel: link.relationship_type().to_string(),
                    to: packet.items.iter().position(|item| {
                        item.state == "stored" && item.reference == link.target_node_id()
                    })?,
                })
            })
            .collect();
        packet.omitted = packet.eligible - packet.items.len();
        packet.omitted_conflicts = conflicts
            - packet
                .items
                .iter()
                .filter(|item| item.reason == "explicit_conflict")
                .count();
        packet.partial = packet.omitted > 0 || packet.items.iter().any(|item| item.text_omitted);
        if serde_json::to_vec(&packet)
            .expect("packet serializes")
            .len()
            <= TARGET_BYTES
            || packet.items.len() <= 1
        {
            break;
        }
        packet.items.pop();
    }
    packet
}
