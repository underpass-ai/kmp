use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use kmp_domain::RelationSemanticClass;
use kmp_testkit::{
    GraphBatch, GraphBatchNode, GraphBatchNodeDetail, GraphBatchRelation, GraphBatchRetryPolicy,
    GraphBatchSemanticClassifierPolicy, LlmEvaluatorConfig, LlmProvider,
    build_graph_batch_request_body, classify_graph_batch_semantic_classes_with_policy,
    request_graph_batch_with_policy,
};
use serde::Serialize;
use serde_json::json;

const RUN_ENV: &str = "RUN_VLLM_BLIND_STRUCTURAL_SMOKE";
const ROOT_NODE_ID: &str = "incident-2026-04-08-payments-latency";
const REQUEST_FIXTURE: &str = include_str!(
    "../../../api/examples/inference-prompts/vllm-graph-materialization.blind.request.json"
);

type TestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum StructuralNodeRole {
    Finding,
    Evidence,
    Action,
}

#[derive(Debug, Clone, Serialize)]
struct RelationScore {
    source_node_id: String,
    target_node_id: String,
    target_node_kind: String,
    target_roles: Vec<StructuralNodeRole>,
    semantic_class: String,
    accepted_semantic_classes: Vec<String>,
    acceptable: bool,
}

#[derive(Debug, Clone, Serialize)]
struct StructuralScorecard {
    root_present: bool,
    connected_from_root: bool,
    node_count: usize,
    relation_count: usize,
    detail_count: usize,
    finding_candidate_nodes: Vec<String>,
    evidence_candidate_nodes: Vec<String>,
    action_candidate_nodes: Vec<String>,
    detail_target_ids: Vec<String>,
    relation_semantic_classes: Vec<String>,
    acceptable_relation_count: usize,
    relation_scores: Vec<RelationScore>,
}

#[test]
fn blind_structural_scorecard_detects_expected_roles() {
    let batch = sample_blind_batch();
    let scorecard = evaluate_structural_scorecard(&batch);

    assert!(scorecard.root_present);
    assert!(scorecard.connected_from_root);
    assert_eq!(scorecard.node_count, 4);
    assert_eq!(scorecard.relation_count, 3);
    assert_eq!(scorecard.detail_count, 2);
    assert!(
        scorecard
            .finding_candidate_nodes
            .contains(&"db-pool-regression".to_string())
    );
    assert!(
        scorecard
            .evidence_candidate_nodes
            .contains(&"db-env-inspection".to_string())
    );
    assert!(
        scorecard
            .action_candidate_nodes
            .contains(&"rollback-and-traffic-shift".to_string())
    );
    assert_eq!(scorecard.acceptable_relation_count, 3);
}

#[tokio::test]
async fn vllm_blind_structural_smoke_reports_primary_and_reranked_scorecard() -> TestResult<()> {
    if std::env::var(RUN_ENV).as_deref() != Ok("1") {
        eprintln!(
            "skipping blind structural vLLM smoke: set {RUN_ENV}=1 plus LLM_*, LLM_SEMANTIC_CLASSIFIER_*"
        );
        return Ok(());
    }

    let config = LlmEvaluatorConfig::from_env();
    assert_eq!(
        config.provider,
        LlmProvider::OpenAI,
        "blind structural smoke expects LLM_PROVIDER=openai"
    );
    assert!(
        config.semantic_classifier_endpoint.is_some(),
        "blind structural smoke requires LLM_SEMANTIC_CLASSIFIER_ENDPOINT"
    );
    assert!(
        config.semantic_classifier_model.is_some(),
        "blind structural smoke requires LLM_SEMANTIC_CLASSIFIER_MODEL"
    );
    assert!(
        config.semantic_classifier_provider.is_some(),
        "blind structural smoke requires LLM_SEMANTIC_CLASSIFIER_PROVIDER"
    );

    let run_id = std::env::var("VLLM_BLIND_STRUCTURAL_RUN_ID")
        .unwrap_or_else(|_| format!("vllm-blind-structural-{}", unix_timestamp_secs()));
    let request_body = build_graph_batch_request_body(&config, REQUEST_FIXTURE)?;
    let primary_policy = GraphBatchRetryPolicy::from_env();
    let primary = request_graph_batch_with_policy(
        &config,
        request_body,
        "rehydration",
        &run_id,
        primary_policy,
    )
    .await?;
    let primary_scorecard = evaluate_structural_scorecard(&primary.batch);

    assert_eq!(primary.batch.root_node_id, ROOT_NODE_ID);
    assert!(primary_scorecard.root_present);
    assert!(primary_scorecard.connected_from_root);
    assert!(
        !primary_scorecard.finding_candidate_nodes.is_empty(),
        "blind structural smoke requires at least one finding candidate node"
    );
    assert!(
        !primary_scorecard.evidence_candidate_nodes.is_empty(),
        "blind structural smoke requires at least one evidence candidate node"
    );
    assert!(
        !primary_scorecard.action_candidate_nodes.is_empty(),
        "blind structural smoke requires at least one action candidate node"
    );

    let classifier_policy = GraphBatchSemanticClassifierPolicy::from_env();
    let classified = classify_graph_batch_semantic_classes_with_policy(
        &config,
        &primary.batch,
        "rehydration",
        &run_id,
        classifier_policy,
    )
    .await?;
    let classified_scorecard = evaluate_structural_scorecard(&classified.batch);

    assert!(
        classified_scorecard.acceptable_relation_count
            >= primary_scorecard.acceptable_relation_count,
        "reranker should not reduce accepted semantic-class coverage"
    );
    assert!(
        classified_scorecard.acceptable_relation_count >= 2,
        "reranked blind graph should classify at least two of three relations acceptably"
    );

    eprintln!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "run_id": run_id,
            "primary_attempts": primary.primary_attempts,
            "primary_prompt_tokens": primary.prompt_tokens,
            "primary_completion_tokens": primary.completion_tokens,
            "semantic_classifier_attempts": classified.attempts,
            "semantic_classifier_changed_relations": classified.changed_relations,
            "primary": primary_scorecard,
            "reranked": classified_scorecard,
        }))?
    );

    Ok(())
}

