//! Actual gRPC transport over the embedded composition, with an independent
//! writer. The graph metadata, canonical body and both sources carry one epoch.
use std::collections::BTreeMap;

use kmp_application::{
    MemoryCoordinateData, MemoryData, MemoryDimensionData, MemoryEntryData, MemoryEvidenceData,
    MemoryIngestCommand,
};
use kmp_embedded::EmbeddedKernel;
use kmp_proto::v1beta1::{
    InspectInclude, InspectRequest, TraceRequest, TraceSearchOptions,
    kernel_memory_service_client::KernelMemoryServiceClient,
    kernel_memory_service_server::KernelMemoryServiceServer,
};
use kmp_transport_grpc::MemoryGrpcService;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;

const ABOUT: &str = "project:snapshot-wire";
const ENTRY: &str = "project:snapshot-wire:entry:observation:fact";
const TIME: &str = "2026-09-10T10:00:00Z";

fn packet(version: usize) -> MemoryIngestCommand {
    let epoch = format!("v{version}");
    MemoryIngestCommand {
        about: ABOUT.into(),
        memory: MemoryData {
            dimensions: vec![MemoryDimensionData {
                id: "task:proof".into(),
                kind: "task".into(),
                title: None,
                metadata: BTreeMap::new(),
            }],
            entries: vec![MemoryEntryData {
                id: ENTRY.into(),
                kind: "observation".into(),
                text: format!("{epoch}: recorded fact"),
                coordinates: vec![MemoryCoordinateData {
                    dimension: "task".into(),
                    scope_id: "task:proof".into(),
                    occurred_at: Some(TIME.into()),
                    observed_at: Some(TIME.into()),
                    ingested_at: None,
                    valid_from: None,
                    valid_until: None,
                    sequence: None,
                    rank: None,
                    metadata: BTreeMap::new(),
                }],
                metadata: BTreeMap::from([("epoch".into(), epoch.clone())]),
            }],
            relations: vec![],
            evidence: (0..2)
                .map(|i| MemoryEvidenceData {
                    id: format!("evidence:{ABOUT}:source-{i}"),
                    supports: vec![ENTRY.into()],
                    text: format!("{epoch}: source {i}"),
                    source: Some(format!("fixture:{i}")),
                    time: Some(TIME.into()),
                    metadata: BTreeMap::new(),
                    support_clocks: None,
                })
                .collect(),
        },
        provenance: None,
        idempotency_key: epoch,
        dry_run: false,
        default_observation_to_ingestion: false,
        neighborhood_review: None,
        label_policy: Default::default(),
        receipt_context: None,
    }
}

#[tokio::test]
async fn grpc_inspect_and_trace_proof_keep_graph_body_and_sources_together_during_peer_writes()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dir = tempfile::tempdir()?;
    let kernel = EmbeddedKernel::open(dir.path())?;
    kernel.service().ingest(packet(0)).await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(KernelMemoryServiceServer::new(MemoryGrpcService::new(
                kernel.service(),
            )))
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                let _ = stopped.await;
            }),
    );
    let mut client = KernelMemoryServiceClient::connect(endpoint).await?;
    let peer = EmbeddedKernel::open(dir.path())?;
    let writer = tokio::spawn(async move {
        for version in 1..=40 {
            peer.service().ingest(packet(version)).await?;
            tokio::task::yield_now().await;
        }
        Ok::<_, kmp_application::ApplicationError>(())
    });
    for _ in 0..80 {
        let result = client
            .inspect(InspectRequest {
                about: ABOUT.into(),
                r#ref: ENTRY.into(),
                include: Some(InspectInclude {
                    details: true,
                    incoming: true,
                    outgoing: true,
                    raw: false,
                }),
            })
            .await?
            .into_inner();
        let object = result.object.expect("inspected object");
        let epoch = object.text.split(':').next().expect("epoch prefix");
        assert_eq!(
            object.metadata.get("epoch").map(String::as_str),
            Some(epoch)
        );
        assert_eq!(result.evidence.len(), 2);
        for source in &result.evidence {
            assert_eq!(source.text.split(':').next(), Some(epoch));
            assert_eq!(source.supports, [ENTRY]);
        }
        let traced = client
            .trace(TraceRequest {
                about: ABOUT.into(),
                from: ENTRY.into(),
                targets: vec![ENTRY.into()],
                search: Some(TraceSearchOptions {
                    proof: true,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .await?
            .into_inner();
        let root = traced
            .objects
            .iter()
            .find(|o| o.object.as_ref().expect("complete trace proof").r#ref == ENTRY)
            .expect("complete trace proof");
        let root = root.object.as_ref().expect("complete trace proof");
        let epoch = root.metadata.get("epoch").expect("complete trace proof");
        assert_eq!(
            traced
                .proof
                .as_ref()
                .expect("complete trace proof")
                .complete_groups,
            [0]
        );
        assert_eq!(traced.objects.len(), 3);
        assert_eq!(traced.supports.len(), 2);
        assert!(traced.objects.iter().all(|o| {
            o.has_body
                && o.object
                    .as_ref()
                    .expect("complete trace proof")
                    .text
                    .split(':')
                    .next()
                    == Some(epoch.as_str())
        }));
    }
    writer.await??;
    stop.send(()).expect("stop gRPC server");
    server.await??;
    Ok(())
}
