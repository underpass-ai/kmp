use std::path::PathBuf;
use std::sync::Arc;

use kmp_mcp::{KernelMcpGrpcTlsConfig, KernelMcpServer};
use kmp_proto::v1beta1::{
    AcceptedCounts, AnswerReason, AskRequest, AskResponse, DimensionScopeMode, ForwardRequest,
    ForwardResponse, GotoRequest, GotoResponse, IngestRequest, IngestResponse, IngestedMemory,
    InspectRequest, InspectResponse, InspectedLinks, InspectedObject, MemoryConfidence,
    MemoryEvidence, MemoryRelation, MemorySemanticClass, MemorySourceKind, NearRequest,
    NearResponse, ProjectVisualRequest, ProjectVisualResponse, Proof, RawMemoryRef, RewindRequest,
    RewindResponse, TemporalCoordinate, TemporalCursor, TemporalDirection, TemporalEntry,
    TemporalMoveRequest, TemporalMoveResponse, TemporalNearRequest, TemporalState, TraceRequest,
    TraceResponse, WakeClaim, WakePacket, WakeRequest, WakeResponse,
    kernel_memory_service_server::{KernelMemoryService, KernelMemoryServiceServer},
};
use kmp_proto_mapping::v1beta1::recall_projection::{project_ask_response, project_wake_response};
use prost_types::Timestamp;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Certificate, Identity, ServerTlsConfig};
use tonic::{Request, Response, Status};

#[tokio::test]
async fn grpc_backend_maps_kernel_memory_service_responses_to_kmp_tools() {
    let recorded = RecordedMemoryRequests::default();
    let endpoint = spawn_fake_memory_server(recorded.clone()).await;
    let server = KernelMcpServer::grpc(endpoint);

    let wake = call_tool(
        &server,
        1,
        "kmp_wake",
        json!({
            "about": "node:root",
            "role": "implementer",
            "intent": "continue the live incident",
            "depth": 3,
            "budget": {
                "tokens": 1600,
                "detail": "full",
                "max_bytes": 4000
            },
            "page": {"entries": 1},
            "dimensions": {
                "mode": "only",
                "include": ["conversation"]
            }
        }),
    )
    .await;
    assert_eq!(wake["result"]["isError"], false);
    assert_eq!(
        wake["result"]["structuredContent"]["wake"]["objective"],
        "continue the live incident"
    );
    assert_eq!(
        wake["result"]["structuredContent"]["proof"]["path"][0]["class"],
        "evidential"
    );
    let wake_structured = &wake["result"]["structuredContent"];
    assert_eq!(
        wake_structured["projection"]["budget"]["used_bytes"],
        serde_json::to_vec(wake_structured)
            .expect("serialized wake")
            .len()
    );

    let wake_requests = recorded.wakes().await;
    assert_eq!(wake_requests.len(), 1);
    assert_eq!(wake_requests[0].about, "node:root");
    assert_eq!(wake_requests[0].role, "implementer");
    assert_eq!(
        wake_requests[0].budget.as_ref().expect("budget").tokens,
        1600
    );
    assert_eq!(
        wake_requests[0].budget.as_ref().expect("budget").max_bytes,
        4000
    );
    assert_eq!(wake_requests[0].page.as_ref().expect("page").entries, 1);
    assert_eq!(
        wake_requests[0]
            .dimensions
            .as_ref()
            .expect("dimensions")
            .include,
        ["conversation"]
    );

    let ask = call_tool(
        &server,
        2,
        "kmp_ask",
        json!({
            "about": "node:root",
            "question": "What should the next agent trust?",
            "answer_policy": "evidence_or_unknown",
            "budget": {
                "tokens": 654,
                "detail": "full",
                "max_bytes": 4000
            }
        }),
    )
    .await;
    assert_eq!(ask["result"]["isError"], false);
    assert_eq!(
        ask["result"]["structuredContent"]["answer"],
        "Trust the typed KernelMemoryService response."
    );
    assert_eq!(
        ask["result"]["structuredContent"]["because"][0]["ref"],
        "claim:typed-answer"
    );
    let ask_structured = &ask["result"]["structuredContent"];
    assert_eq!(
        ask_structured["projection"]["budget"]["used_bytes"],
        serde_json::to_vec(ask_structured)
            .expect("serialized ask")
            .len()
    );

    let trace = call_tool(
        &server,
        3,
        "kmp_trace",
        json!({
            "about": "node",
            "from": "node:root",
            "to": "node:target",
            "goal": "prove path",
            "budget": {
                "tokens": 111
            }
        }),
    )
    .await;
    assert_eq!(trace["result"]["isError"], false);
    assert_eq!(
        trace["result"]["structuredContent"]["trace"][0]["from"],
        "node:root"
    );
    assert_eq!(
        trace["result"]["structuredContent"]["trace"][0]["to"],
        "node:target"
    );
    assert_eq!(recorded.traces().await[0].goal, "prove path");

    let inspect = call_tool(
        &server,
        4,
        "kmp_inspect",
        json!({
            "about": "node",
            "ref": "node:target",
            "include": {
                "incoming": true,
                "outgoing": true,
                "details": true,
                "raw": true
            }
        }),
    )
    .await;
    assert_eq!(inspect["result"]["isError"], false);
    assert_eq!(
        inspect["result"]["structuredContent"]["object"]["ref"],
        "node:target"
    );
    assert_eq!(
        inspect["result"]["structuredContent"]["links"]["incoming"][0]["from"],
        "node:source"
    );
    assert_eq!(
        inspect["result"]["structuredContent"]["raw"][0]["ref"],
        "node:target"
    );
}

