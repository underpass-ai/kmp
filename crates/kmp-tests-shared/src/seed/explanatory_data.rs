#![allow(dead_code)]

use std::collections::BTreeMap;
use std::error::Error;

use async_nats::Client;
use kmp_application::{
    GraphNodeMaterializedData, GraphNodeMaterializedEvent, NodeDetailMaterializedData,
    NodeDetailMaterializedEvent, ProjectionEnvelope, RelatedNodeExplanationData,
    RelatedNodeReference,
};
use kmp_domain::RelationSemanticClass;

use crate::debug::debug_log_value;

pub const SUBJECT_PREFIX: &str = "rehydration";

pub const ROOT_NODE_ID: &str = "node:incident:port-manifold-breach";
pub const ROOT_NODE_KIND: &str = "incident";

pub const DECISION_NODE_ID: &str = "node:decision:reroute-reserve-power";
pub const DECISION_NODE_KIND: &str = "decision";

pub const FOCUS_NODE_ID: &str = "node:task:reroute-eps-grid";
pub const FOCUS_NODE_KIND: &str = "task";
pub const FOCUS_DETAIL: &str =
    "Redirect reserve power from noncritical comfort systems to port-side containment relays.";

pub const ARTIFACT_NODE_ID: &str = "node:artifact:telemetry-capture";
pub const ARTIFACT_NODE_KIND: &str = "artifact";

pub const ROOT_TO_DECISION_RELATION: &str = "TRIGGERS";
pub const DECISION_TO_TASK_RELATION: &str = "AUTHORIZES";
pub const TASK_TO_ARTIFACT_RELATION: &str = "VERIFIED_BY";

pub const ROOT_TO_DECISION_RATIONALE: &str = "containment margin dropped below the safe threshold";
pub const DECISION_TO_TASK_RATIONALE: &str = "reserve power must be diverted before repair";
pub const DECISION_TO_TASK_MOTIVATION: &str =
    "stabilize the containment loop before engineers enter the manifold bay";
pub const TASK_TO_ARTIFACT_METHOD: &str = "post-reroute telemetry validation";

pub const FAILURE_FOCUS_NODE_ID: &str = "node:artifact:failed-telemetry";
pub const FAILURE_FOCUS_NODE_KIND: &str = "artifact";
pub const FAILURE_DETAIL: &str = "The containment telemetry shows the manifold never recovered because the reroute kept comfort systems online.";
pub const BAD_DECISION_NODE_ID: &str = "node:decision:preserve-comfort-load";
pub const BAD_TASK_NODE_ID: &str = "node:task:apply-minimal-reroute";
pub const RECOVERY_DECISION_RELATION: &str = "SUPERSEDED_BY";
pub const RECOVERY_DECISION_RATIONALE: &str =
    "the minimal reroute must be replaced with a containment-priority reroute";
pub const RECOVERY_DECISION_MOTIVATION: &str =
    "restart from the failed decision point and fully divert reserve power into containment";
pub const BAD_DECISION_TO_TASK_RATIONALE: &str =
    "minimize operational disruption by keeping passenger comfort systems online";
pub const FAILURE_EVIDENCE_RATIONALE: &str =
    "telemetry shows containment remained below threshold after the minimal reroute";
pub const RECOVERY_SUCCESS_RATIONALE: &str =
    "telemetry confirms the containment-priority reroute restored safe manifold margins";

pub const HANDOFF_ROOT_NODE_ID: &str = "node:incident:reactor-bay-coolant-leak";
pub const HANDOFF_ROOT_NODE_KIND: &str = "incident";
pub const HANDOFF_INITIAL_DECISION_NODE_ID: &str = "node:decision:attempt-remote-isolation";
pub const HANDOFF_RESUME_DECISION_NODE_ID: &str = "node:decision:dispatch-eva-specialist";
pub const HANDOFF_TASK_STARTED_NODE_ID: &str = "node:task:remote-isolation-attempt";
pub const HANDOFF_RESUMED_TASK_NODE_ID: &str = "node:task:manual-override-isolation";
pub const HANDOFF_BLOCKER_NODE_ID: &str = "node:artifact:manual-override-jam";
pub const HANDOFF_INITIAL_RATIONALE: &str =
    "coolant loss must be contained before pressure reaches the reactor bay";
pub const HANDOFF_INITIAL_MOTIVATION: &str =
    "attempt remote isolation first to avoid exposing engineers to the leak corridor";
pub const HANDOFF_BLOCKER_RATIONALE: &str =
    "remote actuators cannot move the manual override because debris jammed the valve cage";
pub const HANDOFF_RESUME_RATIONALE: &str =
    "resume from the failed remote isolation attempt with a manual override team";
pub const HANDOFF_RESUME_MOTIVATION: &str =
    "handoff to an EVA specialist already staged near the override panel";
pub const HANDOFF_RESUMED_DETAIL: &str = "Continue from the failed remote isolation attempt, clear the valve cage manually, and complete the coolant isolation without restarting the containment plan.";
pub const HANDOFF_SUCCESS_ARTIFACT_NODE_ID: &str = "node:artifact:coolant-isolation-confirmed";
pub const HANDOFF_SUCCESS_RATIONALE: &str = "post-override telemetry confirms the coolant isolation completed and reactor-bay pressure stabilized";
pub const HANDOFF_SUCCESS_METHOD: &str = "post-override coolant telemetry validation";

