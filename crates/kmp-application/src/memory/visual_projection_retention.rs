//! Conservative retained-allocation accounting, not serialized size or RSS.
//! Charge vector capacities and string capacities before cloning. Tree entries
//! carry a 1 KiB allowance each for partially occupied B-tree nodes. The cache
//! also has a fixed slot limit; allocator bookkeeping and callers' returned
//! clones are not a process-wide memory limit.

use std::{collections::BTreeMap, mem::size_of};

use super::{VisualProjectionResult, visual_projection_identity::VisualProjectionIdentity};

pub(super) fn retained_bytes(
    identity: &VisualProjectionIdentity,
    result: &VisualProjectionResult,
) -> usize {
    let query = &identity.query;
    let dimensions = &query.dimensions;
    let mut bytes = 1024
        + size_of::<VisualProjectionResult>()
        + size_of::<VisualProjectionIdentity>()
        + identity.revision.as_str().len();
    bytes += query.about.capacity()
        + query.from.capacity()
        + query.to.capacity()
        + optional(&query.cursor);
    for set in [
        dimensions.dimensions(),
        dimensions.abouts(),
        dimensions.scope_ids(),
    ] {
        bytes += set.len() * 1024 + set.iter().map(String::capacity).sum::<usize>();
    }
    bytes += dimensions.selectors().len() * 1024;
    for selector in dimensions.selectors() {
        bytes += selector.key().len()
            + selector.values().len() * 1024
            + selector
                .values()
                .iter()
                .map(|value| value.len())
                .sum::<usize>();
    }
    bytes += result.contract.capacity()
        + result.about.capacity()
        + result.range.from.capacity()
        + result.range.to.capacity()
        + result.content_hash.capacity()
        + optional(&result.page.next_cursor);
    bytes += strings(&result.included_dimensions)
        + strings(&result.missing_dimensions)
        + strings(&result.missing)
        + counts(&result.by_kind);
    bytes += slots(&result.bins);
    for bin in &result.bins {
        bytes += bin.dimension.capacity()
            + bin.scope_id.capacity()
            + bin.from.capacity()
            + bin.to.capacity()
            + counts(&bin.by_kind);
    }
    bytes += slots(&result.clusters);
    for cluster in &result.clusters {
        bytes += cluster.dimension.capacity()
            + cluster.scope_id.capacity()
            + cluster.from.capacity()
            + cluster.to.capacity()
            + strings(&cluster.refs)
            + counts(&cluster.by_kind);
    }
    bytes += slots(&result.entries);
    for entry in &result.entries {
        bytes += entry.ref_id.capacity()
            + entry.kind.capacity()
            + entry.text.capacity()
            + slots(&entry.coordinates);
        for coordinate in &entry.coordinates {
            bytes += coordinate.dimension.capacity() + coordinate.scope_id.capacity();
            for value in [
                &coordinate.occurred_at,
                &coordinate.observed_at,
                &coordinate.ingested_at,
                &coordinate.valid_from,
                &coordinate.valid_until,
                &coordinate.method,
                &coordinate.why,
                &coordinate.motivation,
            ] {
                bytes += optional(value);
            }
        }
    }
    bytes += slots(&result.relations);
    for relation in &result.relations {
        bytes += relation.from.capacity()
            + relation.to.capacity()
            + relation.rel.capacity()
            + relation.class.capacity();
        for value in [
            &relation.why,
            &relation.evidence,
            &relation.confidence,
            &relation.method,
        ] {
            bytes += optional(value);
        }
        if let Some(clocks) = &relation.clocks {
            for value in [
                &clocks.occurred_at,
                &clocks.observed_at,
                &clocks.ingested_at,
                &clocks.valid_from,
                &clocks.valid_until,
            ] {
                bytes += optional(value);
            }
        }
    }
    bytes += slots(&result.metrics);
    for metric in &result.metrics {
        bytes += metric.name.capacity() + metric.unit.capacity() + metric.scope.capacity();
    }
    bytes += slots(&result.labels);
    for label in &result.labels {
        bytes += label.dimension.capacity()
            + label.scope_id.capacity()
            + label.value.capacity()
            + optional(&label.last_observed_at);
    }
    bytes
}

fn optional(value: &Option<String>) -> usize {
    value.as_ref().map_or(0, String::capacity)
}

fn slots<T>(values: &Vec<T>) -> usize {
    values.capacity() * size_of::<T>()
}

fn strings(values: &Vec<String>) -> usize {
    slots(values) + values.iter().map(String::capacity).sum::<usize>()
}

fn counts(values: &BTreeMap<String, usize>) -> usize {
    values.len() * 1024 + values.keys().map(String::capacity).sum::<usize>()
}
