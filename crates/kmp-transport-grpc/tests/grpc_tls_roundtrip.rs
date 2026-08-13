use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Once};

use kmp_config::{AppConfig, GrpcTlsConfig, GrpcTlsMode};
use kmp_domain::{
    ContextPathNeighborhood, GraphNeighborhoodReader, KmpBundle, MemoryAboutIndexReader,
    NodeDetailProjection, NodeDetailReader, NodeNeighborhood, NodeRelationshipReader,
    NodeRelationships, PortError, ProjectionMutation, ProjectionWriter, SnapshotSaveOptions,
    SnapshotStore,
};
use kmp_observability::quality_observers::NoopQualityObserver;
use kmp_proto::v1beta1::{
    GetContextRequest, context_query_service_client::ContextQueryServiceClient,
};
use kmp_testkit::InMemoryContextEventStore;
use kmp_transport_grpc::GrpcServer;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time::sleep;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};

struct EmptyGraphNeighborhoodReader;

impl GraphNeighborhoodReader for EmptyGraphNeighborhoodReader {
    async fn load_neighborhood(
        &self,
        _root_node_id: &str,
        _depth: u32,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        Ok(None)
    }

    async fn load_context_path(
        &self,
        _root_node_id: &str,
        _target_node_id: &str,
        _subtree_depth: u32,
    ) -> Result<Option<ContextPathNeighborhood>, PortError> {
        Ok(None)
    }
}

impl MemoryAboutIndexReader for EmptyGraphNeighborhoodReader {
    async fn list_memory_abouts(&self) -> Result<Vec<String>, PortError> {
        Ok(Vec::new())
    }
}

impl NodeRelationshipReader for EmptyGraphNeighborhoodReader {
    async fn load_node_relationships(
        &self,
        _node_id: &str,
    ) -> Result<Option<NodeRelationships>, PortError> {
        Ok(None)
    }
}

impl ProjectionWriter for EmptyGraphNeighborhoodReader {
    async fn apply_mutations(&self, _mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        Ok(())
    }
}

struct EmptyNodeDetailReader;

impl NodeDetailReader for EmptyNodeDetailReader {
    async fn load_node_detail(
        &self,
        _node_id: &str,
    ) -> Result<Option<NodeDetailProjection>, PortError> {
        Ok(None)
    }

    async fn load_node_details_batch(
        &self,
        node_ids: Vec<String>,
    ) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
        let mut results = Vec::with_capacity(node_ids.len());
        for node_id in &node_ids {
            results.push(self.load_node_detail(node_id).await?);
        }
        Ok(results)
    }
}

impl ProjectionWriter for EmptyNodeDetailReader {
    async fn apply_mutations(&self, _mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        Ok(())
    }
}

struct NoopSnapshotStore;

impl SnapshotStore for NoopSnapshotStore {
    async fn save_bundle_with_options(
        &self,
        _bundle: &KmpBundle,
        _options: SnapshotSaveOptions,
    ) -> Result<(), PortError> {
        Ok(())
    }
}

struct TlsFixturePaths {
    _dir: TempDir,
    server_cert: PathBuf,
    server_key: PathBuf,
    client_ca: PathBuf,
    ca_cert_pem: String,
    client_cert_pem: String,
    client_key_pem: String,
}

