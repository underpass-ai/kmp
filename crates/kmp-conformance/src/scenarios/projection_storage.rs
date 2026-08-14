//! Port-level storage semantics: what `ProjectionWriter` mutations must make
//! observable through the graph, detail, relationship, and about-index ports.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use kmp_domain::{
    GraphNeighborhoodReader, MemoryAboutIndexReader, NodeDetailProjection, NodeDetailReader,
    NodeProjection, NodeRelationProjection, NodeRelationshipReader, ProjectionMutation,
    ProjectionWriter, RelationExplanation, RelationSemanticClass,
};

use crate::ConformanceBackendFactory;

fn node(node_id: &str, node_kind: &str, title: &str) -> NodeProjection {
    NodeProjection {
        node_id: node_id.to_string(),
        node_kind: node_kind.to_string(),
        title: title.to_string(),
        summary: format!("Summary for {node_id}"),
        status: "ACTIVE".to_string(),
        labels: vec!["conformance".to_string()],
        properties: BTreeMap::new(),
        provenance: None,
    }
}

fn structural_relation(source: &str, target: &str, relation_type: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: source.to_string(),
        target_node_id: target.to_string(),
        relation_type: relation_type.to_string(),
        explanation: RelationExplanation::new(RelationSemanticClass::Structural)
            .with_rationale("conformance structural relation"),
    }))
}

fn relation_keys(relations: &[NodeRelationProjection]) -> BTreeSet<(String, String, String)> {
    relations
        .iter()
        .map(|relation| {
            (
                relation.source_node_id.clone(),
                relation.target_node_id.clone(),
                relation.relation_type.clone(),
            )
        })
        .collect()
}

fn neighbor_ids(neighbors: &[NodeProjection]) -> BTreeSet<String> {
    neighbors
        .iter()
        .map(|neighbor| neighbor.node_id.clone())
        .collect()
}

pub async fn write_read_coherence_projected_nodes_are_readable(
    factory: &impl ConformanceBackendFactory,
) {
    let backend = factory.fresh().await;
    let writer = backend.projection_writer();

    writer
        .apply_mutations(vec![
            ProjectionMutation::UpsertNode(node("conf:root", "case", "Root")),
            ProjectionMutation::UpsertNode(node("conf:leaf", "claim", "Leaf")),
            structural_relation("conf:root", "conf:leaf", "records"),
            ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
                node_id: "conf:leaf".to_string(),
                detail: "leaf detail".to_string(),
                content_hash: "hash-1".to_string(),
                revision: 1,
            }),
        ])
        .await
        .expect("projected mutations should apply");

    let neighborhood = backend
        .graph
        .load_neighborhood("conf:root", 1)
        .await
        .expect("neighborhood read should succeed")
        .expect("projected root must be readable after write");
    assert_eq!(neighborhood.root.node_id, "conf:root");
    assert_eq!(neighborhood.root.node_kind, "case");
    assert_eq!(
        neighbor_ids(&neighborhood.neighbors),
        BTreeSet::from(["conf:leaf".to_string()]),
        "projected neighbor must be readable after write"
    );
    assert_eq!(
        relation_keys(&neighborhood.relations),
        BTreeSet::from([(
            "conf:root".to_string(),
            "conf:leaf".to_string(),
            "records".to_string()
        )])
    );

    let detail = backend
        .detail
        .load_node_detail("conf:leaf")
        .await
        .expect("detail read should succeed")
        .expect("projected detail must be readable after write");
    assert_eq!(detail.detail, "leaf detail");
    assert_eq!(detail.revision, 1);

    let missing = backend
        .graph
        .load_neighborhood("conf:absent", 1)
        .await
        .expect("missing root read should succeed");
    assert!(missing.is_none(), "unknown roots must read as None");
}

