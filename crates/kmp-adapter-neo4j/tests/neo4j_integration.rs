use std::collections::BTreeMap;
use std::error::Error;
use std::time::Duration;

use kmp_adapter_neo4j::Neo4jProjectionReader;
use kmp_domain::{RelationExplanation, RelationSemanticClass};
use kmp_ports::{
    GraphNeighborhoodReader, NodeProjection, NodeRelationProjection, NodeRelationshipReader,
    ProjectionMutation, ProjectionWriter,
};
use kmp_testkit::ensure_testcontainers_runtime;
use neo4rs::{Graph, query};
use testcontainers::{
    GenericImage, ImageExt,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};
use tokio::time::{sleep, timeout};

const NEO4J_INTERNAL_PORT: u16 = 7687;
const NEO4J_IMAGE: &str = "docker.io/neo4j";
const NEO4J_TAG: &str = "5.26.0-community";
const NEO4J_PASSWORD: &str = "underpass-test-password";
const TEST_TIMEOUT: Duration = Duration::from_secs(45);
const CONNECT_RETRY_ATTEMPTS: usize = 15;
const CONNECT_RETRY_DELAY: Duration = Duration::from_secs(1);

#[tokio::test]
async fn load_neighborhood_respects_directed_depth() -> Result<(), Box<dyn Error + Send + Sync>> {
    run_with_timeout(async {
        let container = start_neo4j_container().await?;
        let host = container.get_host().await?;
        let port = container.get_host_port_ipv4(NEO4J_INTERNAL_PORT).await?;
        let graph =
            connect_with_retry(format!("neo4j://{host}:{port}"), "neo4j", NEO4J_PASSWORD).await?;
        graph.run(query("MATCH (n) DETACH DELETE n")).await?;

        let store =
            Neo4jProjectionReader::new(format!("neo4j://neo4j:{NEO4J_PASSWORD}@{host}:{port}"))?;
        store
            .apply_mutations(vec![
                ProjectionMutation::UpsertNode(NodeProjection {
                    node_id: "node-root".to_string(),
                    node_kind: "capability".to_string(),
                    title: "Projection kernel".to_string(),
                    summary: "Root summary".to_string(),
                    status: "ACTIVE".to_string(),
                    labels: vec!["projection".to_string()],
                    properties: BTreeMap::from([
                        ("created_by".to_string(), "planner".to_string()),
                        ("token_budget_hint".to_string(), "8192".to_string()),
                    ]),
                    provenance: None,
                }),
                ProjectionMutation::UpsertNode(NodeProjection {
                    node_id: "decision-1".to_string(),
                    node_kind: "decision".to_string(),
                    title: "Adopt CQRS".to_string(),
                    summary: "Decision summary".to_string(),
                    status: "ACCEPTED".to_string(),
                    labels: vec!["decision".to_string()],
                    properties: BTreeMap::from([("owner".to_string(), "architect".to_string())]),
                    provenance: None,
                }),
                ProjectionMutation::UpsertNode(NodeProjection {
                    node_id: "task-1".to_string(),
                    node_kind: "node".to_string(),
                    title: "Move query side".to_string(),
                    summary: "Task summary".to_string(),
                    status: "READY".to_string(),
                    labels: vec!["work-item".to_string()],
                    properties: BTreeMap::from([("priority".to_string(), "5".to_string())]),
                    provenance: None,
                }),
                ProjectionMutation::UpsertNode(NodeProjection {
                    node_id: "blocker-1".to_string(),
                    node_kind: "risk".to_string(),
                    title: "Inbound blocker".to_string(),
                    summary: "Should not be reachable from the root".to_string(),
                    status: "OPEN".to_string(),
                    labels: vec!["risk".to_string()],
                    properties: BTreeMap::new(),
                    provenance: None,
                }),
                ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
                    source_node_id: "node-root".to_string(),
                    target_node_id: "decision-1".to_string(),
                    relation_type: "records".to_string(),
                    explanation: RelationExplanation::new(RelationSemanticClass::Structural)
                        .with_sequence(1),
                })),
                ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
                    source_node_id: "decision-1".to_string(),
                    target_node_id: "task-1".to_string(),
                    relation_type: "informs".to_string(),
                    explanation: RelationExplanation::new(RelationSemanticClass::Motivational)
                        .with_sequence(2)
                        .with_rationale("the task implements the accepted decision"),
                })),
                ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
                    source_node_id: "blocker-1".to_string(),
                    target_node_id: "node-root".to_string(),
                    relation_type: "blocks".to_string(),
                    explanation: RelationExplanation::new(RelationSemanticClass::Constraint),
                })),
            ])
            .await?;

        let shallow_neighborhood = store
            .load_neighborhood("node-root", 1)
            .await?
            .expect("seeded neighborhood should load");

        assert_eq!(shallow_neighborhood.root.node_id, "node-root");
        assert_eq!(shallow_neighborhood.root.title, "Projection kernel");
        assert_eq!(
            shallow_neighborhood.root.properties["created_by"],
            "planner"
        );
        assert_eq!(shallow_neighborhood.neighbors.len(), 1);
        assert!(
            shallow_neighborhood
                .neighbors
                .iter()
                .any(|node| node.node_id == "decision-1" && node.node_kind == "decision")
        );
        assert!(
            shallow_neighborhood
                .neighbors
                .iter()
                .all(|node| node.node_id != "task-1" && node.node_id != "blocker-1")
        );
        assert_eq!(shallow_neighborhood.relations.len(), 1);
        assert!(shallow_neighborhood.relations.iter().any(|relation| {
            relation.source_node_id == "node-root"
                && relation.target_node_id == "decision-1"
                && relation.relation_type == "records"
        }));

        let deep_neighborhood = store
            .load_neighborhood("node-root", 3)
            .await?
            .expect("seeded deep neighborhood should load");

        assert_eq!(deep_neighborhood.neighbors.len(), 2);
        assert!(
            deep_neighborhood
                .neighbors
                .iter()
                .any(|node| node.node_id == "task-1" && node.status == "READY")
        );
        assert!(
            deep_neighborhood
                .neighbors
                .iter()
                .all(|node| node.node_id != "blocker-1")
        );
        assert_eq!(deep_neighborhood.relations.len(), 2);
        assert!(deep_neighborhood.relations.iter().any(|relation| {
            relation.source_node_id == "decision-1"
                && relation.target_node_id == "task-1"
                && relation.relation_type == "informs"
                && relation.explanation.rationale()
                    == Some("the task implements the accepted decision")
        }));
        assert!(deep_neighborhood.relations.iter().all(|relation| {
            !(relation.source_node_id == "blocker-1"
                && relation.target_node_id == "node-root"
                && relation.relation_type == "blocks")
        }));

        let direct_relationships = store
            .load_node_relationships("node-root")
            .await?
            .expect("seeded node should load direct relationships");
        assert!(direct_relationships.outgoing.iter().any(|relation| {
            relation.source_node_id == "node-root"
                && relation.target_node_id == "decision-1"
                && relation.relation_type == "records"
                && relation.explanation.sequence() == Some(1)
        }));
        assert!(direct_relationships.incoming.iter().any(|relation| {
            relation.source_node_id == "blocker-1"
                && relation.target_node_id == "node-root"
                && relation.relation_type == "blocks"
                && relation.explanation.semantic_class() == &RelationSemanticClass::Constraint
        }));

        Ok(())
    })
    .await
}

