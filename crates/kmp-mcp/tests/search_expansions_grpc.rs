//! Search expansions (P15) over the kernel's gRPC write, against a real
//! store, and their parity with `kmp_write_memory`.
//!
//! `MemoryEntry.search_expansions` is additive: an older client that never
//! sets it writes exactly what it wrote before and reads no report. A client
//! that proposes expansions gets the same lint as `kmp_write_memory`, and,
//! since the kernel serves no judge, the same outcome as `kmp_write_memory`
//! on a backend that cannot judge: nothing is stored and the report says why.

use kmp_application::SearchExpansionReport;
use kmp_mcp::KernelMcpServer;
use kmp_proto::v1beta1::kernel_memory_service_client::KernelMemoryServiceClient;
use kmp_proto::v1beta1::kernel_memory_service_server::KernelMemoryServiceServer;
use kmp_proto::v1beta1::{
    IngestRequest, IngestResponse, InspectRequest, Memory, MemoryDimension, MemoryEntry,
    MemoryProvenance, MemorySourceKind, SearchExpansionsReport, TemporalCoordinate,
};
use serde_json::{Value, json};

const MCP_ABOUT: &str = "question:paraphrase-mcp";
const WIRE_ABOUT: &str = "question:paraphrase-wire";
const ROLLOUT: &str = "The rollout slipped because the auditors had not signed off.";
const PARAPHRASE: &str = "Why was the launch postponed?";
const ECHO: &str = "the rollout slipped";

struct Kernel {
    endpoint: String,
    stop: tokio::sync::oneshot::Sender<()>,
    serving: tokio::task::JoinHandle<Result<(), tonic::transport::Error>>,
    _dir: tempfile::TempDir,
}

async fn kernel() -> Kernel {
    let dir = tempfile::tempdir().expect("dir");
    let kernel = kmp_embedded::EmbeddedKernel::open(dir.path()).expect("kernel");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("port");
    let endpoint = format!("http://{}", listener.local_addr().expect("address"));
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(KernelMemoryServiceServer::new(
                kmp_transport_grpc::MemoryGrpcService::new(kernel.service()),
            ))
            .serve_with_incoming_shutdown(
                tokio_stream::wrappers::TcpListenerStream::new(listener),
                async {
                    let _ = stopped.await;
                },
            ),
    );
    Kernel {
        endpoint,
        stop,
        serving,
        _dir: dir,
    }
}

async fn call(server: &KernelMcpServer, name: &str, arguments: Value) -> Value {
    let line = json!({"jsonrpc":"2.0", "id":1, "method":"tools/call",
        "params":{"name":name, "arguments":arguments}})
    .to_string();
    let response = server.handle_json_line(&line).await.expect("MCP response");
    let response: Value = serde_json::from_str(&response).expect("JSON");
    assert!(response.get("error").is_none(), "{response}");
    assert_ne!(response["result"]["isError"], true, "{response}");
    response["result"]["structuredContent"].clone()
}

fn ingest(key: &str, text: &str, expansions: &[&str], dry_run: bool) -> IngestRequest {
    IngestRequest {
        about: WIRE_ABOUT.into(),
        idempotency_key: key.into(),
        dry_run,
        provenance: Some(MemoryProvenance {
            source_kind: MemorySourceKind::Agent as i32,
            source_agent: "agent:test".into(),
            observed_at: Some(prost_types::Timestamp {
                seconds: 1_790_000_000,
                nanos: 0,
            }),
            ..MemoryProvenance::default()
        }),
        memory: Some(Memory {
            dimensions: vec![MemoryDimension {
                id: "work:main".into(),
                kind: "work".into(),
                ..MemoryDimension::default()
            }],
            entries: vec![MemoryEntry {
                id: format!("{WIRE_ABOUT}:entry:decision:rollout"),
                kind: "decision".into(),
                text: text.into(),
                coordinates: vec![TemporalCoordinate {
                    dimension: "work".into(),
                    scope_id: "work:main".into(),
                    occurred_at: Some(prost_types::Timestamp {
                        seconds: 1_790_000_000,
                        nanos: 0,
                    }),
                    ..TemporalCoordinate::default()
                }],
                search_expansions: expansions.iter().map(|text| text.to_string()).collect(),
                ..MemoryEntry::default()
            }],
            ..Memory::default()
        }),
        ..IngestRequest::default()
    }
}

async fn wire(endpoint: &str, request: IngestRequest) -> Result<IngestResponse, tonic::Status> {
    KernelMemoryServiceClient::connect(endpoint.to_string())
        .await
        .expect("client")
        .ingest(request)
        .await
        .map(tonic::Response::into_inner)
}

async fn stored_metadata_keys(endpoint: &str, reference: &str) -> Vec<String> {
    let inspected = KernelMemoryServiceClient::connect(endpoint.to_string())
        .await
        .expect("client")
        .inspect(InspectRequest {
            about: WIRE_ABOUT.into(),
            r#ref: reference.into(),
            ..InspectRequest::default()
        })
        .await
        .expect("inspect")
        .into_inner();
    let object = inspected.object.expect("object");
    assert_eq!(object.text, ROLLOUT, "the entry is written as it came");
    object.metadata.into_keys().collect()
}

