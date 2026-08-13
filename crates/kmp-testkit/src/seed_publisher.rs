//! Converts generated seeds into NATS projection events and publishes them.
//!
//! Bridges the dataset generator with the kernel's projection pipeline.

use kmp_application::{
    GraphNodeMaterializedData, GraphNodeMaterializedEvent, NodeDetailMaterializedData,
    NodeDetailMaterializedEvent, ProjectionEnvelope, RelatedNodeExplanationData,
    RelatedNodeReference,
};

use crate::dataset_generator::{GeneratedRelation, GeneratedSeed};

/// Convert a generated seed into serialized NATS messages.
///
/// Returns a list of (subject, payload) pairs ready for NATS publish.
#[allow(clippy::type_complexity)]
pub fn seed_to_projection_events(
    seed: &GeneratedSeed,
    subject_prefix: &str,
    run_id: &str,
) -> Result<Vec<(String, Vec<u8>)>, Box<dyn std::error::Error + Send + Sync>> {
    let mut messages = Vec::new();

    // Root node event with all relations from root
    let root_relations: Vec<&GeneratedRelation> = seed
        .relations
        .iter()
        .filter(|r| r.source_node_id == seed.root.node_id)
        .collect();

    let root_event = GraphNodeMaterializedEvent {
        envelope: ProjectionEnvelope {
            event_id: format!("evt-{run_id}-root"),
            correlation_id: format!("corr-{run_id}"),
            causation_id: format!("cause-{run_id}"),
            occurred_at: chrono_now(),
            aggregate_id: seed.root.node_id.clone(),
            aggregate_type: "context-node".to_string(),
            schema_version: "v1beta1".to_string(),
        },
        data: {
            let (sk, sa, oa) = provenance_for_node(&seed.root.node_kind, &seed.root.labels);
            GraphNodeMaterializedData {
                node_id: seed.root.node_id.clone(),
                node_kind: seed.root.node_kind.clone(),
                title: seed.root.title.clone(),
                summary: seed.root.summary.clone(),
                status: "ACTIVE".to_string(),
                labels: seed.root.labels.clone(),
                properties: seed.root.properties.clone(),
                related_nodes: root_relations
                    .iter()
                    .map(|r| relation_to_reference(r))
                    .collect(),
                source_kind: sk,
                source_agent: sa,
                observed_at: oa,
            }
        },
    };

    let subject = format!("{subject_prefix}.graph.node.materialized");
    let payload = serde_json::to_vec(&root_event)?;
    messages.push((subject.clone(), payload));

    // Root detail event
    if let Some(ref detail) = seed.root.detail {
        let detail_event = NodeDetailMaterializedEvent {
            envelope: ProjectionEnvelope {
                event_id: format!("evt-{run_id}-root-detail"),
                correlation_id: format!("corr-{run_id}"),
                causation_id: format!("cause-{run_id}"),
                occurred_at: chrono_now(),
                aggregate_id: seed.root.node_id.clone(),
                aggregate_type: "context-node-detail".to_string(),
                schema_version: "v1beta1".to_string(),
            },
            data: NodeDetailMaterializedData {
                node_id: seed.root.node_id.clone(),
                detail: detail.clone(),
                content_hash: format!("hash-{run_id}-root"),
                revision: 1,
            },
        };
        let detail_subject = format!("{subject_prefix}.node.detail.materialized");
        messages.push((detail_subject.clone(), serde_json::to_vec(&detail_event)?));
    }

    // Node events for each non-root node
    for (i, node) in seed.nodes.iter().enumerate() {
        let node_relations: Vec<&GeneratedRelation> = seed
            .relations
            .iter()
            .filter(|r| r.source_node_id == node.node_id)
            .collect();

        let node_event = GraphNodeMaterializedEvent {
            envelope: ProjectionEnvelope {
                event_id: format!("evt-{run_id}-node-{i}"),
                correlation_id: format!("corr-{run_id}"),
                causation_id: format!("cause-{run_id}"),
                occurred_at: chrono_now(),
                aggregate_id: node.node_id.clone(),
                aggregate_type: "context-node".to_string(),
                schema_version: "v1beta1".to_string(),
            },
            data: {
                let (sk, sa, oa) = provenance_for_node(&node.node_kind, &node.labels);
                GraphNodeMaterializedData {
                    node_id: node.node_id.clone(),
                    node_kind: node.node_kind.clone(),
                    title: node.title.clone(),
                    summary: node.summary.clone(),
                    status: "ACTIVE".to_string(),
                    labels: node.labels.clone(),
                    properties: node.properties.clone(),
                    related_nodes: node_relations
                        .iter()
                        .map(|r| relation_to_reference(r))
                        .collect(),
                    source_kind: sk,
                    source_agent: sa,
                    observed_at: oa,
                }
            },
        };

        messages.push((subject.clone(), serde_json::to_vec(&node_event)?));

        if let Some(ref detail) = node.detail {
            let detail_event = NodeDetailMaterializedEvent {
                envelope: ProjectionEnvelope {
                    event_id: format!("evt-{run_id}-node-{i}-detail"),
                    correlation_id: format!("corr-{run_id}"),
                    causation_id: format!("cause-{run_id}"),
                    occurred_at: chrono_now(),
                    aggregate_id: node.node_id.clone(),
                    aggregate_type: "context-node-detail".to_string(),
                    schema_version: "v1beta1".to_string(),
                },
                data: NodeDetailMaterializedData {
                    node_id: node.node_id.clone(),
                    detail: detail.clone(),
                    content_hash: format!("hash-{run_id}-node-{i}"),
                    revision: 1,
                },
            };
            let detail_subject = format!("{subject_prefix}.node.detail.materialized");
            messages.push((detail_subject.clone(), serde_json::to_vec(&detail_event)?));
        }
    }

    Ok(messages)
}