#[tokio::test]
async fn apply_mutations_persists_generic_nodes_and_relations()
-> Result<(), Box<dyn Error + Send + Sync>> {
    run_with_timeout(async {
        let container = start_neo4j_container().await?;
        let host = container.get_host().await?;
        let port = container.get_host_port_ipv4(NEO4J_INTERNAL_PORT).await?;
        let graph =
            connect_with_retry(format!("neo4j://{host}:{port}"), "neo4j", NEO4J_PASSWORD).await?;
        graph.run(query("MATCH (n) DETACH DELETE n")).await?;

        let store =
            Neo4jProjectionReader::new(format!("neo4j://neo4j:{NEO4J_PASSWORD}@{host}:{port}"))?;
        store
            .apply_mutations(vec![
                ProjectionMutation::UpsertNode(NodeProjection {
                    node_id: "node-123".to_string(),
                    node_kind: "capability".to_string(),
                    title: "Projection consumer foundation".to_string(),
                    summary: "Node centric projection input".to_string(),
                    status: "ACTIVE".to_string(),
                    labels: vec!["projection".to_string(), "foundation".to_string()],
                    properties: BTreeMap::from([("phase".to_string(), "build".to_string())]),
                    provenance: None,
                }),
                ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
                    source_node_id: "node-123".to_string(),
                    target_node_id: "node-122".to_string(),
                    relation_type: "depends_on".to_string(),
                    explanation: RelationExplanation::new(RelationSemanticClass::Constraint)
                        .with_rationale("the capability relies on an upstream dependency"),
                })),
            ])
            .await?;

        let node_row = single_row(
            &graph,
            query(
                "
MATCH (node:ProjectionNode {node_id: $node_id})
RETURN node.node_kind AS node_kind,
       node.title AS title,
       node.status AS status,
       node.node_labels AS node_labels,
       node.properties_json AS properties_json
                ",
            )
            .param("node_id", "node-123"),
        )
        .await?;

        let relation_row = single_row(
            &graph,
            query(
                "
MATCH (:ProjectionNode {node_id: $source_node_id})-[edge:RELATED_TO {relation_type: $relation_type}]->(:ProjectionNode {node_id: $target_node_id})
RETURN count(edge) AS edge_count,
       edge.properties_json AS properties_json
                ",
            )
            .param("source_node_id", "node-123")
            .param("target_node_id", "node-122")
            .param("relation_type", "depends_on"),
        )
        .await?;

        let node_kind: String = node_row.get("node_kind")?;
        let title: String = node_row.get("title")?;
        let status: String = node_row.get("status")?;
        let node_labels: Vec<String> = node_row.get("node_labels")?;
        let properties_json: String = node_row.get("properties_json")?;
        let edge_count: i64 = relation_row.get("edge_count")?;
        let edge_properties_json: String = relation_row.get("properties_json")?;

        assert_eq!(node_kind, "capability");
        assert_eq!(title, "Projection consumer foundation");
        assert_eq!(status, "ACTIVE");
        assert_eq!(
            node_labels,
            vec!["projection".to_string(), "foundation".to_string()]
        );
        assert!(properties_json.contains("\"phase\":\"build\""));
        assert_eq!(edge_count, 1);
        assert!(edge_properties_json.contains("\"semantic_class\":\"constraint\""));
        assert!(edge_properties_json.contains("\"rationale\":\"the capability relies on an upstream dependency\""));

        Ok(())
    })
    .await
}

