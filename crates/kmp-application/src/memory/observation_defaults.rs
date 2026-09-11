//! Resolve the semantic writer's implicit observation at the ingestion boundary.
//! Canonical ingest opts in explicitly; replay digests use the unresolved command.

use super::MemoryIngestCommand;

pub(super) fn resolve(command: &MemoryIngestCommand, ingested_at: &str) -> MemoryIngestCommand {
    let mut resolved = command.clone();
    if let Some(provenance) = &mut resolved.provenance {
        provenance
            .observed_at
            .get_or_insert_with(|| ingested_at.to_owned());
    }
    for coordinate in resolved
        .memory
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.coordinates)
        .chain(
            resolved
                .memory
                .relations
                .iter_mut()
                .filter(|relation| relation.semantic_class.trim() == "structural")
                .filter_map(|relation| relation.coordinate.as_mut()),
        )
    {
        // Search-summary updates carry already stored coordinates. An unknown
        // historical observation remains unknown, even if this write is new.
        if coordinate.ingested_at.is_none() {
            coordinate
                .observed_at
                .get_or_insert_with(|| ingested_at.to_owned());
        }
    }
    for evidence in &mut resolved.memory.evidence {
        evidence.time.get_or_insert_with(|| ingested_at.to_owned());
        if let Some(clocks) = &mut evidence.support_clocks
            && clocks.ingested_at.is_none()
        {
            clocks
                .observed_at
                .get_or_insert_with(|| ingested_at.to_owned());
        }
    }
    // Explicit proof clock objects carry the semantic member's own declaration.
    // Absence still uses canonical packet inheritance in namespaced_memory.
    // Restored clocks, including unknown historical observations, stay intact.
    for clocks in resolved
        .memory
        .relations
        .iter_mut()
        .filter(|relation| relation.semantic_class.trim() != "structural")
        .filter(|relation| {
            !relation.coordinate.as_ref().is_some_and(|coordinate| {
                coordinate.observed_at.is_some() || coordinate.ingested_at.is_some()
            })
        })
        .filter_map(|relation| relation.clocks.as_mut())
        .filter(|clocks| clocks.ingested_at.is_none())
    {
        clocks
            .observed_at
            .get_or_insert_with(|| ingested_at.to_owned());
    }
    resolved
}