fn evaluate_structural_scorecard(batch: &GraphBatch) -> StructuralScorecard {
    let root_present = batch
        .nodes
        .iter()
        .any(|node| node.node_id == batch.root_node_id);
    let connected_from_root = is_connected_from_root(batch);
    let detail_map = batch
        .node_details
        .iter()
        .map(|detail| (detail.node_id.as_str(), detail.detail.as_str()))
        .collect::<BTreeMap<_, _>>();

    let mut finding_candidate_nodes = Vec::new();
    let mut evidence_candidate_nodes = Vec::new();
    let mut action_candidate_nodes = Vec::new();

    for node in &batch.nodes {
        if node.node_id == batch.root_node_id {
            continue;
        }
        let roles = infer_node_roles(node, detail_map.get(node.node_id.as_str()).copied());
        if roles.contains(&StructuralNodeRole::Finding) {
            finding_candidate_nodes.push(node.node_id.clone());
        }
        if roles.contains(&StructuralNodeRole::Evidence) {
            evidence_candidate_nodes.push(node.node_id.clone());
        }
        if roles.contains(&StructuralNodeRole::Action) {
            action_candidate_nodes.push(node.node_id.clone());
        }
    }

    let mut relation_semantic_classes = Vec::new();
    let mut relation_scores = Vec::new();
    let mut acceptable_relation_count = 0;

    for relation in &batch.relations {
        relation_semantic_classes.push(relation.semantic_class.as_str().to_string());

        let Some(target_node) = batch
            .nodes
            .iter()
            .find(|node| node.node_id == relation.target_node_id)
        else {
            continue;
        };

        let target_roles = infer_node_roles(
            target_node,
            detail_map.get(target_node.node_id.as_str()).copied(),
        );
        let accepted_semantic_classes = accepted_semantic_classes_for_roles(&target_roles);
        let acceptable = accepted_semantic_classes.contains(&relation.semantic_class);
        if acceptable {
            acceptable_relation_count += 1;
        }

        relation_scores.push(RelationScore {
            source_node_id: relation.source_node_id.clone(),
            target_node_id: relation.target_node_id.clone(),
            target_node_kind: target_node.node_kind.clone(),
            target_roles: target_roles.into_iter().collect(),
            semantic_class: relation.semantic_class.as_str().to_string(),
            accepted_semantic_classes: accepted_semantic_classes
                .into_iter()
                .map(|class| class.as_str().to_string())
                .collect(),
            acceptable,
        });
    }

    StructuralScorecard {
        root_present,
        connected_from_root,
        node_count: batch.nodes.len(),
        relation_count: batch.relations.len(),
        detail_count: batch.node_details.len(),
        finding_candidate_nodes,
        evidence_candidate_nodes,
        action_candidate_nodes,
        detail_target_ids: batch
            .node_details
            .iter()
            .map(|detail| detail.node_id.clone())
            .collect(),
        relation_semantic_classes,
        acceptable_relation_count,
        relation_scores,
    }
}