/// `(expansion, why)` of each refusal, the part both surfaces share.
fn wire_refusals(report: &SearchExpansionsReport) -> Vec<(String, String)> {
    report
        .refused
        .iter()
        .map(|refused| (refused.expansion.clone(), refused.why.clone()))
        .collect()
}

fn mcp_refusals(report: &Value) -> Vec<(String, String)> {
    report["refused"]
        .as_array()
        .expect("refused")
        .iter()
        .map(|refused| {
            (
                refused["expansion"]
                    .as_str()
                    .expect("expansion")
                    .to_string(),
                refused["why"].as_str().expect("why").to_string(),
            )
        })
        .collect()
}

#[tokio::test]
async fn an_older_client_writes_as_before_and_reads_no_report() {
    let kernel = kernel().await;
    let response = wire(&kernel.endpoint, ingest("wire:old", ROLLOUT, &[], false))
        .await
        .expect("ingest");
    assert_eq!(response.search_expansions, None);
    assert!(response.memory.expect("memory").accepted.is_some());
    let _ = kernel.stop.send(());
    let _ = kernel.serving.await;
}

#[tokio::test]
async fn the_wire_and_kmp_write_memory_read_a_proposal_alike_and_store_none_without_a_judge() {
    let kernel = kernel().await;

    // kmp_write_memory through the same kernel: its backend cannot judge.
    let server = KernelMcpServer::grpc(kernel.endpoint.clone());
    let written = call(
        &server,
        "kmp_write_memory",
        json!({
            "about": MCP_ABOUT, "actor": "agent:test",
            "memories": [{"id": "rollout", "kind": "decision", "summary": ROLLOUT,
                "evidence": "release notes", "labels": {"work": ["main"]},
                "search_expansions": [PARAPHRASE, ECHO]}]
        }),
    )
    .await;
    assert_eq!(written["status"], "committed", "{written}");
    let mcp = &written["search_expansions"];
    assert_eq!(mcp["stored"], json!({}), "{mcp}");
    assert!(
        mcp["not_stored"]
            .as_str()
            .is_some_and(|why| !why.is_empty())
    );
    let reference = written["local_refs"]["rollout"].as_str().expect("ref");
    let stored_text = call(
        &server,
        "kmp_inspect",
        json!({"about": MCP_ABOUT, "ref": reference}),
    )
    .await["object"]["text"]
        .as_str()
        .expect("text")
        .to_string();

    // The same proposal over the wire, against the same stored text.
    let response = wire(
        &kernel.endpoint,
        ingest("wire:new", &stored_text, &[PARAPHRASE, ECHO], false),
    )
    .await
    .expect("ingest");
    let report = response.search_expansions.expect("a report");
    assert!(report.stored.is_empty(), "{report:?}");
    assert_eq!(report.judged_by, "");
    assert_eq!(report.not_stored, SearchExpansionReport::NO_JUDGE_REASON);
    assert_eq!(wire_refusals(&report), mcp_refusals(mcp));
    assert_eq!(
        wire_refusals(&report),
        [(
            ECHO.to_string(),
            "repeats the memory's words, which adds nothing to search".to_string()
        )]
    );
    assert_eq!(
        report.refused[0].r#ref,
        format!("{WIRE_ABOUT}:entry:decision:rollout")
    );

    // The entry is written; no expansion is.
    let keys = stored_metadata_keys(
        &kernel.endpoint,
        &format!("{WIRE_ABOUT}:entry:decision:rollout"),
    )
    .await;
    assert!(
        keys.iter()
            .all(|key| !kmp_domain::SearchExpansions::is_metadata_key(key)),
        "{keys:?}"
    );

    let _ = kernel.stop.send(());
    let _ = kernel.serving.await;
}

#[tokio::test]
async fn a_preview_and_a_proposal_the_lint_refuses_whole_say_what_kmp_write_memory_says() {
    let kernel = kernel().await;
    let preview = wire(
        &kernel.endpoint,
        ingest("wire:preview", ROLLOUT, &[PARAPHRASE], true),
    )
    .await
    .expect("preview");
    assert_eq!(
        preview.search_expansions.expect("report").not_stored,
        SearchExpansionReport::PREVIEW_REASON
    );

    let refused = wire(
        &kernel.endpoint,
        ingest("wire:echo", ROLLOUT, &[ECHO], false),
    )
    .await
    .expect("ingest")
    .search_expansions
    .expect("report");
    assert_eq!(refused.refused.len(), 1);
    assert_eq!(refused.not_stored, "", "nothing awaited a judge");

    let _ = kernel.stop.send(());
    let _ = kernel.serving.await;
}

#[tokio::test]
async fn more_expansions_than_a_memory_keeps_is_refused_before_anything_is_written() {
    let kernel = kernel().await;
    let status = wire(
        &kernel.endpoint,
        ingest("wire:many", ROLLOUT, &[PARAPHRASE; 7], false),
    )
    .await
    .expect_err("too many");
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
    assert!(
        status.message().contains("at most 6"),
        "{}",
        status.message()
    );
    let _ = kernel.stop.send(());
    let _ = kernel.serving.await;
}
