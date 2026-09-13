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
async fn visual_projection_cache_preserves_real_grpc_results_after_peer_edits()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use kmp_proto::v1beta1::{ProjectVisualRequest, VisualLevelOfDetail};
    use kmp_proto_mapping::v1beta1::{
        visual_projection_query_from_proto, visual_projection_response_from_result,
    };
    let dir = tempfile::tempdir()?;
    let kernel = EmbeddedKernel::open(dir.path())?;
    kernel.service().ingest(packet(0)).await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let server = tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(KernelMemoryServiceServer::new(MemoryGrpcService::new(
                kernel.service(),
            )))
            .serve_with_incoming(TcpListenerStream::new(listener)),
    );
    let mut client = KernelMemoryServiceClient::connect(endpoint).await?;
    let peer = EmbeddedKernel::open(dir.path())?;
    let mut previous = None;
    for version in 0..3 {
        if version > 0 {
            peer.service().ingest(packet(version)).await?;
        }
        for lod in [
            VisualLevelOfDetail::Atlas,
            VisualLevelOfDetail::Episode,
            VisualLevelOfDetail::Moment,
        ] {
            let request = ProjectVisualRequest {
                about: ABOUT.into(),
                from: Some("2026-09-01T00:00:00Z".parse()?),
                to: Some("2026-10-01T00:00:00Z".parse()?),
                level_of_detail: lod as i32,
                ..Default::default()
            };
            let expected = visual_projection_response_from_result(
                peer.service()
                    .visual_projection(
                        visual_projection_query_from_proto(request.clone()).map_err(|e| *e)?,
                    )
                    .await?,
            );
            let first = client.project_visual(request.clone()).await?.into_inner();
            assert_eq!(first, expected);
            assert_eq!(client.project_visual(request).await?.into_inner(), expected);
            if lod == VisualLevelOfDetail::Moment
                && let Some(before) = previous.replace(first.clone())
            {
                assert_ne!(before, first);
            }
        }
    }
    server.abort();
    Ok(())
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
                expect_revision: 0,
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

#[tokio::test]
async fn lexical_cache_preserves_real_grpc_ask_after_peer_edits()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use kmp_proto::v1beta1::{AskRequest, MemoryBudget, MemoryDetailLevel};
    use kmp_proto_mapping::v1beta1::recall_projection::project_ask_response;
    use kmp_proto_mapping::v1beta1::{
        LexicalBridge, ask_query_from_proto, ask_response_from_result,
    };
    let scratch = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tmp"));
    std::fs::create_dir_all(scratch)?;
    let dir = tempfile::tempdir_in(scratch)?;
    let kernel = EmbeddedKernel::open(dir.path())?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let server = tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(KernelMemoryServiceServer::new(MemoryGrpcService::new(
                kernel.service(),
            )))
            .serve_with_incoming(TcpListenerStream::new(listener)),
    );
    let mut client = KernelMemoryServiceClient::connect(endpoint).await?;
    let peer = EmbeddedKernel::open(dir.path())?;
    for version in 0..3 {
        let mut update = packet(version);
        for n in 1..16 {
            let mut entry = update.memory.entries[0].clone();
            entry.id = format!("{ENTRY}-{n}");
            entry.text = format!("v{version}: recorded fact {n} cache valkey audit");
            update.memory.entries.push(entry);
        }
        peer.service().ingest(update).await?;
        for question in ["recorded fact", "cache valkey audit", "unrelated zeppelin"] {
            let request = AskRequest {
                about: ABOUT.into(),
                question: question.into(),
                budget: Some(MemoryBudget {
                    max_bytes: 2_000_000,
                    detail: MemoryDetailLevel::Full as i32,
                    ..Default::default()
                }),
                ..Default::default()
            };
            let query = ask_query_from_proto(request.clone()).map_err(|e| *e)?;
            let result = peer.service().ask(query.clone()).await?;
            assert!(result.read_revision.is_some());
            let expected = project_ask_response(
                ask_response_from_result(
                    question,
                    None,
                    query.answer_policy,
                    query.max_entries,
                    result,
                    &LexicalBridge::none(),
                    &query.temporal,
                )
                .map_err(|e| *e)?,
                &request,
            )?;
            if question == "recorded fact" {
                assert!(!expected.because.is_empty());
            }
            for _ in 0..2 {
                assert_eq!(client.ask(request.clone()).await?.into_inner(), expected);
            }
        }
    }
    server.abort();
    Ok(())
}