fn is_connected_from_root(batch: &GraphBatch) -> bool {
    let known_nodes = batch
        .nodes
        .iter()
        .map(|node| node.node_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut adjacency = BTreeMap::<&str, Vec<&str>>::new();
    for relation in &batch.relations {
        adjacency
            .entry(relation.source_node_id.as_str())
            .or_default()
            .push(relation.target_node_id.as_str());
    }

    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::from([batch.root_node_id.as_str()]);
    while let Some(node_id) = queue.pop_front() {
        if !visited.insert(node_id) {
            continue;
        }
        if let Some(targets) = adjacency.get(node_id) {
            for target in targets {
                queue.push_back(target);
            }
        }
    }

    known_nodes.iter().all(|node_id| visited.contains(node_id))
}

fn infer_node_roles(node: &GraphBatchNode, detail: Option<&str>) -> BTreeSet<StructuralNodeRole> {
    let mut roles = BTreeSet::new();
    let text = node_text(node, detail);
    let kind = node.node_kind.to_ascii_lowercase();

    if has_any(
        &kind,
        &[
            "finding",
            "issue",
            "cause",
            "regression",
            "evidence",
            "inspection",
        ],
    ) || has_any(
        &text,
        &[
            "db maxconnections",
            "db max connections",
            "maxconnections",
            "max connections",
            "config map",
            "wait time",
            "error rate",
            "rollout 2026.04.08.3",
            "db_max_connections",
        ],
    ) {
        roles.insert(StructuralNodeRole::Finding);
    }

    if has_any(&kind, &["evidence", "inspection", "observation", "metric"])
        || has_any(
            &text,
            &[
                "inspection",
                "showed",
                "observed",
                "db_max_connections",
                "wait time",
                "error rate",
                "config map",
                "replica",
                "metric",
                "dashboard",
                "env",
            ],
        )
    {
        roles.insert(StructuralNodeRole::Evidence);
    }

    if has_any(&kind, &["task", "decision", "action", "mitigation"])
        || has_any(
            &text,
            &[
                "rollback",
                "shifted",
                "shift most traffic",
                "secondary region",
                "reroute",
                "mitigation",
                "traffic shift",
                "oncall started",
            ],
        )
    {
        roles.insert(StructuralNodeRole::Action);
    }

    roles
}

fn accepted_semantic_classes_for_roles(
    roles: &BTreeSet<StructuralNodeRole>,
) -> Vec<RelationSemanticClass> {
    let mut classes = Vec::new();
    if roles.contains(&StructuralNodeRole::Finding) {
        push_unique_class(&mut classes, RelationSemanticClass::Causal);
        push_unique_class(&mut classes, RelationSemanticClass::Evidential);
    }
    if roles.contains(&StructuralNodeRole::Evidence) {
        push_unique_class(&mut classes, RelationSemanticClass::Evidential);
    }
    if roles.contains(&StructuralNodeRole::Action) {
        push_unique_class(&mut classes, RelationSemanticClass::Procedural);
        push_unique_class(&mut classes, RelationSemanticClass::Motivational);
    }
    if classes.is_empty() {
        classes.push(RelationSemanticClass::Structural);
    }
    classes
}

fn push_unique_class(classes: &mut Vec<RelationSemanticClass>, class: RelationSemanticClass) {
    if !classes.contains(&class) {
        classes.push(class);
    }
}

fn node_text(node: &GraphBatchNode, detail: Option<&str>) -> String {
    let mut text = String::new();
    text.push_str(&node.title);
    text.push(' ');
    text.push_str(&node.summary);
    text.push(' ');
    for value in node.properties.values() {
        text.push_str(value);
        text.push(' ');
    }
    if let Some(detail) = detail {
        text.push_str(detail);
    }
    text.to_ascii_lowercase()
}

fn has_any(text: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| text.contains(pattern))
}

fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs()
}