#[tokio::test]
async fn grpc_backend_maps_temporal_tools_to_kernel_memory_service() {
    let recorded = RecordedMemoryRequests::default();
    let endpoint = spawn_fake_memory_server(recorded.clone()).await;
    let server = KernelMcpServer::grpc(endpoint);

    let forward = call_tool(
        &server,
        7,
        "kmp_forward",
        json!({
            "about": "question:temporal",
            "from": {
                "ref": "claim:rachel-denver"
            },
            "dimensions": {
                "mode": "only",
                "include": ["conversation"],
                "scope": "all_abouts",
                "scope_ids": ["conversation:rachel"]
            },
            "limit": {
                "entries": 5,
                "tokens": 1000
            },
            "include": {
                "relations": true,
                "raw_refs": false
            },
            "depth": 4
        }),
    )
    .await;

    assert_eq!(forward["result"]["isError"], false);
    assert_eq!(
        forward["result"]["structuredContent"]["temporal"]["direction"],
        "forward"
    );
    assert_eq!(
        forward["result"]["structuredContent"]["entries"][0]["ref"],
        "claim:rachel-austin"
    );

    let moves = recorded.moves().await;
    assert_eq!(moves.len(), 1);
    assert_eq!(moves[0].method, "forward");
    assert_eq!(
        moves[0].request.cursor.as_ref().expect("cursor").r#ref,
        "claim:rachel-denver"
    );
    assert_eq!(
        moves[0]
            .request
            .dimensions
            .as_ref()
            .expect("dimensions")
            .scope,
        DimensionScopeMode::AllAbouts as i32
    );
    assert_eq!(
        moves[0]
            .request
            .dimensions
            .as_ref()
            .expect("dimensions")
            .scope_ids,
        vec!["conversation:rachel".to_string()]
    );
    assert!(!moves[0].request.include.as_ref().expect("include").raw_refs);
    assert_eq!(moves[0].request.budget.as_ref().expect("budget").depth, 4);

    let near = call_tool(
        &server,
        8,
        "kmp_near",
        json!({
            "about": "question:temporal",
            "around": {
                "time": "2026-04-12T15:03:00Z"
            },
            "window": {
                "before_entries": 1,
                "after_entries": 1
            }
        }),
    )
    .await;
    assert_eq!(near["result"]["isError"], false);
    assert_eq!(
        near["result"]["structuredContent"]["temporal"]["direction"],
        "near"
    );
    assert!(
        recorded.nears().await[0]
            .around
            .as_ref()
            .expect("cursor")
            .time
            .is_some()
    );
}

#[tokio::test]
async fn grpc_backend_maps_temporal_raw_refs_to_kernel_memory_service() {
    let recorded = RecordedMemoryRequests::default();
    let endpoint = spawn_fake_memory_server(recorded.clone()).await;
    let server = KernelMcpServer::grpc(endpoint);

    let forward = call_tool(
        &server,
        17,
        "kmp_forward",
        json!({
            "about": "question:temporal",
            "from": {
                "ref": "claim:rachel-denver"
            },
            "include": {
                "raw_refs": true
            }
        }),
    )
    .await;

    assert_eq!(forward["result"]["isError"], false);
    assert_eq!(
        forward["result"]["structuredContent"]["raw_refs"][0]["ref"],
        "claim:rachel-austin"
    );
    assert!(
        recorded.moves().await[0]
            .request
            .include
            .as_ref()
            .expect("include")
            .raw_refs
    );
}

