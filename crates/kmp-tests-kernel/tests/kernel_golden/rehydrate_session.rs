use std::error::Error;

use kmp_proto::v1beta1::RehydrateSessionRequest;
use kmp_tests_shared::seed::kernel_data::{
    DEVELOPER_ROLE, REHYDRATE_TIMELINE_EVENTS, ROOT_NODE_ID,
};

use crate::support::kernel_golden_contract::{
    expected_rehydrate_session_response, normalize_rehydrate_session_response,
};
use crate::support::seeded_kernel_fixture::SeededKernelFixture;

#[tokio::test]
async fn grpc_rehydrate_session_matches_v1beta1_golden_contract()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let mut fixture = SeededKernelFixture::start().await?;

    let result = async {
        let response = fixture
            .query_client()
            .rehydrate_session(RehydrateSessionRequest {
                root_node_id: ROOT_NODE_ID.to_string(),
                roles: vec![DEVELOPER_ROLE.to_string()],
                timeline_window: REHYDRATE_TIMELINE_EVENTS as u32,
                persist_snapshot: false,
                snapshot_ttl: None,
            })
            .await?
            .into_inner();

        assert_eq!(
            normalize_rehydrate_session_response(response),
            expected_rehydrate_session_response(REHYDRATE_TIMELINE_EVENTS as u32, false)
        );
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    }
    .await;

    fixture.shutdown().await?;
    result
}
