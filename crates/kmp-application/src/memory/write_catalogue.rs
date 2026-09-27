//! What a memory write reads of its about, point by point (DESIGN L6, write
//! in O(delta)). The translation asks three things of the about: its
//! dimensions and labels, whether the refs the write names stand in it, and
//! the sequence frontier of each coordinate it writes. Each is answered here
//! from [`MemoryWriteFacts`] exactly as [`ExistingMemoryRefs`] answers it
//! from the about's depth-1 neighbourhood, without materializing the about.

use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::{MemoryDimensionIdentity, MemoryWriteFacts, MemoryWriteFactsRequest};

use super::dimension_registry::DimensionRegistry;
use super::ingest::normalize_coordinate;
use super::{ExistingMemoryRefs, MemoryIngestCommand};

/// The about's dimensions and labels, as a write reads them before it asks
/// anything about refs or sequences.
pub(super) fn catalogue(about: &str, facts: &MemoryWriteFacts) -> ExistingMemoryRefs {
    if !facts.exists {
        return ExistingMemoryRefs::default();
    }
    let mut existing = ExistingMemoryRefs {
        refs: BTreeSet::from([about.to_string()]),
        ..ExistingMemoryRefs::default()
    };
    for (dimension, kind) in &facts.dimensions {
        existing.refs.insert(dimension.clone());
        existing.dimensions.insert(dimension.clone());
        if let Some(kind) = kind {
            let value = MemoryDimensionIdentity::parse(dimension)
                .map(|identity| identity.dimension_id().to_string())
                .unwrap_or_else(|| dimension.clone());
            existing.labels.insert((kind.clone(), value));
        }
    }
    existing
}

/// The point reads the translation of `command` will make against
/// `existing`: every ref a relation or an evidence names as written (a name
/// the registry rewrites becomes a dimension, which the catalogue already
/// holds), and every coordinate its entries leave the sequence of to the
/// kernel, normalized as the translation normalizes it.
pub(super) fn request(
    command: &MemoryIngestCommand,
    existing: &ExistingMemoryRefs,
) -> MemoryWriteFactsRequest {
    let mut request = MemoryWriteFactsRequest::new(command.about.clone());
    for relation in &command.memory.relations {
        request.refs.insert(relation.source_ref.clone());
        request.refs.insert(relation.target_ref.clone());
    }
    for evidence in &command.memory.evidence {
        request.refs.extend(evidence.supports.iter().cloned());
    }
    // A frontier is read only where a coordinate leaves its sequence to the
    // kernel: one the writer names never reads it, whatever else it holds.
    request.sequence_keys = coordinates(command, existing)
        .into_iter()
        .filter(|(_, sequence)| sequence.is_none())
        .map(|(key, _)| key)
        .collect();
    request
}

/// The frontiers of `loaded` once `command` is written: what the
/// translation assigns, coordinate by coordinate, from the frontiers it read.
pub(super) fn advanced(
    command: &MemoryIngestCommand,
    existing: &ExistingMemoryRefs,
    loaded: &BTreeMap<(String, String), u32>,
) -> BTreeMap<(String, String), u32> {
    let mut frontiers = loaded.clone();
    for (key, sequence) in coordinates(command, existing) {
        let Some(frontier) = frontiers.get_mut(&key) else {
            continue;
        };
        *frontier = match sequence {
            Some(sequence) => (*frontier).max(sequence),
            None => frontier.saturating_add(1),
        };
    }
    frontiers
}

/// Every coordinate the entries of `command` write, in order, normalized as
/// the translation normalizes it, with the sequence the writer named.
fn coordinates(
    command: &MemoryIngestCommand,
    existing: &ExistingMemoryRefs,
) -> Vec<((String, String), Option<u32>)> {
    let Ok(mut registry) = DimensionRegistry::new(&command.about, existing) else {
        return Vec::new();
    };
    for dimension in &command.memory.dimensions {
        // A dimension the translation refuses refuses the write before any
        // coordinate is read.
        if registry.declare(&dimension.kind, &dimension.id).is_err() {
            return Vec::new();
        }
    }
    command
        .memory
        .entries
        .iter()
        .flat_map(|entry| entry.coordinates.iter())
        .filter_map(|coordinate| normalize_coordinate(coordinate, "", "", &registry).ok())
        .map(|coordinate| {
            (
                (coordinate.dimension, coordinate.scope_id),
                coordinate.sequence,
            )
        })
        .collect()
}

/// The catalogue completed with what the store answered for `request`: the
/// refs that stand in the about and the frontier of every coordinate asked.
pub(super) fn answered(
    mut existing: ExistingMemoryRefs,
    facts: &MemoryWriteFacts,
) -> ExistingMemoryRefs {
    if !facts.exists {
        return existing;
    }
    existing.refs.extend(facts.present_refs.iter().cloned());
    for (key, frontier) in &facts.sequence_frontiers {
        if *frontier > 0 {
            existing.max_sequences.insert(key.clone(), *frontier);
        }
    }
    existing
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{MemoryCoordinateData, MemoryData, MemoryDimensionData, MemoryEntryData};

    fn coordinate(sequence: Option<u32>) -> MemoryCoordinateData {
        MemoryCoordinateData {
            dimension: "work".to_string(),
            scope_id: "work:main".to_string(),
            occurred_at: Some("2026-09-01T10:00:00Z".to_string()),
            observed_at: None,
            ingested_at: None,
            valid_from: None,
            valid_until: None,
            sequence,
            rank: None,
            metadata: Default::default(),
        }
    }

    fn command(sequences: &[Option<u32>]) -> MemoryIngestCommand {
        MemoryIngestCommand {
            about: "project:p".to_string(),
            memory: MemoryData {
                dimensions: vec![MemoryDimensionData {
                    id: "work:main".to_string(),
                    kind: "work".to_string(),
                    title: None,
                    metadata: Default::default(),
                }],
                entries: sequences
                    .iter()
                    .enumerate()
                    .map(|(index, sequence)| MemoryEntryData {
                        id: format!("project:p:e{index}"),
                        kind: "observation".to_string(),
                        text: "text".to_string(),
                        coordinates: vec![coordinate(*sequence)],
                        metadata: Default::default(),
                    })
                    .collect(),
                relations: Vec::new(),
                evidence: Vec::new(),
            },
            provenance: None,
            idempotency_key: "k".to_string(),
            dry_run: false,
            label_policy: Default::default(),
            receipt_context: None,
            default_observation_to_ingestion: false,
            neighborhood_review: None,
        }
    }

    #[test]
    fn only_coordinates_left_to_the_kernel_read_a_frontier() {
        let existing = ExistingMemoryRefs::default();
        assert!(
            request(&command(&[Some(4)]), &existing)
                .sequence_keys
                .is_empty()
        );
        let asked = request(&command(&[Some(4), None]), &existing).sequence_keys;
        assert_eq!(asked.len(), 1);
    }

    #[test]
    fn the_frontier_advances_as_the_translation_assigns() {
        let existing = ExistingMemoryRefs::default();
        let written = command(&[None, Some(9), None]);
        let key = request(&written, &existing)
            .sequence_keys
            .into_iter()
            .next()
            .expect("one key");
        let loaded = BTreeMap::from([(key.clone(), 3)]);
        assert_eq!(advanced(&written, &existing, &loaded)[&key], 10);
    }
}
