#![cfg(feature = "container-tests")]

use kmp_tests_shared::debug::debug_log;
use kmp_tests_shared::fixtures::TestFixture;
use kmp_tests_shared::ports::ClosureSeed;
use kmp_tests_shared::runtime::fake_underpass::FakeUnderpassRuntime;
use kmp_tests_shared::runtime::workspace::RecordingRuntime;
use kmp_tests_shared::seed::generic_data::{
    FOCUS_DETAIL, FOCUS_NODE_ID, FOCUS_TITLE, ROOT_NODE_ID, ROOT_NODE_KIND, ROOT_TITLE,
    publish_projection_events,
};
use kmp_transport_grpc::agentic_reference::{
    AgentRequest, BasicContextAgent, SUMMARY_PATH, UnderpassRuntimeClient,
};

#[tokio::test]
async fn basic_agent_uses_kernel_context_to_drive_runtime_actions() {
    debug_log("starting test basic_agent_uses_kernel_context_to_drive_runtime_actions");
    let fixture = TestFixture::builder()
        .with_neo4j()
        .with_valkey()
        .with_nats()
        .with_projection_runtime()
        .with_grpc_server()
        .with_seed(ClosureSeed::new(|ctx| {
            let client = ctx.nats_client().clone();
            Box::pin(async move { publish_projection_events(&client).await })
        }))
        .with_readiness_check(ROOT_NODE_ID, FOCUS_NODE_ID)
        .build()
        .await
        .expect("test fixture should start");
    assert!(fixture.nats_url().starts_with("nats://"));
    let runtime = RecordingRuntime::default();
    let mut agent = BasicContextAgent::new(fixture.query_client(), runtime);

    let execution = agent
        .execute(AgentRequest::reference_defaults(
            ROOT_NODE_ID,
            ROOT_NODE_KIND,
        ))
        .await
        .expect("agent should complete with kernel context");

    assert_eq!(execution.selected_node_id, FOCUS_NODE_ID);
    assert_eq!(execution.written_path, SUMMARY_PATH);
    assert!(execution.listed_files.contains(SUMMARY_PATH));
    assert!(execution.written_content.contains(ROOT_TITLE));
    assert!(execution.written_content.contains(FOCUS_TITLE));
    assert!(execution.written_content.contains(FOCUS_DETAIL));

    let recorded_file = agent
        .runtime()
        .read_file(SUMMARY_PATH)
        .expect("recording runtime should expose the written file")
        .expect("summary file should exist");
    assert!(recorded_file.contains("# Context Summary"));

    fixture.shutdown().await.expect("fixture should shut down");
    debug_log("finished test basic_agent_uses_kernel_context_to_drive_runtime_actions");
}

#[tokio::test]
async fn basic_agent_uses_underpass_runtime_contract_with_kernel_context() {
    debug_log("starting test basic_agent_uses_underpass_runtime_contract_with_kernel_context");
    let fixture = TestFixture::builder()
        .with_neo4j()
        .with_valkey()
        .with_nats()
        .with_projection_runtime()
        .with_grpc_server()
        .with_seed(ClosureSeed::new(|ctx| {
            let client = ctx.nats_client().clone();
            Box::pin(async move { publish_projection_events(&client).await })
        }))
        .with_readiness_check(ROOT_NODE_ID, FOCUS_NODE_ID)
        .build()
        .await
        .expect("test fixture should start");
    assert!(fixture.nats_url().starts_with("nats://"));
    let runtime_server = FakeUnderpassRuntime::start()
        .await
        .expect("runtime server should start");
    let runtime = UnderpassRuntimeClient::connect(runtime_server.base_url())
        .await
        .expect("runtime client should connect");
    let mut agent = BasicContextAgent::new(fixture.query_client(), runtime);

    let execution = agent
        .execute(AgentRequest::reference_defaults(
            ROOT_NODE_ID,
            ROOT_NODE_KIND,
        ))
        .await
        .expect("agent should complete with runtime http contract");

    assert_eq!(execution.selected_node_id, FOCUS_NODE_ID);
    assert_eq!(execution.written_path, SUMMARY_PATH);
    assert!(execution.listed_files.contains(SUMMARY_PATH));
    assert!(execution.written_content.contains(ROOT_TITLE));
    assert!(execution.written_content.contains(FOCUS_TITLE));
    assert!(execution.written_content.contains(FOCUS_DETAIL));

    let runtime_file = runtime_server
        .read_file(SUMMARY_PATH)
        .expect("runtime file should be readable");
    assert!(runtime_file.is_some());

    runtime_server
        .shutdown()
        .await
        .expect("runtime server should shut down");
    fixture.shutdown().await.expect("fixture should shut down");
    debug_log("finished test basic_agent_uses_underpass_runtime_contract_with_kernel_context");
}
