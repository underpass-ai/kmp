//! The Ask and Wake log lines of the gRPC API name their caller and carry the
//! keyed fingerprints of the question and the intent — never the text. One
//! test in its own binary, with a global subscriber, so no other test can
//! race it for the log.

use std::collections::BTreeMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use kmp_application::{
    MemoryCoordinateData, MemoryData, MemoryDimensionData, MemoryEntryData, MemoryEvidenceData,
    MemoryIngestCommand,
};
use kmp_embedded::EmbeddedKernel;
use kmp_observability::{FingerprintSalt, TELEMETRY_SALT_FILE};
use kmp_proto::v1beta1::kernel_memory_service_server::KernelMemoryService;
use kmp_proto::v1beta1::{AskRequest, WakeRequest};
use kmp_transport_grpc::MemoryGrpcService;
use serde_json::Value;
use tonic::Request;

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
async fn ask_and_wake_lines_carry_client_and_fingerprints_never_text() {
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
    let salt_path = store.path().join(TELEMETRY_SALT_FILE);
    let service = MemoryGrpcService::new(kernel.service()).with_telemetry_salt(salt_path.clone());

    let mut ask = Request::new(AskRequest {
        about: "project:telemetry".into(),
        question: "What private thing is current?".into(),
        ..Default::default()
    });
    ask.metadata_mut()
        .insert("kmp-client-name", "codex-mcp-client".parse().expect("name"));
    service.ask(ask).await.expect("ask");
    service
        .wake(Request::new(WakeRequest {
            about: "project:telemetry".into(),
            intent: "resume private incident".into(),
            ..Default::default()
        }))
        .await
        .expect("wake");

    let text = String::from_utf8(log.0.lock().expect("log").clone()).expect("utf8");
    let responses: Vec<Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|line| line["fields"]["message"] == "kernel memory grpc response")
        .collect();
    assert!(!text.contains("private"), "{text}");
    assert_eq!(responses.len(), 2, "{text}");
    let salt = FingerprintSalt::load_or_create(&salt_path).expect("salt");
    for line in &responses {
        let fields = &line["fields"];
        let expected = match fields["rpc"].as_str() {
            Some("KernelMemoryService.Ask") => {
                assert_eq!(fields["client_name"], "codex-mcp-client");
                salt.fingerprint_text("question", "What private thing is current?")
            }
            _ => salt.fingerprint_text("intent", "resume private incident"),
        };
        assert_eq!(fields["subject_fingerprint"].as_str(), expected.as_deref());
    }
}
