//! A local, read-only web viewer over KMP memory: the graph, its notes, the
//! timeline and causal traces, rendered for a human the way `kmp_wake`,
//! `kmp_inspect`, `kmp_time` (near) and `kmp_trace` render them for an
//! agent — same facade, same semantics, no parallel read model.
//!
//! Served in-process over an already-open kernel on purpose: the viewer sees
//! every write the moment it projects and shares the exact same facade. There
//! is no second database connection, daemon, sync protocol or parallel read
//! model.
//!
//! The surface is deliberately small: a hand-rolled HTTP/1.1 server on a
//! loopback address and a UI compiled into the binary. Static assets carry
//! content-versioned immutable URLs and precompressed representations; there
//! is no HTTP framework, runtime bundler, CDN or asset download.

mod http;
mod mcp_app;
mod query_params;
#[cfg(test)]
mod renderer_asset;
mod response_body;
mod routes;
mod static_asset;
mod time_format;
pub mod view;
pub mod views;

use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use kmp_application::{KernelMemoryApplicationService, ObservabilityQueryPort};
use kmp_domain::{
    ContextEventStore, GraphNeighborhoodReader, MemoryAboutIndexReader, NodeDetailReader,
    NodeRelationshipReader, ProjectionWriter, SnapshotStore,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;

use crate::http::{HttpRequest, HttpResponse, host_is_local, read_request, write_response};
use crate::static_asset::route_static_asset;
pub use crate::view::ViewRegistry;
pub use crate::view::application::Applied;
pub use crate::view::application::commands::{ApplyIntentCommand, OpenViewCommand};
pub use crate::view::application::dto::{
    AvoidedHopDto, FocusDto, LabelSelectorDto, PathChainDto, PathEndDto, PathHopDto, PathsDto,
    ProjectionDto, TimeRangeDto, TraceSelectionDto, ViewIntentDto, ViewStateDto,
};
pub use crate::view::application::mappers::{logical_digest, view_state_dto};
pub use crate::view::domain::{Clock, DEFAULT_VIEW_ID, SemanticZoom, ViewError, ViewState};
pub use mcp_app::mcp_app_html;

/// Environment variable that, when set on an embedded MCP session, mounts the
/// viewer on that address (e.g. `127.0.0.1:7317`).
pub const VIEWER_ADDR_ENV: &str = "KMP_VIEWER_ADDR";

/// The default viewer address when none is configured.
pub const DEFAULT_VIEWER_ADDR: &str = "127.0.0.1:7317";

/// A request head must arrive within this. A browser sends its head at once;
/// a socket that stays silent longer is idle or hostile, and it holds one of
/// the few connection slots until it goes. Nobody needs the capability to
/// open a socket, so this is what keeps idle sockets from switching the
/// viewer off.
const HEAD_TIMEOUT: Duration = Duration::from_secs(5);

/// Writing one response must complete within this. The kernel is in-process
/// and local; a slower write means a stuck client, not a slow store.
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

/// Connections served at once. A tab holds a long poll and a handful of
/// reads; a few tabs and an agent fit many times over. The listener stops
/// accepting while every slot is taken, so a flood waits in the kernel's
/// backlog instead of spending the descriptors the MCP process and its store
/// need.
const MAX_CONNECTIONS: usize = 64;

/// How long the accept loop rests after the operating system refuses a
/// connection (descriptors exhausted, for instance) before it tries again.
const ACCEPT_RETRY: Duration = Duration::from_millis(100);

/// How often a persisting accept failure is written to stderr.
const ACCEPT_ERROR_REPORT_EVERY: Duration = Duration::from_secs(60);

/// How long a browser keeps the session cookie without a request. Every
/// authenticated answer renews it, so an open tab never loses it; a cookie
/// left by a session that ended goes on its own instead of piling up until
/// every request head is too large to read.
const COOKIE_MAX_AGE_SECONDS: u64 = 24 * 60 * 60;

/// The viewer over one memory facade.
///
/// Generic over the same stores as [`KernelMemoryApplicationService`], so the
/// embedded edition mounts it over the selected local engine and any future
/// edition can mount it over its own composition unchanged.
pub struct MemoryViewerServer<G, D, S, E, W> {
    service: Arc<KernelMemoryApplicationService<G, D, S, E, W>>,
    data_dir: Option<String>,
    observability: Option<Arc<dyn ObservabilityQueryPort>>,
    observability_unavailable_reason: Option<String>,
    capability: ViewerCapability,
    limits: ConnectionLimits,
    /// One projection read at a time from this viewer. A read that misses
    /// the cache loads the whole about and holds it while it projects; a
    /// browser panning in quick steps used to fire several at once, and the
    /// agent's process held one copy of the about per read. Reads queue here
    /// instead, a read whose browser hung up leaves the queue (see
    /// `handle_connection`), and a queued read that finds its answer cached
    /// by the one ahead is served from the cache.
    projection_turn: Semaphore,
    /// The address `serve` was given, recorded when it starts: the Host the
    /// printed link names must be one this server answers to, and the cookie
    /// is named after its port.
    bound: OnceLock<SocketAddr>,
}

/// How many connections the server holds at once and how long a silent one
/// may keep its slot.
#[derive(Clone, Copy, Debug)]
struct ConnectionLimits {
    max_connections: usize,
    head_timeout: Duration,
}

impl Default for ConnectionLimits {
    fn default() -> Self {
        Self {
            max_connections: MAX_CONNECTIONS,
            head_timeout: HEAD_TIMEOUT,
        }
    }
}

struct ViewerCapability {
    token: String,
}

impl ViewerCapability {
    fn generate() -> io::Result<Self> {
        let mut bytes = [0u8; 32];
        getrandom::getrandom(&mut bytes).map_err(|error| {
            io::Error::other(format!(
                "viewer capability could not be generated from the operating system: {error}"
            ))
        })?;
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut token = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            token.push(HEX[(byte >> 4) as usize] as char);
            token.push(HEX[(byte & 0x0f) as usize] as char);
        }
        Ok(Self { token })
    }

    /// One name per port. A session that starts on a port a previous session
    /// used overwrites that session's cookie instead of adding one; cookies
    /// ignore the port, so two live sessions on different ports must not
    /// share a name.
    fn cookie_name(port: u16) -> String {
        format!("kmp_chronoloom_{port}")
    }

    fn matches(&self, candidate: &str) -> bool {
        if candidate.len() != self.token.len() {
            return false;
        }
        candidate
            .bytes()
            .zip(self.token.bytes())
            .fold(0u8, |difference, (left, right)| difference | (left ^ right))
            == 0
    }

    fn cookie_header(&self, port: u16) -> String {
        format!(
            "{}={}; HttpOnly; SameSite=Strict; Path=/; Max-Age={COOKIE_MAX_AGE_SECONDS}",
            Self::cookie_name(port),
            self.token
        )
    }
}