#[tokio::test]
async fn grpc_backend_maps_kmp_ingest_to_kernel_memory_service() {
    let recorded = RecordedMemoryRequests::default();
    let endpoint = spawn_fake_memory_server(recorded.clone()).await;
    let server = KernelMcpServer::grpc(endpoint);

    let ingest = call_tool(
        &server,
        5,
        "kmp_ingest",
        json!({
            "about": "question:830ce83f",
            "memory": {
                "dimensions": [
                    {
                        "id": "conversation:rachel",
                        "kind": "conversation"
                    }
                ],
                "entries": [
                    {
                        "id": "claim:rachel-austin",
                        "kind": "claim",
                        "text": "Rachel moved to Austin.",
                        "coordinates": [
                            {
                                "dimension": "conversation",
                                "scope_id": "conversation:rachel",
                                "sequence": 1,
                                "occurred_at": "2026-04-12T15:05:00Z"
                            }
                        ]
                    }
                ],
                "relations": [
                    {
                        "from": "claim:rachel-austin",
                        "to": "claim:rachel-denver",
                        "rel": "supersedes",
                        "class": "evidential",
                        "why": "The later statement corrects the earlier one.",
                        "evidence": "Rachel corrected the destination.",
                        "confidence": "high"
                    }
                ],
                "evidence": [
                    {
                        "id": "evidence:rachel-turn-2",
                        "supports": ["claim:rachel-austin"],
                        "text": "Rachel corrected the destination.",
                        "source": "conversation turn 2"
                    }
                ]
            },
            "provenance": {
                "source_kind": "agent",
                "source_agent": "longmemeval-adapter",
                "observed_at": "2026-05-04T10:00:00Z",
                "correlation_id": "corr:830ce83f",
                "causation_id": "eval:item:830ce83f"
            },
            "idempotency_key": "ingest:830ce83f:1"
        }),
    )
    .await;

    assert_eq!(ingest["result"]["isError"], false);
    assert_eq!(
        ingest["result"]["structuredContent"]["memory"]["memory_id"],
        "memory:830ce83f:1"
    );
    assert_eq!(
        ingest["result"]["structuredContent"]["memory"]["accepted"]["entries"],
        1
    );
    assert_eq!(
        ingest["result"]["structuredContent"]["memory"]["read_after_write_ready"],
        true
    );

    let ingests = recorded.ingests().await;
    assert_eq!(ingests.len(), 1);
    let request = &ingests[0];
    assert_eq!(request.about, "question:830ce83f");
    assert_eq!(request.idempotency_key, "ingest:830ce83f:1");
    assert_eq!(
        request.provenance.as_ref().expect("provenance").source_kind,
        MemorySourceKind::Agent as i32
    );
    assert_eq!(
        request
            .provenance
            .as_ref()
            .expect("provenance")
            .source_agent,
        "longmemeval-adapter"
    );
    let memory = request.memory.as_ref().expect("memory");
    assert_eq!(memory.dimensions[0].id, "conversation:rachel");
    assert_eq!(memory.entries[0].coordinates[0].sequence, Some(1));
    assert_eq!(memory.relations[0].source_ref, "claim:rachel-austin");
    assert_eq!(memory.relations[0].target_ref, "claim:rachel-denver");
}

#[tokio::test]
async fn grpc_backend_forwards_incremental_ingest_with_empty_dimensions() {
    let recorded = RecordedMemoryRequests::default();
    let endpoint = spawn_fake_memory_server(recorded.clone()).await;
    let server = KernelMcpServer::grpc(endpoint);

    let ingest = call_tool(
        &server,
        6,
        "kmp_ingest",
        json!({
            "about": "question:830ce83f",
            "memory": {
                "dimensions": [],
                "entries": [
                    {
                        "id": "claim:rachel-denver",
                        "kind": "claim",
                        "text": "Rachel moved to Denver.",
                        "coordinates": [
                            {
                                "dimension": "conversation",
                                "scope_id": "conversation:rachel",
                                "sequence": 2,
                                "occurred_at": "2026-04-13T15:05:00Z"
                            }
                        ]
                    }
                ],
                "relations": [],
                "evidence": []
            },
            "idempotency_key": "ingest:830ce83f:2"
        }),
    )
    .await;

    assert_eq!(ingest["result"]["isError"], false);
    let ingests = recorded.ingests().await;
    assert_eq!(ingests.len(), 1);
    let memory = ingests[0].memory.as_ref().expect("memory");
    assert!(memory.dimensions.is_empty());
    assert_eq!(memory.entries[0].id, "claim:rachel-denver");
}

#[tokio::test]
async fn grpc_backend_dry_run_ingest_does_not_call_kernel_memory_service() {
    let recorded = RecordedMemoryRequests::default();
    let endpoint = spawn_fake_memory_server(recorded.clone()).await;
    let server = KernelMcpServer::grpc(endpoint);

    let ingest = call_tool(
        &server,
        6,
        "kmp_ingest",
        json!({
            "about": "question:dry-run",
            "memory": {
                "dimensions": [{"id": "conversation:dry-run", "kind": "conversation"}],
                "entries": [
                    {
                        "id": "question:dry-run:claim:dry-run",
                        "kind": "claim",
                        "text": "Dry run memory.",
                        "coordinates": [
                            {
                                "dimension": "conversation",
                                "scope_id": "conversation:dry-run",
                                "sequence": 1
                            }
                        ]
                    }
                ]
            },
            "idempotency_key": "ingest:dry-run:1",
            "dry_run": true
        }),
    )
    .await;

    assert_eq!(ingest["result"]["isError"], false);
    assert!(
        ingest["result"]["structuredContent"]["warnings"][0]
            .as_str()
            .expect("dry run warning should be text")
            .contains("KernelMemoryService.Ingest")
    );
    assert!(recorded.ingests().await.is_empty());
}

