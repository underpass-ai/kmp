use std::collections::BTreeMap;
use std::error::Error;

use async_nats::Client;
use kmp_application::{
    GraphNodeMaterializedData, GraphNodeMaterializedEvent, NodeDetailMaterializedData,
    NodeDetailMaterializedEvent, ProjectionEnvelope, RelatedNodeExplanationData,
    RelatedNodeReference,
};
use kmp_domain::RelationSemanticClass;

pub const DEFAULT_SUBJECT_PREFIX: &str = "rehydration";

pub const ROOT_NODE_ID: &str = "incident:starship-odyssey:port-manifold-breach";
pub const ROOT_LABEL: &str = "Story";
pub const ROOT_TITLE: &str = "Stabilize the Odyssey before the jump window closes";
pub const ROOT_DETAIL: &str = "Odyssey is handling a cascading containment failure in the port antimatter manifold. Engineering, navigation, medical, and deck operations must stay synchronized while the bridge delays the jump and protects civilian decks.";

pub const TASK_ID: &str = "task:stabilize-port-manifold";
pub const TASK_TITLE: &str = "Stabilize the port antimatter manifold";
pub const TASK_DETAIL: &str = "Seal the breach, rebalance plasma flow across the port injectors, and keep the warp core below the automatic scram threshold while the ship remains in sublight.";

pub const POWER_TASK_ID: &str = "task:reroute-eps-grid";
pub const NAV_TASK_ID: &str = "task:calibrate-nav-drift";

pub const DECISION_ID: &str = "decision:reroute-reserve-power";
pub const DECISION_TITLE: &str = "Reroute reserve power to containment";
pub const DECISION_DETAIL: &str = "The chief engineer approved diverting reserve power from comfort systems to propulsion containment so the manifold can survive the repair window without triggering a full reactor scram.";
pub const JUMP_DECISION_ID: &str = "decision:delay-jump-window";

pub const PROPULSION_SUBSYSTEM_TITLE: &str = "Propulsion containment";
pub const CHIEF_ENGINEER_TITLE: &str = "Chief Engineer T. Garcia";
pub const EXPLORER_WORKSTREAM_ID: &str = "workstream:containment-control-loop";
pub const EXPLORER_WORKSTREAM_TITLE: &str = "Containment control loop recovery";
pub const EXPLORER_WORKSTREAM_DETAIL: &str = "Engineering split the manifold recovery into an explorer-friendly branch so command can zoom from the incident to the containment control loop, then down to the alignment checklist, and finally to the telemetry artifact that proves the fix is safe.";
pub const EXPLORER_CHECKLIST_ID: &str = "checklist:align-plasma-baffles";
pub const EXPLORER_CHECKLIST_TITLE: &str = "Align the plasma baffles";
pub const EXPLORER_LEAF_ID: &str = "artifact:plasma-baffle-telemetry";
pub const EXPLORER_LEAF_TITLE: &str = "Plasma baffle telemetry capture";
pub const EXPLORER_LEAF_DETAIL: &str = "The telemetry capture isolates the oscillation to baffle segment C and confirms the manifold can be stabilized without triggering a full reactor scram.";

pub const RELATION_DEPENDS_ON: &str = "DEPENDS_ON";
pub const RELATION_IMPACTS: &str = "IMPACTS";
pub const RELATION_DECISION_REQUIRES: &str = "DECISION_REQUIRES";

pub const EXPECTED_TASK_COUNT: usize = 6;
pub const EXPECTED_DECISION_COUNT: usize = 4;
pub const EXPECTED_DECISION_EDGE_COUNT: usize = 3;
pub const EXPECTED_IMPACT_COUNT: usize = 4;
pub const EXPECTED_COMPLETED_TASK_COUNT: i32 = 2;
pub const EXPECTED_ROOT_RELATION_COUNT: usize = 15;
pub const EXPECTED_NEIGHBOR_COUNT: usize = 17;
pub const EXPECTED_RELATIONSHIP_COUNT: usize = 30;
pub const EXPECTED_DETAIL_COUNT: usize = 17;
pub const EXPECTED_SELECTED_NODE_COUNT: u32 = 18;
pub const EXPECTED_SELECTED_RELATIONSHIP_COUNT: u32 = 30;
pub const EXPECTED_TOKEN_BUDGET_HINT: i32 = 2304;

