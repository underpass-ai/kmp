use kmp_mcp::{
    EmbeddedKernelMcpBackend, GRPC_ENDPOINT_ENV, GRPC_TLS_CA_PATH_ENV, GRPC_TLS_CERT_PATH_ENV,
    GRPC_TLS_DOMAIN_NAME_ENV, GRPC_TLS_KEY_PATH_ENV, GRPC_TLS_MODE_ENV, KernelMcpServer,
    MCP_BACKEND_ENV,
};
use std::io::{self, BufRead, Write};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cli_args = std::env::args().skip(1);
    if let Some(command) = cli_args.next() {
        let path = cli_args.next();
        std::process::exit(run_cli_command(&command, path.as_deref()).await);
    }

    let _log_guard = init_tracing();

    let server = match server_from_env().await {
        Ok(server) => server,
        Err(message) => {
            eprintln!("kmp-mcp: {message}");
            eprintln!(
                "kmp-mcp: set {GRPC_ENDPOINT_ENV} for live gRPC, or set {MCP_BACKEND_ENV}=fixture explicitly for fixture mode"
            );
            std::process::exit(2);
        }
    };
    let stdin = io::stdin();
    let mut stdout = io::stdout();

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
        eprintln!("kmp-mcp: using embedded backend (kernel in-process)");
    } else {
        eprintln!("kmp-mcp: using explicit fixture backend");
    }

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        if let Some(response) = server.handle_json_line(&line).await {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }

    Ok(())
}

/// Builds the MCP server, mounting the local web viewer over the embedded
/// backend when `KMP_VIEWER_ADDR` asks for it. The viewer must share
/// this process's kernel: the embedded store is single-writer (ADR-011), so
/// an in-process mount is the only way to watch a live session.
async fn server_from_env() -> Result<KernelMcpServer, String> {
    let viewer_addr = std::env::var(kmp_viewer::VIEWER_ADDR_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let backend_is_embedded = std::env::var(MCP_BACKEND_ENV)
        .map(|value| value.trim().eq_ignore_ascii_case("embedded"))
        .unwrap_or(false);

    let Some(addr) = viewer_addr else {
        return KernelMcpServer::try_from_env();
    };
    if !backend_is_embedded {
        return Err(format!(
            "{} is set but {MCP_BACKEND_ENV} is not `embedded`; the viewer mounts over the \
             in-process kernel only",
            kmp_viewer::VIEWER_ADDR_ENV
        ));
    }

    let resolved = kmp_embedded::resolve_data_dir_from_env().map_err(|e| e.to_string())?;
    let backend = EmbeddedKernelMcpBackend::open(resolved.path())?;
    spawn_viewer(backend.kernel(), &addr).await?;
    Ok(KernelMcpServer::with_backend(backend))
}

/// Binds the viewer on `addr` (loopback only) and serves it in the
/// background for the life of this process.
async fn spawn_viewer(kernel: &kmp_embedded::EmbeddedKernel, addr: &str) -> Result<(), String> {
    let viewer = std::sync::Arc::new(kmp_viewer::MemoryViewerServer::new(
        kernel.service(),
        Some(kernel.data_dir().display().to_string()),
    ));
    let listener = kmp_viewer::bind_loopback(addr)
        .await
        .map_err(|error| format!("viewer could not bind `{addr}`: {error}"))?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| format!("viewer listener has no local address: {error}"))?;
    eprintln!("kmp-mcp: memory viewer at http://{local_addr}/");
    tokio::spawn(async move {
        if let Err(error) = viewer.serve(listener).await {
            tracing::error!(%error, "memory viewer stopped");
        }
    });
    Ok(())
}

/// Logs go to stderr always (stdout belongs to MCP JSON-RPC). In embedded
/// mode they are additionally journaled to `<data-dir>/logs/` with daily
/// rotation (ADR-012 layout) so a session can be investigated after the
/// host discards stderr. The returned guard must live for the process.
fn init_tracing() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let filter =
        || EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("kmp_mcp=info"));
    let stderr_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_writer(io::stderr)
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
fn embedded_log_dir() -> Option<std::path::PathBuf> {
    let backend = std::env::var(MCP_BACKEND_ENV).ok()?;
    if !backend.trim().eq_ignore_ascii_case("embedded") {
        return None;
    }
    let resolved = kmp_embedded::resolve_data_dir_from_env().ok()?;
    let log_dir = resolved.path().join("logs");
    std::fs::create_dir_all(&log_dir).ok()?;
    Some(log_dir)
}