#[tokio::test]
async fn grpc_backend_connects_to_mutual_tls_kernel_memory_service() {
    install_test_crypto_provider();

    let certs = TestTlsFiles::write();
    let endpoint =
        spawn_fake_memory_server_with_mutual_tls(RecordedMemoryRequests::default()).await;
    let server = KernelMcpServer::grpc_with_tls(endpoint, certs.client_config());

    let inspect = call_tool(
        &server,
        9,
        "kmp_inspect",
        json!({
            "about": "node",
            "ref": "node:mtls"
        }),
    )
    .await;

    assert_eq!(inspect["result"]["isError"], false);
    assert_eq!(
        inspect["result"]["structuredContent"]["object"]["ref"],
        "node:mtls"
    );
}

async fn spawn_fake_memory_server(recorded: RecordedMemoryRequests) -> String {
    spawn_fake_memory_server_with_tls(None, recorded).await
}

async fn spawn_fake_memory_server_with_mutual_tls(recorded: RecordedMemoryRequests) -> String {
    spawn_fake_memory_server_with_tls(
        Some(
            ServerTlsConfig::new()
                .identity(Identity::from_pem(TEST_SERVER_CERT, TEST_SERVER_KEY))
                .client_ca_root(Certificate::from_pem(TEST_CA_CERT)),
        ),
        recorded,
    )
    .await
}

async fn spawn_fake_memory_server_with_tls(
    tls_config: Option<ServerTlsConfig>,
    recorded: RecordedMemoryRequests,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("fake gRPC server should bind to an ephemeral port");
    let addr = listener
        .local_addr()
        .expect("fake gRPC server should expose its local address");
    let incoming = TcpListenerStream::new(listener);

    tokio::spawn(async move {
        let mut builder = tonic::transport::Server::builder();
        if let Some(tls_config) = tls_config {
            builder = builder
                .tls_config(tls_config)
                .expect("fake gRPC server TLS config should be valid");
        }

        builder
            .add_service(KernelMemoryServiceServer::new(FakeMemoryService {
                recorded,
            }))
            .serve_with_incoming(incoming)
            .await
            .expect("fake gRPC server should run");
    });

    format!("http://{addr}")
}

async fn call_tool(server: &KernelMcpServer, id: u64, name: &str, arguments: Value) -> Value {
    let response = server
        .handle_json_line(
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "tools/call",
                "params": {
                    "name": name,
                    "arguments": arguments
                }
            })
            .to_string(),
        )
        .await
        .expect("tools/call should produce a response");

    serde_json::from_str(&response).expect("tools/call response should be valid JSON")
}

#[derive(Clone, Debug)]
struct RecordedTemporalMove {
    method: &'static str,
    request: TemporalMoveRequest,
}

#[derive(Clone, Default)]
struct RecordedMemoryRequests {
    ingests: Arc<Mutex<Vec<IngestRequest>>>,
    wakes: Arc<Mutex<Vec<WakeRequest>>>,
    asks: Arc<Mutex<Vec<AskRequest>>>,
    moves: Arc<Mutex<Vec<RecordedTemporalMove>>>,
    nears: Arc<Mutex<Vec<TemporalNearRequest>>>,
    traces: Arc<Mutex<Vec<TraceRequest>>>,
    inspects: Arc<Mutex<Vec<InspectRequest>>>,
}

impl RecordedMemoryRequests {
    async fn ingests(&self) -> Vec<IngestRequest> {
        self.ingests.lock().await.clone()
    }

    async fn wakes(&self) -> Vec<WakeRequest> {
        self.wakes.lock().await.clone()
    }

    async fn moves(&self) -> Vec<RecordedTemporalMove> {
        self.moves.lock().await.clone()
    }

    async fn nears(&self) -> Vec<TemporalNearRequest> {
        self.nears.lock().await.clone()
    }

    async fn traces(&self) -> Vec<TraceRequest> {
        self.traces.lock().await.clone()
    }
}

#[derive(Clone)]
struct FakeMemoryService {
    recorded: RecordedMemoryRequests,
}