pub const STARSHIP_NODE_IDS: &[&str] = &[
    ROOT_NODE_ID,
    DECISION_ID,
    JUMP_DECISION_ID,
    "decision:isolate-docking-ring",
    "decision:manual-throttle-guard",
    TASK_ID,
    POWER_TASK_ID,
    NAV_TASK_ID,
    "task:seal-docking-ring-twelve",
    "task:stage-medical-response",
    "task:validate-telemetry-mirror",
    "subsystem:propulsion",
    "subsystem:navigation",
    "subsystem:life-support",
    "crew:chief-engineer",
    EXPLORER_WORKSTREAM_ID,
    EXPLORER_CHECKLIST_ID,
    EXPLORER_LEAF_ID,
];

pub type ProjectionMessagesResult = Result<Vec<(String, Vec<u8>)>, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Copy)]
struct RelationSeed {
    node_id: &'static str,
    relation_type: &'static str,
}

#[derive(Clone, Copy)]
struct DetailSeed {
    event_id: &'static str,
    detail: &'static str,
    content_hash: &'static str,
    revision: u64,
}

#[derive(Clone, Copy)]
struct NodeSeed {
    event_id: &'static str,
    node_id: &'static str,
    node_kind: &'static str,
    title: &'static str,
    summary: &'static str,
    status: &'static str,
    labels: &'static [&'static str],
    properties: &'static [(&'static str, &'static str)],
    related_nodes: &'static [RelationSeed],
    detail: Option<DetailSeed>,
}

const ROOT_RELATED_NODES: &[RelationSeed] = &[
    RelationSeed {
        node_id: DECISION_ID,
        relation_type: "RECORDS",
    },
    RelationSeed {
        node_id: JUMP_DECISION_ID,
        relation_type: "RECORDS",
    },
    RelationSeed {
        node_id: "decision:isolate-docking-ring",
        relation_type: "RECORDS",
    },
    RelationSeed {
        node_id: "decision:manual-throttle-guard",
        relation_type: "RECORDS",
    },
    RelationSeed {
        node_id: TASK_ID,
        relation_type: "HAS_TASK",
    },
    RelationSeed {
        node_id: POWER_TASK_ID,
        relation_type: "HAS_TASK",
    },
    RelationSeed {
        node_id: NAV_TASK_ID,
        relation_type: "HAS_TASK",
    },
    RelationSeed {
        node_id: "task:seal-docking-ring-twelve",
        relation_type: "HAS_TASK",
    },
    RelationSeed {
        node_id: "task:stage-medical-response",
        relation_type: "HAS_TASK",
    },
    RelationSeed {
        node_id: "task:validate-telemetry-mirror",
        relation_type: "HAS_TASK",
    },
    RelationSeed {
        node_id: "subsystem:propulsion",
        relation_type: "AFFECTS",
    },
    RelationSeed {
        node_id: "subsystem:navigation",
        relation_type: "AFFECTS",
    },
    RelationSeed {
        node_id: "subsystem:life-support",
        relation_type: "AFFECTS",
    },
    RelationSeed {
        node_id: "crew:chief-engineer",
        relation_type: "ASSIGNS",
    },
    RelationSeed {
        node_id: EXPLORER_WORKSTREAM_ID,
        relation_type: "TRACKS",
    },
];