pub async fn ensure_node_preserves_existing_upsert_overwrites(
    factory: &impl ConformanceBackendFactory,
) {
    let backend = factory.fresh().await;
    let writer = backend.projection_writer();

    writer
        .apply_mutations(vec![ProjectionMutation::UpsertNode(node(
            "conf:node",
            "claim",
            "Original",
        ))])
        .await
        .expect("initial upsert should apply");

    writer
        .apply_mutations(vec![ProjectionMutation::EnsureNode(node(
            "conf:node",
            "placeholder",
            "Ensured",
        ))])
        .await
        .expect("ensure should apply");
    let ensured = backend
        .graph
        .load_neighborhood("conf:node", 1)
        .await
        .expect("read should succeed")
        .expect("node must exist");
    assert_eq!(
        ensured.root.title, "Original",
        "EnsureNode must not overwrite an existing node"
    );

    writer
        .apply_mutations(vec![ProjectionMutation::UpsertNode(node(
            "conf:node",
            "decision",
            "Replaced",
        ))])
        .await
        .expect("upsert should apply");
    let replaced = backend
        .graph
        .load_neighborhood("conf:node", 1)
        .await
        .expect("read should succeed")
        .expect("node must exist");
    assert_eq!(
        replaced.root.title, "Replaced",
        "UpsertNode must overwrite an existing node"
    );
    assert_eq!(replaced.root.node_kind, "decision");
}

pub async fn relation_upsert_creates_placeholder_endpoints_and_is_idempotent(
    factory: &impl ConformanceBackendFactory,
) {
    let backend = factory.fresh().await;
    let writer = backend.projection_writer();

    writer
        .apply_mutations(vec![structural_relation(
            "conf:early-source",
            "conf:early-target",
            "supports",
        )])
        .await
        .expect("relation before nodes should apply");

    let neighborhood = backend
        .graph
        .load_neighborhood("conf:early-source", 1)
        .await
        .expect("read should succeed")
        .expect("relation upsert must materialize placeholder endpoints");
    assert_eq!(
        neighborhood.root.node_kind, "placeholder",
        "unmaterialized relation source must be a placeholder node"
    );
    assert_eq!(
        neighbor_ids(&neighborhood.neighbors),
        BTreeSet::from(["conf:early-target".to_string()])
    );

    writer
        .apply_mutations(vec![
            structural_relation("conf:early-source", "conf:early-target", "supports"),
            structural_relation("conf:early-source", "conf:early-target", "supports"),
        ])
        .await
        .expect("repeated relation upsert should apply");
    let repeated = backend
        .graph
        .load_neighborhood("conf:early-source", 1)
        .await
        .expect("read should succeed")
        .expect("root must exist");
    assert_eq!(
        repeated.relations.len(),
        1,
        "relations must be keyed by (source, target, type); re-upsert must not duplicate"
    );

    writer
        .apply_mutations(vec![ProjectionMutation::UpsertNode(node(
            "conf:early-source",
            "claim",
            "Materialized",
        ))])
        .await
        .expect("late node materialization should apply");
    let materialized = backend
        .graph
        .load_neighborhood("conf:early-source", 1)
        .await
        .expect("read should succeed")
        .expect("root must exist");
    assert_eq!(
        materialized.root.node_kind, "claim",
        "late UpsertNode must replace the placeholder"
    );
}