fn sample_blind_batch() -> GraphBatch {
    GraphBatch {
        root_node_id: ROOT_NODE_ID.to_string(),
        correlation_id: None,
        causation_id: None,
        occurred_at: None,
        nodes: vec![
            GraphBatchNode {
                node_id: ROOT_NODE_ID.to_string(),
                node_kind: "incident".to_string(),
                title: "Payments latency incident".to_string(),
                summary: "P95 latency exceeded 2s after rollout 2026.04.08.3.".to_string(),
                status: "ACTIVE".to_string(),
                labels: vec!["incident".to_string()],
                properties: BTreeMap::new(),
                source_kind: Some("human".to_string()),
                source_agent: Some("pir".to_string()),
                observed_at: Some("2026-04-08T12:00:00Z".to_string()),
            },
            GraphBatchNode {
                node_id: "db-pool-regression".to_string(),
                node_kind: "finding".to_string(),
                title: "DB connection pool regression".to_string(),
                summary: "Rollout reduced DB maxConnections from 50 to 5 and DB wait time rose."
                    .to_string(),
                status: "ACTIVE".to_string(),
                labels: vec!["finding".to_string()],
                properties: BTreeMap::new(),
                source_kind: Some("projection".to_string()),
                source_agent: Some("pir".to_string()),
                observed_at: Some("2026-04-08T12:03:00Z".to_string()),
            },
            GraphBatchNode {
                node_id: "db-env-inspection".to_string(),
                node_kind: "evidence".to_string(),
                title: "Replica env inspection".to_string(),
                summary: "Pod env inspection showed DB_MAX_CONNECTIONS=5 on two replicas."
                    .to_string(),
                status: "ACTIVE".to_string(),
                labels: vec!["evidence".to_string()],
                properties: BTreeMap::new(),
                source_kind: Some("human".to_string()),
                source_agent: Some("pir".to_string()),
                observed_at: Some("2026-04-08T12:04:00Z".to_string()),
            },
            GraphBatchNode {
                node_id: "rollback-and-traffic-shift".to_string(),
                node_kind: "action".to_string(),
                title: "Rollback and traffic shift".to_string(),
                summary:
                    "Oncall started rollback and shifted most traffic to the secondary region."
                        .to_string(),
                status: "ACTIVE".to_string(),
                labels: vec!["mitigation".to_string()],
                properties: BTreeMap::new(),
                source_kind: Some("human".to_string()),
                source_agent: Some("pir".to_string()),
                observed_at: Some("2026-04-08T12:06:00Z".to_string()),
            },
        ],
        relations: vec![
            GraphBatchRelation {
                source_node_id: ROOT_NODE_ID.to_string(),
                target_node_id: "db-pool-regression".to_string(),
                relation_type: "HAS_FINDING".to_string(),
                semantic_class: RelationSemanticClass::Causal,
                rationale: Some("The rollout changed DB maxConnections from 50 to 5.".to_string()),
                motivation: None,
                method: None,
                decision_id: None,
                caused_by_node_id: Some("db-pool-regression".to_string()),
                evidence: None,
                confidence: Some("medium".to_string()),
                sequence: None,
            },
            GraphBatchRelation {
                source_node_id: ROOT_NODE_ID.to_string(),
                target_node_id: "db-env-inspection".to_string(),
                relation_type: "SUPPORTED_BY".to_string(),
                semantic_class: RelationSemanticClass::Evidential,
                rationale: None,
                motivation: None,
                method: None,
                decision_id: None,
                caused_by_node_id: None,
                evidence: Some("Replica env inspection showed DB_MAX_CONNECTIONS=5.".to_string()),
                confidence: Some("high".to_string()),
                sequence: None,
            },
            GraphBatchRelation {
                source_node_id: ROOT_NODE_ID.to_string(),
                target_node_id: "rollback-and-traffic-shift".to_string(),
                relation_type: "MITIGATED_BY".to_string(),
                semantic_class: RelationSemanticClass::Procedural,
                rationale: None,
                motivation: None,
                method: Some(
                    "Rollback began first and traffic shifted to the secondary region while it propagated."
                        .to_string(),
                ),
                decision_id: None,
                caused_by_node_id: None,
                evidence: None,
                confidence: Some("high".to_string()),
                sequence: Some(1),
            },
        ],
        node_details: vec![
            GraphBatchNodeDetail {
                node_id: "db-env-inspection".to_string(),
                detail: "Pod env inspection on two replicas showed DB_MAX_CONNECTIONS=5."
                    .to_string(),
                content_hash: None,
                revision: Some(1),
            },
            GraphBatchNodeDetail {
                node_id: "rollback-and-traffic-shift".to_string(),
                detail: "Oncall started rollback and shifted most traffic to the secondary region."
                    .to_string(),
                content_hash: None,
                revision: Some(1),
            },
        ],
    }
}