impl TlsFixturePaths {
    fn new() -> io::Result<Self> {
        let dir = tempfile::tempdir()?;
        let ca_key = dir.path().join("ca.key");
        let client_ca = dir.path().join("ca.crt");
        let server_key = dir.path().join("server.key");
        let server_csr = dir.path().join("server.csr");
        let server_ext = dir.path().join("server.ext");
        let server_cert = dir.path().join("server.crt");
        let client_key = dir.path().join("client.key");
        let client_csr = dir.path().join("client.csr");
        let client_ext = dir.path().join("client.ext");
        let client_cert = dir.path().join("client.crt");

        run_openssl([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-days",
            "3650",
            "-nodes",
            "-keyout",
            &path_string(&ca_key)?,
            "-out",
            &path_string(&client_ca)?,
            "-subj",
            "/CN=rehydration-test-ca",
        ])?;

        run_openssl([
            "req",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            &path_string(&server_key)?,
            "-out",
            &path_string(&server_csr)?,
            "-subj",
            "/CN=localhost",
            "-addext",
            "subjectAltName=DNS:localhost,IP:127.0.0.1",
        ])?;
        fs::write(
            &server_ext,
            "[v3_req]\nsubjectAltName=DNS:localhost,IP:127.0.0.1\nextendedKeyUsage=serverAuth\n",
        )?;
        run_openssl([
            "x509",
            "-req",
            "-in",
            &path_string(&server_csr)?,
            "-CA",
            &path_string(&client_ca)?,
            "-CAkey",
            &path_string(&ca_key)?,
            "-CAcreateserial",
            "-out",
            &path_string(&server_cert)?,
            "-days",
            "3650",
            "-extfile",
            &path_string(&server_ext)?,
            "-extensions",
            "v3_req",
        ])?;

        run_openssl([
            "req",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            &path_string(&client_key)?,
            "-out",
            &path_string(&client_csr)?,
            "-subj",
            "/CN=rehydration-test-client",
        ])?;
        fs::write(&client_ext, "[v3_req]\nextendedKeyUsage=clientAuth\n")?;
        run_openssl([
            "x509",
            "-req",
            "-in",
            &path_string(&client_csr)?,
            "-CA",
            &path_string(&client_ca)?,
            "-CAkey",
            &path_string(&ca_key)?,
            "-CAcreateserial",
            "-out",
            &path_string(&client_cert)?,
            "-days",
            "3650",
            "-extfile",
            &path_string(&client_ext)?,
            "-extensions",
            "v3_req",
        ])?;

        let ca_cert_pem = fs::read_to_string(&client_ca)?;
        let client_cert_pem = fs::read_to_string(client_cert)?;
        let client_key_pem = fs::read_to_string(client_key)?;

        Ok(Self {
            _dir: dir,
            server_cert,
            server_key,
            client_ca,
            ca_cert_pem,
            client_cert_pem,
            client_key_pem,
        })
    }
}

fn ensure_crypto_provider() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

#[tokio::test]
async fn grpc_server_supports_tls_roundtrip() {
    ensure_crypto_provider();
    let tls_fixture = TlsFixturePaths::new().expect("TLS fixture files should be written");
    let running = start_server(GrpcTlsConfig {
        mode: GrpcTlsMode::Server,
        cert_path: Some(tls_fixture.server_cert.clone()),
        key_path: Some(tls_fixture.server_key.clone()),
        client_ca_path: None,
    })
    .await
    .expect("TLS server should start");

    let channel = connect_tls_channel(&running.endpoint, &tls_fixture, false)
        .await
        .expect("TLS client should connect");
    let mut query_client = ContextQueryServiceClient::new(channel);
    let status = get_context(&mut query_client)
        .await
        .expect_err("empty graph should return NOT_FOUND over TLS");
    assert_eq!(status.code(), tonic::Code::NotFound);

    stop_server(running)
        .await
        .expect("server should stop cleanly");
}

#[tokio::test]
async fn grpc_server_rejects_clients_without_certificate_in_mutual_mode() {
    ensure_crypto_provider();
    let tls_fixture = TlsFixturePaths::new().expect("TLS fixture files should be written");
    let running = start_server(GrpcTlsConfig {
        mode: GrpcTlsMode::Mutual,
        cert_path: Some(tls_fixture.server_cert.clone()),
        key_path: Some(tls_fixture.server_key.clone()),
        client_ca_path: Some(tls_fixture.client_ca.clone()),
    })
    .await
    .expect("mTLS server should start");

    let connection = connect_tls_channel(&running.endpoint, &tls_fixture, false).await;
    match connection {
        Ok(channel) => {
            let mut query_client = ContextQueryServiceClient::new(channel);
            let error = get_context(&mut query_client)
                .await
                .expect_err("mTLS request without client cert should fail");
            let message = error.to_string().to_ascii_lowercase();
            assert!(
                message.contains("tls")
                    || message.contains("certificate")
                    || message.contains("handshake")
                    || message.contains("transport error")
                    || message.contains("unknown ca"),
                "unexpected error: {error}"
            );
        }
        Err(error) => {
            let message = error.to_string().to_ascii_lowercase();
            assert!(
                message.contains("tls")
                    || message.contains("certificate")
                    || message.contains("handshake")
                    || message.contains("transport error")
                    || message.contains("unknown ca"),
                "unexpected connection error: {error}"
            );
        }
    }

    stop_server(running)
        .await
        .expect("server should stop cleanly");
}

