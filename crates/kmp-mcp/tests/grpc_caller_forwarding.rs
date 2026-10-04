//! An MCP server on a gRPC backend names its session's host to the kernel:
//! the kernel's Ask and Wake lines log that host, not this server's
//! `user-agent`. Its own binary, with a global subscriber for both sides.
#[path = "support/isolated_home.rs"]
mod isolated_home;

use std::collections::BTreeMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use kmp_application::{
    MemoryCoordinateData, MemoryData, MemoryDimensionData, MemoryEntryData, MemoryEvidenceData,
    MemoryIngestCommand,
};
use kmp_embedded::EmbeddedKernel;
use kmp_mcp::{KernelMcpServer, McpSession};
use kmp_proto::v1beta1::kernel_memory_service_server::KernelMemoryServiceServer;
use kmp_transport_grpc::MemoryGrpcService;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;

fn packet() -> MemoryIngestCommand {
    MemoryIngestCommand {
        about: "project:telemetry".into(),
        memory: MemoryData {
            dimensions: vec![MemoryDimensionData {
                id: "task:proof".into(),
                kind: "task".into(),
                title: None,
                metadata: BTreeMap::new(),
            }],
            entries: vec![MemoryEntryData {
                id: "project:telemetry:entry:observation:fact".into(),
                kind: "observation".into(),
                text: "The rollout of the cache failed on Tuesday.".into(),
                coordinates: vec![MemoryCoordinateData {
                    dimension: "task".into(),
                    scope_id: "task:proof".into(),
                    occurred_at: Some("2026-09-10T10:00:00Z".into()),
                    observed_at: Some("2026-09-10T10:00:00Z".into()),
                    ingested_at: None,
                    valid_from: None,
                    valid_until: None,
                    sequence: None,
                    rank: None,
                    metadata: BTreeMap::new(),
                }],
                metadata: BTreeMap::new(),
            }],
            relations: vec![],
            evidence: vec![MemoryEvidenceData {
                id: "evidence:project:telemetry:one".into(),
                supports: vec!["project:telemetry:entry:observation:fact".into()],
                text: "Source: the rollout of the cache failed on Tuesday.".to_string(),
                source: Some("telemetry test".into()),
                time: Some("2026-09-10T10:00:00Z".into()),
                metadata: BTreeMap::new(),
                support_clocks: None,
            }],
        },
        provenance: None,
        idempotency_key: "telemetry-lines".into(),
        dry_run: false,
        default_observation_to_ingestion: false,
        neighborhood_review: None,
        label_policy: Default::default(),
        receipt_context: None,
    }
}

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("log").extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[tokio::test]
async fn the_kernel_logs_the_mcp_host_the_call_was_made_for() {
    let log = Captured::default();
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
            .with_writer(log.clone())
            .finish(),
    )
    .expect("the only subscriber of this binary");

    let store = tempfile::tempdir().expect("store");
    let kernel = EmbeddedKernel::open(store.path()).expect("kernel");
    kernel.service().ingest(packet()).await.expect("seed");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let endpoint = format!("http://{}", listener.local_addr().expect("address"));
    tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(KernelMemoryServiceServer::new(MemoryGrpcService::new(
                kernel.service(),
            )))
            .serve_with_incoming(TcpListenerStream::new(listener)),
    );

    let server =
        KernelMcpServer::grpc(endpoint).with_remote_state_home(&isolated_home::data_home());
    let session = McpSession::new();
    let initialize = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "codex-mcp-client", "version": "0.154.0"}}});
    server
        .handle_json_line_in(&initialize.to_string(), &session)
        .await
        .expect("initialize");
    let wake = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
        "name": "kmp_wake", "arguments": {"about": "project:telemetry"}}});
    let reply = server
        .handle_json_line_in(&wake.to_string(), &session)
        .await
        .expect("wake");
    let reply: Value = serde_json::from_str(&reply).expect("json");
    assert_eq!(reply["result"]["isError"], false, "{reply}");

    let text = String::from_utf8(log.0.lock().expect("log").clone()).expect("utf8");
    let kernel_line = text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|line| {
            line["fields"]["message"] == "kernel memory grpc response"
                && line["fields"]["rpc"] == "KernelMemoryService.Wake"
        })
        .expect("the kernel's Wake line");
    assert_eq!(kernel_line["fields"]["client_name"], "codex-mcp-client");
    assert_eq!(kernel_line["fields"]["client_version"], "0.154.0");
}
