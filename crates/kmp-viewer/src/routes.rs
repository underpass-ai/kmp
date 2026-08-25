//! Request handlers: each `/api/*` route is one read of the kernel's memory
//! facade, translated to the viewer's wire format. No route writes — the
//! viewer observes the memory, the agent operates it.

use kmp_application::{
    ApplicationError, InspectMemoryQuery, TemporalIncludeOptions, TemporalMemoryQuery,
    TraceMemoryQuery, TracePageRequest, WakeMemoryQuery,
};
use kmp_domain::{
    ContextEventStore, DimensionSelection, DomainError, GraphNeighborhoodReader,
    MemoryAboutIndexReader, NodeDetailReader, NodeRelationshipReader, PortError, ProjectionWriter,
    ResolutionTier, SnapshotStore, TemporalCursor, TemporalDirection, TemporalWindow,
};

use crate::MemoryViewerServer;
use crate::http::{HttpRequest, HttpResponse};
use crate::views;

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

const DEFAULT_GRAPH_DEPTH: u32 = 2;
const MAX_GRAPH_DEPTH: u32 = 6;
const DEFAULT_TOKEN_BUDGET: u32 = 16_384;
const MAX_TOKEN_BUDGET: u32 = 262_144;
const DEFAULT_WINDOW_ENTRIES: usize = 8;
const MAX_WINDOW_ENTRIES: usize = 256;
const MAX_BATCH_IDS: usize = 64;

