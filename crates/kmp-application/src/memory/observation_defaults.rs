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
    }
    resolved
}
