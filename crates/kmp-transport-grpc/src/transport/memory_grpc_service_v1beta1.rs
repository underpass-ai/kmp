use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use kmp_application::{ApplicationError, KernelMemoryApplicationService, TemporalMemoryResult};
use kmp_domain::{
    ContextEventStore, DimensionScopeMode, DimensionSelection, DimensionSelectionMode,
    GraphNeighborhoodReader, KmpBundle, MemoryAboutIndexReader, MemoryDimensionIdentity,
    NodeDetailReader, NodeRelationshipReader, ProjectionWriter, SnapshotStore, TemporalDirection,
};
use kmp_proto::v1beta1::{
    AskRequest, AskResponse, ForwardRequest, ForwardResponse, GotoRequest, GotoResponse,
    IngestRequest, IngestResponse, InspectRequest, InspectResponse, NearRequest, NearResponse,
    ProjectVisualRequest, ProjectVisualResponse, RelateRequest, RelateResponse, RewindRequest,
    RewindResponse, TemporalMoveRequest, TemporalMoveResponse, TemporalNearRequest, TraceRequest,
    TraceResponse, WakeRequest, WakeResponse, kernel_memory_service_server::KernelMemoryService,
};
use opentelemetry::KeyValue;
use prost::Message;
use tonic::{Code, Request, Response, Status};

use crate::transport::proto_mapping_v1beta1::{
    ask_query_from_proto, ask_response_from_result, ingest_command_from_proto,
    ingest_response_from_outcome, inspect_query_from_proto, inspect_response_from_result,
    relate_query_from_proto, relate_response_from_result, temporal_query_from_move_proto,
    temporal_query_from_near_proto, temporal_response_from_result, trace_query_from_proto,
    trace_response_from_result, visual_projection_query_from_proto,
    visual_projection_response_from_result, wake_query_from_proto, wake_response_from_result,
};
use crate::transport::support::map_application_error;
use kmp_proto_mapping::v1beta1::recall_projection::{
    RecallProjectionError, project_ask_response, project_wake_response,
};
use kmp_proto_mapping::v1beta1::{LexicalBridge, abouts_in_bundle};

pub struct MemoryGrpcServiceV1Beta1<G, D, S, E, W> {
    application: Arc<KernelMemoryApplicationService<G, D, S, E, W>>,
    /// The word table `ask` bridges languages with. The served kernel has no
    /// store directory to find one beside, so it is handed in, and none is
    /// the default.
    lexical_bridge: Arc<LexicalBridge>,
}

impl<G, D, S, E, W> MemoryGrpcServiceV1Beta1<G, D, S, E, W> {
    pub fn new(application: Arc<KernelMemoryApplicationService<G, D, S, E, W>>) -> Self {
        Self {
            application,
            lexical_bridge: Arc::new(LexicalBridge::none()),
        }
    }

    pub fn with_lexical_bridge(mut self, lexical_bridge: Arc<LexicalBridge>) -> Self {
        self.lexical_bridge = lexical_bridge;
        self
    }
}