const KERNEL_GRAPH_NODES: &[NodeSeed] = &[
    NodeSeed {
        event_id: "evt-kernel-root-1",
        node_id: ROOT_NODE_ID,
        node_kind: ROOT_LABEL,
        title: ROOT_TITLE,
        summary: "A containment failure in the port antimatter manifold is destabilizing propulsion, navigation, and life-support coordination.",
        status: "AT_RISK",
        labels: &[ROOT_LABEL],
        properties: &[
            ("created_by", "bridge-planner"),
            ("plan_id", "mission-odyssey-red-alert"),
        ],
        related_nodes: ROOT_RELATED_NODES,
        detail: Some(DetailSeed {
            event_id: "evt-kernel-root-detail-1",
            detail: ROOT_DETAIL,
            content_hash: "hash-odyssey-root",
            revision: 5,
        }),
    },
    NodeSeed {
        event_id: "evt-kernel-workstream-control-loop-1",
        node_id: EXPLORER_WORKSTREAM_ID,
        node_kind: "Workstream",
        title: EXPLORER_WORKSTREAM_TITLE,
        summary: "Track the recovery branch from the containment control loop down to the telemetry artifact that validates the repair.",
        status: "IN_PROGRESS",
        labels: &["Workstream", "Explorer"],
        properties: &[("owner", CHIEF_ENGINEER_TITLE), ("branch", "explorer-demo")],
        related_nodes: &[RelationSeed {
            node_id: EXPLORER_CHECKLIST_ID,
            relation_type: "GUIDES",
        }],
        detail: Some(DetailSeed {
            event_id: "evt-kernel-workstream-control-loop-detail-1",
            detail: EXPLORER_WORKSTREAM_DETAIL,
            content_hash: "hash-odyssey-workstream-control-loop",
            revision: 2,
        }),
    },
    NodeSeed {
        event_id: "evt-kernel-checklist-baffles-1",
        node_id: EXPLORER_CHECKLIST_ID,
        node_kind: "Checklist",
        title: EXPLORER_CHECKLIST_TITLE,
        summary: "Walk the alignment sequence that keeps the manifold stable while the control loop is recovered.",
        status: "READY",
        labels: &["Checklist", "Explorer"],
        properties: &[
            ("owner", "Lt. S. Rao"),
            ("sequence", "containment-recovery"),
        ],
        related_nodes: &[RelationSeed {
            node_id: EXPLORER_LEAF_ID,
            relation_type: "PRODUCES",
        }],
        detail: None,
    },
    NodeSeed {
        event_id: "evt-kernel-artifact-baffle-telemetry-1",
        node_id: EXPLORER_LEAF_ID,
        node_kind: "Artifact",
        title: EXPLORER_LEAF_TITLE,
        summary: "Captured telemetry that proves the plasma baffles are aligned closely enough to avoid a scram.",
        status: "READY",
        labels: &["Artifact", "Explorer"],
        properties: &[
            ("format", "telemetry-capture"),
            ("owner", "Engineering telemetry"),
        ],
        related_nodes: &[],
        detail: Some(DetailSeed {
            event_id: "evt-kernel-artifact-baffle-telemetry-detail-1",
            detail: EXPLORER_LEAF_DETAIL,
            content_hash: "hash-odyssey-artifact-baffle-telemetry",
            revision: 1,
        }),
    },
    NodeSeed {
        event_id: "evt-kernel-decision-power-1",
        node_id: DECISION_ID,
        node_kind: "Decision",
        title: DECISION_TITLE,
        summary: "Sacrifice noncritical comfort systems to keep containment margins above minimum.",
        status: "ACCEPTED",
        labels: &["Decision"],
        properties: &[
            ("decided_by", CHIEF_ENGINEER_TITLE),
            ("decided_at", "2026-03-18T00:03:00Z"),
        ],
        related_nodes: &[RelationSeed {
            node_id: TASK_ID,
            relation_type: RELATION_IMPACTS,
        }],
        detail: Some(DetailSeed {
            event_id: "evt-kernel-decision-power-detail-1",
            detail: DECISION_DETAIL,
            content_hash: "hash-odyssey-decision-power",
            revision: 4,
        }),
    },
    NodeSeed {
        event_id: "evt-kernel-decision-jump-1",
        node_id: JUMP_DECISION_ID,
        node_kind: "Decision",
        title: "Delay the scheduled jump",
        summary: "Keep the Odyssey in sublight until manifold pressure is stable.",
        status: "ACCEPTED",
        labels: &["Decision"],
        properties: &[
            ("decided_by", "Captain N. Vale"),
            ("decided_at", "2026-03-18T00:02:00Z"),
        ],
        related_nodes: &[
            RelationSeed {
                node_id: DECISION_ID,
                relation_type: RELATION_DECISION_REQUIRES,
            },
            RelationSeed {
                node_id: NAV_TASK_ID,
                relation_type: RELATION_IMPACTS,
            },
        ],
        detail: Some(DetailSeed {
            event_id: "evt-kernel-decision-jump-detail-1",
            detail: "The bridge delayed the jump because even a short spike in manifold pressure would turn the alignment burn into a reactor event.",
            content_hash: "hash-odyssey-decision-jump",
            revision: 2,
        }),
    },
    NodeSeed {
        event_id: "evt-kernel-decision-ring-1",
        node_id: "decision:isolate-docking-ring",
        node_kind: "Decision",
        title: "Isolate docking ring twelve",
        summary: "Seal the damaged ring to preserve pressure and reactor safety.",
        status: "ACCEPTED",
        labels: &["Decision"],
        properties: &[
            ("decided_by", "Operations Lead M. Chen"),
            ("decided_at", "2026-03-18T00:04:00Z"),
        ],
        related_nodes: &[
            RelationSeed {
                node_id: DECISION_ID,
                relation_type: RELATION_DECISION_REQUIRES,
            },
            RelationSeed {
                node_id: "task:seal-docking-ring-twelve",
                relation_type: RELATION_IMPACTS,
            },
        ],
        detail: Some(DetailSeed {
            event_id: "evt-kernel-decision-ring-detail-1",
            detail: "Deck operations isolated ring twelve to stop pressure loss before the fracture could propagate into civilian access corridors.",
            content_hash: "hash-odyssey-decision-ring",
            revision: 3,
        }),
    },
    NodeSeed {
        event_id: "evt-kernel-decision-throttle-1",
        node_id: "decision:manual-throttle-guard",
        node_kind: "Decision",
        title: "Enable manual throttle guard",
        summary: "Disable autopilot throttle corrections while telemetry is noisy.",
        status: "PROPOSED",
        labels: &["Decision"],
        properties: &[("decided_by", "Navigation Officer I. Shah")],
        related_nodes: &[
            RelationSeed {
                node_id: JUMP_DECISION_ID,
                relation_type: "DECISION_DEPENDS_ON",
            },
            RelationSeed {
                node_id: "task:validate-telemetry-mirror",
                relation_type: RELATION_IMPACTS,
            },
        ],
        detail: Some(DetailSeed {
            event_id: "evt-kernel-decision-throttle-detail-1",
            detail: "Navigation proposed manual throttle guard so autopilot does not chase bad telemetry while engineers rebalance the port injectors.",
            content_hash: "hash-odyssey-decision-throttle",
            revision: 1,
        }),
    },
    NodeSeed {
        event_id: "evt-kernel-task-manifold-1",
        node_id: TASK_ID,
        node_kind: "Task",
        title: TASK_TITLE,
        summary: "Seal the breach, rebalance plasma flow, and keep the warp core below the scram threshold.",
        status: "READY",
        labels: &["Task"],
        properties: &[("role", "ENG"), ("priority", "1")],
        related_nodes: &[],
        detail: Some(DetailSeed {
            event_id: "evt-kernel-task-manifold-detail-1",
            detail: TASK_DETAIL,
            content_hash: "hash-odyssey-task-manifold",
            revision: 6,
        }),
    },
    NodeSeed {
        event_id: "evt-kernel-task-eps-1",
        node_id: POWER_TASK_ID,
        node_kind: "Task",
        title: "Reroute EPS through dorsal relays",
        summary: "Feed propulsion and shields without overloading the damaged port trunk.",
        status: "IN_PROGRESS",
        labels: &["Task"],
        properties: &[("role", "ENG"), ("priority", "2")],
        related_nodes: &[RelationSeed {
            node_id: TASK_ID,
            relation_type: RELATION_DEPENDS_ON,
        }],
        detail: Some(DetailSeed {
            event_id: "evt-kernel-task-eps-detail-1",
            detail: "Engineering is moving reserve load to dorsal relays so propulsion containment can stay powered while the manifold is repaired.",
            content_hash: "hash-odyssey-task-eps",
            revision: 3,
        }),
    },
    NodeSeed {
        event_id: "evt-kernel-task-nav-1",
        node_id: NAV_TASK_ID,
        node_kind: "Task",
        title: "Calibrate inertial drift compensation",
        summary: "Hold the ship on course while propulsion output oscillates.",
        status: "IN_PROGRESS",
        labels: &["Task"],
        properties: &[("role", "NAV"), ("priority", "2")],
        related_nodes: &[],
        detail: Some(DetailSeed {
            event_id: "evt-kernel-task-nav-detail-1",
            detail: "Navigation is recalibrating drift compensation to keep the ship on course while propulsion output remains unstable.",
            content_hash: "hash-odyssey-task-nav",
            revision: 2,
        }),
    },
    NodeSeed {
        event_id: "evt-kernel-task-ring-1",
        node_id: "task:seal-docking-ring-twelve",
        node_kind: "Task",
        title: "Seal docking ring twelve",
        summary: "Prevent atmosphere loss if the fracture propagates to the outer hull.",
        status: "DONE",
        labels: &["Task"],
        properties: &[("role", "OPS"), ("priority", "3")],
        related_nodes: &[],
        detail: Some(DetailSeed {
            event_id: "evt-kernel-task-ring-detail-1",
            detail: "Deck operations sealed ring twelve and redirected traffic away from the damaged hull segment.",
            content_hash: "hash-odyssey-task-ring",
            revision: 4,
        }),
    },
    NodeSeed {
        event_id: "evt-kernel-task-medical-1",
        node_id: "task:stage-medical-response",
        node_kind: "Task",
        title: "Stage medical response teams",
        summary: "Prepare triage near engineering and civilian decks.",
        status: "READY",
        labels: &["Task"],
        properties: &[("role", "MED"), ("priority", "3")],
        related_nodes: &[],
        detail: Some(DetailSeed {
            event_id: "evt-kernel-task-medical-detail-1",
            detail: "Medical teams are staged near engineering accessways in case containment fluctuations injure maintenance crews or nearby passengers.",
            content_hash: "hash-odyssey-task-medical",
            revision: 2,
        }),
    },
    NodeSeed {
        event_id: "evt-kernel-task-telemetry-1",
        node_id: "task:validate-telemetry-mirror",
        node_kind: "Task",
        title: "Validate telemetry mirror to command deck",
        summary: "Ensure the bridge and engineering read the same containment state.",
        status: "DONE",
        labels: &["Task"],
        properties: &[("role", "OPS"), ("priority", "2")],
        related_nodes: &[RelationSeed {
            node_id: NAV_TASK_ID,
            relation_type: RELATION_DEPENDS_ON,
        }],
        detail: Some(DetailSeed {
            event_id: "evt-kernel-task-telemetry-detail-1",
            detail: "Operations validated the telemetry mirror so command and engineering work from the same containment numbers during the incident.",
            content_hash: "hash-odyssey-task-telemetry",
            revision: 2,
        }),
    },
    NodeSeed {
        event_id: "evt-kernel-subsystem-propulsion-1",
        node_id: "subsystem:propulsion",
        node_kind: "Subsystem",
        title: PROPULSION_SUBSYSTEM_TITLE,
        summary: "Warp plasma containment and antimatter manifold controls.",
        status: "DEGRADED",
        labels: &["Subsystem"],
        properties: &[("owner", CHIEF_ENGINEER_TITLE)],
        related_nodes: &[RelationSeed {
            node_id: TASK_ID,
            relation_type: "SUPPORTS",
        }],
        detail: Some(DetailSeed {
            event_id: "evt-kernel-subsystem-propulsion-detail-1",
            detail: "Propulsion containment is degraded but still responsive enough to survive the repair window if reserve power remains available.",
            content_hash: "hash-odyssey-subsystem-propulsion",
            revision: 5,
        }),
    },
    NodeSeed {
        event_id: "evt-kernel-subsystem-navigation-1",
        node_id: "subsystem:navigation",
        node_kind: "Subsystem",
        title: "Astrogation array",
        summary: "Drift compensation and jump alignment sensors.",
        status: "DEGRADED",
        labels: &["Subsystem"],
        properties: &[("owner", "Navigation Officer I. Shah")],
        related_nodes: &[RelationSeed {
            node_id: NAV_TASK_ID,
            relation_type: "SUPPORTS",
        }],
        detail: Some(DetailSeed {
            event_id: "evt-kernel-subsystem-navigation-detail-1",
            detail: "The astrogation array is receiving noisy inertial data and cannot safely support a jump until drift compensation is recalibrated.",
            content_hash: "hash-odyssey-subsystem-navigation",
            revision: 3,
        }),
    },
    NodeSeed {
        event_id: "evt-kernel-subsystem-life-support-1",
        node_id: "subsystem:life-support",
        node_kind: "Subsystem",
        title: "Life-support grid",
        summary: "Atmosphere balancing and med-bay pressure routing.",
        status: "STRESSED",
        labels: &["Subsystem"],
        properties: &[("owner", "Medical Chief A. Duran")],
        related_nodes: &[RelationSeed {
            node_id: "task:stage-medical-response",
            relation_type: "SUPPORTS",
        }],
        detail: Some(DetailSeed {
            event_id: "evt-kernel-subsystem-life-support-detail-1",
            detail: "Life-support is stable but operating with reduced pressure headroom near engineering and the isolated docking ring.",
            content_hash: "hash-odyssey-subsystem-life-support",
            revision: 2,
        }),
    },
    NodeSeed {
        event_id: "evt-kernel-crew-chief-engineer-1",
        node_id: "crew:chief-engineer",
        node_kind: "CrewMember",
        title: CHIEF_ENGINEER_TITLE,
        summary: "Owns propulsion stabilization and containment recovery.",
        status: "ACTIVE",
        labels: &["CrewMember"],
        properties: &[("station", "Main engineering")],
        related_nodes: &[RelationSeed {
            node_id: TASK_ID,
            relation_type: "OWNS",
        }],
        detail: Some(DetailSeed {
            event_id: "evt-kernel-crew-chief-engineer-detail-1",
            detail: "Chief Engineer T. Garcia is coordinating engineering, bridge, and deck operations so the repair sequence stays within containment tolerances.",
            content_hash: "hash-odyssey-crew-chief-engineer",
            revision: 1,
        }),
    },
];