#[tokio::test]
async fn grpc_server_accepts_clients_with_certificate_in_mutual_mode() {
    ensure_crypto_provider();
    let tls_fixture = TlsFixturePaths::new().expect("TLS fixture files should be written");
    let running = start_server(GrpcTlsConfig {
        mode: GrpcTlsMode::Mutual,
        cert_path: Some(tls_fixture.server_cert.clone()),
        key_path: Some(tls_fixture.server_key.clone()),
        client_ca_path: Some(tls_fixture.client_ca.clone()),
    })
    .await
    .expect("mTLS server should start");

    let channel = connect_tls_channel(&running.endpoint, &tls_fixture, true)
        .await
        .expect("mTLS client should connect with a certificate");
    let mut query_client = ContextQueryServiceClient::new(channel);
    let status = get_context(&mut query_client)
        .await
        .expect_err("empty graph should return NOT_FOUND over mTLS");
    assert_eq!(status.code(), tonic::Code::NotFound);

    stop_server(running)
        .await
        .expect("server should stop cleanly");
}

struct RunningTlsServer {
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_task: tokio::task::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>,
    endpoint: String,
}

async fn start_server(
    grpc_tls: GrpcTlsConfig,
) -> Result<RunningTlsServer, Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let server = GrpcServer::new(
        AppConfig {
            service_name: "kmp".to_string(),
            grpc_bind: addr.to_string(),
            grpc_tls,
            graph_uri: "neo4j://localhost:7687".to_string(),
            detail_uri: "redis://localhost:6379".to_string(),
            snapshot_uri: "redis://localhost:6379".to_string(),
            events_subject_prefix: "rehydration".to_string(),
        },
        EmptyGraphNeighborhoodReader,
        EmptyNodeDetailReader,
        NoopSnapshotStore,
        InMemoryContextEventStore::new(),
        Arc::new(NoopQualityObserver),
    );
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let server_task = tokio::spawn(async move {
        server
            .serve_with_listener_shutdown(listener, async move {
                let _ = shutdown_rx.await;
            })
            .await
    });

    Ok(RunningTlsServer {
        shutdown_tx: Some(shutdown_tx),
        server_task,
        endpoint: format!("https://localhost:{}", addr.port()),
    })
}

async fn stop_server(
    server: RunningTlsServer,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let RunningTlsServer {
        shutdown_tx,
        server_task,
        endpoint: _,
    } = server;
    if let Some(tx) = shutdown_tx {
        let _ = tx.send(());
    }

    server_task.await?
}

async fn connect_tls_channel(
    endpoint: &str,
    tls_fixture: &TlsFixturePaths,
    include_client_identity: bool,
) -> Result<Channel, Box<dyn std::error::Error + Send + Sync>> {
    let mut tls = ClientTlsConfig::new()
        .ca_certificate(Certificate::from_pem(tls_fixture.ca_cert_pem.clone()))
        .domain_name("localhost");
    if include_client_identity {
        tls = tls.identity(Identity::from_pem(
            tls_fixture.client_cert_pem.clone(),
            tls_fixture.client_key_pem.clone(),
        ));
    }

    let endpoint = Endpoint::from_shared(endpoint.to_string())?.tls_config(tls)?;
    let mut attempts = 0u8;

    loop {
        match endpoint.clone().connect().await {
            Ok(channel) => return Ok(channel),
            Err(error) if attempts < 20 => {
                attempts += 1;
                sleep(std::time::Duration::from_millis(25)).await;
                if error.to_string().contains("Connection refused") {
                    continue;
                }
                return Err(Box::new(error));
            }
            Err(error) => return Err(Box::new(error)),
        }
    }
}

async fn get_context(
    client: &mut ContextQueryServiceClient<Channel>,
) -> Result<tonic::Response<kmp_proto::v1beta1::GetContextResponse>, tonic::Status> {
    client
        .get_context(GetContextRequest {
            root_node_id: "case-123".to_string(),
            role: "developer".to_string(),
            token_budget: 1024,
            requested_scopes: vec!["graph".to_string()],
            depth: 0,
            max_tier: 0,
            rehydration_mode: 0,
        })
        .await
}

fn path_string(path: &std::path::Path) -> io::Result<String> {
    path.to_str()
        .map(ToString::to_string)
        .ok_or_else(|| io::Error::other(format!("path is not valid UTF-8: {}", path.display())))
}

fn run_openssl<const N: usize>(args: [&str; N]) -> io::Result<()> {
    let output = Command::new("openssl").args(args).output()?;
    if output.status.success() {
        return Ok(());
    }

    Err(io::Error::other(format!(
        "openssl command failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}