#[tonic::async_trait]
impl KernelMemoryService for FakeMemoryService {
    async fn ingest(
        &self,
        request: Request<IngestRequest>,
    ) -> Result<Response<IngestResponse>, Status> {
        let request = request.into_inner();
        self.recorded.ingests.lock().await.push(request.clone());
        let memory = request.memory.as_ref();
        let memory_id = memory_id_from_idempotency_key(&request.idempotency_key);

        Ok(Response::new(IngestResponse {
            summary: format!("Ingested memory for {}.", request.about),
            memory: Some(IngestedMemory {
                about: request.about,
                memory_id,
                accepted: Some(AcceptedCounts {
                    entries: memory
                        .map(|memory| memory.entries.len())
                        .unwrap_or_default() as u32,
                    relations: memory
                        .map(|memory| memory.relations.len())
                        .unwrap_or_default() as u32,
                    evidence: memory
                        .map(|memory| memory.evidence.len())
                        .unwrap_or_default() as u32,
                }),
                read_after_write_ready: true,
            }),
            warnings: Vec::new(),
        }))
    }

    async fn wake(&self, request: Request<WakeRequest>) -> Result<Response<WakeResponse>, Status> {
        let request = request.into_inner();
        self.recorded.wakes.lock().await.push(request.clone());

        let response = WakeResponse {
            summary: format!("Wake summary for {}.", request.about),
            wake: Some(WakePacket {
                objective: if request.intent.trim().is_empty() {
                    "continue".to_string()
                } else {
                    request.intent.clone()
                },
                current_state: vec![format!("State for {}.", request.about)],
                causal_spine: vec![WakeClaim {
                    claim: "Typed wake claim.".to_string(),
                    because: "KernelMemoryService.Wake returned it.".to_string(),
                    evidence_ref: "evidence:typed".to_string(),
                }],
                open_loops: Vec::new(),
                next_actions: Vec::new(),
                guardrails: Vec::new(),
            }),
            proof: Some(proof(&request.about, "claim:typed-wake")),
            resume_cursor: None,
            warnings: Vec::new(),
            projection: None,
            truncation: None,
        };
        Ok(Response::new(
            project_wake_response(response, &request)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
        ))
    }

    async fn ask(&self, request: Request<AskRequest>) -> Result<Response<AskResponse>, Status> {
        let request = request.into_inner();
        self.recorded.asks.lock().await.push(request.clone());

        let response = AskResponse {
            summary: "Typed deterministic answer.".to_string(),
            answer: "Trust the typed KernelMemoryService response.".to_string(),
            because: vec![AnswerReason {
                claim: "Typed answer claim.".to_string(),
                evidence: "Typed answer evidence.".to_string(),
                r#ref: "claim:typed-answer".to_string(),
            }],
            proof: Some(proof(&request.about, "claim:typed-answer")),
            warnings: Vec::new(),
            projection: None,
            truncation: None,
            asked_as: String::new(),
        };
        Ok(Response::new(
            project_ask_response(response, &request)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
        ))
    }

    async fn goto(&self, request: Request<GotoRequest>) -> Result<Response<GotoResponse>, Status> {
        let response = self
            .temporal_move(
                "goto",
                TemporalDirection::Goto,
                temporal_move_request_from_goto(request.into_inner()),
            )
            .await?;
        Ok(Response::new(goto_response_from_temporal(response)))
    }

    async fn near(&self, request: Request<NearRequest>) -> Result<Response<NearResponse>, Status> {
        let request = temporal_near_request_from_near(request.into_inner());
        self.recorded.nears.lock().await.push(request.clone());

        Ok(Response::new(near_response_from_temporal(
            temporal_response(TemporalDirection::Near, request.around),
        )))
    }

    async fn rewind(
        &self,
        request: Request<RewindRequest>,
    ) -> Result<Response<RewindResponse>, Status> {
        let response = self
            .temporal_move(
                "rewind",
                TemporalDirection::Rewind,
                temporal_move_request_from_rewind(request.into_inner()),
            )
            .await?;
        Ok(Response::new(rewind_response_from_temporal(response)))
    }

    async fn forward(
        &self,
        request: Request<ForwardRequest>,
    ) -> Result<Response<ForwardResponse>, Status> {
        let response = self
            .temporal_move(
                "forward",
                TemporalDirection::Forward,
                temporal_move_request_from_forward(request.into_inner()),
            )
            .await?;
        Ok(Response::new(forward_response_from_temporal(response)))
    }

    async fn project_visual(
        &self,
        _request: Request<ProjectVisualRequest>,
    ) -> Result<Response<ProjectVisualResponse>, Status> {
        Err(Status::unimplemented(
            "not used by this backend contract test",
        ))
    }

    async fn trace(
        &self,
        request: Request<TraceRequest>,
    ) -> Result<Response<TraceResponse>, Status> {
        let request = request.into_inner();
        self.recorded.traces.lock().await.push(request.clone());

        Ok(Response::new(TraceResponse {
            summary: format!("Trace from {} to {}.", request.from, request.to),
            trace: vec![relation(&request.from, &request.to, "supports")],
            warnings: Vec::new(),
            page: None,
            quality: None,
        }))
    }