pub const CONSTRAINT_ROOT_NODE_ID: &str = "node:constraint:keep-crew-outside-plasma-bay";
pub const CONSTRAINT_ROOT_NODE_KIND: &str = "constraint";
pub const CONSTRAINT_DECISION_NODE_ID: &str = "node:decision:remote-calibration-first";
pub const CONSTRAINT_TASK_NODE_ID: &str = "node:task:run-remote-calibration";
pub const CONSTRAINT_RELATION: &str = "CONSTRAINS";
pub const CONSTRAINT_RATIONALE: &str =
    "radiation spikes make manual calibration inside the plasma bay unsafe";
pub const CONSTRAINT_TASK_RATIONALE: &str =
    "remote calibration is slower but avoids sending crew into the bay";
pub const CONSTRAINT_TASK_MOTIVATION: &str =
    "restore coolant flow without violating the exclusion constraint";
pub const CONSTRAINT_TASK_DETAIL: &str = "Use the remote calibration channel, accept reduced throughput, and keep the crew outside the plasma bay until the dosimeter alarm clears and the safety officer lifts the exclusion zone.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationExplanationMode {
    Explanatory,
    StructuralOnly,
    DetailOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailMode {
    WithDetail,
    WithoutDetail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphScale {
    Micro,
    Meso,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionSeedVariant {
    pub relation_mode: RelationExplanationMode,
    pub detail_mode: DetailMode,
    pub graph_scale: GraphScale,
}

impl ProjectionSeedVariant {
    pub const FULL_EXPLANATORY_WITH_DETAIL: Self = Self {
        relation_mode: RelationExplanationMode::Explanatory,
        detail_mode: DetailMode::WithDetail,
        graph_scale: GraphScale::Micro,
    };

    pub const STRUCTURAL_ONLY_WITH_DETAIL: Self = Self {
        relation_mode: RelationExplanationMode::StructuralOnly,
        detail_mode: DetailMode::WithDetail,
        graph_scale: GraphScale::Micro,
    };

    pub const DETAIL_ONLY_WITH_DETAIL: Self = Self {
        relation_mode: RelationExplanationMode::DetailOnly,
        detail_mode: DetailMode::WithDetail,
        graph_scale: GraphScale::Micro,
    };

    pub const FULL_EXPLANATORY_WITHOUT_DETAIL: Self = Self {
        relation_mode: RelationExplanationMode::Explanatory,
        detail_mode: DetailMode::WithoutDetail,
        graph_scale: GraphScale::Micro,
    };

    pub fn with_graph_scale(self, graph_scale: GraphScale) -> Self {
        Self {
            graph_scale,
            ..self
        }
    }

    pub fn detail_enabled(self) -> bool {
        self.detail_mode == DetailMode::WithDetail
    }
}

type ProjectionMessagesResult = Result<Vec<(String, Vec<u8>)>, Box<dyn Error + Send + Sync>>;

pub async fn publish_explanatory_projection_events(
    client: &Client,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    publish_explanatory_projection_events_variant(
        client,
        ProjectionSeedVariant::FULL_EXPLANATORY_WITH_DETAIL,
    )
    .await
}

pub async fn publish_explanatory_projection_events_variant(
    client: &Client,
    variant: ProjectionSeedVariant,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    publish_messages(client, explanatory_projection_messages(variant)?).await
}

pub async fn publish_flawed_task_projection_events(
    client: &Client,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    publish_flawed_task_projection_events_variant(
        client,
        ProjectionSeedVariant::FULL_EXPLANATORY_WITH_DETAIL,
    )
    .await
}

pub async fn publish_flawed_task_projection_events_variant(
    client: &Client,
    variant: ProjectionSeedVariant,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    publish_messages(client, flawed_projection_messages(variant)?).await
}

pub async fn publish_handoff_projection_events_variant(
    client: &Client,
    variant: ProjectionSeedVariant,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    publish_messages(client, handoff_projection_messages(variant)?).await
}

pub async fn publish_constraint_projection_events_variant(
    client: &Client,
    variant: ProjectionSeedVariant,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    publish_messages(client, constraint_projection_messages(variant)?).await
}

async fn publish_messages(
    client: &Client,
    messages: Vec<(String, Vec<u8>)>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for (subject, payload) in messages {
        debug_log_value("publishing explanatory seed subject", &subject);
        client.publish(subject, payload.into()).await?;
    }
    client.flush().await?;
    Ok(())
}

fn focus_detail_for_variant(variant: ProjectionSeedVariant) -> String {
    match variant.relation_mode {
        RelationExplanationMode::DetailOnly => format!(
            "{FOCUS_DETAIL} because {DECISION_TO_TASK_RATIONALE} motivation={DECISION_TO_TASK_MOTIVATION} decision={DECISION_NODE_ID} caused_by={ROOT_NODE_ID}."
        ),
        _ => FOCUS_DETAIL.to_string(),
    }
}

fn failure_detail_for_variant(variant: ProjectionSeedVariant) -> String {
    match variant.relation_mode {
        RelationExplanationMode::DetailOnly => format!(
            "{FAILURE_DETAIL} because {FAILURE_EVIDENCE_RATIONALE} decision={BAD_DECISION_NODE_ID} caused_by={BAD_DECISION_NODE_ID}. Rehydrate from decision={BAD_DECISION_NODE_ID}."
        ),
        _ => FAILURE_DETAIL.to_string(),
    }
}

fn handoff_resumed_detail_for_variant(variant: ProjectionSeedVariant) -> String {
    match variant.relation_mode {
        RelationExplanationMode::DetailOnly => format!(
            "{HANDOFF_RESUMED_DETAIL} because {HANDOFF_RESUME_RATIONALE} blocker={HANDOFF_BLOCKER_RATIONALE} motivation={HANDOFF_RESUME_MOTIVATION} decision={HANDOFF_RESUME_DECISION_NODE_ID} caused_by={HANDOFF_TASK_STARTED_NODE_ID}."
        ),
        _ => HANDOFF_RESUMED_DETAIL.to_string(),
    }
}

fn constraint_task_detail_for_variant(variant: ProjectionSeedVariant) -> String {
    match variant.relation_mode {
        RelationExplanationMode::DetailOnly => format!(
            "{CONSTRAINT_TASK_DETAIL} because {CONSTRAINT_RATIONALE} motivation={CONSTRAINT_TASK_MOTIVATION} decision={CONSTRAINT_DECISION_NODE_ID} caused_by={CONSTRAINT_ROOT_NODE_ID}."
        ),
        _ => CONSTRAINT_TASK_DETAIL.to_string(),
    }
}

fn extend_with_meso_root_relations(
    related_nodes: &mut Vec<RelatedNodeReference>,
    variant: ProjectionSeedVariant,
    scenario: &str,
) {
    if variant.graph_scale != GraphScale::Meso {
        return;
    }

    for branch in 1..=5 {
        let evidence = format!("{scenario} distractor evidence {branch}");
        let rationale = format!(
            "{scenario} distractor branch {branch} appeared plausible but was not selected"
        );
        related_nodes.push(RelatedNodeReference {
            node_id: meso_noise_decision_id(scenario, branch),
            relation_type: ROOT_TO_DECISION_RELATION.to_string(),
            explanation: relation_explanation(
                variant,
                RelationSemanticClass::Causal,
                Some(rationale.as_str()),
                None,
                None,
                None,
                None,
                Some(evidence.as_str()),
                Some("low"),
                Some(10 + branch),
            ),
        });
    }
}

fn append_meso_noise_messages(
    messages: &mut Vec<(String, Vec<u8>)>,
    variant: ProjectionSeedVariant,
    scenario: &str,
    root_node_id: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if variant.graph_scale != GraphScale::Meso {
        return Ok(());
    }

    for branch in 1..=5 {
        let decision_id = meso_noise_decision_id(scenario, branch);
        let task_id = meso_noise_task_id(scenario, branch);
        let artifact_id = meso_noise_artifact_id(scenario, branch);
        let distractor_rationale = format!(
            "{scenario} distractor branch {branch} optimizes a secondary objective instead of the recovery path"
        );
        let distractor_motivation =
            format!("avoid unnecessary disruption in distractor branch {branch}");
        let distractor_method = format!("synthetic distractor verification {branch}");

        messages.push((
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope(
                    &format!("evt-{scenario}-meso-decision-{branch}"),
                    &decision_id,
                    "node",
                ),
                data: GraphNodeMaterializedData {
                    node_id: decision_id.clone(),
                    node_kind: DECISION_NODE_KIND.to_string(),
                    title: format!("Distractor decision {branch}"),
                    summary: format!(
                        "Secondary branch {branch} that should not dominate the recovery path."
                    ),
                    status: "observed".to_string(),
                    labels: vec!["decision".to_string(), "distractor".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: vec![RelatedNodeReference {
                        node_id: task_id.clone(),
                        relation_type: DECISION_TO_TASK_RELATION.to_string(),
                        explanation: relation_explanation(
                            variant,
                            RelationSemanticClass::Motivational,
                            Some(distractor_rationale.as_str()),
                            Some(distractor_motivation.as_str()),
                            None,
                            Some(decision_id.as_str()),
                            Some(root_node_id),
                            None,
                            Some("low"),
                            Some(20 + branch),
                        ),
                    }],
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ));
        messages.push((
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope(
                    &format!("evt-{scenario}-meso-task-{branch}"),
                    &task_id,
                    "node",
                ),
                data: GraphNodeMaterializedData {
                    node_id: task_id.clone(),
                    node_kind: FOCUS_NODE_KIND.to_string(),
                    title: format!("Distractor task {branch}"),
                    summary: format!(
                        "Secondary task {branch} attached to the scenario root for meso-scale noise."
                    ),
                    status: "ready".to_string(),
                    labels: vec!["task".to_string(), "distractor".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: vec![RelatedNodeReference {
                        node_id: artifact_id.clone(),
                        relation_type: TASK_TO_ARTIFACT_RELATION.to_string(),
                        explanation: relation_explanation(
                            variant,
                            RelationSemanticClass::Evidential,
                            Some(distractor_rationale.as_str()),
                            None,
                            Some(distractor_method.as_str()),
                            Some(decision_id.as_str()),
                            Some(decision_id.as_str()),
                            Some("synthetic distractor artifact"),
                            Some("low"),
                            Some(30 + branch),
                        ),
                    }],
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ));
        messages.push((
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope(
                    &format!("evt-{scenario}-meso-artifact-{branch}"),
                    &artifact_id,
                    "node",
                ),
                data: GraphNodeMaterializedData {
                    node_id: artifact_id.clone(),
                    node_kind: ARTIFACT_NODE_KIND.to_string(),
                    title: format!("Distractor artifact {branch}"),
                    summary: format!(
                        "Verification artifact for distractor branch {branch} in the meso graph."
                    ),
                    status: "ready".to_string(),
                    labels: vec!["artifact".to_string(), "distractor".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: Vec::new(),
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ));
    }

    Ok(())
}

fn meso_noise_decision_id(scenario: &str, branch: u32) -> String {
    format!("node:decision:{scenario}:distractor-{branch}")
}

fn meso_noise_task_id(scenario: &str, branch: u32) -> String {
    format!("node:task:{scenario}:distractor-{branch}")
}

fn meso_noise_artifact_id(scenario: &str, branch: u32) -> String {
    format!("node:artifact:{scenario}:distractor-{branch}")
}

fn explanatory_projection_messages(variant: ProjectionSeedVariant) -> ProjectionMessagesResult {
    let mut root_related_nodes = vec![RelatedNodeReference {
        node_id: DECISION_NODE_ID.to_string(),
        relation_type: ROOT_TO_DECISION_RELATION.to_string(),
        explanation: relation_explanation(
            variant,
            RelationSemanticClass::Causal,
            Some(ROOT_TO_DECISION_RATIONALE),
            None,
            None,
            None,
            None,
            Some("port manifold anomaly report"),
            Some("high"),
            Some(1),
        ),
    }];
    extend_with_meso_root_relations(&mut root_related_nodes, variant, "explanatory");

    let mut messages = vec![
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope("evt-root-1", ROOT_NODE_ID, "node"),
                data: GraphNodeMaterializedData {
                    node_id: ROOT_NODE_ID.to_string(),
                    node_kind: ROOT_NODE_KIND.to_string(),
                    title: "Port manifold breach".to_string(),
                    summary:
                        "A containment failure is cascading through the port-side manifold."
                            .to_string(),
                    status: "at_risk".to_string(),
                    labels: vec!["incident".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: root_related_nodes,
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope("evt-decision-1", DECISION_NODE_ID, "node"),
                data: GraphNodeMaterializedData {
                    node_id: DECISION_NODE_ID.to_string(),
                    node_kind: DECISION_NODE_KIND.to_string(),
                    title: "Reroute reserve power".to_string(),
                    summary: "Shift reserve power into containment before repair crews proceed."
                        .to_string(),
                    status: "accepted".to_string(),
                    labels: vec!["decision".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: vec![RelatedNodeReference {
                        node_id: FOCUS_NODE_ID.to_string(),
                        relation_type: DECISION_TO_TASK_RELATION.to_string(),
                        explanation: relation_explanation(
                            variant,
                            RelationSemanticClass::Motivational,
                            Some(DECISION_TO_TASK_RATIONALE),
                            Some(DECISION_TO_TASK_MOTIVATION),
                            None,
                            Some(DECISION_NODE_ID),
                            Some(ROOT_NODE_ID),
                            None,
                            Some("high"),
                            Some(2),
                        ),
                    }],
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope("evt-task-1", FOCUS_NODE_ID, "node"),
                data: GraphNodeMaterializedData {
                    node_id: FOCUS_NODE_ID.to_string(),
                    node_kind: FOCUS_NODE_KIND.to_string(),
                    title: "Reroute EPS grid".to_string(),
                    summary: "Move reserve EPS power into the damaged containment branch."
                        .to_string(),
                    status: "ready".to_string(),
                    labels: vec!["task".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: vec![RelatedNodeReference {
                        node_id: ARTIFACT_NODE_ID.to_string(),
                        relation_type: TASK_TO_ARTIFACT_RELATION.to_string(),
                        explanation: relation_explanation(
                            variant,
                            RelationSemanticClass::Evidential,
                            Some("telemetry must validate the reroute before the manifold is reopened"),
                            None,
                            Some(TASK_TO_ARTIFACT_METHOD),
                            Some(DECISION_NODE_ID),
                            Some(DECISION_NODE_ID),
                            Some("telemetry capture"),
                            Some("high"),
                            Some(3),
                        ),
                    }],
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope("evt-artifact-1", ARTIFACT_NODE_ID, "node"),
                data: GraphNodeMaterializedData {
                    node_id: ARTIFACT_NODE_ID.to_string(),
                    node_kind: ARTIFACT_NODE_KIND.to_string(),
                    title: "Containment telemetry capture".to_string(),
                    summary:
                        "Telemetry proving the EPS reroute kept the manifold inside safe tolerances."
                            .to_string(),
                    status: "ready".to_string(),
                    labels: vec!["artifact".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: Vec::new(),
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
    ];

    if variant.detail_enabled() {
        messages.push((
            subject("node.detail.materialized"),
            serde_json::to_vec(&NodeDetailMaterializedEvent {
                envelope: envelope("evt-task-detail-1", FOCUS_NODE_ID, "node_detail"),
                data: NodeDetailMaterializedData {
                    node_id: FOCUS_NODE_ID.to_string(),
                    detail: focus_detail_for_variant(variant),
                    content_hash: "sha256:eps-grid-reroute-v1".to_string(),
                    revision: 3,
                },
            })?,
        ));
    }

    append_meso_noise_messages(&mut messages, variant, "explanatory", ROOT_NODE_ID)?;
    Ok(messages)
}

fn flawed_projection_messages(variant: ProjectionSeedVariant) -> ProjectionMessagesResult {
    let mut root_related_nodes = vec![RelatedNodeReference {
        node_id: BAD_DECISION_NODE_ID.to_string(),
        relation_type: ROOT_TO_DECISION_RELATION.to_string(),
        explanation: relation_explanation(
            variant,
            RelationSemanticClass::Causal,
            Some("crew attempted to preserve passenger comfort while troubleshooting"),
            None,
            None,
            None,
            None,
            Some("bridge comfort-load request"),
            Some("low"),
            Some(1),
        ),
    }];
    extend_with_meso_root_relations(&mut root_related_nodes, variant, "flawed");

    let mut messages = vec![
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope("evt-failure-root-1", ROOT_NODE_ID, "node"),
                data: GraphNodeMaterializedData {
                    node_id: ROOT_NODE_ID.to_string(),
                    node_kind: ROOT_NODE_KIND.to_string(),
                    title: "Port manifold breach".to_string(),
                    summary: "A containment failure is cascading through the port-side manifold."
                        .to_string(),
                    status: "at_risk".to_string(),
                    labels: vec!["incident".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: root_related_nodes,
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope("evt-bad-decision-1", BAD_DECISION_NODE_ID, "node"),
                data: GraphNodeMaterializedData {
                    node_id: BAD_DECISION_NODE_ID.to_string(),
                    node_kind: DECISION_NODE_KIND.to_string(),
                    title: "Preserve comfort load".to_string(),
                    summary:
                        "Keep passenger comfort systems online while attempting a minimal reroute."
                            .to_string(),
                    status: "accepted".to_string(),
                    labels: vec!["decision".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: vec![
                        RelatedNodeReference {
                            node_id: BAD_TASK_NODE_ID.to_string(),
                            relation_type: DECISION_TO_TASK_RELATION.to_string(),
                            explanation: relation_explanation(
                                variant,
                                RelationSemanticClass::Motivational,
                                Some(BAD_DECISION_TO_TASK_RATIONALE),
                                Some(
                                    "avoid visible disruption while containment is still unstable",
                                ),
                                None,
                                Some(BAD_DECISION_NODE_ID),
                                Some(ROOT_NODE_ID),
                                None,
                                Some("low"),
                                Some(2),
                            ),
                        },
                        RelatedNodeReference {
                            node_id: DECISION_NODE_ID.to_string(),
                            relation_type: RECOVERY_DECISION_RELATION.to_string(),
                            explanation: relation_explanation(
                                variant,
                                RelationSemanticClass::Motivational,
                                Some(RECOVERY_DECISION_RATIONALE),
                                Some(RECOVERY_DECISION_MOTIVATION),
                                None,
                                Some(DECISION_NODE_ID),
                                Some(BAD_DECISION_NODE_ID),
                                Some("failure telemetry and rehydration analysis"),
                                Some("high"),
                                Some(4),
                            ),
                        },
                    ],
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope("evt-bad-task-1", BAD_TASK_NODE_ID, "node"),
                data: GraphNodeMaterializedData {
                    node_id: BAD_TASK_NODE_ID.to_string(),
                    node_kind: FOCUS_NODE_KIND.to_string(),
                    title: "Apply minimal reroute".to_string(),
                    summary: "Move a small amount of reserve power while preserving comfort load."
                        .to_string(),
                    status: "done".to_string(),
                    labels: vec!["task".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: vec![RelatedNodeReference {
                        node_id: FAILURE_FOCUS_NODE_ID.to_string(),
                        relation_type: TASK_TO_ARTIFACT_RELATION.to_string(),
                        explanation: relation_explanation(
                            variant,
                            RelationSemanticClass::Evidential,
                            Some(FAILURE_EVIDENCE_RATIONALE),
                            None,
                            Some(TASK_TO_ARTIFACT_METHOD),
                            Some(BAD_DECISION_NODE_ID),
                            Some(BAD_DECISION_NODE_ID),
                            Some("failed telemetry capture"),
                            Some("high"),
                            Some(3),
                        ),
                    }],
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope("evt-failure-artifact-1", FAILURE_FOCUS_NODE_ID, "node"),
                data: GraphNodeMaterializedData {
                    node_id: FAILURE_FOCUS_NODE_ID.to_string(),
                    node_kind: FAILURE_FOCUS_NODE_KIND.to_string(),
                    title: "Failed containment telemetry".to_string(),
                    summary: "Telemetry proving the minimal reroute did not restore safe margins."
                        .to_string(),
                    status: "failed".to_string(),
                    labels: vec!["artifact".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: Vec::new(),
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope("evt-recovery-decision-1", DECISION_NODE_ID, "node"),
                data: GraphNodeMaterializedData {
                    node_id: DECISION_NODE_ID.to_string(),
                    node_kind: DECISION_NODE_KIND.to_string(),
                    title: "Reroute reserve power".to_string(),
                    summary:
                        "Replace the failed minimal reroute with the containment-priority plan."
                            .to_string(),
                    status: "accepted".to_string(),
                    labels: vec!["decision".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: vec![RelatedNodeReference {
                        node_id: FOCUS_NODE_ID.to_string(),
                        relation_type: DECISION_TO_TASK_RELATION.to_string(),
                        explanation: relation_explanation(
                            variant,
                            RelationSemanticClass::Motivational,
                            Some(DECISION_TO_TASK_RATIONALE),
                            Some(DECISION_TO_TASK_MOTIVATION),
                            None,
                            Some(DECISION_NODE_ID),
                            Some(BAD_DECISION_NODE_ID),
                            None,
                            Some("high"),
                            Some(5),
                        ),
                    }],
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope("evt-recovery-task-1", FOCUS_NODE_ID, "node"),
                data: GraphNodeMaterializedData {
                    node_id: FOCUS_NODE_ID.to_string(),
                    node_kind: FOCUS_NODE_KIND.to_string(),
                    title: "Reroute EPS grid".to_string(),
                    summary: "Re-run the reroute with containment prioritized over comfort load."
                        .to_string(),
                    status: "ready".to_string(),
                    labels: vec!["task".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: vec![RelatedNodeReference {
                        node_id: ARTIFACT_NODE_ID.to_string(),
                        relation_type: TASK_TO_ARTIFACT_RELATION.to_string(),
                        explanation: relation_explanation(
                            variant,
                            RelationSemanticClass::Evidential,
                            Some(RECOVERY_SUCCESS_RATIONALE),
                            None,
                            Some(TASK_TO_ARTIFACT_METHOD),
                            Some(DECISION_NODE_ID),
                            Some(BAD_DECISION_NODE_ID),
                            Some("restored telemetry capture"),
                            Some("high"),
                            Some(6),
                        ),
                    }],
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope("evt-recovery-artifact-1", ARTIFACT_NODE_ID, "node"),
                data: GraphNodeMaterializedData {
                    node_id: ARTIFACT_NODE_ID.to_string(),
                    node_kind: ARTIFACT_NODE_KIND.to_string(),
                    title: "Restored containment telemetry".to_string(),
                    summary:
                        "Telemetry proving the corrected reroute restored safe containment margins."
                            .to_string(),
                    status: "ready".to_string(),
                    labels: vec!["artifact".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: Vec::new(),
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
    ];

    if variant.detail_enabled() {
        messages.push((
            subject("node.detail.materialized"),
            serde_json::to_vec(&NodeDetailMaterializedEvent {
                envelope: envelope(
                    "evt-failure-artifact-detail-1",
                    FAILURE_FOCUS_NODE_ID,
                    "node_detail",
                ),
                data: NodeDetailMaterializedData {
                    node_id: FAILURE_FOCUS_NODE_ID.to_string(),
                    detail: failure_detail_for_variant(variant),
                    content_hash: "sha256:failed-telemetry".to_string(),
                    revision: 2,
                },
            })?,
        ));
        messages.push((
            subject("node.detail.materialized"),
            serde_json::to_vec(&NodeDetailMaterializedEvent {
                envelope: envelope("evt-recovery-task-detail-1", FOCUS_NODE_ID, "node_detail"),
                data: NodeDetailMaterializedData {
                    node_id: FOCUS_NODE_ID.to_string(),
                    detail: focus_detail_for_variant(variant),
                    content_hash: "sha256:eps-grid-reroute-recovery-v1".to_string(),
                    revision: 4,
                },
            })?,
        ));
    }

    append_meso_noise_messages(&mut messages, variant, "flawed", ROOT_NODE_ID)?;
    Ok(messages)
}

fn handoff_projection_messages(variant: ProjectionSeedVariant) -> ProjectionMessagesResult {
    let mut root_related_nodes = vec![RelatedNodeReference {
        node_id: HANDOFF_INITIAL_DECISION_NODE_ID.to_string(),
        relation_type: ROOT_TO_DECISION_RELATION.to_string(),
        explanation: relation_explanation(
            variant,
            RelationSemanticClass::Causal,
            Some(HANDOFF_INITIAL_RATIONALE),
            None,
            None,
            None,
            None,
            Some("reactor bay pressure alarm"),
            Some("high"),
            Some(1),
        ),
    }];
    extend_with_meso_root_relations(&mut root_related_nodes, variant, "handoff");

    let mut messages = vec![
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope("evt-handoff-root-1", HANDOFF_ROOT_NODE_ID, "node"),
                data: GraphNodeMaterializedData {
                    node_id: HANDOFF_ROOT_NODE_ID.to_string(),
                    node_kind: HANDOFF_ROOT_NODE_KIND.to_string(),
                    title: "Reactor-bay coolant leak".to_string(),
                    summary: "Coolant loss is pushing pressure back into the reactor bay."
                        .to_string(),
                    status: "at_risk".to_string(),
                    labels: vec!["incident".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: root_related_nodes,
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope(
                    "evt-handoff-initial-decision-1",
                    HANDOFF_INITIAL_DECISION_NODE_ID,
                    "node",
                ),
                data: GraphNodeMaterializedData {
                    node_id: HANDOFF_INITIAL_DECISION_NODE_ID.to_string(),
                    node_kind: DECISION_NODE_KIND.to_string(),
                    title: "Attempt remote isolation".to_string(),
                    summary: "Try to contain the leak remotely before dispatching a manual team."
                        .to_string(),
                    status: "accepted".to_string(),
                    labels: vec!["decision".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: vec![RelatedNodeReference {
                        node_id: HANDOFF_TASK_STARTED_NODE_ID.to_string(),
                        relation_type: DECISION_TO_TASK_RELATION.to_string(),
                        explanation: relation_explanation(
                            variant,
                            RelationSemanticClass::Motivational,
                            Some(HANDOFF_INITIAL_RATIONALE),
                            Some(HANDOFF_INITIAL_MOTIVATION),
                            None,
                            Some(HANDOFF_INITIAL_DECISION_NODE_ID),
                            Some(HANDOFF_ROOT_NODE_ID),
                            None,
                            Some("high"),
                            Some(2),
                        ),
                    }],
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope(
                    "evt-handoff-started-task-1",
                    HANDOFF_TASK_STARTED_NODE_ID,
                    "node",
                ),
                data: GraphNodeMaterializedData {
                    node_id: HANDOFF_TASK_STARTED_NODE_ID.to_string(),
                    node_kind: FOCUS_NODE_KIND.to_string(),
                    title: "Run remote isolation".to_string(),
                    summary: "Drive the coolant isolation remotely from the control room."
                        .to_string(),
                    status: "blocked".to_string(),
                    labels: vec!["task".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: vec![RelatedNodeReference {
                        node_id: HANDOFF_BLOCKER_NODE_ID.to_string(),
                        relation_type: "BLOCKED_BY".to_string(),
                        explanation: relation_explanation(
                            variant,
                            RelationSemanticClass::Evidential,
                            Some(HANDOFF_BLOCKER_RATIONALE),
                            None,
                            Some("remote actuator self-test"),
                            Some(HANDOFF_INITIAL_DECISION_NODE_ID),
                            Some(HANDOFF_TASK_STARTED_NODE_ID),
                            Some("override actuator telemetry"),
                            Some("high"),
                            Some(3),
                        ),
                    }],
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope("evt-handoff-blocker-1", HANDOFF_BLOCKER_NODE_ID, "node"),
                data: GraphNodeMaterializedData {
                    node_id: HANDOFF_BLOCKER_NODE_ID.to_string(),
                    node_kind: ARTIFACT_NODE_KIND.to_string(),
                    title: "Manual override jam".to_string(),
                    summary: "Debris jammed the valve cage and stopped the remote actuator."
                        .to_string(),
                    status: "ready".to_string(),
                    labels: vec!["artifact".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: vec![RelatedNodeReference {
                        node_id: HANDOFF_RESUME_DECISION_NODE_ID.to_string(),
                        relation_type: ROOT_TO_DECISION_RELATION.to_string(),
                        explanation: relation_explanation(
                            variant,
                            RelationSemanticClass::Causal,
                            Some("the blocked override requires a manual specialist to continue"),
                            None,
                            None,
                            None,
                            Some(HANDOFF_TASK_STARTED_NODE_ID),
                            Some("override jam alert"),
                            Some("high"),
                            Some(4),
                        ),
                    }],
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope(
                    "evt-handoff-resume-decision-1",
                    HANDOFF_RESUME_DECISION_NODE_ID,
                    "node",
                ),
                data: GraphNodeMaterializedData {
                    node_id: HANDOFF_RESUME_DECISION_NODE_ID.to_string(),
                    node_kind: DECISION_NODE_KIND.to_string(),
                    title: "Dispatch EVA specialist".to_string(),
                    summary:
                        "Hand the blocked isolation over to the EVA team at the override panel."
                            .to_string(),
                    status: "accepted".to_string(),
                    labels: vec!["decision".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: vec![RelatedNodeReference {
                        node_id: HANDOFF_RESUMED_TASK_NODE_ID.to_string(),
                        relation_type: DECISION_TO_TASK_RELATION.to_string(),
                        explanation: relation_explanation(
                            variant,
                            RelationSemanticClass::Motivational,
                            Some(HANDOFF_RESUME_RATIONALE),
                            Some(HANDOFF_RESUME_MOTIVATION),
                            None,
                            Some(HANDOFF_RESUME_DECISION_NODE_ID),
                            Some(HANDOFF_TASK_STARTED_NODE_ID),
                            None,
                            Some("high"),
                            Some(5),
                        ),
                    }],
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope(
                    "evt-handoff-resumed-task-1",
                    HANDOFF_RESUMED_TASK_NODE_ID,
                    "node",
                ),
                data: GraphNodeMaterializedData {
                    node_id: HANDOFF_RESUMED_TASK_NODE_ID.to_string(),
                    node_kind: FOCUS_NODE_KIND.to_string(),
                    title: "Perform manual override isolation".to_string(),
                    summary: "Continue the isolation manually from the jammed override panel."
                        .to_string(),
                    status: "ready".to_string(),
                    labels: vec!["task".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: vec![RelatedNodeReference {
                        node_id: HANDOFF_SUCCESS_ARTIFACT_NODE_ID.to_string(),
                        relation_type: TASK_TO_ARTIFACT_RELATION.to_string(),
                        explanation: relation_explanation(
                            variant,
                            RelationSemanticClass::Evidential,
                            Some(HANDOFF_SUCCESS_RATIONALE),
                            None,
                            Some(HANDOFF_SUCCESS_METHOD),
                            Some(HANDOFF_RESUME_DECISION_NODE_ID),
                            Some(HANDOFF_TASK_STARTED_NODE_ID),
                            Some("coolant isolation telemetry"),
                            Some("high"),
                            Some(6),
                        ),
                    }],
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope(
                    "evt-handoff-success-artifact-1",
                    HANDOFF_SUCCESS_ARTIFACT_NODE_ID,
                    "node",
                ),
                data: GraphNodeMaterializedData {
                    node_id: HANDOFF_SUCCESS_ARTIFACT_NODE_ID.to_string(),
                    node_kind: ARTIFACT_NODE_KIND.to_string(),
                    title: "Coolant isolation confirmed".to_string(),
                    summary:
                        "Telemetry proving the manual override completed the coolant isolation."
                            .to_string(),
                    status: "ready".to_string(),
                    labels: vec!["artifact".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: Vec::new(),
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
    ];

    if variant.detail_enabled() {
        messages.push((
            subject("node.detail.materialized"),
            serde_json::to_vec(&NodeDetailMaterializedEvent {
                envelope: envelope(
                    "evt-handoff-resumed-detail-1",
                    HANDOFF_RESUMED_TASK_NODE_ID,
                    "node_detail",
                ),
                data: NodeDetailMaterializedData {
                    node_id: HANDOFF_RESUMED_TASK_NODE_ID.to_string(),
                    detail: handoff_resumed_detail_for_variant(variant),
                    content_hash: "sha256:handoff-resume-manual-override".to_string(),
                    revision: 1,
                },
            })?,
        ));
    }

    append_meso_noise_messages(&mut messages, variant, "handoff", HANDOFF_ROOT_NODE_ID)?;
    Ok(messages)
}

fn constraint_projection_messages(variant: ProjectionSeedVariant) -> ProjectionMessagesResult {
    let mut root_related_nodes = vec![RelatedNodeReference {
        node_id: CONSTRAINT_DECISION_NODE_ID.to_string(),
        relation_type: CONSTRAINT_RELATION.to_string(),
        explanation: relation_explanation(
            variant,
            RelationSemanticClass::Constraint,
            Some(CONSTRAINT_RATIONALE),
            None,
            None,
            None,
            None,
            Some("bay dosimeter alarm"),
            Some("high"),
            Some(1),
        ),
    }];
    extend_with_meso_root_relations(&mut root_related_nodes, variant, "constraint");

    let mut messages = vec![
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope("evt-constraint-root-1", CONSTRAINT_ROOT_NODE_ID, "node"),
                data: GraphNodeMaterializedData {
                    node_id: CONSTRAINT_ROOT_NODE_ID.to_string(),
                    node_kind: CONSTRAINT_ROOT_NODE_KIND.to_string(),
                    title: "Keep crew outside the plasma bay".to_string(),
                    summary:
                        "A safety exclusion zone blocks any manual calibration inside the bay."
                            .to_string(),
                    status: "active".to_string(),
                    labels: vec!["constraint".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: root_related_nodes,
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope(
                    "evt-constraint-decision-1",
                    CONSTRAINT_DECISION_NODE_ID,
                    "node",
                ),
                data: GraphNodeMaterializedData {
                    node_id: CONSTRAINT_DECISION_NODE_ID.to_string(),
                    node_kind: DECISION_NODE_KIND.to_string(),
                    title: "Remote calibration first".to_string(),
                    summary: "Restore coolant flow remotely before any engineer enters the bay."
                        .to_string(),
                    status: "accepted".to_string(),
                    labels: vec!["decision".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: vec![RelatedNodeReference {
                        node_id: CONSTRAINT_TASK_NODE_ID.to_string(),
                        relation_type: DECISION_TO_TASK_RELATION.to_string(),
                        explanation: relation_explanation(
                            variant,
                            RelationSemanticClass::Motivational,
                            Some(CONSTRAINT_TASK_RATIONALE),
                            Some(CONSTRAINT_TASK_MOTIVATION),
                            None,
                            Some(CONSTRAINT_DECISION_NODE_ID),
                            Some(CONSTRAINT_ROOT_NODE_ID),
                            None,
                            Some("high"),
                            Some(2),
                        ),
                    }],
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&GraphNodeMaterializedEvent {
                envelope: envelope("evt-constraint-task-1", CONSTRAINT_TASK_NODE_ID, "node"),
                data: GraphNodeMaterializedData {
                    node_id: CONSTRAINT_TASK_NODE_ID.to_string(),
                    node_kind: FOCUS_NODE_KIND.to_string(),
                    title: "Run remote calibration".to_string(),
                    summary: "Use the remote channel even though it is slower than an on-site fix."
                        .to_string(),
                    status: "ready".to_string(),
                    labels: vec!["task".to_string()],
                    properties: BTreeMap::new(),
                    related_nodes: Vec::new(),
                    source_kind: None,
                    source_agent: None,
                    observed_at: None,
                },
            })?,
        ),
    ];

    if variant.detail_enabled() {
        messages.push((
            subject("node.detail.materialized"),
            serde_json::to_vec(&NodeDetailMaterializedEvent {
                envelope: envelope(
                    "evt-constraint-task-detail-1",
                    CONSTRAINT_TASK_NODE_ID,
                    "node_detail",
                ),
                data: NodeDetailMaterializedData {
                    node_id: CONSTRAINT_TASK_NODE_ID.to_string(),
                    detail: constraint_task_detail_for_variant(variant),
                    content_hash: "sha256:constraint-remote-calibration".to_string(),
                    revision: 1,
                },
            })?,
        ));
    }

    append_meso_noise_messages(
        &mut messages,
        variant,
        "constraint",
        CONSTRAINT_ROOT_NODE_ID,
    )?;
    Ok(messages)
}

#[allow(clippy::too_many_arguments)]
fn relation_explanation(
    variant: ProjectionSeedVariant,
    semantic_class: RelationSemanticClass,
    rationale: Option<&str>,
    motivation: Option<&str>,
    method: Option<&str>,
    decision_id: Option<&str>,
    caused_by_node_id: Option<&str>,
    evidence: Option<&str>,
    confidence: Option<&str>,
    sequence: Option<u32>,
) -> RelatedNodeExplanationData {
    match variant.relation_mode {
        RelationExplanationMode::Explanatory => RelatedNodeExplanationData {
            semantic_class,
            rationale: rationale.map(str::to_string),
            motivation: motivation.map(str::to_string),
            method: method.map(str::to_string),
            decision_id: decision_id.map(str::to_string),
            caused_by_node_id: caused_by_node_id.map(str::to_string),
            evidence: evidence.map(str::to_string),
            confidence: confidence.map(str::to_string),
            sequence,
        },
        RelationExplanationMode::StructuralOnly | RelationExplanationMode::DetailOnly => {
            RelatedNodeExplanationData {
                semantic_class: RelationSemanticClass::Structural,
                rationale: None,
                motivation: None,
                method: None,
                decision_id: None,
                caused_by_node_id: None,
                evidence: None,
                confidence: None,
                sequence,
            }
        }
    }
}

fn subject(suffix: &str) -> String {
    format!("{SUBJECT_PREFIX}.{suffix}")
}

fn envelope(event_id: &str, aggregate_id: &str, aggregate_type: &str) -> ProjectionEnvelope {
    ProjectionEnvelope {
        event_id: event_id.to_string(),
        correlation_id: "corr-paper-use-cases".to_string(),
        causation_id: "cause-paper-use-cases".to_string(),
        occurred_at: "2026-03-18T12:00:00Z".to_string(),
        aggregate_id: aggregate_id.to_string(),
        aggregate_type: aggregate_type.to_string(),
        schema_version: "v1beta1".to_string(),
    }
}