pub async fn neighborhood_traversal_is_depth_bounded_and_directed(
    factory: &impl ConformanceBackendFactory,
) {
    let backend = factory.fresh().await;
    let writer = backend.projection_writer();

    writer
        .apply_mutations(vec![
            ProjectionMutation::UpsertNode(node("conf:a", "case", "A")),
            ProjectionMutation::UpsertNode(node("conf:b", "claim", "B")),
            ProjectionMutation::UpsertNode(node("conf:c", "claim", "C")),
            ProjectionMutation::UpsertNode(node("conf:d", "claim", "D")),
            ProjectionMutation::UpsertNode(node("conf:upstream", "claim", "Upstream")),
            structural_relation("conf:a", "conf:b", "records"),
            structural_relation("conf:b", "conf:c", "supports"),
            structural_relation("conf:c", "conf:d", "supports"),
            structural_relation("conf:upstream", "conf:a", "supports"),
        ])
        .await
        .expect("chain should apply");

    let depth_one = backend
        .graph
        .load_neighborhood("conf:a", 1)
        .await
        .expect("read should succeed")
        .expect("root must exist");
    assert_eq!(
        neighbor_ids(&depth_one.neighbors),
        BTreeSet::from(["conf:b".to_string()]),
        "depth 1 must stop after one outgoing hop"
    );

    let depth_two = backend
        .graph
        .load_neighborhood("conf:a", 2)
        .await
        .expect("read should succeed")
        .expect("root must exist");
    assert_eq!(
        neighbor_ids(&depth_two.neighbors),
        BTreeSet::from(["conf:b".to_string(), "conf:c".to_string()]),
        "depth 2 must include exactly the two-hop frontier"
    );
    assert!(
        !neighbor_ids(&depth_two.neighbors).contains("conf:upstream"),
        "traversal must follow outgoing direction only"
    );
    assert_eq!(
        relation_keys(&depth_two.relations),
        BTreeSet::from([
            (
                "conf:a".to_string(),
                "conf:b".to_string(),
                "records".to_string()
            ),
            (
                "conf:b".to_string(),
                "conf:c".to_string(),
                "supports".to_string()
            ),
        ]),
        "relations must cover exactly the edges among selected nodes"
    );

    let depth_three = backend
        .graph
        .load_neighborhood("conf:a", 3)
        .await
        .expect("read should succeed")
        .expect("root must exist");
    assert_eq!(
        neighbor_ids(&depth_three.neighbors),
        BTreeSet::from([
            "conf:b".to_string(),
            "conf:c".to_string(),
            "conf:d".to_string()
        ])
    );
}

pub async fn context_path_resolves_shortest_path_with_target_subtree(
    factory: &impl ConformanceBackendFactory,
) {
    let backend = factory.fresh().await;
    let writer = backend.projection_writer();

    writer
        .apply_mutations(vec![
            ProjectionMutation::UpsertNode(node("conf:root", "case", "Root")),
            ProjectionMutation::UpsertNode(node("conf:mid", "decision", "Mid")),
            ProjectionMutation::UpsertNode(node("conf:target", "claim", "Target")),
            ProjectionMutation::UpsertNode(node("conf:child", "claim", "Child")),
            ProjectionMutation::UpsertNode(node("conf:isolated", "claim", "Isolated")),
            structural_relation("conf:root", "conf:mid", "records"),
            structural_relation("conf:mid", "conf:target", "supports"),
            structural_relation("conf:target", "conf:child", "supports"),
            structural_relation("conf:isolated", "conf:root", "supports"),
        ])
        .await
        .expect("path graph should apply");

    let path = backend
        .graph
        .load_context_path("conf:root", "conf:target", 1)
        .await
        .expect("path read should succeed")
        .expect("a directed path must resolve");
    assert_eq!(
        path.path_node_ids,
        vec![
            "conf:root".to_string(),
            "conf:mid".to_string(),
            "conf:target".to_string()
        ],
        "path_node_ids must list the shortest directed path in order"
    );
    let ids = neighbor_ids(&path.neighbors);
    assert!(
        ids.contains("conf:child"),
        "target subtree at subtree_depth=1 must ride along the path"
    );
    assert!(
        !ids.contains("conf:isolated"),
        "nodes outside path and subtree must not be selected"
    );

    let unreachable = backend
        .graph
        .load_context_path("conf:target", "conf:root", 1)
        .await
        .expect("path read should succeed");
    assert!(
        unreachable.is_none(),
        "no directed path must resolve as None"
    );

    let missing_target = backend
        .graph
        .load_context_path("conf:root", "conf:absent", 1)
        .await
        .expect("path read should succeed");
    assert!(missing_target.is_none(), "missing target must be None");
}