pub async fn publish_projection_events(
    client: &Client,
    subject_prefix: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    publish_projection_events_for_run(client, subject_prefix, "kernel-e2e").await
}

pub async fn publish_projection_events_for_run(
    client: &Client,
    subject_prefix: &str,
    run_id: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for (subject, payload) in projection_messages_for_run(subject_prefix, run_id)? {
        client.publish(subject, payload.into()).await?;
    }
    client.flush().await?;
    Ok(())
}

pub fn projection_messages(subject_prefix: &str) -> ProjectionMessagesResult {
    projection_messages_for_run(subject_prefix, "kernel-e2e")
}

pub fn projection_messages_for_run(subject_prefix: &str, run_id: &str) -> ProjectionMessagesResult {
    let mut messages = Vec::with_capacity(KERNEL_GRAPH_NODES.len() * 2);

    for node in KERNEL_GRAPH_NODES {
        messages.push((
            subject(subject_prefix, "graph.node.materialized"),
            serde_json::to_vec(&graph_node_event(*node, run_id))?,
        ));

        if let Some(detail) = node.detail {
            messages.push((
                subject(subject_prefix, "node.detail.materialized"),
                serde_json::to_vec(&detail_event(*node, detail, run_id))?,
            ));
        }
    }

    Ok(messages)
}

