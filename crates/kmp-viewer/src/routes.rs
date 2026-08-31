//! Request handlers: each `/api/*` route is one read of the kernel's memory
//! facade, translated to the viewer's wire format. No route writes — the
//! viewer observes the memory, the agent operates it.

use kmp_application::{
    ApplicationError, InspectMemoryQuery, MAX_VISUAL_BINS, MAX_VISUAL_PAGE_ENTRIES,
    ObservabilityQuery, TemporalIncludeOptions, TemporalMemoryQuery, TraceMemoryQuery,
    TracePageRequest, VisualLevelOfDetail, VisualProjectionQuery, WakeMemoryQuery,
};
use kmp_domain::{
    ContextEventStore, DomainError, GraphNeighborhoodReader, MemoryAboutIndexReader,
    NodeDetailReader, NodeRelationshipReader, PortError, ProjectionWriter, SnapshotStore,
    TemporalWindow,
};

use crate::http::{HttpRequest, HttpResponse};
use crate::query_params::{
    axis_param, budget_param, cursor_param, depth_param, dimension_selection, direction_param,
    numeric_param, tier_param, window_param,
};
use crate::{MemoryViewerServer, view, views};

/// Unwraps a parameter, or returns its refusal from the enclosing handler.
///
/// The handlers answer with an `HttpResponse` rather than a `Result`, so `?`
/// is not available to them.
macro_rules! param_or_refuse {
    ($expr:expr) => {
        match $expr {
            Ok(value) => value,
            Err(response) => return response,
        }
    };
}

/// Who the viewer says it is in the kernel's own accounting: every recall it
/// triggers is attributed and explainable in telemetry.
const VIEWER_ROLE: &str = "viewer";

const MAX_BATCH_IDS: usize = 64;
const DEFAULT_VISUAL_FROM: &str = "1900-01-01T00:00:00Z";
const DEFAULT_VISUAL_TO: &str = "2100-01-01T00:00:00Z";

pub(crate) const INDEX_HTML: &str = include_str!("../ui/index.html");
pub(crate) const LOOM_CSS: &str = include_str!("../ui/loom.css");
/// The composition root: wires the loom's modules and starts it.
pub(crate) const LOOM_JS: &str = include_str!("../ui/loom.js");
/// The loom's pure algorithmic half — clocks, lanes, bins, prisms, axes —
/// kept free of DOM and renderer so it can be reasoned about alone.
pub(crate) const LOOM_CORE_JS: &str = include_str!("../ui/loom-core.js");
/// The script-tag modules of the browser application, in the exact order
/// `index.html` loads them and `mcp_app` inlines them: state, the backend
/// port, the use cases, then the adapters, with the composition root last.
pub(crate) const LOOM_MODULES: [(&str, &str); 9] = [
    ("loom-state.js", include_str!("../ui/loom-state.js")),
    ("loom-api.js", include_str!("../ui/loom-api.js")),
    ("loom-panels.js", include_str!("../ui/loom-panels.js")),
    ("loom-viewport.js", include_str!("../ui/loom-viewport.js")),
    ("loom-data.js", include_str!("../ui/loom-data.js")),
    ("loom-selection.js", include_str!("../ui/loom-selection.js")),
    ("loom-sync.js", include_str!("../ui/loom-sync.js")),
    ("loom-scene.js", include_str!("../ui/loom-scene.js")),
    ("loom-gestures.js", include_str!("../ui/loom-gestures.js")),
];
/// Vendored render engine, pinned and hash-verified in `ui/vendor/VENDOR.md`.
pub(crate) const PIXI_JS: &str = include_str!("../ui/vendor/pixi.min.js");
/// Pixi's no-eval shader path, required because the viewer's CSP forbids
/// `unsafe-eval`; same provenance record as the engine itself.
pub(crate) const PIXI_UNSAFE_EVAL_JS: &str = include_str!("../ui/vendor/pixi-unsafe-eval.min.js");

