//! Bringing the server up from the environment, and saying so.
//!
//! One concept: turning environment variables into a running
//! [`KernelMcpServer`], and reporting the outcome to whoever is watching —
//! stderr for a person, the log for a diagnosis after the fact. It decides
//! nothing about what the server then answers.

use kmp_mcp::{
    EmbeddedKernelMcpBackend, GRPC_ENDPOINT_ENV, GRPC_TLS_CA_PATH_ENV, GRPC_TLS_CERT_PATH_ENV,
    GRPC_TLS_DOMAIN_NAME_ENV, GRPC_TLS_KEY_PATH_ENV, GRPC_TLS_MODE_ENV, KernelMcpServer,
    MCP_BACKEND_ENV,
};
use std::io::{self};
use std::sync::OnceLock;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// Why a start failed, and whether choosing a backend would fix it.
///
/// This used to be decided by matching the front of the message, which is a
/// guess about wording rather than about what happened: every new failure
/// defaulted to "choose a backend" and sent people to look exactly where the
/// problem was not. A store that refused to open and a viewer port already
/// taken are both correctly configured sessions.
pub(crate) struct StartupFailure {
    message: String,
    backend_selection: bool,
}

impl StartupFailure {
    /// The backend could not be chosen from the environment. Saying which
    /// variables settle it is genuinely the next step.
    fn selecting_a_backend(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            backend_selection: true,
        }
    }

    /// The backend was clear; something after it went wrong.
    fn after_the_backend_was_chosen(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            backend_selection: false,
        }
    }
}

/// Builds the MCP server, mounting the local web viewer over the embedded
/// backend. Sharing this process's kernel keeps the viewer on the exact live
/// facade without a second store connection or read model.
pub(crate) async fn server_from_env() -> Result<KernelMcpServer, StartupFailure> {
    let viewer = kmp_mcp::viewer::viewer_addr_from_env();
    let backend_is_embedded = backend_is_embedded_from_env();

    let Some(addr) = viewer.addr() else {
        return KernelMcpServer::try_from_env().map_err(StartupFailure::selecting_a_backend);
    };
    if !backend_is_embedded {
        // Only a request that was actually made can be refused. A default
        // that met a gRPC session is not a misconfiguration to report.
        return if viewer.was_asked_for() {
            Err(StartupFailure::after_the_backend_was_chosen(format!(
                "{} is set but {MCP_BACKEND_ENV} is not `embedded`; the viewer mounts over the \
                 in-process kernel only",
                kmp_viewer::VIEWER_ADDR_ENV
            )))
        } else {
            KernelMcpServer::try_from_env().map_err(StartupFailure::selecting_a_backend)
        };
    }

    let resolved = kmp_embedded::resolve_data_dir_from_env()
        .map_err(|error| StartupFailure::after_the_backend_was_chosen(error.to_string()))?;
    let engine = kmp_embedded::resolve_engine_for_data_dir_from_env(resolved.path())
        .map_err(|error| StartupFailure::after_the_backend_was_chosen(error.to_string()))?;
    let lease = kmp_embedded::user_data_home()
        .map(|data_home| {
            kmp_mcp::uninstall::StoreSessionLease::acquire(&data_home, resolved.path())
        })
        .transpose()
        .map_err(StartupFailure::after_the_backend_was_chosen)?;
    let commit_native = kmp_embedded::CommitNativeBundle::for_resolved(&resolved);
    let backend = EmbeddedKernelMcpBackend::open_with_engine_and_commit_native(
        resolved.path(),
        engine,
        commit_native,
    )
    .map_err(StartupFailure::after_the_backend_was_chosen)?;
    remember_this_memory(resolved.path());
    let url = match spawn_viewer(backend.kernel(), addr).await {
        Ok(url) => Some(url),
        // An explicit address is a contract. The default is only a preference:
        // if another session owns it, this process falls forward to a free
        // loopback port below and advertises its own capability URL.
        Err(message) if viewer.was_asked_for() => {
            // You asked for this address, so you get told why it did not
            // happen and what to type next — not a shrug.
            return Err(StartupFailure::after_the_backend_was_chosen(format!(
                "{message}\nkmp-mcp: usually another project's session already has it. Name a \
                 free port with {}=127.0.0.1:7318, or {}=off to go without.",
                kmp_viewer::VIEWER_ADDR_ENV,
                kmp_viewer::VIEWER_ADDR_ENV
            )));
        }
        Err(message) => {
            eprintln!("kmp-mcp: {message}; choosing a free per-session loopback port instead");
            match spawn_viewer(backend.kernel(), "127.0.0.1:0").await {
                Ok(url) => Some(url),
                Err(fallback) => {
                    eprintln!(
                        "kmp-mcp: viewer fallback also failed: {fallback}; continuing without it. \
                         Set {}=off to stop offering it.",
                        kmp_viewer::VIEWER_ADDR_ENV
                    );
                    None
                }
            }
        }
    };
    let server = KernelMcpServer::with_embedded_backend(backend)
        .with_orphaned_bundle(resolved.orphaned_bundle().cloned());
    let server = match lease {
        Some(lease) => server.with_store_session_lease(lease),
        None => server,
    };
    Ok(match url {
        Some(url) => server.serving_viewer_at(url),
        None => server,
    })
}

