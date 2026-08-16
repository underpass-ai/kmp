use kmp_mcp::{
    EmbeddedKernelMcpBackend, GRPC_ENDPOINT_ENV, GRPC_TLS_CA_PATH_ENV, GRPC_TLS_CERT_PATH_ENV,
    GRPC_TLS_DOMAIN_NAME_ENV, GRPC_TLS_KEY_PATH_ENV, GRPC_TLS_MODE_ENV, KernelMcpServer,
    MCP_BACKEND_ENV,
};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli_args: Vec<String> = std::env::args().skip(1).collect();
    if let Some((command, rest)) = cli_args.split_first() {
        let rest: Vec<&str> = rest.iter().map(String::as_str).collect();
        std::process::exit(run_cli_command(command, &rest).await);
    }

    let _log_guard = init_tracing();

    let server = match server_from_env().await {
        Ok(server) => server,
        Err(message) => {
            eprintln!("kmp-mcp: {message}");
            // Only a backend-selection failure is fixed by choosing a
            // backend. After a store that refused to open — wrong engine,
            // locked, too new — this line sent people to look exactly where
            // the problem was not.
            if !message.starts_with("embedded store")
                && !message.starts_with("unknown storage engine")
            {
                eprintln!(
                    "kmp-mcp: set {GRPC_ENDPOINT_ENV} for live gRPC, or set {MCP_BACKEND_ENV}=fixture explicitly for fixture mode"
                );
            }
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
        match server.embedded_engine() {
            Some(engine) => {
                eprintln!("kmp-mcp: using embedded backend (kernel in-process, {engine} engine)")
            }
            None => eprintln!("kmp-mcp: using embedded backend (kernel in-process)"),
        }
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
    let engine = kmp_embedded::resolve_engine_from_env().map_err(|e| e.to_string())?;
    let backend = EmbeddedKernelMcpBackend::open_with_engine(resolved.path(), engine)?;
    spawn_viewer(backend.kernel(), &addr).await?;
    Ok(KernelMcpServer::with_embedded_backend(backend))
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
/// between embedded stores, `migrate <source-dir> <destination-dir>` moves a
/// store this binary refuses to open into one it does, and `viewer [addr]`
/// serves the local web viewer over the store; stdout carries the command
/// result only.
async fn run_cli_command(command: &str, args: &[&str]) -> i32 {
    let path = args.first().copied();
    match command {
        "export" | "import" => {}
        "migrate" => return run_migrate_command(args).await,
        "viewer" => return run_viewer_command(path).await,
        "--version" | "-V" | "version" => {
            // The layouts this build opens, so a user can tell at a glance
            // whether their binary carries the sqlite engine (ADR-018).
            use kmp_embedded::StorageEngine;
            let mut formats = vec![format!("{}", StorageEngine::Redb.format_version())];
            if StorageEngine::Sqlite.is_compiled() {
                formats.push(format!(
                    "{} (sqlite)",
                    StorageEngine::Sqlite.format_version()
                ));
            }
            println!(
                "kmp-mcp {} (store format{} {})",
                env!("CARGO_PKG_VERSION"),
                if formats.len() > 1 { "s" } else { "" },
                formats.join(", ")
            );
            return 0;
        }
        other => {
            eprintln!(
                "kmp-mcp: unknown command `{other}`; run without arguments for MCP \
                 stdio mode, or use `export <file>` / `import <file>` / \
                 `migrate <source-dir> <destination-dir> [--engine redb|sqlite]` / \
                 `viewer [addr]` / `--version`"
            );
            return 2;
        }
    }

    let resolved = match kmp_embedded::resolve_data_dir_from_env() {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    // No path means the project's committed copy. Only a project-scoped store
    // has one — an explicit data dir or the per-user default belongs to no
    // repository, and picking a file for them would write memory somewhere
    // the operator did not choose.
    let path = match path.map(PathBuf::from) {
        Some(path) => path,
        None => match kmp_embedded::project_bundle_path(&resolved) {
            Some(path) => path,
            None => {
                eprintln!(
                    "kmp-mcp: {command} needs a bundle file path here. The default \
                     `{}` is the project's committed memory, and this store is not \
                     project-scoped: it resolved to `{}` by the `{}` rule.",
                    kmp_embedded::PROJECT_BUNDLE_PATH,
                    resolved.path().display(),
                    resolved.rule_name()
                );
                return 2;
            }
        },
    };
    let path = path.as_path();
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
                // `.kmp/` will not exist on the first save, and failing on
                // that would make the convention useless exactly once per
                // repository — at the moment someone tries it.
                if let Some(parent) = path.parent()
                    && !parent.as_os_str().is_empty()
                    && let Err(error) = std::fs::create_dir_all(parent)
                {
                    eprintln!("kmp-mcp: could not create `{}`: {error}", parent.display());
                    return 2;
                }
                if let Err(error) = std::fs::write(path, bundle) {
                    eprintln!("kmp-mcp: could not write `{}`: {error}", path.display());
                    return 2;
                }
                println!(
                    "{{\"exported_to\":\"{}\",\"data_dir\":\"{}\"}}",
                    path.display(),
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
                    eprintln!("kmp-mcp: could not read `{}`: {error}", path.display());
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

/// `migrate <source-dir> <destination-dir>`: replays the source's history
/// into a new store this binary can open.
///
/// Both directories are explicit on purpose. This command runs precisely
/// when the environment-resolved store is the one that will not open, and
/// asking an operator to fix that by exporting an environment variable is
/// how the wrong directory gets migrated over the right one.
async fn run_migrate_command(args: &[&str]) -> i32 {
    let (Some(source), Some(destination)) = (args.first(), args.get(1)) else {
        eprintln!(
            "kmp-mcp: migrate requires <source-dir> <destination-dir> [--engine redb|sqlite]"
        );
        return 2;
    };
    // `--engine` picks the destination's engine; without it the destination
    // keeps the default. `--engine sqlite` is how a store that one agent host
    // owns becomes one that two can share (ADR-018).
    let engine = match parse_engine_flag(&args[2..]) {
        Ok(engine) => engine,
        Err(message) => {
            eprintln!("kmp-mcp: {message}");
            return 2;
        }
    };
    match kmp_embedded::migrate_data_dir_to(
        std::path::Path::new(source),
        std::path::Path::new(destination),
        engine,
    )
    .await
    {
        Ok(receipt) => {
            match serde_json::to_string(&receipt) {
                Ok(json) => println!("{json}"),
                Err(error) => {
                    eprintln!(
                        "kmp-mcp: migration succeeded but its receipt is unprintable: {error}"
                    );
                    return 2;
                }
            }
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp: migration failed: {error}");
            2
        }
    }
}

/// Parses the optional `--engine <name>` that follows the two directories.
fn parse_engine_flag(rest: &[&str]) -> Result<kmp_embedded::StorageEngine, String> {
    use kmp_embedded::StorageEngine;
    match rest {
        [] => Ok(StorageEngine::Redb),
        ["--engine", "redb"] => Ok(StorageEngine::Redb),
        ["--engine", "sqlite"] => Ok(StorageEngine::Sqlite),
        ["--engine", other] => Err(format!(
            "unknown engine `{other}`; expected `redb` or `sqlite`"
        )),
        ["--engine"] => Err("--engine needs a value: `redb` or `sqlite`".to_string()),
        [other, ..] => Err(format!(
            "unexpected argument `{other}`; migrate takes <source-dir> <destination-dir> \
             [--engine redb|sqlite]"
        )),
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