#[tonic::async_trait]
impl<G, D, S, E, W> KernelMemoryService for MemoryGrpcServiceV1Beta1<G, D, S, E, W>
where
    G: GraphNeighborhoodReader
        + MemoryAboutIndexReader
        + NodeRelationshipReader
        + Send
        + Sync
        + 'static,
    D: NodeDetailReader + Send + Sync + 'static,
    S: SnapshotStore + Send + Sync + 'static,
    E: ContextEventStore + Send + Sync + 'static,
    W: ProjectionWriter + Send + Sync + 'static,
{
    #[tracing::instrument(skip(self, request), fields(rpc = "KernelMemory.Ingest"))]
    async fn ingest(
        &self,
        request: Request<IngestRequest>,
    ) -> Result<Response<IngestResponse>, Status> {
        let start = Instant::now();
        let command = ingest_command_from_proto(request.into_inner())
            .map_err(|status| map_proto_error("KernelMemoryService.Ingest", &start, *status))?;
        tracing::info!(
            rpc = "KernelMemoryService.Ingest",
            about = %command.about,
            dry_run = command.dry_run,
            entries = command.memory.entries.len(),
            relations = command.memory.relations.len(),
            evidence = command.memory.evidence.len(),
            "kernel memory grpc request"
        );
        let outcome = self.application.ingest(command).await.map_err(|error| {
            map_application_error_with_log("KernelMemoryService.Ingest", &start, error)
        })?;
        tracing::info!(
            rpc = "KernelMemoryService.Ingest",
            about = %outcome.about,
            accepted_entries = outcome.accepted.entries,
            accepted_relations = outcome.accepted.relations,
            accepted_evidence = outcome.accepted.evidence,
            read_after_write_ready = outcome.read_after_write_ready,
            "kernel memory grpc response"
        );
        record_kmp_grpc_rpc(
            "KernelMemoryService.Ingest",
            "success",
            "none",
            start.elapsed(),
        );

        Ok(Response::new(ingest_response_from_outcome(outcome)))
    }

    #[tracing::instrument(skip(self, request), fields(rpc = "KernelMemory.Wake"))]
    async fn wake(&self, request: Request<WakeRequest>) -> Result<Response<WakeResponse>, Status> {
        let start = Instant::now();
        let request = request.into_inner();
        let projection_request = request.clone();
        let query = wake_query_from_proto(request.clone())
            .map_err(|status| map_proto_error("KernelMemoryService.Wake", &start, *status))?;
        let intent = query.intent.clone();
        let max_entries = query.max_entries;
        let temporal = query.temporal.clone();
        log_dimensioned_request("KernelMemoryService.Wake", &query.about, &query.dimensions);
        let result = self.application.wake(query).await.map_err(|error| {
            map_application_error_with_log("KernelMemoryService.Wake", &start, error)
        })?;
        let selected_abouts = selected_abouts_from_bundle(&result.bundle);
        let response = project_wake_response(
            wake_response_from_result(&intent, max_entries, result, &temporal)
                .map_err(|status| map_proto_error("KernelMemoryService.Wake", &start, *status))?,
            &projection_request,
        )
        .map_err(|error| {
            map_proto_error(
                "KernelMemoryService.Wake",
                &start,
                recall_projection_status(error),
            )
        })?;
        let proof_paths = response
            .proof
            .as_ref()
            .map(|proof| proof.path.len())
            .unwrap_or_default();
        tracing::info!(
            rpc = "KernelMemoryService.Wake",
            selected_abouts = ?selected_abouts,
            proof_paths,
            warnings = response.warnings.len(),
            "kernel memory grpc response"
        );
        record_kmp_grpc_rpc(
            "KernelMemoryService.Wake",
            "success",
            "none",
            start.elapsed(),
        );

        Ok(Response::new(response))
    }

    #[tracing::instrument(skip(self, request), fields(rpc = "KernelMemory.Ask"))]
    async fn ask(&self, request: Request<AskRequest>) -> Result<Response<AskResponse>, Status> {
        let start = Instant::now();
        let request = request.into_inner();
        let projection_request = request.clone();
        let question = request.question.clone();
        let query = ask_query_from_proto(request)
            .map_err(|status| map_proto_error("KernelMemoryService.Ask", &start, *status))?;
        let answer_policy = query.answer_policy;
        let max_entries = query.max_entries;
        let asked_as = query.asked_as.clone();
        let temporal = query.temporal.clone();
        log_dimensioned_request("KernelMemoryService.Ask", &query.about, &query.dimensions);
        let result = self.application.ask(query).await.map_err(|error| {
            map_application_error_with_log("KernelMemoryService.Ask", &start, error)
        })?;
        let selected_abouts = selected_abouts_from_bundle(&result.bundle);
        let response = project_ask_response(
            ask_response_from_result(
                &question,
                asked_as.as_deref(),
                answer_policy,
                max_entries,
                result,
                &self.lexical_bridge,
                &temporal,
            )
            .map_err(|status| map_proto_error("KernelMemoryService.Ask", &start, *status))?,
            &projection_request,
        )
        .map_err(|error| {
            map_proto_error(
                "KernelMemoryService.Ask",
                &start,
                recall_projection_status(error),
            )
        })?;
        tracing::info!(
            rpc = "KernelMemoryService.Ask",
            selected_abouts = ?selected_abouts,
            evidence = response.because.len(),
            answer = %answer_presence_label(&response.answer),
            warnings = response.warnings.len(),
            "kernel memory grpc response"
        );
        record_kmp_grpc_rpc(
            "KernelMemoryService.Ask",
            "success",
            "none",
            start.elapsed(),
        );

        Ok(Response::new(response))
    }

    #[tracing::instrument(skip(self, request), fields(rpc = "KernelMemory.Goto"))]
    async fn goto(&self, request: Request<GotoRequest>) -> Result<Response<GotoResponse>, Status> {
        self.temporal_move(
            temporal_move_request_from_goto(request.into_inner()),
            TemporalDirection::Goto,
        )
        .await
        .map(|response| Response::new(goto_response_from_temporal(response)))
    }

    #[tracing::instrument(skip(self, request), fields(rpc = "KernelMemory.Near"))]
    async fn near(&self, request: Request<NearRequest>) -> Result<Response<NearResponse>, Status> {
        let start = Instant::now();
        let request = temporal_near_request_from_near(request.into_inner());
        let requested_cursor = request.around.clone().unwrap_or_default();
        let query = temporal_query_from_near_proto(request)
            .map_err(|status| map_proto_error("KernelMemoryService.Near", &start, *status))?;
        log_temporal_request("KernelMemoryService.Near", &query.about, &query.dimensions);
        let result = self.application.temporal(query).await.map_err(|error| {
            map_application_error_with_log("KernelMemoryService.Near", &start, error)
        })?;
        let selected_abouts = selected_abouts_from_temporal_result(&result);
        let response =
            temporal_response_from_result(requested_cursor, TemporalDirection::Near, result);
        tracing::info!(
            rpc = "KernelMemoryService.Near",
            selected_abouts = ?selected_abouts,
            entries = response.entries.len(),
            warnings = response.warnings.len(),
            "kernel memory grpc response"
        );
        record_kmp_grpc_rpc(
            "KernelMemoryService.Near",
            "success",
            "none",
            start.elapsed(),
        );

        Ok(Response::new(near_response_from_temporal(response)))
    }

    #[tracing::instrument(skip(self, request), fields(rpc = "KernelMemory.Rewind"))]
    async fn rewind(
        &self,
        request: Request<RewindRequest>,
    ) -> Result<Response<RewindResponse>, Status> {
        self.temporal_move(
            temporal_move_request_from_rewind(request.into_inner()),
            TemporalDirection::Rewind,
        )
        .await
        .map(|response| Response::new(rewind_response_from_temporal(response)))
    }

    #[tracing::instrument(skip(self, request), fields(rpc = "KernelMemory.Forward"))]
    async fn forward(
        &self,
        request: Request<ForwardRequest>,
    ) -> Result<Response<ForwardResponse>, Status> {
        self.temporal_move(
            temporal_move_request_from_forward(request.into_inner()),
            TemporalDirection::Forward,
        )
        .await
        .map(|response| Response::new(forward_response_from_temporal(response)))
    }

    #[tracing::instrument(skip(self, request), fields(rpc = "KernelMemory.ProjectVisual"))]
    async fn project_visual(
        &self,
        request: Request<ProjectVisualRequest>,
    ) -> Result<Response<ProjectVisualResponse>, Status> {
        let start = Instant::now();
        let query = visual_projection_query_from_proto(request.into_inner()).map_err(|status| {
            map_proto_error("KernelMemoryService.ProjectVisual", &start, *status)
        })?;
        tracing::info!(
            rpc = "KernelMemoryService.ProjectVisual",
            about = %query.about,
            axis = ?query.axis,
            level_of_detail = ?query.level_of_detail,
            "kernel visual projection grpc request"
        );
        let result = self
            .application
            .visual_projection(query)
            .await
            .map_err(|error| {
                map_application_error_with_log("KernelMemoryService.ProjectVisual", &start, error)
            })?;
        let response = visual_projection_response_from_result(result);
        tracing::info!(
            rpc = "KernelMemoryService.ProjectVisual",
            entries = response.entries.len(),
            bins = response.bins.len(),
            clusters = response.clusters.len(),
            truncated = response.truncated,
            "kernel visual projection grpc response"
        );
        record_kmp_grpc_rpc(
            "KernelMemoryService.ProjectVisual",
            "success",
            "none",
            start.elapsed(),
        );
        Ok(Response::new(response))
    }

    #[tracing::instrument(skip(self, request), fields(rpc = "KernelMemory.Relate"))]
    async fn relate(
        &self,
        request: Request<RelateRequest>,
    ) -> Result<Response<RelateResponse>, Status> {
        let start = Instant::now();
        let request = request.into_inner();
        let query = relate_query_from_proto(request)
            .map_err(|status| map_proto_error("KernelMemoryService.Relate", &start, *status))?;
        log_dimensioned_request(
            "KernelMemoryService.Relate",
            &query.about,
            &query.dimensions,
        );
        let result = self
            .application
            .relate(query.clone())
            .await
            .map_err(|error| {
                map_application_error_with_log("KernelMemoryService.Relate", &start, error)
            })?;
        let response = relate_response_from_result(result, &query)
            .map_err(|status| map_proto_error("KernelMemoryService.Relate", &start, *status))?;
        tracing::info!(
            rpc = "KernelMemoryService.Relate",
            facts = response.facts.len(),
            coordinate = response.coordinate.len(),
            warnings = response.warnings.len(),
            "kernel memory grpc response"
        );
        record_kmp_grpc_rpc(
            "KernelMemoryService.Relate",
            "success",
            "none",
            start.elapsed(),
        );

        Ok(Response::new(response))
    }

    #[tracing::instrument(skip(self, request), fields(rpc = "KernelMemory.Trace"))]
    async fn trace(
        &self,
        request: Request<TraceRequest>,
    ) -> Result<Response<TraceResponse>, Status> {
        let start = Instant::now();
        let request = request.into_inner();
        tracing::info!(
            rpc = "KernelMemoryService.Trace",
            from = %request.from,
            to = %request.to,
            "kernel memory grpc request"
        );
        let query = trace_query_from_proto(request)
            .map_err(|status| map_proto_error("KernelMemoryService.Trace", &start, *status))?;
        let page = query.page.clone();
        let result = self.application.trace(query).await.map_err(|error| {
            map_application_error_with_log("KernelMemoryService.Trace", &start, error)
        })?;
        let response = trace_response_from_result(result, page);
        tracing::info!(
            rpc = "KernelMemoryService.Trace",
            path = response.trace.len(),
            has_more = response.page.as_ref().is_some_and(|page| page.has_more),
            warnings = response.warnings.len(),
            "kernel memory grpc response"
        );
        record_kmp_grpc_rpc(
            "KernelMemoryService.Trace",
            "success",
            "none",
            start.elapsed(),
        );

        Ok(Response::new(response))
    }

    #[tracing::instrument(skip(self, request), fields(rpc = "KernelMemory.Inspect"))]
    async fn inspect(
        &self,
        request: Request<InspectRequest>,
    ) -> Result<Response<InspectResponse>, Status> {
        let start = Instant::now();
        let query = inspect_query_from_proto(request.into_inner())
            .map_err(|status| map_proto_error("KernelMemoryService.Inspect", &start, *status))?;
        tracing::info!(
            rpc = "KernelMemoryService.Inspect",
            ref_id = %query.ref_id,
            include_details = query.include_details,
            include_incoming = query.include_incoming,
            include_outgoing = query.include_outgoing,
            include_raw = query.include_raw,
            "kernel memory grpc request"
        );
        let result = self.application.inspect(query).await.map_err(|error| {
            map_application_error_with_log("KernelMemoryService.Inspect", &start, error)
        })?;
        let response = inspect_response_from_result(result);
        tracing::info!(
            rpc = "KernelMemoryService.Inspect",
            incoming = response
                .links
                .as_ref()
                .map(|links| links.incoming.len())
                .unwrap_or_default(),
            outgoing = response
                .links
                .as_ref()
                .map(|links| links.outgoing.len())
                .unwrap_or_default(),
            evidence = response.evidence.len(),
            raw_refs = response.raw.len(),
            raw_coordinates = response
                .raw
                .iter()
                .map(|raw| raw.coordinates.len())
                .sum::<usize>(),
            warnings = response.warnings.len(),
            "kernel memory grpc response"
        );
        record_kmp_grpc_rpc(
            "KernelMemoryService.Inspect",
            "success",
            "none",
            start.elapsed(),
        );

        Ok(Response::new(response))
    }
}

