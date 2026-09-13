use super::*;
use crate::memory::*;
use kmp_domain::{DimensionSelection, GraphReadRevision, TemporalAxis};

fn identity(bins: usize, revision: &str) -> VisualProjectionIdentity {
    VisualProjectionIdentity {
        revision: GraphReadRevision::new(revision).expect("revision"),
        query: VisualProjectionQuery {
            about: "project:fixture".into(),
            from: "2026-09-01T00:00:00Z".into(),
            to: "2026-10-01T00:00:00Z".into(),
            axis: TemporalAxis::Observed,
            dimensions: DimensionSelection::all(),
            level_of_detail: VisualLevelOfDetail::Atlas,
            bin_count: bins,
            page_entries: 100,
            cursor: None,
            depth: 8,
        },
    }
}

fn projection() -> VisualProjectionResult {
    VisualProjectionResult {
        contract: "kmp.visual.projection.v1".into(),
        about: "project:fixture".into(),
        axis: TemporalAxisView::Observed,
        level_of_detail: VisualLevelOfDetail::Atlas,
        range: VisualRange {
            from: "2026-09-01T00:00:00Z".into(),
            to: "2026-10-01T00:00:00Z".into(),
        },
        bins: vec![],
        clusters: vec![],
        entries: vec![],
        by_kind: Default::default(),
        relations: vec![],
        metrics: vec![],
        labels: vec![],
        included_dimensions: vec![],
        missing_dimensions: vec![],
        revision: 1,
        content_hash: "fixture".into(),
        page: VisualProjectionPage {
            returned: 0,
            total: 0,
            has_more: false,
            next_cursor: None,
        },
        truncated: false,
        missing: vec![],
    }
}

#[test]
fn least_recently_used_queries_are_evicted_and_results_remain_owned() {
    let mut cache = VisualProjectionCache::default();
    let mut result = projection();
    for i in 0..MAX_ENTRIES {
        cache.put(identity(i, "store:a:1"), &result);
    }
    let retained = cache
        .get(&identity(0, "store:a:1"))
        .expect("refresh oldest");
    cache.put(identity(100, "store:a:1"), &result);
    assert!(cache.get(&identity(1, "store:a:1")).is_none());
    assert!(cache.get(&identity(0, "store:a:1")).is_some());
    assert!(
        cache.get(&identity(0, "store:b:1")).is_none(),
        "stores never alias"
    );
    result.missing.push("caller change".into());
    assert!(retained.missing.is_empty());
    assert_eq!(cache.entries.len(), MAX_ENTRIES);
    assert_eq!(
        cache.retained,
        cache.entries.iter().map(|entry| entry.2).sum::<usize>()
    );
}

#[test]
fn byte_limit_evicts_before_slot_limit_and_oversized_keys_or_results_are_not_retained() {
    let mut cache = VisualProjectionCache::default();
    let mut result = projection();
    result.missing.push("x".repeat(MAX_RETAINED_BYTES / 3));
    for i in 0..MAX_ENTRIES {
        cache.put(identity(i, "store:a:1"), &result);
        assert!(cache.retained <= MAX_RETAINED_BYTES);
    }
    assert_eq!(cache.entries.len(), 2);
    let before = cache.retained;
    let mut oversized = identity(100, "store:a:1");
    oversized.query.cursor = Some("x".repeat(MAX_RETAINED_BYTES));
    cache.put(oversized.clone(), &result);
    assert!(cache.get(&oversized).is_none());
    assert_eq!(cache.retained, before);
    result.missing.push("x".repeat(MAX_RETAINED_BYTES));
    cache.put(identity(101, "store:a:1"), &result);
    assert!(cache.get(&identity(101, "store:a:1")).is_none());
    assert_eq!(cache.retained, before);
    cache.put(identity(0, "store:a:2"), &projection());
    assert_eq!(cache.entries.len(), 1, "old revisions are released");
    assert!(cache.retained < before);
}

#[test]
fn memory_charge_includes_spare_capacity_and_nested_relation_clocks() {
    let key = identity(1, "store:a:1");
    let mut result = projection();
    let before = retained_bytes(&key, &result);
    result.entries.reserve(100);
    assert!(retained_bytes(&key, &result) >= before + 100 * std::mem::size_of::<VisualEntry>());
    let before = retained_bytes(&key, &result);
    result.relations.push(VisualRelation {
        clocks: Some(MemoryRelationClocks {
            observed_at: Some("x".repeat(8192)),
            ..Default::default()
        }),
        from: "a".into(),
        to: "b".into(),
        rel: "supports".into(),
        class: "evidential".into(),
        why: None,
        evidence: None,
        confidence: None,
        method: None,
    });
    assert!(retained_bytes(&key, &result) >= before + 8192);
}