pub async fn node_relationships_split_incoming_and_outgoing(
    factory: &impl ConformanceBackendFactory,
) {
    let backend = factory.fresh().await;
    let writer = backend.projection_writer();

    writer
        .apply_mutations(vec![
            ProjectionMutation::UpsertNode(node("conf:hub", "claim", "Hub")),
            ProjectionMutation::UpsertNode(node("conf:in", "evidence", "In")),
            ProjectionMutation::UpsertNode(node("conf:out", "claim", "Out")),
            structural_relation("conf:in", "conf:hub", "supports"),
            structural_relation("conf:hub", "conf:out", "supports"),
        ])
        .await
        .expect("relationship graph should apply");

    let relationships = backend
        .graph
        .load_node_relationships("conf:hub")
        .await
        .expect("relationship read should succeed")
        .expect("existing node must report relationships");
    assert_eq!(
        relation_keys(&relationships.incoming),
        BTreeSet::from([(
            "conf:in".to_string(),
            "conf:hub".to_string(),
            "supports".to_string()
        )])
    );
    assert_eq!(
        relation_keys(&relationships.outgoing),
        BTreeSet::from([(
            "conf:hub".to_string(),
            "conf:out".to_string(),
            "supports".to_string()
        )])
    );

    let missing = backend
        .graph
        .load_node_relationships("conf:absent")
        .await
        .expect("relationship read should succeed");
    assert!(missing.is_none(), "unknown nodes must read as None");
}

pub async fn about_index_lists_anchors_and_filters_by_dimension(
    factory: &impl ConformanceBackendFactory,
) {
    let backend = factory.fresh().await;
    let writer = backend.projection_writer();

    let timeline_dimension = "about:question:a:dimension:timeline:sessions";
    writer
        .apply_mutations(vec![
            ProjectionMutation::UpsertNode(node("question:a", "memory_anchor", "A")),
            ProjectionMutation::UpsertNode(node("question:b", "memory_anchor", "B")),
            ProjectionMutation::UpsertNode(node("conf:not-anchor", "claim", "Not anchor")),
            ProjectionMutation::UpsertNode(node(
                timeline_dimension,
                "memory_dimension",
                "Timeline",
            )),
            structural_relation("question:a", timeline_dimension, "has_dimension"),
        ])
        .await
        .expect("anchors should apply");

    let abouts = backend
        .graph
        .list_memory_abouts()
        .await
        .expect("about index read should succeed");
    assert_eq!(
        abouts,
        vec!["question:a".to_string(), "question:b".to_string()],
        "about index must list memory_anchor nodes sorted, and only those"
    );

    let by_dimension = backend
        .graph
        .list_memory_abouts_by_dimensions(&["timeline:sessions".to_string()])
        .await
        .expect("filtered about index read should succeed");
    assert_eq!(
        by_dimension,
        vec!["question:a".to_string()],
        "dimension filter must match namespaced dimension suffixes"
    );

    let by_exact_id = backend
        .graph
        .list_memory_abouts_by_dimensions(&[timeline_dimension.to_string()])
        .await
        .expect("filtered about index read should succeed");
    assert_eq!(by_exact_id, vec!["question:a".to_string()]);

    let no_match = backend
        .graph
        .list_memory_abouts_by_dimensions(&["timeline:other".to_string()])
        .await
        .expect("filtered about index read should succeed");
    assert!(no_match.is_empty());
}

pub async fn node_detail_upsert_is_last_write_wins(factory: &impl ConformanceBackendFactory) {
    let backend = factory.fresh().await;
    let writer = backend.projection_writer();

    for (revision, text) in [(1u64, "first"), (2u64, "second")] {
        writer
            .apply_mutations(vec![ProjectionMutation::UpsertNodeDetail(
                NodeDetailProjection {
                    node_id: "conf:detail".to_string(),
                    detail: text.to_string(),
                    content_hash: format!("hash-{revision}"),
                    revision,
                },
            )])
            .await
            .expect("detail upsert should apply");
    }

    let detail = backend
        .detail
        .load_node_detail("conf:detail")
        .await
        .expect("detail read should succeed")
        .expect("detail must exist");
    assert_eq!(detail.detail, "second", "detail upsert is last-write-wins");
    assert_eq!(detail.revision, 2);

    let batch = backend
        .detail
        .load_node_details_batch(vec!["conf:detail".to_string(), "conf:absent".to_string()])
        .await
        .expect("batch read should succeed");
    assert_eq!(batch.len(), 2);
    assert!(batch[0].is_some());
    assert!(
        batch[1].is_none(),
        "batch reads must preserve per-id None for unknown nodes"
    );
}