impl<G, D, S, E, W> MemoryGrpcServiceV1Beta1<G, D, S, E, W>
where
    G: GraphNeighborhoodReader
        + MemoryAboutIndexReader
        + NodeRelationshipReader
        + Send
        + Sync
        + 'static,
    D: NodeDetailReader + Send + Sync + 'static,
    S: SnapshotStore + Send + Sync + 'static,
    E: ContextEventStore + Send + Sync + 'static,
    W: ProjectionWriter + Send + Sync + 'static,
{
    async fn temporal_move(
        &self,
        request: TemporalMoveRequest,
        direction: TemporalDirection,
    ) -> Result<TemporalMoveResponse, Status> {
        let start = Instant::now();
        let requested_cursor = request.cursor.clone().unwrap_or_default();
        let rpc = temporal_rpc_label(direction);
        let query = temporal_query_from_move_proto(request, direction)
            .map_err(|status| map_proto_error(rpc, &start, *status))?;
        log_temporal_request(rpc, &query.about, &query.dimensions);
        let result = self
            .application
            .temporal(query)
            .await
            .map_err(|error| map_application_error_with_log(rpc, &start, error))?;
        let selected_abouts = selected_abouts_from_temporal_result(&result);
        let response = temporal_response_from_result(requested_cursor, direction, result);
        tracing::info!(
            rpc,
            selected_abouts = ?selected_abouts,
            entries = response.entries.len(),
            warnings = response.warnings.len(),
            "kernel memory grpc response"
        );
        record_kmp_grpc_rpc(rpc, "success", "none", start.elapsed());

        Ok(response)
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

fn log_dimensioned_request(rpc: &'static str, about: &str, dimensions: &DimensionSelection) {
    tracing::info!(
        rpc,
        about,
        dimension_mode = %dimension_mode_label(dimensions.mode()),
        dimension_scope = %dimension_scope_label(dimensions.scope_mode()),
        dimensions = ?dimensions.dimensions().iter().cloned().collect::<Vec<_>>(),
        abouts = ?dimensions.abouts().iter().cloned().collect::<Vec<_>>(),
        scope_ids = ?dimensions.scope_ids().iter().cloned().collect::<Vec<_>>(),
        "kernel memory grpc request"
    );
}

fn log_temporal_request(rpc: &'static str, about: &str, dimensions: &DimensionSelection) {
    log_dimensioned_request(rpc, about, dimensions);
}

fn selected_abouts_from_bundle(bundle: &KmpBundle) -> Vec<String> {
    selected_abouts_from_bundle_and_scope_ids(bundle, std::iter::empty())
}

fn selected_abouts_from_temporal_result(result: &TemporalMemoryResult) -> Vec<String> {
    selected_abouts_from_bundle_and_scope_ids(
        &result.source_bundle,
        result
            .traversal
            .entries()
            .iter()
            .flat_map(|entry| entry.coordinates().iter())
            .map(|coordinate| coordinate.scope_id()),
    )
}

fn selected_abouts_from_bundle_and_scope_ids<'a>(
    bundle: &KmpBundle,
    scope_ids: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let mut selected = abouts_in_bundle(bundle);
    let mut seen = selected.iter().cloned().collect::<BTreeSet<_>>();

    for scope_id in scope_ids {
        let Some(identity) = MemoryDimensionIdentity::parse(scope_id) else {
            continue;
        };
        let about = identity.about().to_string();
        if seen.insert(about.clone()) {
            selected.push(about);
        }
    }

    selected
}

fn map_proto_error(rpc: &'static str, start: &Instant, status: Status) -> Status {
    log_grpc_error(rpc, &status);
    record_kmp_grpc_rpc(
        rpc,
        "error",
        status_code_label(status.code()),
        start.elapsed(),
    );
    status
}

fn recall_projection_status(error: RecallProjectionError) -> Status {
    let message = error.to_string();
    match error.cursor_detail() {
        Some(detail) => Status::with_details(
            Code::InvalidArgument,
            message,
            detail.encode_to_vec().into(),
        ),
        None => Status::invalid_argument(message),
    }
}

fn map_application_error_with_log(
    rpc: &'static str,
    start: &Instant,
    error: ApplicationError,
) -> Status {
    let status = map_application_error(error);
    log_grpc_error(rpc, &status);
    record_kmp_grpc_rpc(
        rpc,
        "error",
        status_code_label(status.code()),
        start.elapsed(),
    );
    status
}

fn log_grpc_error(rpc: &'static str, status: &Status) {
    tracing::warn!(
        rpc,
        code = ?status.code(),
        message = %status.message(),
        "kernel memory grpc error"
    );
}

fn record_kmp_grpc_rpc(
    rpc: &'static str,
    status: &'static str,
    code: &'static str,
    duration: Duration,
) {
    let attrs = [
        KeyValue::new("rpc", rpc),
        KeyValue::new("status", status),
        KeyValue::new("code", code),
    ];
    let meter = opentelemetry::global::meter("kmp");
    meter
        .u64_counter("rehydration.kmp.grpc.calls")
        .build()
        .add(1, &attrs);
    meter
        .f64_histogram("rehydration.kmp.grpc.duration")
        .build()
        .record(duration.as_secs_f64(), &attrs);
}

fn status_code_label(code: tonic::Code) -> &'static str {
    match code {
        tonic::Code::Ok => "ok",
        tonic::Code::Cancelled => "cancelled",
        tonic::Code::Unknown => "unknown",
        tonic::Code::InvalidArgument => "invalid_argument",
        tonic::Code::DeadlineExceeded => "deadline_exceeded",
        tonic::Code::NotFound => "not_found",
        tonic::Code::AlreadyExists => "already_exists",
        tonic::Code::PermissionDenied => "permission_denied",
        tonic::Code::ResourceExhausted => "resource_exhausted",
        tonic::Code::FailedPrecondition => "failed_precondition",
        tonic::Code::Aborted => "aborted",
        tonic::Code::OutOfRange => "out_of_range",
        tonic::Code::Unimplemented => "unimplemented",
        tonic::Code::Internal => "internal",
        tonic::Code::Unavailable => "unavailable",
        tonic::Code::DataLoss => "data_loss",
        tonic::Code::Unauthenticated => "unauthenticated",
    }
}

