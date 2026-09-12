//! Condense over the real gRPC transport, against a real store.
//!
//! The descriptor a card must declare cannot be written down in a fixture: it
//! is a digest of the bytes this store actually holds. So the test reads it
//! the way an agent does — from a descriptor-only trace — and then writes the
//! card with it. Anything less proves the schema, not the tool.

use std::collections::BTreeMap;

use kmp_application::{
    MemoryCoordinateData, MemoryData, MemoryDimensionData, MemoryEntryData, MemoryEvidenceData,
    MemoryIngestCommand,
};
use kmp_embedded::EmbeddedKernel;
use kmp_proto::v1beta1::{
    CondenseRequest, TraceBodyRefs, TraceRequest, TraceSearchOptions,
    kernel_memory_service_client::KernelMemoryServiceClient,
    kernel_memory_service_server::KernelMemoryServiceServer,
};
use kmp_transport_grpc::MemoryGrpcService;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;

const ABOUT: &str = "project:condense-wire";
const ENTRY: &str = "project:condense-wire:entry:observation:fact";
const SOURCE: &str = "evidence:project:condense-wire:one";
const TIME: &str = "2026-09-10T10:00:00Z";
const BODY: &str = "A body long enough that a reader would rather keep a card than read it again.";

fn packet() -> MemoryIngestCommand {
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
                text: BODY.into(),
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
                metadata: BTreeMap::new(),
            }],
            relations: vec![],
            evidence: vec![MemoryEvidenceData {
                id: SOURCE.into(),
                supports: vec![ENTRY.into()],
                text: format!("Source: {BODY}"),
                source: Some("condense parity test".into()),
                time: Some(TIME.into()),
                metadata: BTreeMap::new(),
                support_clocks: None,
            }],
        },
        provenance: None,
        idempotency_key: "condense-parity".into(),
        dry_run: false,
        default_observation_to_ingestion: false,
        neighborhood_review: None,
        label_policy: Default::default(),
        receipt_context: None,
    }
}

fn descriptor_only() -> TraceRequest {
    TraceRequest {
        about: ABOUT.into(),
        from: ENTRY.into(),
        to: ENTRY.into(),
        search: Some(TraceSearchOptions {
            proof: true,
            proof_refs: Some(TraceBodyRefs { refs: vec![] }),
            ..TraceSearchOptions::default()
        }),
        ..TraceRequest::default()
    }
}

fn condense(
    node_id: &str,
    card: &str,
    descriptor: &kmp_proto::v1beta1::NodeBodyDescriptor,
    expect_absent: bool,
    expect_card_revision: u64,
) -> CondenseRequest {
    CondenseRequest {
        about: ABOUT.into(),
        r#ref: node_id.into(),
        language: "es".into(),
        scope: "node_body".into(),
        card: card.into(),
        source_revision: descriptor.revision,
        source_content_hash: descriptor.content_hash.clone(),
        source_record_digest: descriptor.record_digest.clone(),
        expect_card_revision,
        expect_absent,
        actor: "condense-parity".into(),
    }
}

#[tokio::test]
async fn condense_over_grpc_binds_a_card_to_the_descriptor_the_store_returned()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dir = tempfile::tempdir()?;
    let kernel = EmbeddedKernel::open(dir.path())?;
    kernel.service().ingest(packet()).await?;
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

    // The descriptor as the wire hands it over, never constructed here.
    let traced = client.trace(descriptor_only()).await?.into_inner();
    let object = traced
        .objects
        .iter()
        .find(|object| {
            object
                .object
                .as_ref()
                .is_some_and(|inspected| inspected.r#ref == SOURCE)
        })
        .expect("the shared source is in the selected proof table");
    assert_eq!(object.body_state, "not_requested");
    assert!(
        object
            .object
            .as_ref()
            .expect("inspected object")
            .text
            .is_empty(),
        "a descriptor-only read carries no canonical text"
    );
    let descriptor = object.descriptor.clone().expect("descriptor");
    assert!(descriptor.record_digest.starts_with("sha256:"));

    let first = client
        .condense(condense(
            SOURCE,
            "La fuente sostiene la entrada.",
            &descriptor,
            true,
            0,
        ))
        .await?
        .into_inner();
    let card = first.card.expect("card");
    assert_eq!(first.r#ref, SOURCE);
    assert_eq!(card.card_revision, 1);
    assert_eq!(card.status, "valid");
    assert_eq!(card.source_record_digest, descriptor.record_digest);
    assert_eq!(card.source_revision, descriptor.revision);
    assert_eq!(card.source_body_bytes, descriptor.body_bytes);
    assert_eq!(card.authored_by, "condense-parity");
    assert!(!card.authored_at.is_empty(), "the kernel stamps authorship");

    // Compare-and-set, over the wire: the same first-write claim is refused
    // with the stored revision named, and the declared one is accepted.
    let replayed = client
        .condense(condense(SOURCE, "Otro resumen.", &descriptor, true, 0))
        .await
        .expect_err("a second first-write must lose");
    assert_eq!(replayed.code(), tonic::Code::Aborted);
    assert!(replayed.message().contains("card revision 1"), "{replayed}");

    let replaced = client
        .condense(condense(
            SOURCE,
            "Resumen corregido.",
            &descriptor,
            false,
            1,
        ))
        .await?
        .into_inner()
        .card
        .expect("card");
    assert_eq!(replaced.card_revision, 2);
    assert_eq!(replaced.text, "Resumen corregido.");

    // A declared body version the store does not have is a conflict, not a
    // write against whatever happens to be current.
    let mut moved = descriptor.clone();
    moved.record_digest = format!("sha256:{}", "0".repeat(64));
    let stale = client
        .condense(condense(SOURCE, "Resumen viejo.", &moved, false, 2))
        .await
        .expect_err("a card may not be bound to a version that is not there");
    assert_eq!(stale.code(), tonic::Code::Aborted);

    // And the canonical body is exactly where it was.
    let after = client.trace(descriptor_only()).await?.into_inner();
    let unchanged = after
        .objects
        .iter()
        .find(|object| {
            object
                .object
                .as_ref()
                .is_some_and(|inspected| inspected.r#ref == SOURCE)
        })
        .expect("source")
        .descriptor
        .clone()
        .expect("descriptor");
    assert_eq!(unchanged, descriptor, "condensing moves no canonical row");

    let _ = stop.send(());
    server.await??;
    Ok(())
}
