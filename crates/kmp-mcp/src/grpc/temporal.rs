use kmp_proto::v1beta1::{
    ForwardRequest, ForwardResponse, GotoRequest, GotoResponse, NearRequest, NearResponse,
    RewindRequest, RewindResponse, TemporalMoveRequest, TemporalMoveResponse, TemporalNearRequest,
};

pub(super) fn method_name(direction: &str) -> &'static str {
    match direction {
        "goto" => "Goto",
        "rewind" => "Rewind",
        "forward" => "Forward",
        _ => "TemporalMove",
    }
}

pub(super) fn goto_request_from_temporal(request: TemporalMoveRequest) -> GotoRequest {
    GotoRequest {
        about: request.about,
        cursor: request.cursor,
        dimensions: request.dimensions,
        window: request.window,
        limit: request.limit,
        include: request.include,
        budget: request.budget,
    }
}

pub(super) fn rewind_request_from_temporal(request: TemporalMoveRequest) -> RewindRequest {
    RewindRequest {
        about: request.about,
        cursor: request.cursor,
        dimensions: request.dimensions,
        window: request.window,
        limit: request.limit,
        include: request.include,
        budget: request.budget,
    }
}

pub(super) fn forward_request_from_temporal(request: TemporalMoveRequest) -> ForwardRequest {
    ForwardRequest {
        about: request.about,
        cursor: request.cursor,
        dimensions: request.dimensions,
        window: request.window,
        limit: request.limit,
        include: request.include,
        budget: request.budget,
    }
}

pub(super) fn near_request_from_temporal(request: TemporalNearRequest) -> NearRequest {
    NearRequest {
        about: request.about,
        around: request.around,
        dimensions: request.dimensions,
        window: request.window,
        limit: request.limit,
        include: request.include,
        budget: request.budget,
    }
}

pub(super) fn temporal_response_from_goto(response: GotoResponse) -> TemporalMoveResponse {
    TemporalMoveResponse {
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

pub(super) fn temporal_response_from_near(response: NearResponse) -> TemporalMoveResponse {
    TemporalMoveResponse {
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

pub(super) fn temporal_response_from_rewind(response: RewindResponse) -> TemporalMoveResponse {
    TemporalMoveResponse {
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

pub(super) fn temporal_response_from_forward(response: ForwardResponse) -> TemporalMoveResponse {
    TemporalMoveResponse {
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