fn temporal_rpc_label(direction: TemporalDirection) -> &'static str {
    match direction {
        TemporalDirection::Goto => "KernelMemoryService.Goto",
        TemporalDirection::Near => "KernelMemoryService.Near",
        TemporalDirection::Rewind => "KernelMemoryService.Rewind",
        TemporalDirection::Forward => "KernelMemoryService.Forward",
    }
}

fn dimension_mode_label(value: DimensionSelectionMode) -> &'static str {
    match value {
        DimensionSelectionMode::All => "all",
        DimensionSelectionMode::Only => "only",
        DimensionSelectionMode::Except => "except",
    }
}

fn dimension_scope_label(value: DimensionScopeMode) -> &'static str {
    match value {
        DimensionScopeMode::CurrentAbout => "current_about",
        DimensionScopeMode::Abouts => "abouts",
        DimensionScopeMode::AllAbouts => "all_abouts",
    }
}

fn answer_presence_label(answer: &str) -> &'static str {
    if answer.trim().is_empty() {
        "empty"
    } else if answer == "UNKNOWN" {
        "unknown"
    } else {
        "deterministic"
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use kmp_domain::{BundleMetadata, BundleNode, CaseId, KmpBundle, Role};

    use super::{selected_abouts_from_bundle, selected_abouts_from_bundle_and_scope_ids};

    #[test]
    fn selected_abouts_preserve_resolved_root_order() {
        let bundle = KmpBundle::new(
            CaseId::new("question:current").expect("valid case id"),
            Role::new("memory").expect("valid role"),
            node("question:current", "memory_anchor"),
            vec![
                node("question:other", "memory_anchor"),
                node("claim:current", "claim"),
            ],
            Vec::new(),
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("bundle should be valid");

        assert_eq!(
            selected_abouts_from_bundle(&bundle),
            vec!["question:current", "question:other"]
        );
    }

    #[test]
    fn selected_abouts_include_temporal_scope_about_ids() {
        let bundle = KmpBundle::new(
            CaseId::new("question:current").expect("valid case id"),
            Role::new("memory").expect("valid role"),
            node("question:current", "memory_anchor"),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("bundle should be valid");

        assert_eq!(
            selected_abouts_from_bundle_and_scope_ids(
                &bundle,
                [
                    "about:question:other:dimension:timeline",
                    "about:question:current:dimension:timeline",
                ],
            ),
            vec!["question:current", "question:other"]
        );
    }

    fn node(node_id: &str, node_kind: &str) -> BundleNode {
        BundleNode::new(
            node_id,
            node_kind,
            node_id,
            node_id,
            "ACTIVE",
            Vec::new(),
            BTreeMap::new(),
        )
    }
}