    async fn inspect(
        &self,
        request: Request<InspectRequest>,
    ) -> Result<Response<InspectResponse>, Status> {
        let request = request.into_inner();
        self.recorded.inspects.lock().await.push(request.clone());

        Ok(Response::new(InspectResponse {
            summary: format!("Inspect {}.", request.r#ref),
            object: Some(InspectedObject {
                r#ref: request.r#ref.clone(),
                kind: "claim".to_string(),
                text: format!("Node detail for {}.", request.r#ref),
                metadata: Default::default(),
                source: String::new(),
            }),
            links: Some(InspectedLinks {
                incoming: vec![relation("node:source", &request.r#ref, "supports")],
                outgoing: vec![relation(&request.r#ref, "node:target", "supports")],
            }),
            evidence: vec![evidence(&request.r#ref)],
            warnings: Vec::new(),
            raw: if request.include.as_ref().is_some_and(|include| include.raw) {
                vec![raw_ref(&request.r#ref)]
            } else {
                Vec::new()
            },
            quality: None,
        }))
    }
}

impl FakeMemoryService {
    async fn temporal_move(
        &self,
        method: &'static str,
        direction: TemporalDirection,
        request: TemporalMoveRequest,
    ) -> Result<TemporalMoveResponse, Status> {
        self.recorded.moves.lock().await.push(RecordedTemporalMove {
            method,
            request: request.clone(),
        });

        Ok(temporal_response(direction, request.cursor))
    }
}

fn temporal_move_request_from_goto(request: GotoRequest) -> TemporalMoveRequest {
    TemporalMoveRequest {
        about: request.about,
        cursor: request.cursor,
        dimensions: request.dimensions,
        window: request.window,
        limit: request.limit,
        include: request.include,
        budget: request.budget,
        axis: request.axis,
    }
}

fn temporal_move_request_from_rewind(request: RewindRequest) -> TemporalMoveRequest {
    TemporalMoveRequest {
        about: request.about,
        cursor: request.cursor,
        dimensions: request.dimensions,
        window: request.window,
        limit: request.limit,
        include: request.include,
        budget: request.budget,
        axis: request.axis,
    }
}

fn temporal_move_request_from_forward(request: ForwardRequest) -> TemporalMoveRequest {
    TemporalMoveRequest {
        about: request.about,
        cursor: request.cursor,
        dimensions: request.dimensions,
        window: request.window,
        limit: request.limit,
        include: request.include,
        budget: request.budget,
        axis: request.axis,
    }
}

fn temporal_near_request_from_near(request: NearRequest) -> TemporalNearRequest {
    TemporalNearRequest {
        about: request.about,
        around: request.around,
        dimensions: request.dimensions,
        window: request.window,
        limit: request.limit,
        include: request.include,
        budget: request.budget,
        axis: request.axis,
    }
}

fn goto_response_from_temporal(response: TemporalMoveResponse) -> GotoResponse {
    GotoResponse {
        summary: response.summary,
        temporal: response.temporal,
        coverage: response.coverage,
        entries: response.entries,
        proof: response.proof,
        warnings: response.warnings,
        raw_refs: response.raw_refs,
        page: response.page,
        quality: response.quality,
    }
}

fn near_response_from_temporal(response: TemporalMoveResponse) -> NearResponse {
    NearResponse {
        summary: response.summary,
        temporal: response.temporal,
        coverage: response.coverage,
        entries: response.entries,
        proof: response.proof,
        warnings: response.warnings,
        raw_refs: response.raw_refs,
        page: response.page,
        quality: response.quality,
    }
}

fn rewind_response_from_temporal(response: TemporalMoveResponse) -> RewindResponse {
    RewindResponse {
        summary: response.summary,
        temporal: response.temporal,
        coverage: response.coverage,
        entries: response.entries,
        proof: response.proof,
        warnings: response.warnings,
        raw_refs: response.raw_refs,
        page: response.page,
        quality: response.quality,
    }
}

fn forward_response_from_temporal(response: TemporalMoveResponse) -> ForwardResponse {
    ForwardResponse {
        summary: response.summary,
        temporal: response.temporal,
        coverage: response.coverage,
        entries: response.entries,
        proof: response.proof,
        warnings: response.warnings,
        raw_refs: response.raw_refs,
        page: response.page,
        quality: response.quality,
    }
}

fn temporal_response(
    direction: TemporalDirection,
    requested: Option<TemporalCursor>,
) -> TemporalMoveResponse {
    TemporalMoveResponse {
        summary: "Returned typed temporal entries.".to_string(),
        temporal: Some(TemporalState {
            direction: direction as i32,
            axis: kmp_proto::v1beta1::TemporalAxis::Unspecified as i32,
            requested,
            resolved: Some(coordinate(2)),
        }),
        coverage: None,
        entries: vec![TemporalEntry {
            r#ref: "claim:rachel-austin".to_string(),
            kind: "claim".to_string(),
            text: "Rachel later corrected the destination to Austin.".to_string(),
            coordinates: vec![coordinate(2)],
            metadata: Default::default(),
        }],
        proof: Some(proof("claim:rachel-denver", "claim:rachel-austin")),
        warnings: Vec::new(),
        raw_refs: vec![raw_ref("claim:rachel-austin")],
        page: Some(kmp_proto::v1beta1::PageInfo {
            returned: 1,
            total: 1,
            has_more: false,
            next_cursor: String::new(),
        }),
        quality: None,
    }
}

fn proof(source: &str, target: &str) -> Proof {
    Proof {
        path: vec![relation(source, target, "supports")],
        evidence: vec![evidence(source)],
        conflicts: Vec::new(),
        superseded: Vec::new(),
        expired: Vec::new(),
        missing: Vec::new(),
        frontier_size: 0,
        matched_terms: Vec::new(),
        matched_relations: Vec::new(),
        confidence: MemoryConfidence::High as i32,
        interval: None,
        axis: 0,
        nearest_outside: None,
        as_of: None,
        abouts_selected: Vec::new(),
        abouts_empty_in_selection: Vec::new(),
    }
}

fn relation(source_ref: &str, target_ref: &str, rel: &str) -> MemoryRelation {
    MemoryRelation {
        source_ref: source_ref.to_string(),
        target_ref: target_ref.to_string(),
        rel: rel.to_string(),
        semantic_class: MemorySemanticClass::Evidential as i32,
        why: "Typed relation rationale.".to_string(),
        evidence: "Typed relation evidence.".to_string(),
        confidence: MemoryConfidence::High as i32,
        sequence: Some(1),
        explanation: None,
        evidence_refs: Vec::new(),
    }
}

fn evidence(source: &str) -> MemoryEvidence {
    MemoryEvidence {
        id: "evidence:typed".to_string(),
        supports: vec![source.to_string()],
        text: "Typed evidence.".to_string(),
        source: source.to_string(),
        time: None,
        metadata: Default::default(),
    }
}

fn raw_ref(ref_id: &str) -> RawMemoryRef {
    RawMemoryRef {
        r#ref: ref_id.to_string(),
        kind: "claim".to_string(),
        text: format!("Raw text for {ref_id}."),
        coordinates: vec![coordinate(2)],
        detail: format!("Raw detail for {ref_id}."),
        content_hash: "hash-raw".to_string(),
        revision: 1,
    }
}

fn coordinate(sequence: u32) -> TemporalCoordinate {
    TemporalCoordinate {
        dimension: "conversation".to_string(),
        scope_id: "conversation:rachel".to_string(),
        occurred_at: Some(Timestamp {
            seconds: 1_765_000_000,
            nanos: 0,
        }),
        observed_at: None,
        ingested_at: None,
        valid_from: None,
        valid_until: None,
        sequence: Some(sequence),
        rank: None,
        metadata: Default::default(),
    }
}

fn memory_id_from_idempotency_key(idempotency_key: &str) -> String {
    idempotency_key
        .strip_prefix("ingest:")
        .map(|suffix| format!("memory:{suffix}"))
        .unwrap_or_else(|| format!("memory:{idempotency_key}"))
}

struct TestTlsFiles {
    _dir: tempfile::TempDir,
    ca_path: PathBuf,
    cert_path: PathBuf,
    key_path: PathBuf,
}

impl TestTlsFiles {
    fn write() -> Self {
        let dir = tempfile::tempdir().expect("test TLS tempdir should be created");
        let ca_path = dir.path().join("ca.crt");
        let cert_path = dir.path().join("client.crt");
        let key_path = dir.path().join("client.key");

        std::fs::write(&ca_path, TEST_CA_CERT).expect("test CA should be written");
        std::fs::write(&cert_path, TEST_CLIENT_CERT).expect("test client cert should be written");
        std::fs::write(&key_path, TEST_CLIENT_KEY).expect("test client key should be written");

        Self {
            _dir: dir,
            ca_path,
            cert_path,
            key_path,
        }
    }

    fn client_config(&self) -> KernelMcpGrpcTlsConfig {
        KernelMcpGrpcTlsConfig::mutual(
            self.ca_path.clone(),
            self.cert_path.clone(),
            self.key_path.clone(),
            Some("localhost".to_string()),
        )
    }
}

fn install_test_crypto_provider() {
    let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
}

// Static test-only certificates generated for the local mTLS fake server.
// They are not trusted by production code and exist only to avoid shelling out
// to OpenSSL during `cargo test`.
const TEST_CA_CERT: &str = r#"-----BEGIN CERTIFICATE-----
MIIBmDCCAT+gAwIBAgIUDrShLER4kZWk+jp6yLZMPVnc4KAwCgYIKoZIzj0EAwIw
IjEgMB4GA1UEAwwXcmVoeWRyYXRpb24tbWNwLXRlc3QtY2EwHhcNMjYwNTAzMTc1
NTMyWhcNMzYwNDMwMTc1NTMyWjAiMSAwHgYDVQQDDBdyZWh5ZHJhdGlvbi1tY3At
dGVzdC1jYTBZMBMGByqGSM49AgEGCCqGSM49AwEHA0IABARF7qIfl/QvSiZvX8di
hbYIuq/gzFG6qNa0v86901BvKM21S9zna0xfZbxodZd7mfwzvSDnHFJIAfymQ/a0
SSmjUzBRMB0GA1UdDgQWBBSDc7yfQDotnauGfYkkeHrMc+gSBjAfBgNVHSMEGDAW
gBSDc7yfQDotnauGfYkkeHrMc+gSBjAPBgNVHRMBAf8EBTADAQH/MAoGCCqGSM49
BAMCA0cAMEQCIH0m3WBS/qGq20lcXg+iO+RFHkE768JcjuB/viffYkzuAiAZczMl
OKRQEYO/3ZfaPZkHZsL99dUFy3czK6cAtly4Lg==
-----END CERTIFICATE-----
"#;

const TEST_SERVER_CERT: &str = r#"-----BEGIN CERTIFICATE-----
MIIBpTCCAUugAwIBAgIUDt2LOgaFR696iyRVM+ipq3EAEbcwCgYIKoZIzj0EAwIw
IjEgMB4GA1UEAwwXcmVoeWRyYXRpb24tbWNwLXRlc3QtY2EwHhcNMjYwNTAzMTc1
NjI3WhcNMzYwNDMwMTc1NjI3WjAUMRIwEAYDVQQDDAlsb2NhbGhvc3QwWTATBgcq
hkjOPQIBBggqhkjOPQMBBwNCAAQZadG+mSPC5wBjpC4V3TUX7ZGXJ8ypWKo6Pmah
zDGI2jVtXosZKtYkT7mrykgyO/U5lBMi4j6FZBR3ScXEYdzho20wazAUBgNVHREE
DTALgglsb2NhbGhvc3QwEwYDVR0lBAwwCgYIKwYBBQUHAwEwHQYDVR0OBBYEFAvZ
kmZyjsv0PhtiTOLZa2qQJhdbMB8GA1UdIwQYMBaAFINzvJ9AOi2dq4Z9iSR4esxz
6BIGMAoGCCqGSM49BAMCA0gAMEUCIEJHCWnCaKr73gefBhAYNZQjPUh5IomVAI0F
czqsWVUnAiEAyv098AZ9D0VKjWCNdfu+Q0jg96A4BD3SG+122qXiQgg=
-----END CERTIFICATE-----
"#;

const TEST_SERVER_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg0ShbEuRisGPwMym1
C0vGZ/hD1c91p9fA5nvtWKoNRYahRANCAAQZadG+mSPC5wBjpC4V3TUX7ZGXJ8yp
WKo6PmahzDGI2jVtXosZKtYkT7mrykgyO/U5lBMi4j6FZBR3ScXEYdzh
-----END PRIVATE KEY-----
"#;

const TEST_CLIENT_CERT: &str = r#"-----BEGIN CERTIFICATE-----
MIIBoTCCAUegAwIBAgIUDt2LOgaFR696iyRVM+ipq3EAEbYwCgYIKoZIzj0EAwIw
IjEgMB4GA1UEAwwXcmVoeWRyYXRpb24tbWNwLXRlc3QtY2EwHhcNMjYwNTAzMTc1
NjAzWhcNMzYwNDMwMTc1NjAzWjAmMSQwIgYDVQQDDBtyZWh5ZHJhdGlvbi1tY3At
dGVzdC1jbGllbnQwWTATBgcqhkjOPQIBBggqhkjOPQMBBwNCAARbE50GqAcOulov
ZQLg9fRjq1LTMvjC3mi/wb+E6QDWGw/nobEuShCwGt8UhaXv6x1iOzlwDt1pBPYi
F9bXka0ho1cwVTATBgNVHSUEDDAKBggrBgEFBQcDAjAdBgNVHQ4EFgQU93T3BJ6O
zaMFcyOIBZ9xKPxNnC0wHwYDVR0jBBgwFoAUg3O8n0A6LZ2rhn2JJHh6zHPoEgYw
CgYIKoZIzj0EAwIDSAAwRQIgfG8Rt+loR9K/khgov0WMDLcMngb4y1aimUp92r+0
l50CIQDuNxk8bkC5XbTX7JP29dAkyrc55Pf728d08jt1TlRQjw==
-----END CERTIFICATE-----
"#;

const TEST_CLIENT_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgNuVBGBwRhxnlxsLV
qdYqw8bDQbReT4w+psRqj/cT1buhRANCAARbE50GqAcOulovZQLg9fRjq1LTMvjC
3mi/wb+E6QDWGw/nobEuShCwGt8UhaXv6x1iOzlwDt1pBPYiF9bXka0h
-----END PRIVATE KEY-----
"#;
