#![allow(dead_code, unused_imports)]

use std::error::Error;

use async_nats::Client;

use crate::debug::debug_log_value;
pub use kmp_transport_grpc::starship_e2e::{
    CHIEF_ENGINEER_TITLE, DECISION_DETAIL, DECISION_ID, DECISION_TITLE,
    EXPECTED_COMPLETED_TASK_COUNT, EXPECTED_DECISION_COUNT, EXPECTED_DECISION_EDGE_COUNT,
    EXPECTED_DETAIL_COUNT, EXPECTED_IMPACT_COUNT, EXPECTED_NEIGHBOR_COUNT,
    EXPECTED_RELATIONSHIP_COUNT, EXPECTED_SELECTED_NODE_COUNT,
    EXPECTED_SELECTED_RELATIONSHIP_COUNT, EXPECTED_TASK_COUNT, EXPECTED_TOKEN_BUDGET_HINT,
    EXPLORER_CHECKLIST_ID, EXPLORER_CHECKLIST_TITLE, EXPLORER_LEAF_DETAIL, EXPLORER_LEAF_ID,
    EXPLORER_LEAF_TITLE, EXPLORER_WORKSTREAM_DETAIL, EXPLORER_WORKSTREAM_ID,
    EXPLORER_WORKSTREAM_TITLE, JUMP_DECISION_ID, POWER_TASK_ID, PROPULSION_SUBSYSTEM_TITLE,
    RELATION_DECISION_REQUIRES, RELATION_DEPENDS_ON, RELATION_IMPACTS, ROOT_DETAIL, ROOT_LABEL,
    ROOT_NODE_ID, ROOT_TITLE, TASK_DETAIL, TASK_ID, TASK_TITLE,
};

pub const SUBJECT_PREFIX: &str = kmp_transport_grpc::starship_e2e::DEFAULT_SUBJECT_PREFIX;

pub async fn publish_kernel_e2e_projection_events(
    client: &Client,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for (subject, payload) in kmp_transport_grpc::starship_e2e::projection_messages(SUBJECT_PREFIX)?
    {
        debug_log_value("publishing kernel e2e subject", &subject);
        client.publish(subject, payload.into()).await?;
    }
    client.flush().await?;
    Ok(())
}
