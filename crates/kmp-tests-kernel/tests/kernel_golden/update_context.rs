use std::error::Error;

use kmp_proto::v1beta1::{ContextChange, ContextChangeOperation, UpdateContextRequest};
use kmp_tests_shared::seed::kernel_data::{DEVELOPER_ROLE, ROOT_NODE_ID, TASK_ID};

use crate::support::kernel_golden_contract::{
    expected_update_context_response, normalize_update_context_response,
};
use crate::support::seeded_kernel_fixture::SeededKernelFixture;

#[tokio::test]
async fn grpc_update_context_matches_v1beta1_golden_contract()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let mut fixture = SeededKernelFixture::start().await?;

    let result = async {
        let response = fixture
            .command_client()
            .update_context(UpdateContextRequest {
                root_node_id: ROOT_NODE_ID.to_string(),
                role: DEVELOPER_ROLE.to_string(),
                work_item_id: TASK_ID.to_string(),
                changes: vec![ContextChange {
                    operation: ContextChangeOperation::Update as i32,
                    entity_kind: "decision".to_string(),
                    entity_id: "decision-9".to_string(),
                    payload_json: "{\"status\":\"accepted\"}".to_string(),
                    reason: "refined".to_string(),
                    scopes: vec!["decisions".to_string()],
                }],
                metadata: None,
                precondition: None,
            })
            .await?
            .into_inner();

        assert_eq!(
            normalize_update_context_response(response),
            expected_update_context_response()
        );
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    }
    .await;

    fixture.shutdown().await?;
    result
}