impl<G, D, S, E, W> MemoryViewerServer<G, D, S, E, W>
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
    /// `data_dir` is display-only: shown in the UI header so a human knows
    /// which store they are looking at.
    pub fn new(
        service: Arc<KernelMemoryApplicationService<G, D, S, E, W>>,
        data_dir: Option<String>,
    ) -> io::Result<Self> {
        ViewRegistry::shared().set_available_overlays(Vec::new());
        Ok(Self {
            service,
            data_dir,
            observability: None,
            observability_unavailable_reason: None,
            capability: ViewerCapability::generate()?,
            limits: ConnectionLimits::default(),
            projection_turn: Semaphore::new(1),
            bound: OnceLock::new(),
        })
    }

    /// Changes how many connections are served at once and how long a
    /// silent socket keeps its slot. The defaults suit every session; tests
    /// shrink them to exercise a flood without exhausting descriptors.
    pub fn with_connection_limits(
        mut self,
        max_connections: usize,
        head_timeout: Duration,
    ) -> Self {
        self.limits = ConnectionLimits {
            max_connections: max_connections.max(1),
            head_timeout,
        };
        self
    }

    /// Turns the bound loopback URL into the one-session invitation a human
    /// can open. The first request exchanges this capability for an HttpOnly
    /// session cookie and redirects to the clean URL.
    pub fn capability_url(&self, base_url: &str) -> String {
        let separator = if base_url.contains('?') { '&' } else { '?' };
        format!("{base_url}{separator}k={}", self.capability.token)
    }

    /// Adds a telemetry read side without coupling the renderer to a local or
    /// remote adapter.
    pub fn with_observability(mut self, observability: Arc<dyn ObservabilityQueryPort>) -> Self {
        ViewRegistry::shared().set_available_overlays(observability.available_series());
        self.observability = Some(observability);
        self.observability_unavailable_reason = None;
        self
    }

    /// Preserves why the optional pulse could not be mounted. The memory
    /// viewer itself remains useful, but its API and startup line must not
    /// misdiagnose a journal lock as missing configuration.
    pub fn with_observability_unavailable(mut self, reason: impl Into<String>) -> Self {
        self.observability_unavailable_reason = Some(reason.into());
        self
    }

    /// Serves until the process ends. Callers spawn this next to their real
    /// work; the viewer never outlives the session. A failed accept is
    /// reported and retried: one refused connection must not switch the
    /// viewer off for the rest of the session.
    pub async fn serve(self: Arc<Self>, listener: TcpListener) -> io::Result<()> {
        let _ = self.bound.set(listener.local_addr()?);
        let slots = Arc::new(Semaphore::new(self.limits.max_connections));
        let mut last_report: Option<Instant> = None;
        loop {
            let slot = Arc::clone(&slots)
                .acquire_owned()
                .await
                .map_err(|_| io::Error::other("viewer connection slots closed"))?;
            match listener.accept().await {
                Ok((stream, _peer)) => {
                    let server = Arc::clone(&self);
                    tokio::spawn(async move {
                        server.handle_connection(stream).await;
                        drop(slot);
                    });
                }
                Err(error) => {
                    drop(slot);
                    if last_report.is_none_or(|at| at.elapsed() >= ACCEPT_ERROR_REPORT_EVERY) {
                        eprintln!("kmp-viewer: accepting a connection failed, retrying: {error}");
                        last_report = Some(Instant::now());
                    }
                    tokio::time::sleep(ACCEPT_RETRY).await;
                }
            }
        }
    }

    fn port(&self) -> u16 {
        self.bound.get().map_or(0, SocketAddr::port)
    }

    async fn handle_connection(&self, mut stream: TcpStream) {
        let request =
            match tokio::time::timeout(self.limits.head_timeout, read_request(&mut stream)).await {
                Err(_elapsed) => Err(HttpResponse::error(408, "request timed out")),
                Ok(result) => result,
            };
        let response = match request {
            Err(response) => response,
            Ok(request) => {
                // A client that hangs up has given up on its answer: a
                // superseded projection read must not keep its place in the
                // queue, or hold the memory it would have cost.
                tokio::select! {
                    response = self.respond(&request) => response,
                    () = peer_closed(&stream) => return,
                }
            }
        };
        let _ = tokio::time::timeout(WRITE_TIMEOUT, write_response(&mut stream, &response)).await;
    }

    async fn respond(&self, request: &HttpRequest) -> HttpResponse {
        let port = self.port();
        if !host_is_local(
            request.host.as_deref(),
            self.bound.get().map(SocketAddr::ip),
        ) {
            return HttpResponse::error(
                403,
                "the viewer answers only to localhost, 127.0.0.1, [::1] or the address it is bound to",
            );
        }
        if let Some(response) = route_static_asset(request) {
            return response;
        }
        if request
            .cookie(&ViewerCapability::cookie_name(port))
            .is_some_and(|token| self.capability.matches(token))
        {
            // Every authenticated answer renews the cookie's lifetime.
            return self
                .route(request)
                .await
                .with_header("Set-Cookie", self.capability.cookie_header(port));
        }
        if matches!(request.method.as_str(), "GET" | "HEAD")
            && matches!(request.path.as_str(), "/" | "/index.html")
            && request
                .param("k")
                .is_some_and(|token| self.capability.matches(token))
        {
            let response = HttpResponse::redirect("/")
                .with_header("Set-Cookie", self.capability.cookie_header(port));
            return if request.method == "HEAD" {
                response.without_body()
            } else {
                response
            };
        }
        HttpResponse::error(
            401,
            "this ChronoLoom session needs its capability link; ask the agent to open the loom, or use the link this session printed when its viewer started",
        )
    }
}