fn subject(subject_prefix: &str, suffix: &str) -> String {
    format!("{subject_prefix}.{suffix}")
}

fn graph_node_event(node: NodeSeed, run_id: &str) -> GraphNodeMaterializedEvent {
    GraphNodeMaterializedEvent {
        envelope: base_envelope(node.event_id, node.node_id, "node", run_id),
        data: GraphNodeMaterializedData {
            node_id: node.node_id.to_string(),
            node_kind: node.node_kind.to_string(),
            title: node.title.to_string(),
            summary: node.summary.to_string(),
            status: node.status.to_string(),
            labels: node
                .labels
                .iter()
                .map(|label| (*label).to_string())
                .collect(),
            properties: properties(node.properties),
            related_nodes: node
                .related_nodes
                .iter()
                .enumerate()
                .map(|(sequence, relation)| RelatedNodeReference {
                    node_id: relation.node_id.to_string(),
                    relation_type: relation.relation_type.to_string(),
                    explanation: RelatedNodeExplanationData {
                        semantic_class: relation_semantic_class(relation.relation_type),
                        rationale: None,
                        motivation: None,
                        method: None,
                        decision_id: None,
                        caused_by_node_id: None,
                        evidence: None,
                        confidence: None,
                        sequence: Some((sequence + 1) as u32),
                    },
                })
                .collect(),
            source_kind: None,
            source_agent: None,
            observed_at: None,
        },
    }
}