#[tokio::test]
async fn relation_only_mutation_creates_explicit_placeholder_nodes()
-> Result<(), Box<dyn Error + Send + Sync>> {
    run_with_timeout(async {
        let container = start_neo4j_container().await?;
        let host = container.get_host().await?;
        let port = container.get_host_port_ipv4(NEO4J_INTERNAL_PORT).await?;
        let graph =
            connect_with_retry(format!("neo4j://{host}:{port}"), "neo4j", NEO4J_PASSWORD).await?;
        graph.run(query("MATCH (n) DETACH DELETE n")).await?;

        let store =
            Neo4jProjectionReader::new(format!("neo4j://neo4j:{NEO4J_PASSWORD}@{host}:{port}"))?;
        store
            .apply_mutations(vec![ProjectionMutation::UpsertNodeRelation(Box::new(
                NodeRelationProjection {
                    source_node_id: "decision-missing".to_string(),
                    target_node_id: "finding-missing".to_string(),
                    relation_type: "ADDRESSES".to_string(),
                    explanation: RelationExplanation::new(RelationSemanticClass::Causal)
                        .with_rationale("decision addresses finding"),
                },
            ))])
            .await?;

        let source_row = single_row(
            &graph,
            query(
                "
MATCH (node:ProjectionNode {node_id: $node_id})
RETURN node.node_kind AS node_kind,
       node.title AS title,
       node.summary AS summary,
       node.status AS status,
       node.node_labels AS node_labels,
       node.properties_json AS properties_json
                ",
            )
            .param("node_id", "decision-missing"),
        )
        .await?;

        let target_row = single_row(
            &graph,
            query(
                "
MATCH (node:ProjectionNode {node_id: $node_id})
RETURN node.node_kind AS node_kind,
       node.title AS title,
       node.summary AS summary,
       node.status AS status,
       node.node_labels AS node_labels,
       node.properties_json AS properties_json
                ",
            )
            .param("node_id", "finding-missing"),
        )
        .await?;

        let source_node_kind: String = source_row.get("node_kind")?;
        let source_title: String = source_row.get("title")?;
        let source_summary: String = source_row.get("summary")?;
        let source_status: String = source_row.get("status")?;
        let source_labels: Vec<String> = source_row.get("node_labels")?;
        let source_properties_json: String = source_row.get("properties_json")?;

        let target_node_kind: String = target_row.get("node_kind")?;
        let target_title: String = target_row.get("title")?;
        let target_summary: String = target_row.get("summary")?;
        let target_status: String = target_row.get("status")?;
        let target_labels: Vec<String> = target_row.get("node_labels")?;
        let target_properties_json: String = target_row.get("properties_json")?;

        assert_eq!(source_node_kind, "placeholder");
        assert_eq!(source_title, "[unmaterialized node]");
        assert_eq!(
            source_summary,
            "Referenced by relation before node materialization"
        );
        assert_eq!(source_status, "UNMATERIALIZED");
        assert_eq!(source_labels, vec!["placeholder".to_string()]);
        assert!(source_properties_json.contains("\"placeholder\":\"true\""));
        assert!(
            source_properties_json
                .contains("\"placeholder_reason\":\"relation_materialized_before_node\"")
        );

        assert_eq!(target_node_kind, "placeholder");
        assert_eq!(target_title, "[unmaterialized node]");
        assert_eq!(
            target_summary,
            "Referenced by relation before node materialization"
        );
        assert_eq!(target_status, "UNMATERIALIZED");
        assert_eq!(target_labels, vec!["placeholder".to_string()]);
        assert!(target_properties_json.contains("\"placeholder\":\"true\""));

        Ok(())
    })
    .await
}

