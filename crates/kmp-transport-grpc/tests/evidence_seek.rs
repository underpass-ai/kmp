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
    check(false).await
}

#[tokio::test]
async fn context_sequences_over_grpc_match_embedded_and_preserve_witness_positions()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    check(true).await
}

async fn check(context: bool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dir = tempfile::tempdir()?;
    let kernel = EmbeddedKernel::open(dir.path())?;
    let about = "project:seek-wire";
    let seed = format!("{about}:entry:observation:seed");
    let anchor = if context {
        format!("{about}:entry:observation:anchor")
    } else {
        seed.clone()
    };
    let witness = format!("{about}:entry:observation:report");
    let mut mutations = vec![];
    for id in [&seed, &anchor, &witness] {
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
            source_node_id: anchor.clone(),
            target_node_id: witness.clone(),
            relation_type: "verified_by".into(),
            explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                .with_rationale("The report explicitly verifies this execution.")
                .with_evidence("Report: execution verified.")
                .with_observed_at("2026-09-01T00:00:00Z"),
        },
    )));
    if context {
        mutations.push(ProjectionMutation::UpsertNodeRelation(Box::new(
            NodeRelationProjection {
                source_node_id: seed.clone(),
                target_node_id: anchor,
                relation_type: "uses_background".into(),
                explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                    .with_rationale("The seed refers to this contextual execution.")
                    .with_evidence("Seed: related execution recorded.")
                    .with_observed_at("2026-09-01T00:00:00Z"),
            },
        )));
    }
    kernel.store().apply_mutations(mutations).await?;
    let request = TraceRequest {
        about: about.into(),
        from: seed,
        search: Some(TraceSearchOptions {
            seek: Some(TraceSeekOptions {
                roles: vec![TraceSeekRole {
                    name: "verification".into(),
                    context,
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
    let selection = first.seek.as_ref().expect("seek");
    assert_eq!(
        selection.status,
        if context {
            "review_required"
        } else {
            "compatible"
        }
    );
    assert_eq!(selection.context_discovery, context);
    assert_eq!(selection.declared_obligations_complete, !context);
    let mut page = first.clone();
    let mut candidates = first.candidates.clone();
    let mut groups = first.groups.clone();
    while page.page.as_ref().expect("page").has_more {
        let mut next = request.clone();
        next.page.as_mut().expect("page").cursor = page.page.expect("page").next_cursor;
        page = client.trace(next).await?.into_inner();
        assert_eq!(page.selection_fingerprint, first.selection_fingerprint);
        candidates.extend(page.candidates.clone());
        groups.extend(page.groups.clone());
    }
    assert_eq!(candidates[0].witness, witness);
    assert_eq!(candidates[0].context_hops, u32::from(context));
    assert_eq!(groups[0].candidate_indexes, [0]);
    stop.send(()).expect("stop");
    server.await??;
    Ok(())
}