/// Binds the viewer on `addr` (loopback only) and serves it in the
/// background for the life of this process. Returns the URL it ended up on,
/// which is the bound address rather than the requested one: `:0` and an
/// unspecified host both resolve here, and the caller hands this to a human.
pub(super) async fn spawn_viewer(
    kernel: &kmp_embedded::EmbeddedKernel,
    addr: &str,
) -> Result<String, String> {
    let telemetry_unavailable = kernel.quality_telemetry_error().map(str::to_string);
    let mut viewer = kmp_viewer::MemoryViewerServer::new(
        kernel.service(),
        Some(kernel.data_dir().display().to_string()),
    )
    .map_err(|error| format!("viewer capability could not be created: {error}"))?;
    if let Some(reader) = kernel.quality_telemetry_reader() {
        viewer = viewer.with_observability(std::sync::Arc::new(reader));
    } else if let Some(reason) = telemetry_unavailable.as_deref() {
        viewer = viewer.with_observability_unavailable(reason);
    }
    let viewer = std::sync::Arc::new(viewer);
    let listener = kmp_viewer::bind_loopback(addr)
        .await
        .map_err(|error| format!("viewer could not bind `{addr}`: {error}"))?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| format!("viewer listener has no local address: {error}"))?;
    let base_url = format!("http://{local_addr}/");
    let url = viewer.capability_url(&base_url);
    if let Some(reason) = telemetry_unavailable {
        eprintln!("kmp-mcp: memory viewer at {url}; observability unavailable: {reason}");
    } else {
        eprintln!("kmp-mcp: memory viewer at {url}");
    }
    tokio::spawn(async move {
        if let Err(error) = viewer.serve(listener).await {
            tracing::error!(%error, "memory viewer stopped");
        }
    });
    Ok(url)
}

/// Logs go to stderr through a bounded, lossy queue (stdout belongs to MCP
/// JSON-RPC). A host is allowed to capture stderr without draining it, so the
/// protocol loop must never wait for that pipe. In embedded mode logs are
/// additionally journaled to `<data-dir>/logs/` with daily rotation (ADR-012
/// layout) so a session can be investigated after the host discards stderr.
/// The returned file guard must live for the process.
pub(crate) fn init_tracing() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let filter =
        || EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("kmp_mcp=info"));
    let (stderr_writer, stderr_guard) =
        tracing_appender::non_blocking::NonBlockingBuilder::default()
            .buffered_lines_limit(256)
            .lossy(true)
            .thread_name("kmp-stderr-log")
            .finish(io::stderr());
    // Static values are not dropped at process exit. That matters here:
    // WorkerGuard joins its writer thread, but that thread may legitimately
    // be stuck behind a host-owned, undrained stderr pipe. Keeping the worker
    // process-scoped lets EOF end the MCP process without joining that pipe.
    static STDERR_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();
    STDERR_GUARD.get_or_init(|| stderr_guard);
    let stderr_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_writer(stderr_writer)
        .with_filter(filter());

    let file_layer = embedded_log_dir().map(|log_dir| {
        let (writer, guard) = tracing_appender::non_blocking(tracing_appender::rolling::daily(
            log_dir,
            "kmp-mcp.log",
        ));
        let layer = tracing_subscriber::fmt::layer()
            .json()
            .with_writer(writer)
            .with_filter(filter());
        (layer, guard)
    });

    match file_layer {
        Some((layer, guard)) => {
            tracing_subscriber::registry()
                .with(stderr_layer)
                .with(layer)
                .init();
            Some(guard)
        }
        None => {
            tracing_subscriber::registry().with(stderr_layer).init();
            None
        }
    }
}

/// `<data-dir>/logs/` when running the embedded backend; None otherwise or
/// when resolution fails (the server will fail fast with the real error).
pub(super) fn embedded_log_dir() -> Option<std::path::PathBuf> {
    if !backend_is_embedded_from_env() {
        return None;
    }
    // Beside the memory it serves, when that is knowable. A data dir that
    // will not resolve is itself a startup failure worth recording, so fall
    // back to the per-user state home rather than losing the one line that
    // would explain it.
    let log_dir = kmp_embedded::resolve_data_dir_from_env()
        .map(|resolved| resolved.path().join("logs"))
        .unwrap_or_else(|_| user_state_home().join("kmp").join("logs"));
    std::fs::create_dir_all(&log_dir).ok()?;
    Some(log_dir)
}