#[tokio::test]
async fn load_context_path_returns_shortest_path_and_target_subtree()
-> Result<(), Box<dyn Error + Send + Sync>> {
    run_with_timeout(async {
        let container = start_neo4j_container().await?;
        let host = container.get_host().await?;
        let port = container.get_host_port_ipv4(NEO4J_INTERNAL_PORT).await?;
        let graph =
            connect_with_retry(format!("neo4j://{host}:{port}"), "neo4j", NEO4J_PASSWORD).await?;
        graph.run(query("MATCH (n) DETACH DELETE n")).await?;

        let store =
            Neo4jProjectionReader::new(format!("neo4j://neo4j:{NEO4J_PASSWORD}@{host}:{port}"))?;
        store
            .apply_mutations(vec![
                ProjectionMutation::UpsertNode(NodeProjection {
                    node_id: "node-root".to_string(),
                    node_kind: "mission".to_string(),
                    title: "Root".to_string(),
                    summary: "Root summary".to_string(),
                    status: "ACTIVE".to_string(),
                    labels: vec!["mission".to_string()],
                    properties: BTreeMap::new(),
                    provenance: None,
                }),
                ProjectionMutation::UpsertNode(NodeProjection {
                    node_id: "story-1".to_string(),
                    node_kind: "story".to_string(),
                    title: "Story".to_string(),
                    summary: "Story summary".to_string(),
                    status: "ACTIVE".to_string(),
                    labels: vec!["story".to_string()],
                    properties: BTreeMap::new(),
                    provenance: None,
                }),
                ProjectionMutation::UpsertNode(NodeProjection {
                    node_id: "task-1".to_string(),
                    node_kind: "task".to_string(),
                    title: "Task".to_string(),
                    summary: "Task summary".to_string(),
                    status: "READY".to_string(),
                    labels: vec!["task".to_string()],
                    properties: BTreeMap::new(),
                    provenance: None,
                }),
                ProjectionMutation::UpsertNode(NodeProjection {
                    node_id: "artifact-1".to_string(),
                    node_kind: "artifact".to_string(),
                    title: "Artifact".to_string(),
                    summary: "Artifact summary".to_string(),
                    status: "READY".to_string(),
                    labels: vec!["artifact".to_string()],
                    properties: BTreeMap::new(),
                    provenance: None,
                }),
                ProjectionMutation::UpsertNode(NodeProjection {
                    node_id: "detour-1".to_string(),
                    node_kind: "story".to_string(),
                    title: "Detour".to_string(),
                    summary: "Detour summary".to_string(),
                    status: "ACTIVE".to_string(),
                    labels: vec!["story".to_string()],
                    properties: BTreeMap::new(),
                    provenance: None,
                }),
                ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
                    source_node_id: "node-root".to_string(),
                    target_node_id: "story-1".to_string(),
                    relation_type: "HAS_STORY".to_string(),
                    explanation: RelationExplanation::new(RelationSemanticClass::Structural)
                        .with_sequence(1),
                })),
                ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
                    source_node_id: "story-1".to_string(),
                    target_node_id: "task-1".to_string(),
                    relation_type: "HAS_TASK".to_string(),
                    explanation: RelationExplanation::new(RelationSemanticClass::Structural)
                        .with_sequence(2),
                })),
                ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
                    source_node_id: "task-1".to_string(),
                    target_node_id: "artifact-1".to_string(),
                    relation_type: "HAS_ARTIFACT".to_string(),
                    explanation: RelationExplanation::new(RelationSemanticClass::Structural)
                        .with_sequence(3),
                })),
                ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
                    source_node_id: "node-root".to_string(),
                    target_node_id: "detour-1".to_string(),
                    relation_type: "HAS_STORY".to_string(),
                    explanation: RelationExplanation::new(RelationSemanticClass::Structural),
                })),
            ])
            .await?;

        let context_path = store
            .load_context_path("node-root", "task-1", 8)
            .await?
            .expect("seeded path should load");

        assert_eq!(context_path.root.node_id, "node-root");
        assert_eq!(
            context_path.path_node_ids,
            vec![
                "node-root".to_string(),
                "story-1".to_string(),
                "task-1".to_string()
            ]
        );
        assert_eq!(context_path.neighbors.len(), 3);
        assert!(
            context_path
                .neighbors
                .iter()
                .any(|node| node.node_id == "artifact-1")
        );
        assert!(
            context_path
                .neighbors
                .iter()
                .all(|node| node.node_id != "detour-1")
        );
        assert_eq!(context_path.relations.len(), 3);
        assert!(context_path.relations.iter().any(|relation| {
            relation.source_node_id == "task-1"
                && relation.target_node_id == "artifact-1"
                && relation.relation_type == "HAS_ARTIFACT"
        }));

        Ok(())
    })
    .await
}

