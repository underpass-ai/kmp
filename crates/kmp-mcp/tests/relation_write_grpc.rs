//! The same attachment, source correction and replay through embedded SQLite and real gRPC.

#[path = "support/relation_write_fixture.rs"]
pub mod fixture;
use fixture::*;
use kmp_mcp::KernelMcpServer;
use serde_json::json;

#[tokio::test]
async fn a_link_declared_over_grpc_commits_exactly_what_the_embedded_path_commits() {
    let embedded_dir = scratch();
    let embedded = KernelMcpServer::embedded(embedded_dir.path()).expect("embedded");
    let (alias, notice) = sources(&embedded).await;
    let direct = write(&embedded, junco_link(&alias, &notice)).await;

    let remote_dir = scratch();
    let kernel = kmp_embedded::EmbeddedKernel::open(remote_dir.path()).expect("kernel");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("port");
    let endpoint = format!("http://{}", listener.local_addr().expect("address"));
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(
                kmp_proto::v1beta1::kernel_memory_service_server::KernelMemoryServiceServer::new(
                    kmp_transport_grpc::MemoryGrpcService::new(kernel.service()),
                ),
            )
            .serve_with_incoming_shutdown(
                tokio_stream::wrappers::TcpListenerStream::new(listener),
                async {
                    let _ = stopped.await;
                },
            ),
    );
    let server = KernelMcpServer::grpc(endpoint);
    let (remote_alias, remote_notice) = sources(&server).await;
    assert_eq!(
        remote_alias, alias,
        "refs are deterministic across backends"
    );
    let remote = write(&server, junco_link(&remote_alias, &remote_notice)).await;

    assert_eq!(remote["status"], direct["status"]);
    assert_eq!(remote["attachment"], direct["attachment"]);
    assert_eq!(remote["coverage"], direct["coverage"]);
    assert_eq!(remote["relations"], direct["relations"]);
    let stored = inspected(&server, &remote_alias).await;
    assert_eq!(stored["object"]["text"], ALIAS, "{stored}");

    for (backend, source, target) in [
        (&embedded, &alias, &notice),
        (&server, &remote_alias, &remote_notice),
    ] {
        correct_alias(backend, source).await;
        let retry = write(backend, junco_link(source, target)).await;
        assert_eq!(retry["status"], "replayed", "{retry}");
        assert_eq!(retry["attachment"], direct["attachment"]);
        assert_eq!(
            inspected(backend, source).await["object"]["text"],
            CORRECTED
        );
        let mut changed = junco_link(source, target);
        changed["relations"][0]["evidence"] = json!("A different source under the same key.");
        assert_eq!(refused(backend, changed).await["error"]["code"], "conflict");
    }

    let _ = stop.send(());
    let _ = serving.await;
}
