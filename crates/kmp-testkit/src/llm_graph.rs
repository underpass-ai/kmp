use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use kmp_application::{
    GraphNodeMaterializedData, GraphNodeMaterializedEvent, NodeDetailMaterializedData,
    NodeDetailMaterializedEvent, ProjectionEnvelope, RelatedNodeExplanationData,
    RelatedNodeReference,
};
use kmp_domain::RelationSemanticClass;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub type ProjectionMessage = (String, Vec<u8>);

#[derive(Debug)]
pub enum LlmGraphError {
    Json(serde_json::Error),
    Validation(String),
}

impl fmt::Display for LlmGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(f, "invalid JSON payload: {error}"),
            Self::Validation(message) => f.write_str(message),
        }
    }
}

impl Error for LlmGraphError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Validation(_) => None,
        }
    }
}

impl From<serde_json::Error> for LlmGraphError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmGraphBatch {
    pub root_node_id: String,
    #[serde(default)]
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub causation_id: Option<String>,
    #[serde(default)]
    pub occurred_at: Option<String>,
    pub nodes: Vec<LlmGraphNode>,
    #[serde(default)]
    pub relations: Vec<LlmGraphRelation>,
    #[serde(default)]
    pub node_details: Vec<LlmNodeDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmGraphNode {
    pub node_id: String,
    pub node_kind: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
    #[serde(default)]
    pub source_kind: Option<String>,
    #[serde(default)]
    pub source_agent: Option<String>,
    #[serde(default)]
    pub observed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmGraphRelation {
    pub source_node_id: String,
    pub target_node_id: String,
    pub relation_type: String,
    pub semantic_class: RelationSemanticClass,
    #[serde(default)]
    pub rationale: Option<String>,
    #[serde(default)]
    pub motivation: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub decision_id: Option<String>,
    #[serde(default)]
    pub caused_by_node_id: Option<String>,
    #[serde(default)]
    pub evidence: Option<String>,
    #[serde(default)]
    pub confidence: Option<String>,
    #[serde(default)]
    pub sequence: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmNodeDetail {
    pub node_id: String,
    pub detail: String,
    #[serde(default)]
    pub content_hash: Option<String>,
    #[serde(default)]
    pub revision: Option<u64>,
}

pub type GraphBatch = LlmGraphBatch;
pub type GraphBatchNode = LlmGraphNode;
pub type GraphBatchRelation = LlmGraphRelation;
pub type GraphBatchNodeDetail = LlmNodeDetail;

pub fn namespace_graph_batch(batch: &mut GraphBatch, namespace: &str) {
    let suffix = format!("--{}", normalize_namespace(namespace));
    let node_id_map = batch
        .nodes
        .iter()
        .map(|node| (node.node_id.clone(), format!("{}{}", node.node_id, suffix)))
        .collect::<BTreeMap<_, _>>();

    batch.root_node_id = namespaced_node_id(&batch.root_node_id, &node_id_map, &suffix);

    for node in &mut batch.nodes {
        node.node_id = namespaced_node_id(&node.node_id, &node_id_map, &suffix);
    }

    for relation in &mut batch.relations {
        relation.source_node_id =
            namespaced_node_id(&relation.source_node_id, &node_id_map, &suffix);
        relation.target_node_id =
            namespaced_node_id(&relation.target_node_id, &node_id_map, &suffix);
        namespace_optional_node_reference(&mut relation.caused_by_node_id, &node_id_map);
        namespace_optional_node_reference(&mut relation.decision_id, &node_id_map);
    }

    for detail in &mut batch.node_details {
        detail.node_id = namespaced_node_id(&detail.node_id, &node_id_map, &suffix);
    }
}

pub fn parse_graph_batch(payload: &str) -> Result<GraphBatch, LlmGraphError> {
    parse_llm_graph_batch(payload)
}

pub fn graph_batch_to_projection_events(
    batch: &GraphBatch,
    subject_prefix: &str,
    run_id: &str,
) -> Result<Vec<ProjectionMessage>, LlmGraphError> {
    llm_graph_to_projection_events(batch, subject_prefix, run_id)
}

pub fn parse_llm_graph_batch(payload: &str) -> Result<LlmGraphBatch, LlmGraphError> {
    let cleaned = strip_markdown_fences(payload);
    Ok(serde_json::from_str(&cleaned)?)
}

pub fn llm_graph_to_projection_events(
    batch: &LlmGraphBatch,
    subject_prefix: &str,
    run_id: &str,
) -> Result<Vec<ProjectionMessage>, LlmGraphError> {
    validate_batch(batch)?;

    let occurred_at = batch.occurred_at.clone().unwrap_or_else(chrono_now);
    let correlation_id = batch
        .correlation_id
        .clone()
        .unwrap_or_else(|| format!("corr-{run_id}"));
    let causation_id = batch
        .causation_id
        .clone()
        .unwrap_or_else(|| format!("vllm-batch-{run_id}"));

    let node_subject = subject(subject_prefix, "graph.node.materialized");
    let detail_subject = subject(subject_prefix, "node.detail.materialized");

    let mut messages = Vec::new();
    let ordered_nodes = ordered_nodes(batch);

    for (index, node) in ordered_nodes.iter().enumerate() {
        let related_nodes = batch
            .relations
            .iter()
            .filter(|relation| relation.source_node_id == node.node_id)
            .map(relation_to_reference)
            .collect();

        let event = GraphNodeMaterializedEvent {
            envelope: ProjectionEnvelope {
                event_id: format!("evt-{run_id}-node-{index}"),
                correlation_id: correlation_id.clone(),
                causation_id: causation_id.clone(),
                occurred_at: occurred_at.clone(),
                aggregate_id: node.node_id.clone(),
                aggregate_type: node.node_kind.clone(),
                schema_version: "v1beta1".to_string(),
            },
            data: GraphNodeMaterializedData {
                node_id: node.node_id.clone(),
                node_kind: node.node_kind.clone(),
                title: node.title.clone(),
                summary: node.summary.clone(),
                status: node.status.clone(),
                labels: node.labels.clone(),
                properties: node.properties.clone(),
                related_nodes,
                source_kind: node.source_kind.clone(),
                source_agent: node.source_agent.clone(),
                observed_at: node.observed_at.clone(),
            },
        };
        messages.push((node_subject.clone(), serde_json::to_vec(&event)?));
    }

    for (index, detail) in batch.node_details.iter().enumerate() {
        let event = NodeDetailMaterializedEvent {
            envelope: ProjectionEnvelope {
                event_id: format!("evt-{run_id}-detail-{index}"),
                correlation_id: correlation_id.clone(),
                causation_id: causation_id.clone(),
                occurred_at: occurred_at.clone(),
                aggregate_id: detail.node_id.clone(),
                aggregate_type: "node_detail".to_string(),
                schema_version: "v1beta1".to_string(),
            },
            data: NodeDetailMaterializedData {
                node_id: detail.node_id.clone(),
                detail: detail.detail.clone(),
                content_hash: detail
                    .content_hash
                    .clone()
                    .unwrap_or_else(|| sha256(&detail.detail)),
                revision: detail.revision.unwrap_or(1),
            },
        };
        messages.push((detail_subject.clone(), serde_json::to_vec(&event)?));
    }

    Ok(messages)
}

fn default_status() -> String {
    "ACTIVE".to_string()
}

fn validate_batch(batch: &LlmGraphBatch) -> Result<(), LlmGraphError> {
    if batch.root_node_id.trim().is_empty() {
        return Err(LlmGraphError::Validation(
            "root_node_id must not be empty".to_string(),
        ));
    }
    if batch.nodes.is_empty() {
        return Err(LlmGraphError::Validation(
            "nodes must contain at least the root node".to_string(),
        ));
    }

    let mut node_ids = BTreeSet::new();
    for node in &batch.nodes {
        if node.node_id.trim().is_empty() {
            return Err(LlmGraphError::Validation(
                "node_id must not be empty".to_string(),
            ));
        }
        if node.node_kind.trim().is_empty() {
            return Err(LlmGraphError::Validation(format!(
                "node `{}` must have a non-empty node_kind",
                node.node_id
            )));
        }
        if node.title.trim().is_empty() {
            return Err(LlmGraphError::Validation(format!(
                "node `{}` must have a non-empty title",
                node.node_id
            )));
        }
        if !node_ids.insert(node.node_id.clone()) {
            return Err(LlmGraphError::Validation(format!(
                "duplicate node_id `{}`",
                node.node_id
            )));
        }
    }

    if !node_ids.contains(&batch.root_node_id) {
        return Err(LlmGraphError::Validation(format!(
            "root_node_id `{}` is missing from nodes[]",
            batch.root_node_id
        )));
    }

    let mut detail_ids = BTreeSet::new();
    for detail in &batch.node_details {
        if !node_ids.contains(&detail.node_id) {
            return Err(LlmGraphError::Validation(format!(
                "node_detail `{}` references an unknown node",
                detail.node_id
            )));
        }
        if !detail_ids.insert(detail.node_id.clone()) {
            return Err(LlmGraphError::Validation(format!(
                "duplicate node_detail for `{}`",
                detail.node_id
            )));
        }
        if detail.detail.trim().is_empty() {
            return Err(LlmGraphError::Validation(format!(
                "node_detail `{}` must not be empty",
                detail.node_id
            )));
        }
    }

    let mut relation_keys = BTreeSet::new();
    let mut outgoing = BTreeMap::<String, Vec<String>>::new();
    for relation in &batch.relations {
        if relation.source_node_id == relation.target_node_id {
            return Err(LlmGraphError::Validation(format!(
                "self relation `{}` -> `{}` is not allowed",
                relation.source_node_id, relation.target_node_id
            )));
        }
        if !node_ids.contains(&relation.source_node_id) {
            return Err(LlmGraphError::Validation(format!(
                "relation source `{}` is missing from nodes[]",
                relation.source_node_id
            )));
        }
        if !node_ids.contains(&relation.target_node_id) {
            return Err(LlmGraphError::Validation(format!(
                "relation target `{}` is missing from nodes[]",
                relation.target_node_id
            )));
        }
        if relation.relation_type.trim().is_empty() {
            return Err(LlmGraphError::Validation(format!(
                "relation `{} -> {}` must have a non-empty relation_type",
                relation.source_node_id, relation.target_node_id
            )));
        }
        if let Some(caused_by_node_id) = relation.caused_by_node_id.as_deref()
            && !node_ids.contains(caused_by_node_id)
        {
            return Err(LlmGraphError::Validation(format!(
                "relation `{} -> {}` references unknown caused_by_node_id `{}`",
                relation.source_node_id, relation.target_node_id, caused_by_node_id
            )));
        }
        if let Some(confidence) = relation.confidence.as_deref()
            && !matches!(confidence.trim(), "high" | "medium" | "low")
        {
            return Err(LlmGraphError::Validation(format!(
                "relation `{} -> {}` has invalid confidence `{}`",
                relation.source_node_id, relation.target_node_id, confidence
            )));
        }
        if relation.semantic_class != RelationSemanticClass::Structural {
            if relation.confidence.is_none() {
                return Err(LlmGraphError::Validation(format!(
                    "relation `{} -> {}` with semantic_class `{}` must include confidence",
                    relation.source_node_id,
                    relation.target_node_id,
                    relation.semantic_class.as_str()
                )));
            }
            let has_support = relation
                .rationale
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
                || relation
                    .motivation
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
                || relation
                    .method
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
                || relation
                    .evidence
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty());
            if !has_support {
                return Err(LlmGraphError::Validation(format!(
                    "relation `{} -> {}` with semantic_class `{}` must include rationale, motivation, method, or evidence",
                    relation.source_node_id,
                    relation.target_node_id,
                    relation.semantic_class.as_str()
                )));
            }
        }

        let relation_key = (
            relation.source_node_id.clone(),
            relation.target_node_id.clone(),
            relation.relation_type.clone(),
        );
        if !relation_keys.insert(relation_key) {
            return Err(LlmGraphError::Validation(format!(
                "duplicate relation `{} -> {} ({})`",
                relation.source_node_id, relation.target_node_id, relation.relation_type
            )));
        }

        outgoing
            .entry(relation.source_node_id.clone())
            .or_default()
            .push(relation.target_node_id.clone());
    }

    let reachable = reachable_from_root(&batch.root_node_id, &outgoing);
    for node in &batch.nodes {
        if node.node_id != batch.root_node_id && !reachable.contains(&node.node_id) {
            return Err(LlmGraphError::Validation(format!(
                "node `{}` is not reachable from root_node_id `{}` via outward relations",
                node.node_id, batch.root_node_id
            )));
        }
    }

    Ok(())
}

fn ordered_nodes(batch: &LlmGraphBatch) -> Vec<&LlmGraphNode> {
    let mut ordered = Vec::with_capacity(batch.nodes.len());
    if let Some(root) = batch
        .nodes
        .iter()
        .find(|node| node.node_id == batch.root_node_id)
    {
        ordered.push(root);
    }
    ordered.extend(
        batch
            .nodes
            .iter()
            .filter(|node| node.node_id != batch.root_node_id),
    );
    ordered
}

fn relation_to_reference(relation: &LlmGraphRelation) -> RelatedNodeReference {
    RelatedNodeReference {
        node_id: relation.target_node_id.clone(),
        relation_type: relation.relation_type.clone(),
        explanation: RelatedNodeExplanationData {
            semantic_class: relation.semantic_class,
            rationale: relation.rationale.clone(),
            motivation: relation.motivation.clone(),
            method: relation.method.clone(),
            decision_id: relation.decision_id.clone(),
            caused_by_node_id: relation.caused_by_node_id.clone(),
            evidence: relation.evidence.clone(),
            confidence: relation.confidence.clone(),
            sequence: relation.sequence,
        },
    }
}

fn subject(prefix: &str, suffix: &str) -> String {
    if prefix.trim().is_empty() {
        suffix.to_string()
    } else {
        format!("{prefix}.{suffix}")
    }
}

fn namespaced_node_id(
    node_id: &str,
    node_id_map: &BTreeMap<String, String>,
    suffix: &str,
) -> String {
    node_id_map
        .get(node_id)
        .cloned()
        .unwrap_or_else(|| format!("{node_id}{suffix}"))
}

fn namespace_optional_node_reference(
    node_id: &mut Option<String>,
    node_id_map: &BTreeMap<String, String>,
) {
    if let Some(node_id) = node_id
        && let Some(namespaced) = node_id_map.get(node_id.as_str())
    {
        *node_id = namespaced.clone();
    }
}

fn normalize_namespace(value: &str) -> String {
    let normalized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();

    if normalized.is_empty() {
        "run".to_string()
    } else {
        normalized
    }
}

fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "2026-03-24T{:02}:{:02}:{:02}Z",
        (now.as_secs() / 3600) % 24,
        (now.as_secs() / 60) % 60,
        now.as_secs() % 60
    )
}