/// Resolves when the client closes its side of the connection. A request has
/// no body and this server reads one request per connection, so the only
/// thing the socket can say after the head is that the client hung up. Bytes
/// a client pipelines anyway are left alone; they never become a request.
async fn peer_closed(stream: &TcpStream) {
    let mut probe = [0u8; 1];
    match stream.peek(&mut probe).await {
        Ok(0) | Err(_) => {}
        Ok(_) => std::future::pending::<()>().await,
    }
}

/// Binds the viewer's listener, refusing anything that is not loopback: this
/// is a window into private memory, and "local only" is enforced here rather
/// than remembered in documentation.
pub async fn bind_loopback(addr: &str) -> io::Result<TcpListener> {
    TcpListener::bind(loopback_socket_addr(addr)?).await
}

/// Checks that `addr` is a loopback socket address without binding it, so a
/// session that binds the viewer only when it is asked for can still refuse
/// a wrong address when it starts.
pub fn loopback_socket_addr(addr: &str) -> io::Result<SocketAddr> {
    let addr: SocketAddr = addr.parse().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("viewer address `{addr}` is not a socket address (host:port): {error}"),
        )
    })?;
    if !addr.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "viewer address `{addr}` is not a loopback address; the viewer serves private \
                 memory and only binds a loopback address: 127.0.0.0/8 (such as 127.0.0.1) or \
                 [::1]"
            ),
        ));
    }
    Ok(addr)
}