/// Keep every zero-configuration sidecar on the same backend decision as the
/// server itself. An absent or blank selector means embedded unless a live
/// gRPC endpoint was configured; only an explicit non-embedded selector (or
/// that endpoint fallback) turns embedded behaviour off.
pub(super) fn backend_is_embedded_from_env() -> bool {
    let configured_backend = std::env::var(MCP_BACKEND_ENV).ok();
    let endpoint = std::env::var(kmp_mcp::GRPC_ENDPOINT_ENV).ok();
    match configured_backend.as_deref().map(str::trim) {
        Some(value) if !value.is_empty() => value.eq_ignore_ascii_case("embedded"),
        _ => endpoint.is_none_or(|value| value.trim().is_empty()),
    }
}

pub(super) fn user_state_home() -> std::path::PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".local")
                .join("state")
        })
}

/// Says a start failed to whoever is watching, and to whoever is not.
///
/// A host consumes stderr and shows the user nothing but an absence of tools,
/// so a start that failed this way used to leave no trace anywhere — and the
/// one tool built to answer "why is my memory not working" had nothing to
/// read.
pub(crate) fn report_failure(failure: &StartupFailure) {
    let StartupFailure {
        message,
        backend_selection,
    } = failure;
    tracing::error!(
        reason = %message,
        version = env!("CARGO_PKG_VERSION"),
        "startup failed"
    );
    eprintln!("kmp-mcp: {message}");
    if *backend_selection {
        // The default is the embedded kernel, so a backend failure is always
        // something that was asked for by name. Pointing at gRPC and fixture,
        // and never at embedded, sent people away from the mode the product
        // is.
        eprintln!(
            "kmp-mcp: unset {MCP_BACKEND_ENV} to run the embedded kernel — no service, \
             no endpoint, nothing to configure"
        );
    }
}

/// Says which backend started, on stderr and in the log.
///
/// The mark goes to stderr: stdout is the protocol, and a banner on it would
/// be the first thing a host fails to parse. A start that worked is worth a
/// line in the log too — without one, a log holding only failures cannot tell
/// "it has never started" from "it started and the failure is older than this
/// file".
pub(crate) fn announce(server: &KernelMcpServer) {
    // The mark goes to stderr: stdout is the protocol, and a banner on it
    // would be the first thing a host fails to parse.
    eprintln!(
        "{}\n",
        kmp_mcp::banner::large(kmp_mcp::style::Style::for_stderr())
    );
    if server.backend_name() == "grpc" {
        eprintln!(
            "kmp-mcp: using live gRPC backend from {GRPC_ENDPOINT_ENV} with {GRPC_TLS_MODE_ENV}={}",
            server.grpc_tls_mode_name()
        );
        if server.grpc_tls_mode_name() != "disabled" {
            eprintln!(
                "kmp-mcp: TLS envs: {GRPC_TLS_CA_PATH_ENV}, {GRPC_TLS_CERT_PATH_ENV}, {GRPC_TLS_KEY_PATH_ENV}, {GRPC_TLS_DOMAIN_NAME_ENV}"
            );
        }
    } else if server.backend_name() == "embedded" {
        match server.embedded_engine() {
            Some(engine) => {
                eprintln!("kmp-mcp: using embedded backend (kernel in-process, {engine} engine)")
            }
            None => eprintln!("kmp-mcp: using embedded backend (kernel in-process)"),
        }
    } else {
        eprintln!("kmp-mcp: using explicit fixture backend");
    }

    // A start that worked is worth a line too: without one, a log holding
    // only failures cannot tell "it has never started" from "it started and
    // the failure is older than this file".
    tracing::info!(
        backend = server.backend_name(),
        engine = server.embedded_engine().map(|engine| engine.to_string()),
        version = env!("CARGO_PKG_VERSION"),
        "startup succeeded"
    );
}

/// Records that this data directory exists, so `info` can list it from any
/// other directory later. A project `.kernel` can be anywhere on disk, and
/// nothing that ships could find one you were not standing next to.
///
/// Machine state about this user's filesystem: local only, never in a bundle,
/// and pruned on read when the path is gone.
fn remember_this_memory(path: &std::path::Path) {
    if let Some(data_home) = kmp_embedded::user_data_home() {
        kmp_mcp::memories::remember(&data_home, path);
    }
}