impl<G, D, S, E, W> MemoryViewerServer<G, D, S, E, W>
where
    G: GraphNeighborhoodReader + MemoryAboutIndexReader + NodeRelationshipReader + Send + Sync,
    D: NodeDetailReader + Send + Sync,
    S: SnapshotStore + Send + Sync,
    E: ContextEventStore + Send + Sync,
    W: ProjectionWriter + Send + Sync,
{
    pub(crate) async fn route(&self, request: &HttpRequest) -> HttpResponse {
        // HEAD is GET without the body (RFC 9110 §9.3.2): a server that
        // serves GET has to serve it, and a health check or link checker
        // pointed here reported the viewer as down.
        let head_only = request.method == "HEAD";
        // Memory is read-only here and stays that way. The one exception is
        // the view aggregate — where the human is looking — which the browser
        // reports back so an agent can see it and rebase instead of yanking
        // the loom out from under them. A camera position is not memory, and
        // POST is the honest method for changing one.
        let view_control = matches!(
            request.path.as_str(),
            "/api/view/report" | "/api/view/undo" | "/api/view/open"
        );
        if view_control {
            // A GET has to be safe. Letting one through here would let any
            // page you happen to be visiting move this loom with an <img>
            // tag pointed at loopback.
            if request.method != "POST" {
                return HttpResponse::error(
                    405,
                    "the view state changes by POST; a GET must be safe to make",
                );
            }
            return self.answer(request).await;
        }
        if request.method != "GET" && !head_only {
            return HttpResponse::error(
                405,
                "the viewer never writes memory; only GET is served (POST reaches the view state alone)",
            );
        }
        let response = self.answer(request).await;
        if head_only {
            response.without_body()
        } else {
            response
        }
    }

    async fn answer(&self, request: &HttpRequest) -> HttpResponse {
        if let Some(name) = request.path.strip_prefix("/assets/")
            && let Some((_, source)) = LOOM_MODULES.iter().find(|(module, _)| *module == name)
        {
            return HttpResponse::javascript(source);
        }
        match request.path.as_str() {
            "/" | "/index.html" => HttpResponse::html(INDEX_HTML),
            "/assets/loom.css" => HttpResponse::css(LOOM_CSS),
            "/assets/loom.js" => HttpResponse::javascript(LOOM_JS),
            "/assets/loom-core.js" => HttpResponse::javascript(LOOM_CORE_JS),
            "/assets/pixi.min.js" => HttpResponse::javascript(PIXI_JS),
            "/assets/pixi-unsafe-eval.min.js" => HttpResponse::javascript(PIXI_UNSAFE_EVAL_JS),
            "/api/info" => self.info(),
            "/api/abouts" => self.abouts().await,
            "/api/graph" => self.graph(request).await,
            "/api/node" => self.node(request).await,
            "/api/nodes" => self.nodes(request).await,
            "/api/timeline" => self.timeline(request).await,
            "/api/projection" => self.visual_projection(request).await,
            "/api/observability" => self.observability(request).await,
            "/api/trace" => self.trace(request).await,
            "/api/view" => view::adapters::view_get(request).await,
            "/api/view/open" => view::adapters::view_open(request),
            "/api/view/report" => view::adapters::view_report(request),
            "/api/view/undo" => view::adapters::view_undo(request),
            _ => HttpResponse::error(404, "unknown path"),
        }
    }

    fn info(&self) -> HttpResponse {
        HttpResponse::json(&views::InfoView {
            kernel_version: env!("CARGO_PKG_VERSION").to_string(),
            data_dir: self.data_dir.clone(),
        })
    }

    async fn abouts(&self) -> HttpResponse {
        match self.service.list_abouts().await {
            Ok(abouts) => HttpResponse::json(&views::AboutsView { abouts }),
            Err(error) => application_error_response(&error),
        }
    }

    async fn graph(&self, request: &HttpRequest) -> HttpResponse {
        let Some(about) = request.param("about") else {
            return HttpResponse::error(400, "missing required parameter `about`");
        };
        let dimensions = match dimension_selection(request) {
            Ok(dimensions) => dimensions,
            Err(response) => return response,
        };
        let max_tier = match tier_param(request) {
            Ok(max_tier) => max_tier,
            Err(response) => return response,
        };
        let query = WakeMemoryQuery {
            about: about.to_string(),
            role: VIEWER_ROLE.to_string(),
            intent: "render the memory graph for a human reader".to_string(),
            dimensions,
            token_budget: param_or_refuse!(budget_param(request)),
            depth: param_or_refuse!(depth_param(request)),
            max_tier,
            max_entries: None,
        };
        match self.service.wake(query).await {
            Ok(result) => HttpResponse::json(&views::graph_view(about, &result)),
            Err(error) => application_error_response(&error),
        }
    }

    async fn node(&self, request: &HttpRequest) -> HttpResponse {
        let Some(about) = request.param("about") else {
            return HttpResponse::error(400, "missing required parameter `about`");
        };
        let Some(id) = request.param("id") else {
            return HttpResponse::error(400, "missing required parameter `id`");
        };
        let query = InspectMemoryQuery {
            about: about.to_string(),
            ref_id: id.to_string(),
            include_details: true,
            include_incoming: true,
            include_outgoing: true,
            include_raw: request.param("raw") == Some("1"),
        };
        match self.service.inspect(query).await {
            Ok(result) => HttpResponse::json(&views::node_inspect_view(&result)),
            Err(error) => application_error_response(&error),
        }
    }

    /// Summaries for a batch of ids, so the UI can label freshly expanded
    /// neighbors in one request. Unknown ids are reported, not fatal.
    async fn nodes(&self, request: &HttpRequest) -> HttpResponse {
        let Some(about) = request.param("about") else {
            return HttpResponse::error(400, "missing required parameter `about`");
        };
        let Some(ids) = request.param("ids") else {
            return HttpResponse::error(400, "missing required parameter `ids`");
        };
        let ids: Vec<&str> = ids
            .split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .collect();
        if ids.is_empty() {
            return HttpResponse::error(400, "parameter `ids` holds no ids");
        }
        if ids.len() > MAX_BATCH_IDS {
            return HttpResponse::error(
                400,
                &format!("parameter `ids` holds more than {MAX_BATCH_IDS} ids"),
            );
        }
        let mut nodes = Vec::with_capacity(ids.len());
        let mut missing = Vec::new();
        for id in ids {
            let query = InspectMemoryQuery {
                about: about.to_string(),
                ref_id: id.to_string(),
                include_details: false,
                include_incoming: false,
                include_outgoing: false,
                include_raw: false,
            };
            match self.service.inspect(query).await {
                Ok(result) => nodes.push(views::NodeView::from_graph_node(&result.detail.node)),
                Err(ApplicationError::NotFound(_)) => missing.push(id.to_string()),
                Err(error) => return application_error_response(&error),
            }
        }
        HttpResponse::json(&views::NodeBatchView { nodes, missing })
    }

    async fn timeline(&self, request: &HttpRequest) -> HttpResponse {
        let Some(about) = request.param("about") else {
            return HttpResponse::error(400, "missing required parameter `about`");
        };
        let cursor = match cursor_param(request) {
            Ok(cursor) => cursor,
            Err(response) => return response,
        };
        let direction = match direction_param(request) {
            Ok(direction) => direction,
            Err(response) => return response,
        };
        let axis = match axis_param(request) {
            Ok(axis) => axis,
            Err(response) => return response,
        };
        let dimensions = match dimension_selection(request) {
            Ok(dimensions) => dimensions,
            Err(response) => return response,
        };
        let query = TemporalMemoryQuery {
            about: about.to_string(),
            direction,
            axis,
            cursor,
            dimensions,
            window: TemporalWindow::new(
                param_or_refuse!(window_param(request, "before")),
                param_or_refuse!(window_param(request, "after")),
            ),
            limit_entries: request
                .param("limit")
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|limit| *limit > 0),
            include: TemporalIncludeOptions::default(),
            token_budget: param_or_refuse!(budget_param(request)),
            depth: param_or_refuse!(depth_param(request)),
            max_tier: None,
        };
        match self.service.temporal(query).await {
            Ok(result) => HttpResponse::json(&views::timeline_view(about, &result)),
            Err(error) => application_error_response(&error),
        }
    }

    async fn visual_projection(&self, request: &HttpRequest) -> HttpResponse {
        let Some(about) = request.param("about") else {
            return HttpResponse::error(400, "missing required parameter `about`");
        };
        let axis = match axis_param(request) {
            Ok(axis) => axis,
            Err(response) => return response,
        };
        let dimensions = match dimension_selection(request) {
            Ok(dimensions) => dimensions,
            Err(response) => return response,
        };
        let level_of_detail = match request.param("lod") {
            None | Some("atlas") => VisualLevelOfDetail::Atlas,
            Some("episode") => VisualLevelOfDetail::Episode,
            Some("moment") => VisualLevelOfDetail::Moment,
            Some(value) => {
                return HttpResponse::error(
                    400,
                    &format!("parameter `lod` must be atlas, episode, or moment; got `{value}`"),
                );
            }
        };
        let query = VisualProjectionQuery {
            about: about.to_string(),
            from: request
                .param("from")
                .unwrap_or(DEFAULT_VISUAL_FROM)
                .to_string(),
            to: request.param("to").unwrap_or(DEFAULT_VISUAL_TO).to_string(),
            axis,
            dimensions,
            level_of_detail,
            bin_count: param_or_refuse!(numeric_param(request, "bins", 64usize))
                .clamp(1, MAX_VISUAL_BINS),
            page_entries: param_or_refuse!(numeric_param(request, "limit", 512usize))
                .clamp(1, MAX_VISUAL_PAGE_ENTRIES),
            cursor: request.param("cursor").map(ToString::to_string),
            depth: param_or_refuse!(depth_param(request)),
        };
        match self.service.visual_projection(query).await {
            Ok(result) => HttpResponse::json(&views::visual_projection_view(result)),
            Err(error) => application_error_response(&error),
        }
    }

    async fn observability(&self, request: &HttpRequest) -> HttpResponse {
        let Some(port) = self.observability.as_ref() else {
            let reason = self
                .observability_unavailable_reason
                .as_deref()
                .unwrap_or("quality telemetry is unavailable in this viewer process");
            return HttpResponse::error(503, reason);
        };
        let from_millis = param_or_refuse!(numeric_param(request, "from_ms", 0u64));
        let to_millis = param_or_refuse!(numeric_param(request, "to_ms", u64::MAX));
        if to_millis < from_millis {
            return HttpResponse::error(400, "observability range requires to_ms >= from_ms");
        }
        let query = ObservabilityQuery {
            about: request.param("about").map(ToString::to_string),
            from_millis,
            to_millis,
            series: request
                .param("series")
                .unwrap_or("")
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect(),
            max_points: param_or_refuse!(numeric_param(request, "limit", 2048usize))
                .clamp(1, 16_384),
        };
        match port.query(query).await {
            Ok(result) => HttpResponse::json(&result),
            Err(error) => HttpResponse::error(503, &format!("observability query failed: {error}")),
        }
    }

    async fn trace(&self, request: &HttpRequest) -> HttpResponse {
        let Some(about) = request.param("about") else {
            return HttpResponse::error(400, "missing required parameter `about`");
        };
        let (Some(from), Some(to)) = (request.param("from"), request.param("to")) else {
            return HttpResponse::error(400, "missing required parameters `from` and `to`");
        };
        let query = TraceMemoryQuery {
            about: about.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            role: VIEWER_ROLE.to_string(),
            token_budget: param_or_refuse!(budget_param(request)),
            page: TracePageRequest {
                entries: request
                    .param("entries")
                    .and_then(|value| value.parse::<usize>().ok()),
                cursor: request
                    .param("cursor")
                    .and_then(|value| value.parse::<usize>().ok()),
            },
        };
        match self.service.trace(query).await {
            Ok(result) => HttpResponse::json(&views::trace_view(from, to, &result)),
            Err(error) => application_error_response(&error),
        }
    }
}

/// The kernel's error vocabulary, translated to status codes without leaking
/// anything the message itself does not already say.
fn application_error_response(error: &ApplicationError) -> HttpResponse {
    let message = error.to_string();
    let status = match error {
        ApplicationError::NotFound(_) => 404,
        ApplicationError::Validation(_) => 400,
        ApplicationError::Domain(DomainError::EmptyValue(_)) => 400,
        ApplicationError::Domain(DomainError::InvalidState(_)) => 400,
        ApplicationError::RetryableConflict(_) => 409,
        ApplicationError::Ports(PortError::Unavailable(_)) => 503,
        ApplicationError::Ports(PortError::Conflict(_)) => 409,
        ApplicationError::Ports(PortError::InvalidState(_)) => 500,
    };
    HttpResponse::error(status, &message)
}
