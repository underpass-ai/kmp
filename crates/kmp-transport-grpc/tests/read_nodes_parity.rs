use kmp_domain::*;
use kmp_embedded::EmbeddedKernel;
use kmp_proto::v1beta1::{
    ReadNodesRequest, kernel_memory_service_client::KernelMemoryServiceClient,
    kernel_memory_service_server::KernelMemoryServiceServer,
};
use kmp_proto_mapping::v1beta1::{
    memory_nodes_request_from_proto, memory_nodes_response_from_result,
};
use kmp_transport_grpc::MemoryGrpcService;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;

#[tokio::test]
async fn scoped_node_batches_match_embedded_over_real_grpc_including_budgets()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dir = tempfile::tempdir()?;
    let kernel = EmbeddedKernel::open(dir.path())?;
    let mut changes = vec![];
    for id in ["a", "b", "foreign"] {
        changes.push(ProjectionMutation::UpsertNode(NodeProjection {
            node_id: id.into(),
            node_kind: "observation".into(),
            title: id.into(),
            summary: format!("Node {id}"),
            status: "ACTIVE".into(),
            labels: vec!["entry".into()],
            properties: [(
                "memory_about".into(),
                if id == "foreign" {
                    "project:other"
                } else {
                    "project:test"
                }
                .into(),
            )]
            .into(),
            provenance: None,
        }));
        for label in ["one", "two"] {
            changes.push(ProjectionMutation::UpsertNodeRelation(Box::new(
                NodeRelationProjection {
                    source_node_id: label.into(),
                    target_node_id: id.into(),
                    relation_type: "contains_entry".into(),
                    explanation: RelationExplanation::new(RelationSemanticClass::Structural)
                        .with_dimension("task")
                        .with_scope_id(label)
                        .with_observed_at("2026-09-13T11:00:00.123456789Z")
                        .with_method("kmp_relabel")
                        .with_rationale("The source assigned this task."),
                },
            )));
        }
    }
    kernel.store().apply_mutations(changes).await?;
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
    for max_edges in [1, 2048] {
        let request = ReadNodesRequest {
            expect_snapshot: String::new(),
            about: "project:test".into(),
            refs: vec![
                "a".into(),
                "b".into(),
                "foreign".into(),
                "missing".into(),
                "a".into(),
            ],
            max_edges,
        };
        let expected = memory_nodes_response_from_result(
            kernel
                .service()
                .read_nodes(memory_nodes_request_from_proto(request.clone()).map_err(|e| *e)?)
                .await?,
        );
        let actual = client.read_nodes(request).await?.into_inner();
        assert_eq!(actual, expected);
        if max_edges == 1 {
            assert_eq!(actual.stop_reason, "edge_budget");
            assert!(!actual.nodes[0].coordinates_complete);
        } else {
            assert_eq!(actual.nodes.len(), 2);
            assert_eq!(actual.missing, vec!["foreign", "missing"]);
        }
    }
    let request = ReadNodesRequest {
        about: "project:test".into(),
        refs: vec!["a".into()],
        max_edges: 2048,
        expect_snapshot: String::new(),
    };
    let snapshot = client
        .read_nodes(request.clone())
        .await?
        .into_inner()
        .snapshot;
    assert!(!snapshot.is_empty());
    let bound = ReadNodesRequest {
        expect_snapshot: snapshot,
        ..request
    };
    client.read_nodes(bound.clone()).await?;
    kernel
        .store()
        .apply_mutations(vec![ProjectionMutation::UpsertNodeDetail(
            NodeDetailProjection {
                node_id: "a".into(),
                detail: "changed proof".into(),
                content_hash: "v2".into(),
                revision: 2,
            },
        )])
        .await?;
    assert_eq!(
        client
            .read_nodes(bound)
            .await
            .expect_err("stale snapshot")
            .code(),
        tonic::Code::Aborted
    );
    assert_eq!(
        client
            .read_nodes(ReadNodesRequest {
                expect_snapshot: String::new(),
                about: "project:test".into(),
                refs: vec![],
                max_edges: 0
            })
            .await
            .expect_err("invalid empty batch")
            .code(),
        tonic::Code::InvalidArgument
    );
    stop.send(()).expect("stop server");
    server.await??;
    Ok(())
}