fn relation_semantic_class(relation_type: &str) -> RelationSemanticClass {
    match relation_type {
        "RECORDS" | "TRACKS" | "ASSIGNS" | "GUIDES" | "HAS_TASK" | "AFFECTS" => {
            RelationSemanticClass::Structural
        }
        RELATION_DEPENDS_ON => RelationSemanticClass::Constraint,
        RELATION_IMPACTS => RelationSemanticClass::Causal,
        RELATION_DECISION_REQUIRES | "PRODUCES" => RelationSemanticClass::Procedural,
        _ => RelationSemanticClass::Structural,
    }
}

fn detail_event(node: NodeSeed, detail: DetailSeed, run_id: &str) -> NodeDetailMaterializedEvent {
    NodeDetailMaterializedEvent {
        envelope: base_envelope(detail.event_id, node.node_id, "node_detail", run_id),
        data: NodeDetailMaterializedData {
            node_id: node.node_id.to_string(),
            detail: detail.detail.to_string(),
            content_hash: detail.content_hash.to_string(),
            revision: detail.revision,
        },
    }
}

fn properties(values: &'static [(&'static str, &'static str)]) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect()
}

fn base_envelope(
    event_id: &str,
    aggregate_id: &str,
    aggregate_type: &str,
    run_id: &str,
) -> ProjectionEnvelope {
    ProjectionEnvelope {
        event_id: format!("{event_id}-{run_id}"),
        correlation_id: format!("corr-kernel-e2e-{run_id}"),
        causation_id: format!("cause-kernel-e2e-{run_id}"),
        occurred_at: "2026-03-18T00:00:00Z".to_string(),
        aggregate_id: aggregate_id.to_string(),
        aggregate_type: aggregate_type.to_string(),
        schema_version: "v1beta1".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    fn decoded_messages_for_run(run_id: &str) -> Vec<(String, Value)> {
        projection_messages_for_run(DEFAULT_SUBJECT_PREFIX, run_id)
            .expect("starship seed should render")
            .into_iter()
            .map(|(subject, payload)| {
                (
                    subject,
                    serde_json::from_slice(&payload).expect("payload should decode"),
                )
            })
            .collect()
    }

    fn subject_suffix_count(messages: &[(String, Value)], suffix: &str) -> usize {
        messages
            .iter()
            .filter(|(subject, _)| subject.ends_with(suffix))
            .count()
    }

    #[test]
    fn projection_messages_render_expected_starship_graph() {
        let messages = decoded_messages_for_run("run-42");
        let expected_detail_messages = KERNEL_GRAPH_NODES
            .iter()
            .filter(|node| node.detail.is_some())
            .count();

        assert_eq!(
            messages.len(),
            KERNEL_GRAPH_NODES.len() + expected_detail_messages
        );
        assert_eq!(
            subject_suffix_count(&messages, "graph.node.materialized"),
            STARSHIP_NODE_IDS.len()
        );
        assert_eq!(
            subject_suffix_count(&messages, "node.detail.materialized"),
            EXPECTED_DETAIL_COUNT
        );

        let graph_nodes: Vec<_> = messages
            .iter()
            .filter(|(subject, _)| subject.ends_with("graph.node.materialized"))
            .map(|(_, payload)| &payload["data"])
            .collect();

        let root_node = graph_nodes
            .iter()
            .find(|payload| payload["node_id"] == ROOT_NODE_ID)
            .expect("root node should exist");
        let root_relations = root_node["related_nodes"]
            .as_array()
            .expect("related nodes should be an array");
        assert_eq!(root_node["node_kind"], ROOT_LABEL);
        assert_eq!(root_node["title"], ROOT_TITLE);
        assert_eq!(root_node["status"], "AT_RISK");
        assert_eq!(root_relations.len(), EXPECTED_ROOT_RELATION_COUNT);
        assert!(root_relations.iter().any(|relation| {
            relation["node_id"] == EXPLORER_WORKSTREAM_ID && relation["relation_type"] == "TRACKS"
        }));

        let task_count = graph_nodes
            .iter()
            .filter(|payload| payload["node_kind"] == "Task")
            .count();
        let decision_count = graph_nodes
            .iter()
            .filter(|payload| payload["node_kind"] == "Decision")
            .count();
        let completed_task_count = graph_nodes
            .iter()
            .filter(|payload| payload["node_kind"] == "Task" && payload["status"] == "DONE")
            .count();
        assert_eq!(task_count, EXPECTED_TASK_COUNT);
        assert_eq!(decision_count, EXPECTED_DECISION_COUNT);
        assert_eq!(completed_task_count as i32, EXPECTED_COMPLETED_TASK_COUNT);

        let impact_edge_count = graph_nodes
            .iter()
            .flat_map(|payload| {
                payload["related_nodes"]
                    .as_array()
                    .expect("related nodes should be an array")
                    .iter()
            })
            .filter(|relation| relation["relation_type"] == RELATION_IMPACTS)
            .count();
        let decision_dependency_count = graph_nodes
            .iter()
            .flat_map(|payload| {
                payload["related_nodes"]
                    .as_array()
                    .expect("related nodes should be an array")
                    .iter()
            })
            .filter(|relation| {
                relation["relation_type"] == RELATION_DECISION_REQUIRES
                    || relation["relation_type"] == "DECISION_DEPENDS_ON"
            })
            .count();
        assert_eq!(impact_edge_count, EXPECTED_IMPACT_COUNT);
        assert_eq!(decision_dependency_count, EXPECTED_DECISION_EDGE_COUNT);
    }

    #[test]
    fn projection_messages_include_run_specific_envelopes_and_details() {
        let messages = decoded_messages_for_run("run-99");

        let root_graph_event = messages
            .iter()
            .find(|(_, payload)| {
                payload["data"]["node_id"] == ROOT_NODE_ID
                    && payload["data"].get("detail").is_none()
                    && payload["data"]["title"] == ROOT_TITLE
            })
            .expect("root graph event should exist");
        assert_eq!(
            root_graph_event.0,
            format!("{DEFAULT_SUBJECT_PREFIX}.graph.node.materialized")
        );
        assert_eq!(root_graph_event.1["event_id"], "evt-kernel-root-1-run-99");
        assert_eq!(
            root_graph_event.1["correlation_id"],
            "corr-kernel-e2e-run-99"
        );
        assert_eq!(
            root_graph_event.1["data"]["properties"]["plan_id"],
            "mission-odyssey-red-alert"
        );

        let root_detail_event = messages
            .iter()
            .find(|(_, payload)| {
                payload["data"]["node_id"] == ROOT_NODE_ID
                    && payload["data"]["detail"] == ROOT_DETAIL
            })
            .expect("root detail event should exist");
        assert_eq!(
            root_detail_event.0,
            format!("{DEFAULT_SUBJECT_PREFIX}.node.detail.materialized")
        );
        assert_eq!(
            root_detail_event.1["event_id"],
            "evt-kernel-root-detail-1-run-99"
        );
        assert_eq!(
            root_detail_event.1["data"]["content_hash"],
            "hash-odyssey-root"
        );
        assert_eq!(root_detail_event.1["data"]["revision"], 5);
    }

    #[test]
    fn projection_messages_wrapper_uses_default_subjects_and_run_id() {
        let messages = projection_messages(DEFAULT_SUBJECT_PREFIX).expect("wrapper should render");

        let first_graph_event = messages
            .iter()
            .find(|(subject, _)| subject.ends_with("graph.node.materialized"))
            .expect("graph event should exist");
        let payload: Value =
            serde_json::from_slice(&first_graph_event.1).expect("payload should decode");

        assert_eq!(
            first_graph_event.0,
            format!("{DEFAULT_SUBJECT_PREFIX}.graph.node.materialized")
        );
        assert!(
            payload["event_id"]
                .as_str()
                .expect("event id should be a string")
                .ends_with("-kernel-e2e")
        );
    }
}