/// Non-MCP maintenance surface (everything is a process — no library):
/// `export <file>` and `import <file>` move the append-only event log
/// between embedded stores, `viewer [addr]` serves the local web viewer over
/// the store; stdout carries the command result only.
async fn run_cli_command(command: &str, path: Option<&str>) -> i32 {
    match command {
        "export" | "import" => {}
        "viewer" => return run_viewer_command(path).await,
        "--version" | "-V" | "version" => {
            println!(
                "kmp-mcp {} (store format {})",
                env!("CARGO_PKG_VERSION"),
                kmp_embedded::SUPPORTED_FORMAT_VERSION
            );
            return 0;
        }
        other => {
            eprintln!(
                "kmp-mcp: unknown command `{other}`; run without arguments for MCP \
                 stdio mode, or use `export <file>` / `import <file>` / `viewer [addr]` / \
                 `--version`"
            );
            return 2;
        }
    }

    let Some(path) = path else {
        eprintln!("kmp-mcp: {command} requires a bundle file path");
        return 2;
    };
    let resolved = match kmp_embedded::resolve_data_dir_from_env() {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let kernel = match kmp_embedded::EmbeddedKernel::open(resolved.path()) {
        Ok(kernel) => kernel,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let store = kernel.store();

    match command {
        "export" => match store.export_bundle().await {
            Ok(bundle) => {
                if let Err(error) = std::fs::write(path, bundle) {
                    eprintln!("kmp-mcp: could not write `{path}`: {error}");
                    return 2;
                }
                println!(
                    "{{\"exported_to\":\"{path}\",\"data_dir\":\"{}\"}}",
                    resolved.path().display()
                );
                0
            }
            Err(error) => {
                eprintln!("kmp-mcp: export failed: {error}");
                2
            }
        },
        _ => {
            let bundle = match std::fs::read_to_string(path) {
                Ok(bundle) => bundle,
                Err(error) => {
                    eprintln!("kmp-mcp: could not read `{path}`: {error}");
                    return 2;
                }
            };
            match store
                .import_bundle(
                    &bundle,
                    kmp_application::projection_mutations_for_context_event,
                )
                .await
            {
                Ok(report) => {
                    println!(
                        "{{\"events_imported\":{},\"mutations_applied\":{}}}",
                        report.events_imported, report.rebuild.mutations_applied
                    );
                    0
                }
                Err(error) => {
                    eprintln!("kmp-mcp: import failed: {error}");
                    2
                }
            }
        }
    }
}

/// Standalone viewer over the env-resolved data dir (same resolution as
/// `export`/`import`). Only works while no agent session holds the store —
/// the embedded engine is single-writer per ADR-011; to watch a live session,
/// set `KMP_VIEWER_ADDR` on that session instead.
async fn run_viewer_command(addr: Option<&str>) -> i32 {
    // `viewer` with no argument honours the same env the MCP mode uses.
    let addr = addr
        .map(ToString::to_string)
        .or_else(|| std::env::var(kmp_viewer::VIEWER_ADDR_ENV).ok())
        .unwrap_or_else(|| kmp_viewer::DEFAULT_VIEWER_ADDR.to_string());
    let resolved = match kmp_embedded::resolve_data_dir_from_env() {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let kernel = match kmp_embedded::EmbeddedKernel::open(resolved.path()) {
        Ok(kernel) => kernel,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    if let Err(message) = spawn_viewer(&kernel, &addr).await {
        eprintln!("kmp-mcp: {message}");
        return 2;
    }
    eprintln!("kmp-mcp: serving the viewer until this process is stopped (Ctrl-C)");
    // The viewer task owns the listener; keep the process alive for it.
    std::future::pending::<()>().await;
    0
}