fn sha256(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

fn strip_markdown_fences(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.starts_with("```") {
        let without_opening = if let Some(after_lang) = trimmed.strip_prefix("```json") {
            after_lang
        } else if let Some(after_lang) = trimmed.strip_prefix("```JSON") {
            after_lang
        } else if let Some(after_tick) = trimmed.strip_prefix("```") {
            after_tick
        } else {
            trimmed
        };
        without_opening.trim_end_matches("```").trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn reachable_from_root(
    root_node_id: &str,
    outgoing: &BTreeMap<String, Vec<String>>,
) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![root_node_id.to_string()];

    while let Some(node_id) = stack.pop() {
        if !seen.insert(node_id.clone()) {
            continue;
        }
        if let Some(targets) = outgoing.get(&node_id) {
            stack.extend(targets.iter().cloned());
        }
    }

    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_batch() -> LlmGraphBatch {
        LlmGraphBatch {
            root_node_id: "incident-1".to_string(),
            correlation_id: Some("corr-incident-1".to_string()),
            causation_id: Some("diag-run-1".to_string()),
            occurred_at: Some("2026-04-08T19:00:00Z".to_string()),
            nodes: vec![
                LlmGraphNode {
                    node_id: "incident-1".to_string(),
                    node_kind: "incident".to_string(),
                    title: "Payments latency spike".to_string(),
                    summary: "P95 exceeded 2s".to_string(),
                    status: "INVESTIGATING".to_string(),
                    labels: vec!["incident".to_string(), "p1".to_string()],
                    properties: BTreeMap::new(),
                    source_kind: Some("human".to_string()),
                    source_agent: Some("triage-agent".to_string()),
                    observed_at: None,
                },
                LlmGraphNode {
                    node_id: "finding-1".to_string(),
                    node_kind: "agent_finding".to_string(),
                    title: "Connection pool typo".to_string(),
                    summary: "The deployed pool size was 5 instead of 50".to_string(),
                    status: "CONFIRMED".to_string(),
                    labels: vec!["finding".to_string()],
                    properties: BTreeMap::from([(
                        "service".to_string(),
                        "payments-api".to_string(),
                    )]),
                    source_kind: Some("agent".to_string()),
                    source_agent: Some("diagnostic-agent".to_string()),
                    observed_at: Some("2026-04-08T18:59:00Z".to_string()),
                },
            ],
            relations: vec![LlmGraphRelation {
                source_node_id: "incident-1".to_string(),
                target_node_id: "finding-1".to_string(),
                relation_type: "EXPLAINED_BY".to_string(),
                semantic_class: RelationSemanticClass::Causal,
                rationale: Some(
                    "The typo reduced DB capacity and caused the latency spike".to_string(),
                ),
                motivation: None,
                method: Some("deployment diff".to_string()),
                decision_id: None,
                caused_by_node_id: Some("finding-1".to_string()),
                evidence: Some("config map diff".to_string()),
                confidence: Some("high".to_string()),
                sequence: Some(1),
            }],
            node_details: vec![LlmNodeDetail {
                node_id: "finding-1".to_string(),
                detail: "Pool size changed from 50 to 5 in deployment 2026.04.08.3".to_string(),
                content_hash: None,
                revision: Some(3),
            }],
        }
    }

    #[test]
    fn parse_batch_accepts_markdown_wrapped_json() {
        let parsed = parse_llm_graph_batch(
            r#"
            ```json
            {
              "root_node_id": "incident-1",
              "nodes": [
                { "node_id": "incident-1", "node_kind": "incident", "title": "Incident" }
              ]
            }
            ```
            "#,
        )
        .expect("payload should parse");

        assert_eq!(parsed.root_node_id, "incident-1");
        assert_eq!(parsed.nodes.len(), 1);
    }

    #[test]
    fn translator_emits_root_first_and_groups_relations() {
        let messages =
            llm_graph_to_projection_events(&sample_batch(), "rehydration", "run-7").expect("valid");

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].0, "rehydration.graph.node.materialized");
        assert_eq!(messages[2].0, "rehydration.node.detail.materialized");

        let root_event: GraphNodeMaterializedEvent =
            serde_json::from_slice(&messages[0].1).expect("root event JSON should decode");
        assert_eq!(root_event.data.node_id, "incident-1");
        assert_eq!(root_event.data.related_nodes.len(), 1);
        assert_eq!(
            root_event.data.related_nodes[0].relation_type,
            "EXPLAINED_BY"
        );

        let finding_event: GraphNodeMaterializedEvent =
            serde_json::from_slice(&messages[1].1).expect("finding event JSON should decode");
        assert_eq!(finding_event.data.node_id, "finding-1");
        assert!(finding_event.data.related_nodes.is_empty());

        let detail_event: NodeDetailMaterializedEvent =
            serde_json::from_slice(&messages[2].1).expect("detail event JSON should decode");
        assert_eq!(detail_event.data.node_id, "finding-1");
        assert_eq!(detail_event.data.revision, 3);
        assert!(detail_event.data.content_hash.starts_with("sha256:"));
    }

    #[test]
    fn translator_rejects_relations_to_unknown_nodes() {
        let mut batch = sample_batch();
        batch.relations[0].target_node_id = "missing-node".to_string();

        let error =
            llm_graph_to_projection_events(&batch, "rehydration", "run-9").expect_err("invalid");

        assert!(
            error
                .to_string()
                .contains("relation target `missing-node` is missing"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn translator_rejects_duplicate_relations() {
        let mut batch = sample_batch();
        batch.relations.push(batch.relations[0].clone());

        let error =
            llm_graph_to_projection_events(&batch, "rehydration", "run-10").expect_err("invalid");

        assert!(
            error
                .to_string()
                .contains("duplicate relation `incident-1 -> finding-1 (EXPLAINED_BY)`"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn translator_rejects_unreachable_non_root_nodes() {
        let mut batch = sample_batch();
        batch.nodes.push(LlmGraphNode {
            node_id: "orphan-1".to_string(),
            node_kind: "note".to_string(),
            title: "Orphan".to_string(),
            summary: "Not connected".to_string(),
            status: "ACTIVE".to_string(),
            labels: Vec::new(),
            properties: BTreeMap::new(),
            source_kind: None,
            source_agent: None,
            observed_at: None,
        });

        let error =
            llm_graph_to_projection_events(&batch, "rehydration", "run-11").expect_err("invalid");

        assert!(
            error
                .to_string()
                .contains("node `orphan-1` is not reachable from root_node_id `incident-1`"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn translator_rejects_explanatory_relations_without_confidence() {
        let mut batch = sample_batch();
        batch.relations[0].confidence = None;

        let error =
            llm_graph_to_projection_events(&batch, "rehydration", "run-12").expect_err("invalid");

        assert!(
            error.to_string().contains("must include confidence"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn translator_rejects_invalid_confidence_value() {
        let mut batch = sample_batch();
        batch.relations[0].confidence = Some("certain".to_string());

        let error =
            llm_graph_to_projection_events(&batch, "rehydration", "run-13").expect_err("invalid");

        assert!(
            error
                .to_string()
                .contains("has invalid confidence `certain`"),
            "unexpected error: {error}"
        );
    }
}
