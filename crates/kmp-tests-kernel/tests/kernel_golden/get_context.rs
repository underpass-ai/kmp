use std::error::Error;

use kmp_proto::v1beta1::GetContextRequest;
use kmp_tests_shared::seed::kernel_data::{DEVELOPER_ROLE, ROOT_NODE_ID};

use crate::support::kernel_golden_contract::{
    expected_get_context_response, normalize_get_context_response,
};
use crate::support::seeded_kernel_fixture::SeededKernelFixture;

#[tokio::test]
async fn grpc_get_context_matches_v1beta1_golden_contract()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let mut fixture = SeededKernelFixture::start().await?;

    let result = async {
        let response = fixture
            .query_client()
            .get_context(GetContextRequest {
                root_node_id: ROOT_NODE_ID.to_string(),
                role: DEVELOPER_ROLE.to_string(),
                token_budget: 2048,
                requested_scopes: vec!["graph".to_string()],
                depth: 0,
                max_tier: 0,
                rehydration_mode: 0,
            })
            .await?
            .into_inner();

        assert_eq!(
            normalize_get_context_response(response),
            expected_get_context_response()
        );
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    }
    .await;

    fixture.shutdown().await?;
    result
}
