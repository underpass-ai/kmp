#![allow(dead_code)]

use std::collections::BTreeMap;

use kmp_domain::{
    PortError, ProjectionMutation, ProjectionWriter, RelationExplanation, RelationSemanticClass,
};
use kmp_ports::{NodeDetailProjection, NodeProjection, NodeRelationProjection};

pub const ROOT_NODE_ID: &str = "story-123";
pub const ROOT_NODE_KIND: &str = "story";
pub const ROOT_TITLE: &str = "Hydrate projection shell";
pub const ROOT_SUMMARY: &str = "Root story summary";
pub const ROOT_STATUS: &str = "ACTIVE";
pub const ROOT_LABEL: &str = "Story";
pub const ROOT_CREATED_BY: &str = "planner";
pub const ROOT_PLAN_ID: &str = "plan-42";

pub const DECISION_ID: &str = "decision-1";
pub const DECISION_KIND: &str = "decision";
pub const DECISION_TITLE: &str = "Use beta kernel contract";
pub const DECISION_SUMMARY: &str = "Decision summary";
pub const DECISION_STATUS: &str = "ACCEPTED";
pub const DECISION_LABEL: &str = "Decision";

pub const TASK_ID: &str = "task-1";
pub const TASK_KIND: &str = "task";
pub const TASK_TITLE: &str = "Wire gRPC facade";
pub const TASK_SUMMARY: &str = "Task summary";
pub const TASK_STATUS: &str = "READY";
pub const TASK_LABEL: &str = "Task";
pub const TASK_ROLE: &str = "DEV";
pub const TASK_PRIORITY: &str = "3";

pub const DEVELOPER_ROLE: &str = "developer";

pub const ROOT_DETAIL: &str = "Extended story detail";
pub const ROOT_DETAIL_HASH: &str = "hash-story";
pub const ROOT_DETAIL_REVISION: u64 = 2;
pub const DECISION_DETAIL: &str = "Detailed rationale";
pub const DECISION_DETAIL_HASH: &str = "hash-decision";
pub const DECISION_DETAIL_REVISION: u64 = 3;

pub const RECORDS_RELATION: &str = "records";
pub const HAS_TASK_RELATION: &str = "has_task";

pub const REHYDRATE_TIMELINE_EVENTS: i32 = 7;

pub async fn seed_projection_graph<W>(writer: &W) -> Result<(), PortError>
where
    W: ProjectionWriter + Send + Sync,
{
    writer.apply_mutations(graph_mutations()).await
}

pub async fn seed_node_details<W>(writer: &W) -> Result<(), PortError>
where
    W: ProjectionWriter + Send + Sync,
{
    writer.apply_mutations(node_detail_mutations()).await
}

pub fn allowed_validate_scope_request_scopes() -> Vec<String> {
    [
        "CASE_HEADER",
        "PLAN_HEADER",
        "SUBTASKS_ROLE",
        "DECISIONS_RELEVANT_ROLE",
        "DEPS_RELEVANT",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

pub fn rejected_validate_scope_request_scopes() -> Vec<String> {
    [
        "CASE_HEADER",
        "PLAN_HEADER",
        "SUBTASKS_ROLE",
        "INVALID_SCOPE",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn graph_mutations() -> Vec<ProjectionMutation> {
    vec![
        ProjectionMutation::UpsertNode(NodeProjection {
            node_id: ROOT_NODE_ID.to_string(),
            node_kind: ROOT_NODE_KIND.to_string(),
            title: ROOT_TITLE.to_string(),
            summary: ROOT_SUMMARY.to_string(),
            status: ROOT_STATUS.to_string(),
            labels: vec![ROOT_LABEL.to_string()],
            properties: BTreeMap::from([
                ("created_by".to_string(), ROOT_CREATED_BY.to_string()),
                ("plan_id".to_string(), ROOT_PLAN_ID.to_string()),
            ]),
            provenance: None,
        }),
        ProjectionMutation::UpsertNode(NodeProjection {
            node_id: DECISION_ID.to_string(),
            node_kind: DECISION_KIND.to_string(),
            title: DECISION_TITLE.to_string(),
            summary: DECISION_SUMMARY.to_string(),
            status: DECISION_STATUS.to_string(),
            labels: vec![DECISION_LABEL.to_string()],
            properties: BTreeMap::new(),
            provenance: None,
        }),
        ProjectionMutation::UpsertNode(NodeProjection {
            node_id: TASK_ID.to_string(),
            node_kind: TASK_KIND.to_string(),
            title: TASK_TITLE.to_string(),
            summary: TASK_SUMMARY.to_string(),
            status: TASK_STATUS.to_string(),
            labels: vec![TASK_LABEL.to_string()],
            properties: BTreeMap::from([
                ("role".to_string(), TASK_ROLE.to_string()),
                ("priority".to_string(), TASK_PRIORITY.to_string()),
            ]),
            provenance: None,
        }),
        ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
            source_node_id: ROOT_NODE_ID.to_string(),
            target_node_id: DECISION_ID.to_string(),
            relation_type: RECORDS_RELATION.to_string(),
            explanation: RelationExplanation::new(RelationSemanticClass::Causal)
                .with_rationale("incident recorded decision")
                .with_sequence(1),
        })),
        ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
            source_node_id: ROOT_NODE_ID.to_string(),
            target_node_id: TASK_ID.to_string(),
            relation_type: HAS_TASK_RELATION.to_string(),
            explanation: RelationExplanation::new(RelationSemanticClass::Motivational)
                .with_sequence(2)
                .with_rationale("the task operationalizes the selected beta kernel approach"),
        })),
    ]
}

fn node_detail_mutations() -> Vec<ProjectionMutation> {
    vec![
        ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
            node_id: ROOT_NODE_ID.to_string(),
            detail: ROOT_DETAIL.to_string(),
            content_hash: ROOT_DETAIL_HASH.to_string(),
            revision: ROOT_DETAIL_REVISION,
        }),
        ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
            node_id: DECISION_ID.to_string(),
            detail: DECISION_DETAIL.to_string(),
            content_hash: DECISION_DETAIL_HASH.to_string(),
            revision: DECISION_DETAIL_REVISION,
        }),
    ]
}
