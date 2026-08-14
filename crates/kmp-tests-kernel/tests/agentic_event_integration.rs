#![cfg(feature = "container-tests")]

use std::time::Duration;

use kmp_tests_shared::containers::connect_nats_with_retry;
use kmp_tests_shared::debug::debug_log;
use kmp_tests_shared::fixtures::TestFixture;
use kmp_tests_shared::ports::ClosureSeed;
use kmp_tests_shared::runtime::event_trigger::EventDrivenRuntimeTrigger;
use kmp_tests_shared::runtime::fake_underpass::FakeUnderpassRuntime;
use kmp_tests_shared::runtime::workspace::RecordingRuntime;
use kmp_tests_shared::seed::bundle_event::publish_context_bundle_generated_event;
use kmp_tests_shared::seed::generic_data::{
    FOCUS_DETAIL, FOCUS_NODE_ID, FOCUS_TITLE, ROOT_NODE_ID, ROOT_NODE_KIND, ROOT_TITLE,
    publish_projection_events,
};
use kmp_transport_grpc::agentic_reference::{AgentRequest, SUMMARY_PATH, UnderpassRuntimeClient};

#[tokio::test]
async fn bundle_generated_event_triggers_recording_runtime_workflow() {
    debug_log("starting test bundle_generated_event_triggers_recording_runtime_workflow");
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
    let runtime = RecordingRuntime::default();
    let runtime_observer = runtime.clone();
    let trigger = EventDrivenRuntimeTrigger::new(
        fixture.query_client(),
        runtime,
        fixture.nats_url(),
        AgentRequest::reference_defaults(ROOT_NODE_ID, ROOT_NODE_KIND),
    );
    let (trigger_task, ready_rx) = trigger.spawn();
    ready_rx
        .await
        .expect("bundle generated subscription should become ready");

    let publisher = connect_nats_with_retry(fixture.nats_url())
        .await
        .expect("nats publisher should connect");
    publish_context_bundle_generated_event(&publisher, &["implementer"])
        .await
        .expect("bundle generated event should publish");

    let execution = tokio::time::timeout(Duration::from_secs(20), trigger_task)
        .await
        .expect("event-driven trigger should complete before timeout")
        .expect("trigger task should join")
        .expect("trigger should complete successfully");

    assert_eq!(execution.selected_node_id, FOCUS_NODE_ID);
    assert_eq!(execution.written_path, SUMMARY_PATH);
    assert!(execution.listed_files.contains(SUMMARY_PATH));
    assert!(execution.written_content.contains(ROOT_TITLE));
    assert!(execution.written_content.contains(FOCUS_TITLE));
    assert!(execution.written_content.contains(FOCUS_DETAIL));
    let recorded_file = runtime_observer
        .read_file(SUMMARY_PATH)
        .expect("recording runtime should expose the written file")
        .expect("summary file should exist");
    assert!(recorded_file.contains("# Context Summary"));

    fixture.shutdown().await.expect("fixture should shut down");
    debug_log("finished test bundle_generated_event_triggers_recording_runtime_workflow");
}

#[tokio::test]
async fn bundle_generated_event_triggers_underpass_runtime_contract_workflow() {
    debug_log("starting test bundle_generated_event_triggers_underpass_runtime_contract_workflow");
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
    let runtime_server = FakeUnderpassRuntime::start()
        .await
        .expect("runtime server should start");
    let runtime = UnderpassRuntimeClient::connect(runtime_server.base_url())
        .await
        .expect("runtime client should connect");
    let trigger = EventDrivenRuntimeTrigger::new(
        fixture.query_client(),
        runtime,
        fixture.nats_url(),
        AgentRequest::reference_defaults(ROOT_NODE_ID, ROOT_NODE_KIND),
    );
    let (trigger_task, ready_rx) = trigger.spawn();
    ready_rx
        .await
        .expect("bundle generated subscription should become ready");

    let publisher = connect_nats_with_retry(fixture.nats_url())
        .await
        .expect("nats publisher should connect");
    publish_context_bundle_generated_event(&publisher, &["implementer"])
        .await
        .expect("bundle generated event should publish");

    let execution = tokio::time::timeout(Duration::from_secs(20), trigger_task)
        .await
        .expect("event-driven trigger should complete before timeout")
        .expect("trigger task should join")
        .expect("trigger should complete successfully");

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
    debug_log("finished test bundle_generated_event_triggers_underpass_runtime_contract_workflow");
}
