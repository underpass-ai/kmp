use rehydration_mcp::{
    GRPC_ENDPOINT_ENV, GRPC_TLS_CA_PATH_ENV, GRPC_TLS_CERT_PATH_ENV, GRPC_TLS_DOMAIN_NAME_ENV,
    GRPC_TLS_KEY_PATH_ENV, GRPC_TLS_MODE_ENV, KernelMcpServer, MCP_BACKEND_ENV,
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

    let server = match KernelMcpServer::try_from_env() {
        Ok(server) => server,
        Err(message) => {
            eprintln!("rehydration-mcp: {message}");
            eprintln!(
                "rehydration-mcp: set {GRPC_ENDPOINT_ENV} for live gRPC, or set {MCP_BACKEND_ENV}=fixture explicitly for fixture mode"
            );
            std::process::exit(2);
        }
    };
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    if server.backend_name() == "grpc" {
        eprintln!(
            "rehydration-mcp: using live gRPC backend from {GRPC_ENDPOINT_ENV} with {GRPC_TLS_MODE_ENV}={}",
            server.grpc_tls_mode_name()
        );
        if server.grpc_tls_mode_name() != "disabled" {
            eprintln!(
                "rehydration-mcp: TLS envs: {GRPC_TLS_CA_PATH_ENV}, {GRPC_TLS_CERT_PATH_ENV}, {GRPC_TLS_KEY_PATH_ENV}, {GRPC_TLS_DOMAIN_NAME_ENV}"
            );
        }
    } else if server.backend_name() == "embedded" {
        eprintln!("rehydration-mcp: using embedded backend (kernel in-process)");
    } else {
        eprintln!("rehydration-mcp: using explicit fixture backend");
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

/// Logs go to stderr always (stdout belongs to MCP JSON-RPC). In embedded
/// mode they are additionally journaled to `<data-dir>/logs/` with daily
/// rotation (ADR-012 layout) so a session can be investigated after the
/// host discards stderr. The returned guard must live for the process.
fn init_tracing() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let filter = || {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("rehydration_mcp=info"))
    };
    let stderr_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_writer(io::stderr)
        .with_filter(filter());

    let file_layer = embedded_log_dir().map(|log_dir| {
        let (writer, guard) = tracing_appender::non_blocking(tracing_appender::rolling::daily(
            log_dir,
            "rehydration-mcp.log",
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
    let resolved = rehydration_embedded::resolve_data_dir_from_env().ok()?;
    let log_dir = resolved.path().join("logs");
    std::fs::create_dir_all(&log_dir).ok()?;
    Some(log_dir)
}

/// Non-MCP maintenance surface (everything is a process — no library):
/// `export <file>` and `import <file>` move the append-only event log
/// between embedded stores; stdout carries the command result only.
async fn run_cli_command(command: &str, path: Option<&str>) -> i32 {
    match command {
        "export" | "import" => {}
        "--version" | "-V" | "version" => {
            println!(
                "rehydration-mcp {} (store format {})",
                env!("CARGO_PKG_VERSION"),
                rehydration_embedded::SUPPORTED_FORMAT_VERSION
            );
            return 0;
        }
        other => {
            eprintln!(
                "rehydration-mcp: unknown command `{other}`; run without arguments for MCP \
                 stdio mode, or use `export <file>` / `import <file>` / `--version`"
            );
            return 2;
        }
    }

    let Some(path) = path else {
        eprintln!("rehydration-mcp: {command} requires a bundle file path");
        return 2;
    };
    let resolved = match rehydration_embedded::resolve_data_dir_from_env() {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("rehydration-mcp: {error}");
            return 2;
        }
    };
    let kernel = match rehydration_embedded::EmbeddedKernel::open(resolved.path()) {
        Ok(kernel) => kernel,
        Err(error) => {
            eprintln!("rehydration-mcp: {error}");
            return 2;
        }
    };
    let store = kernel.store();

    match command {
        "export" => match store.export_bundle().await {
            Ok(bundle) => {
                if let Err(error) = std::fs::write(path, bundle) {
                    eprintln!("rehydration-mcp: could not write `{path}`: {error}");
                    return 2;
                }
                println!(
                    "{{\"exported_to\":\"{path}\",\"data_dir\":\"{}\"}}",
                    resolved.path().display()
                );
                0
            }
            Err(error) => {
                eprintln!("rehydration-mcp: export failed: {error}");
                2
            }
        },
        _ => {
            let bundle = match std::fs::read_to_string(path) {
                Ok(bundle) => bundle,
                Err(error) => {
                    eprintln!("rehydration-mcp: could not read `{path}`: {error}");
                    return 2;
                }
            };
            match store
                .import_bundle(
                    &bundle,
                    rehydration_application::projection_mutations_for_context_event,
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
                    eprintln!("rehydration-mcp: import failed: {error}");
                    2
                }
            }
        }
    }
}