async fn run_with_timeout<F>(future: F) -> Result<(), Box<dyn Error + Send + Sync>>
where
    F: std::future::Future<Output = Result<(), Box<dyn Error + Send + Sync>>>,
{
    timeout(TEST_TIMEOUT, future).await.map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("neo4j integration test exceeded {:?}", TEST_TIMEOUT),
        )
    })?
}

async fn start_neo4j_container()
-> Result<testcontainers::ContainerAsync<GenericImage>, Box<dyn Error + Send + Sync>> {
    ensure_testcontainers_runtime()?;

    Ok(GenericImage::new(NEO4J_IMAGE, NEO4J_TAG)
        .with_exposed_port(NEO4J_INTERNAL_PORT.tcp())
        .with_wait_for(WaitFor::seconds(5))
        .with_env_var("NEO4J_AUTH", format!("neo4j/{NEO4J_PASSWORD}"))
        .start()
        .await?)
}

async fn connect_with_retry(
    uri: String,
    user: &str,
    password: &str,
) -> Result<Graph, Box<dyn Error + Send + Sync>> {
    let mut last_error: Option<Box<dyn Error + Send + Sync>> = None;

    for _ in 0..CONNECT_RETRY_ATTEMPTS {
        match Graph::new(&uri, user, password).await {
            Ok(graph) => return Ok(graph),
            Err(error) => {
                last_error = Some(Box::new(error));
                sleep(CONNECT_RETRY_DELAY).await;
            }
        }
    }

    Err(last_error.expect("at least one connection attempt should fail"))
}

async fn single_row(
    graph: &Graph,
    query: neo4rs::Query,
) -> Result<neo4rs::Row, Box<dyn Error + Send + Sync>> {
    let mut rows = graph.execute(query).await?;
    rows.next()
        .await?
        .ok_or_else(|| "expected a row from neo4j query".into())
}