#[cfg(test)]
mod tests {
    use super::ViewerCapability;

    #[test]
    fn the_refusal_states_the_rule_the_check_applies() {
        for accepted in ["127.0.0.1:7317", "127.0.0.2:7420", "[::1]:7317"] {
            assert!(super::loopback_socket_addr(accepted).is_ok(), "{accepted}");
        }
        let refused = super::loopback_socket_addr("0.0.0.0:7317")
            .expect_err("not loopback")
            .to_string();
        assert!(refused.contains("127.0.0.0/8"), "{refused}");
        assert!(refused.contains("[::1]"), "{refused}");
    }

    #[test]
    fn session_capabilities_are_random_wide_and_cookie_isolated() {
        let first = ViewerCapability::generate().expect("OS randomness");
        let second = ViewerCapability::generate().expect("OS randomness");
        assert_eq!(first.token.len(), 64, "256 bits rendered as hex");
        assert!(first.token.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(first.token, second.token);
        // Two live sessions on two ports keep two cookies; a new session on
        // a port overwrites the cookie the previous one left there.
        assert_ne!(
            ViewerCapability::cookie_name(7317),
            ViewerCapability::cookie_name(7318)
        );
        assert_eq!(
            first.cookie_header(7317).split('=').next(),
            second.cookie_header(7317).split('=').next()
        );
        assert!(
            first
                .cookie_header(7317)
                .contains("HttpOnly; SameSite=Strict; Path=/; Max-Age=86400")
        );
    }
}
