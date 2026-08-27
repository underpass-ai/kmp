//! A local, read-only web viewer over KMP memory: the graph, its notes, the
//! timeline and causal traces, rendered for a human the way `kmp_wake`,
//! `kmp_inspect`, `kmp_near` and `kmp_trace` render them for an
//! agent — same facade, same semantics, no parallel read model.
//!
//! Served in-process over an already-open kernel on purpose: the viewer sees
//! every write the moment it projects and shares the exact same facade. There
//! is no second database connection, daemon, sync protocol or parallel read
//! model.
//!
//! The surface is deliberately small: a hand-rolled HTTP/1.1 GET server on a
//! loopback address, a UI compiled into the binary with `include_str!`, and
//! no dependency the embedded edition does not already carry — no HTTP
//! framework, no bundler, no CDN, nothing fetched at runtime.

mod http;
mod routes;
pub mod view_state;
pub mod views;

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use kmp_application::KernelMemoryApplicationService;
use kmp_domain::{
    ContextEventStore, GraphNeighborhoodReader, MemoryAboutIndexReader, NodeDetailReader,
    NodeRelationshipReader, ProjectionWriter, SnapshotStore,
};
use tokio::net::{TcpListener, TcpStream};

use crate::http::{HttpResponse, host_is_local, read_request, write_response};
pub use crate::view_state::{
    Applied, DEFAULT_VIEW_ID, Focus, Projection, Provenance, TimeRange, TraceSelection, ViewError,
    ViewPatch, ViewRegistry, ViewState,
};

/// Environment variable that, when set on an embedded MCP session, mounts the
/// viewer on that address (e.g. `127.0.0.1:7317`).
pub const VIEWER_ADDR_ENV: &str = "KMP_VIEWER_ADDR";

/// The default viewer address when none is configured.
pub const DEFAULT_VIEWER_ADDR: &str = "127.0.0.1:7317";

/// One request must complete within this. The kernel is in-process and local;
/// a slower request means a stuck client, not a slow store.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// The viewer over one memory facade.
///
/// Generic over the same stores as [`KernelMemoryApplicationService`], so the
/// embedded edition mounts it over the selected local engine and any future
/// edition can mount it over its own composition unchanged.
pub struct MemoryViewerServer<G, D, S, E, W> {
    service: Arc<KernelMemoryApplicationService<G, D, S, E, W>>,
    data_dir: Option<String>,
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
    ) -> Self {
        Self { service, data_dir }
    }

    /// Serves until the listener fails or the process ends. Callers spawn
    /// this next to their real work; the viewer never outlives the session.
    pub async fn serve(self: Arc<Self>, listener: TcpListener) -> io::Result<()> {
        loop {
            let (stream, _peer) = listener.accept().await?;
            let server = Arc::clone(&self);
            tokio::spawn(async move {
                server.handle_connection(stream).await;
            });
        }
    }

    async fn handle_connection(&self, mut stream: TcpStream) {
        let response = match tokio::time::timeout(REQUEST_TIMEOUT, read_request(&mut stream)).await
        {
            Err(_elapsed) => HttpResponse::error(400, "request timed out"),
            Ok(Err(response)) => response,
            Ok(Ok(request)) => {
                if host_is_local(request.host.as_deref()) {
                    self.route(&request).await
                } else {
                    HttpResponse::error(
                        403,
                        "the viewer answers only to localhost / 127.0.0.1 / [::1]",
                    )
                }
            }
        };
        let _ = tokio::time::timeout(REQUEST_TIMEOUT, write_response(&mut stream, &response)).await;
    }
}

/// Binds the viewer's listener, refusing anything that is not loopback: this
/// is a window into private memory, and "local only" is enforced here rather
/// than remembered in documentation.
pub async fn bind_loopback(addr: &str) -> io::Result<TcpListener> {
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
                 memory and only binds 127.0.0.1 or [::1]"
            ),
        ));
    }
    TcpListener::bind(addr).await
}