fn relation_to_reference(rel: &GeneratedRelation) -> RelatedNodeReference {
    RelatedNodeReference {
        node_id: rel.target_node_id.clone(),
        relation_type: rel.relation_type.clone(),
        explanation: RelatedNodeExplanationData {
            semantic_class: kmp_domain::RelationSemanticClass::parse(rel.semantic_class.as_str())
                .expect("valid semantic class"),
            rationale: rel.rationale.clone(),
            motivation: rel.motivation.clone(),
            method: rel.method.clone(),
            decision_id: rel.decision_id.clone(),
            caused_by_node_id: rel.caused_by_node_id.clone(),
            evidence: None,
            confidence: None,
            sequence: rel.sequence,
        },
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

/// Returns differentiated provenance based on node role.
///
/// When `BENCHMARK_WITH_PROVENANCE=true`:
/// - Causal chain nodes: `source:agent`, recent timestamp, specialized agent
/// - Distractor/noise nodes: `source:projection`, old timestamp, batch agent
///
/// This gives the LLM signal to distinguish trusted from low-trust sources.
fn provenance_for_node(
    node_kind: &str,
    labels: &[String],
) -> (Option<String>, Option<String>, Option<String>) {
    if std::env::var("BENCHMARK_WITH_PROVENANCE").as_deref() != Ok("true") {
        return (None, None, None);
    }

    let is_noise = labels.iter().any(|l| l == "noise" || l == "distractor");
    if is_noise {
        // Distractors: old batch projection, low trust signal
        (
            Some("projection".to_string()),
            Some("batch-ingest".to_string()),
            Some("2026-03-20T02:00:00Z".to_string()),
        )
    } else {
        // Causal chain: differentiate by node role
        match node_kind {
            // Root nodes (incident, bug_report) come from human operators
            "incident" | "bug_report" => (
                Some("human".to_string()),
                Some("operator-oncall".to_string()),
                Some(chrono_now()),
            ),
            // Decision/hypothesis nodes come from diagnostics agent
            "decision" | "hypothesis" => (
                Some("agent".to_string()),
                Some("diagnostics-agent".to_string()),
                Some(chrono_now()),
            ),
            // Execution nodes (task, fix, investigation) come from execution agent
            "task" | "fix" | "investigation" => (
                Some("agent".to_string()),
                Some("execution-agent".to_string()),
                Some(chrono_now()),
            ),
            // Verification nodes (artifact, evidence, test) are derived
            "artifact" | "evidence" | "test" => (
                Some("derived".to_string()),
                Some("verification-pipeline".to_string()),
                Some(chrono_now()),
            ),
            // Fallback
            _ => (
                Some("agent".to_string()),
                Some("kernel-agent".to_string()),
                Some(chrono_now()),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset_generator::{Domain, GraphSeedConfig, generate_seed};

    #[test]
    fn micro_seed_produces_expected_event_count() {
        let seed = generate_seed(GraphSeedConfig::micro(Domain::Operations));
        let events =
            seed_to_projection_events(&seed, "rehydration", "test-run").expect("should serialize");

        // Root node event + root detail + 3 chain nodes + 3 chain details = 8
        assert!(
            events.len() >= 7,
            "expected at least 7 events, got {}",
            events.len()
        );
        assert!(events.iter().all(|(_, payload)| !payload.is_empty()));
    }

    #[test]
    fn meso_seed_produces_more_events_than_micro() {
        let micro = generate_seed(GraphSeedConfig::micro(Domain::Operations));
        let meso = generate_seed(GraphSeedConfig::meso(Domain::Operations));

        let micro_events =
            seed_to_projection_events(&micro, "rehydration", "micro").expect("should serialize");
        let meso_events =
            seed_to_projection_events(&meso, "rehydration", "meso").expect("should serialize");

        assert!(meso_events.len() > micro_events.len());
    }
}
