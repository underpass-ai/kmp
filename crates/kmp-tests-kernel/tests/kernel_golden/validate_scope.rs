use std::error::Error;

use kmp_proto::v1beta1::{
    ValidateScopeRequest, context_query_service_client::ContextQueryServiceClient,
};
use kmp_tests_shared::empty_ports::{
    EmptyGraphNeighborhoodReader, EmptyNodeDetailReader, NoopSnapshotStore,
};
use kmp_tests_shared::seed::kernel_data::{
    allowed_validate_scope_request_scopes, rejected_validate_scope_request_scopes,
};
use kmp_tests_shared::server::{RunningGrpcServer, stop_server};

use crate::support::kernel_golden_contract::{
    expected_validate_scope_allowed_response, expected_validate_scope_rejected_response,
};

#[tokio::test]
async fn grpc_validate_scope_allowed_matches_v1beta1_golden_contract()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let server = RunningGrpcServer::start(
        EmptyGraphNeighborhoodReader,
        EmptyNodeDetailReader,
        NoopSnapshotStore,
    )
    .await?;
    let channel = server.connect_channel().await?;
    let mut client = ContextQueryServiceClient::new(channel);

    let result = async {
        let response = client
            .validate_scope(ValidateScopeRequest {
                required_scopes: allowed_validate_scope_request_scopes(),
                provided_scopes: allowed_validate_scope_request_scopes(),
            })
            .await?
            .into_inner();

        assert_eq!(response, expected_validate_scope_allowed_response());
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    }
    .await;

    stop_server(server).await?;
    result
}

#[tokio::test]
async fn grpc_validate_scope_rejected_matches_v1beta1_golden_contract()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let server = RunningGrpcServer::start(
        EmptyGraphNeighborhoodReader,
        EmptyNodeDetailReader,
        NoopSnapshotStore,
    )
    .await?;
    let channel = server.connect_channel().await?;
    let mut client = ContextQueryServiceClient::new(channel);

    let result = async {
        let response = client
            .validate_scope(ValidateScopeRequest {
                required_scopes: allowed_validate_scope_request_scopes(),
                provided_scopes: rejected_validate_scope_request_scopes(),
            })
            .await?
            .into_inner();

        assert_eq!(response, expected_validate_scope_rejected_response());
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    }
    .await;

    stop_server(server).await?;
    result
}