pub(crate) const INDEX_HTML: &str = include_str!("../ui/index.html");
pub(crate) const VIEWER_CSS: &str = include_str!("../ui/viewer.css");
pub(crate) const VIEWER_JS: &str = include_str!("../ui/viewer.js");
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
        if request.method != "GET" && !head_only {
            return HttpResponse::error(405, "the viewer is read-only; only GET is served");
        }
        let response = self.answer(request).await;
        if head_only {
            response.without_body()
        } else {
            response
        }
    }

    async fn answer(&self, request: &HttpRequest) -> HttpResponse {
        match request.path.as_str() {
            "/" | "/index.html" => HttpResponse::html(INDEX_HTML),
            "/assets/viewer.css" => HttpResponse::css(VIEWER_CSS),
            "/assets/viewer.js" => HttpResponse::javascript(VIEWER_JS),
            "/assets/pixi.min.js" => HttpResponse::javascript(PIXI_JS),
            "/assets/pixi-unsafe-eval.min.js" => HttpResponse::javascript(PIXI_UNSAFE_EVAL_JS),
            "/api/info" => self.info(),
            "/api/abouts" => self.abouts().await,
            "/api/graph" => self.graph(request).await,
            "/api/node" => self.node(request).await,
            "/api/nodes" => self.nodes(request).await,
            "/api/timeline" => self.timeline(request).await,
            "/api/trace" => self.trace(request).await,
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
        let Some(id) = request.param("id") else {
            return HttpResponse::error(400, "missing required parameter `id`");
        };
        let query = InspectMemoryQuery {
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
        let dimensions = match dimension_selection(request) {
            Ok(dimensions) => dimensions,
            Err(response) => return response,
        };
        let query = TemporalMemoryQuery {
            about: about.to_string(),
            direction,
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

    async fn trace(&self, request: &HttpRequest) -> HttpResponse {
        let (Some(from), Some(to)) = (request.param("from"), request.param("to")) else {
            return HttpResponse::error(400, "missing required parameters `from` and `to`");
        };
        let query = TraceMemoryQuery {
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

/// A number a caller sent, or a refusal naming what was wrong with it.
///
/// Absent means "use the default"; present and unparseable means the caller
/// believes they asked for something. Answering 200 to `depth=abc` as though
/// it read `depth=2` is the one thing every other refusal in this codebase is
/// written not to do — `scope` and `dims` next door already say so by name.
/// Out of range is still clamped: a bound is a policy, not a mistake.
fn numeric_param<T>(request: &HttpRequest, key: &str, default: T) -> Result<T, HttpResponse>
where
    T: std::str::FromStr,
{
    match request.param(key) {
        None => Ok(default),
        Some(value) => value.parse::<T>().map_err(|_| {
            HttpResponse::error(
                400,
                &format!("parameter `{key}` is not a number: `{value}`"),
            )
        }),
    }
}

fn depth_param(request: &HttpRequest) -> Result<u32, HttpResponse> {
    Ok(numeric_param(request, "depth", DEFAULT_GRAPH_DEPTH)?.clamp(1, MAX_GRAPH_DEPTH))
}

fn budget_param(request: &HttpRequest) -> Result<u32, HttpResponse> {
    Ok(numeric_param(request, "budget", DEFAULT_TOKEN_BUDGET)?.clamp(256, MAX_TOKEN_BUDGET))
}

fn window_param(request: &HttpRequest, key: &str) -> Result<usize, HttpResponse> {
    Ok(numeric_param(request, key, DEFAULT_WINDOW_ENTRIES)?.min(MAX_WINDOW_ENTRIES))
}

/// `scope=all` widens recall to every about the kernel indexes — the global
/// graph. `dims=a,b` restricts to those dimension kinds. Defaults mirror
/// `kmp_wake`: the current about, all dimensions.
fn dimension_selection(request: &HttpRequest) -> Result<DimensionSelection, HttpResponse> {
    let selection = match request.param("dims") {
        Some(dims) => {
            let kinds: Vec<String> = dims
                .split(',')
                .map(str::trim)
                .filter(|kind| !kind.is_empty())
                .map(ToString::to_string)
                .collect();
            if kinds.is_empty() {
                return Err(HttpResponse::error(400, "parameter `dims` holds no kinds"));
            }
            DimensionSelection::only(kinds)
        }
        None => DimensionSelection::all(),
    };
    Ok(match request.param("scope") {
        Some("all") => selection.with_all_about_scope(),
        Some("current") | None => selection,
        Some(other) => {
            return Err(HttpResponse::error(
                400,
                &format!("unknown scope `{other}`; expected `current` or `all`"),
            ));
        }
    })
}

fn tier_param(request: &HttpRequest) -> Result<Option<ResolutionTier>, HttpResponse> {
    match request.param("tier") {
        None => Ok(None),
        Some("summary") => Ok(Some(ResolutionTier::L0Summary)),
        Some("spine") => Ok(Some(ResolutionTier::L1CausalSpine)),
        Some("evidence") => Ok(Some(ResolutionTier::L2EvidencePack)),
        Some(other) => Err(HttpResponse::error(
            400,
            &format!("unknown tier `{other}`; expected `summary`, `spine` or `evidence`"),
        )),
    }
}

/// Exactly one of `ref`, `time`, `seq` — the same contract the MCP temporal
/// tools enforce.
fn cursor_param(request: &HttpRequest) -> Result<TemporalCursor, HttpResponse> {
    let cursor = match (
        request.param("ref"),
        request.param("time"),
        request.param("seq"),
    ) {
        (Some(ref_id), None, None) => TemporalCursor::ref_id(ref_id),
        (None, Some(time), None) => TemporalCursor::time(time),
        (None, None, Some(seq)) => match seq.parse::<u32>() {
            Ok(seq) => TemporalCursor::sequence(seq),
            Err(_) => {
                return Err(HttpResponse::error(
                    400,
                    "parameter `seq` must be a positive integer",
                ));
            }
        },
        _ => {
            return Err(HttpResponse::error(
                400,
                "the temporal cursor requires exactly one of `ref`, `time`, or `seq`",
            ));
        }
    };
    cursor.map_err(|error| HttpResponse::error(400, &error.to_string()))
}

fn direction_param(request: &HttpRequest) -> Result<TemporalDirection, HttpResponse> {
    match request.param("direction") {
        None | Some("near") => Ok(TemporalDirection::Near),
        Some("goto") => Ok(TemporalDirection::Goto),
        Some("rewind") => Ok(TemporalDirection::Rewind),
        Some("forward") => Ok(TemporalDirection::Forward),
        Some(other) => Err(HttpResponse::error(
            400,
            &format!("unknown direction `{other}`; expected `goto`, `near`, `rewind` or `forward`"),
        )),
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
        ApplicationError::Ports(PortError::Unavailable(_)) => 503,
        ApplicationError::Ports(PortError::Conflict(_)) => 409,
        ApplicationError::Ports(PortError::InvalidState(_)) => 500,
    };
    HttpResponse::error(status, &message)
}
