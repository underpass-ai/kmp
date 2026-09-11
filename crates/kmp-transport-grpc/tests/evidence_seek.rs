//! Real wire dispatch, same SQLite snapshot semantics as the embedded service.
use kmp_domain::{
    NodeProjection, NodeRelationProjection, ProjectionMutation, ProjectionWriter,
    RelationExplanation, RelationSemanticClass,
};
use kmp_embedded::EmbeddedKernel;
use kmp_proto::v1beta1::{
    PageRequest, TraceRelationStep, TraceRequest, TraceSearchOptions, TraceSeekOptions,
    TraceSeekRole, kernel_memory_service_client::KernelMemoryServiceClient,
    kernel_memory_service_server::KernelMemoryServiceServer,
};
use kmp_transport_grpc::MemoryGrpcService;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;

#[tokio::test]
async fn seek_over_grpc_matches_embedded_and_pages_candidate_groups()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dir = tempfile::tempdir()?;
    let kernel = EmbeddedKernel::open(dir.path())?;
    let about = "project:seek-wire";
    let seed = format!("{about}:entry:observation:seed");
    let witness = format!("{about}:entry:observation:report");
    let mut mutations = vec![];
    for id in [&seed, &witness] {
        mutations.push(ProjectionMutation::UpsertNode(NodeProjection {
            node_id: id.clone(),
            node_kind: "observation".into(),
            title: id.clone(),
            summary: id.clone(),
            status: "ACTIVE".into(),
            labels: vec!["entry".into()],
            properties: [("memory_about".into(), about.into())].into(),
            provenance: None,
        }));
        let dimension = kmp_domain::MemoryDimensionIdentity::new(about, "event", "E1")?.node_id();
        mutations.push(ProjectionMutation::UpsertNodeRelation(Box::new(
            NodeRelationProjection {
                source_node_id: dimension.clone(),
                target_node_id: id.clone(),
                relation_type: "contains_entry".into(),
                explanation: RelationExplanation::new(RelationSemanticClass::Structural)
                    .with_dimension("event")
                    .with_scope_id(dimension)
                    .with_observed_at("2026-09-01T00:00:00Z"),
            },
        )));
    }
    mutations.push(ProjectionMutation::UpsertNodeRelation(Box::new(
        NodeRelationProjection {
            source_node_id: seed.clone(),
            target_node_id: witness.clone(),
            relation_type: "verified_by".into(),
            explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                .with_rationale("The report explicitly verifies this execution.")
                .with_evidence("Report: execution verified.")
                .with_observed_at("2026-09-01T00:00:00Z"),
        },
    )));
    kernel.store().apply_mutations(mutations).await?;
    let request = TraceRequest {
        about: about.into(),
        from: seed,
        search: Some(TraceSearchOptions {
            seek: Some(TraceSeekOptions {
                roles: vec![TraceSeekRole {
                    name: "verification".into(),
                    relation: Some(TraceRelationStep {
                        rel: "verified_by".into(),
                        direction: "outgoing".into(),
                    }),
                    ..Default::default()
                }],
                same_labels: vec!["event".into()],
                ..Default::default()
            }),
            ..Default::default()
        }),
        page: Some(PageRequest {
            entries: 1,
            ..Default::default()
        }),
        ..Default::default()
    };
    let native = kmp_proto_mapping::v1beta1::evidence_seek_request_from_proto(&request)
        .expect("valid")
        .expect("seek");
    let result = kernel.service().evidence_paths(native.clone()).await?;
    let seek = request
        .search
        .as_ref()
        .expect("search")
        .seek
        .as_ref()
        .expect("seek");
    let expected = kmp_proto_mapping::v1beta1::evidence_seek_response_from_result(
        result,
        &native,
        seek,
        kmp_application::TracePageRequest {
            entries: Some(1),
            cursor: None,
        },
    );
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
    let first = client.trace(request.clone()).await?.into_inner();
    assert_eq!(first, expected);
    assert_eq!(first.seek.as_ref().expect("seek").status, "compatible");
    let mut second = request.clone();
    second.page.as_mut().expect("page").cursor = first.page.expect("page").next_cursor;
    let second = client.trace(second).await?.into_inner();
    assert_eq!(second.candidates[0].witness, witness);
    assert!(second.trace.is_empty());
    let mut third = request;
    third.page.as_mut().expect("page").cursor = second.page.expect("page").next_cursor;
    let third = client.trace(third).await?.into_inner();
    assert_eq!(third.groups[0].candidate_indexes, [0]);
    assert!(!third.page.expect("page").has_more);
    assert_eq!(third.selection_fingerprint, first.selection_fingerprint);
    stop.send(()).expect("stop");
    server.await??;
    Ok(())
}
