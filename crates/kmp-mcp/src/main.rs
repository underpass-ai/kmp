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
        Err(StartupFailure {
            message,
            backend_selection,
        }) => {
            // On stderr for whoever is watching, and in the log for whoever
            // is not. A host consumes stderr and shows the user nothing but
            // an absence of tools, so a start that fails this way used to
            // leave no trace anywhere — and the one tool built to answer
            // "why is my memory not working" had nothing to read.
            tracing::error!(
                reason = %message,
                version = env!("CARGO_PKG_VERSION"),
                "startup failed"
            );
            eprintln!("kmp-mcp: {message}");
            if backend_selection {
                // The default is the embedded kernel, so a backend failure is
                // always something that was asked for by name. Pointing at
                // gRPC and fixture, and never at embedded, sent people away
                // from the mode the product is.
                eprintln!(
                    "kmp-mcp: unset {MCP_BACKEND_ENV} to run the embedded kernel — no service, \
                     no endpoint, nothing to configure"
                );
            }
            std::process::exit(2);
        }
    };
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    // The mark goes to stderr: stdout is the protocol, and a banner on it
    // would be the first thing a host fails to parse.
    eprintln!("{}\n", kmp_mcp::banner::LARGE);
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

/// Why a start failed, and whether choosing a backend would fix it.
///
/// This used to be decided by matching the front of the message, which is a
/// guess about wording rather than about what happened: every new failure
/// defaulted to "choose a backend" and sent people to look exactly where the
/// problem was not. A store that refused to open and a viewer port already
/// taken are both correctly configured sessions.
struct StartupFailure {
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
/// backend. The viewer must share this process's kernel: the embedded store
/// is single-writer (ADR-011), so an in-process mount is the only way to
/// watch a live session.
async fn server_from_env() -> Result<KernelMcpServer, StartupFailure> {
    let viewer = kmp_mcp::viewer::viewer_addr_from_env();
    let backend_is_embedded = std::env::var(MCP_BACKEND_ENV)
        .map(|value| value.trim().eq_ignore_ascii_case("embedded"))
        .unwrap_or(false);

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
        // The commonest cause is another project's session already holding
        // the port. That session's viewer still works; this one goes without
        // rather than taking the memory down with it.
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
            eprintln!(
                "kmp-mcp: {message}; continuing without it. Set {}=off to stop offering it.",
                kmp_viewer::VIEWER_ADDR_ENV
            );
            None
        }
    };
    let server = KernelMcpServer::with_embedded_backend(backend);
    Ok(match url {
        Some(url) => server.serving_viewer_at(url),
        None => server,
    })
}

/// Binds the viewer on `addr` (loopback only) and serves it in the
/// background for the life of this process. Returns the URL it ended up on,
/// which is the bound address rather than the requested one: `:0` and an
/// unspecified host both resolve here, and the caller hands this to a human.
async fn spawn_viewer(kernel: &kmp_embedded::EmbeddedKernel, addr: &str) -> Result<String, String> {
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
    let url = format!("http://{local_addr}/");
    eprintln!("kmp-mcp: memory viewer at {url}");
    tokio::spawn(async move {
        if let Err(error) = viewer.serve(listener).await {
            tracing::error!(%error, "memory viewer stopped");
        }
    });
    Ok(url)
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

fn user_state_home() -> std::path::PathBuf {
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

/// Non-MCP maintenance surface (everything is a process — no library):
/// `export <file>` and `import <file>` move the append-only event log
/// between embedded stores, `migrate <source-dir> <destination-dir>` moves a
/// store this binary refuses to open into one it does, and `viewer [addr]`
/// serves the local web viewer over the store; stdout carries the command
/// result only.
async fn run_cli_command(command: &str, args: &[&str]) -> i32 {
    let first_argument = args.first().copied();
    match command {
        "export" | "import" => {}
        "document" => return run_document_command(args).await,
        "snapshot" => return run_snapshot_command(args).await,
        "uninstall" => return run_uninstall_command(args).await,
        "migrate" => return run_migrate_command(args).await,
        "share-memory" => return run_share_memory_command(first_argument).await,
        "viewer" => return run_viewer_command(first_argument).await,
        "--help" | "-h" | "help" => {
            print_help();
            return 0;
        }
        "info" => {
            print!("{}", kmp_mcp::diagnostics::info());
            return 0;
        }
        "doctor" => {
            let (report, code) = kmp_mcp::diagnostics::doctor();
            print!("{report}");
            return code;
        }
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
                 stdio mode, or use `document <about> [--out FILE]` / \
                 `snapshot create|list|verify|read|merge ...` / \
                 `uninstall [--apply] [--purge] [--keep-memory]` / \
                 `export <file>` / `import <file>` / \
                 `migrate <source-dir> <destination-dir> [--engine redb|sqlite]` / \
                 `share-memory [data-dir]` / `viewer [addr]` / `--version` / `--help`"
            );
            return 2;
        }
    }

    let (path, repair_pending) = if command == "export" {
        let mut path = None;
        let mut repair_pending = false;
        for argument in args {
            if *argument == "--repair-pending" {
                repair_pending = true;
            } else if path.replace(*argument).is_some() {
                eprintln!(
                    "kmp-mcp: export takes at most one file path and optional --repair-pending"
                );
                return 2;
            }
        }
        (path, repair_pending)
    } else {
        if args.len() > 1 {
            eprintln!("kmp-mcp: import takes at most one file path");
            return 2;
        }
        (first_argument, false)
    };

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
    let is_project_head = kmp_embedded::project_bundle_path(&resolved).as_deref() == Some(path);
    if repair_pending && !is_project_head {
        eprintln!(
            "kmp-mcp: --repair-pending applies only to the project head \
             `.kmp/memory.jsonl`, not an explicit export path"
        );
        return 2;
    }
    let engine = match kmp_embedded::resolve_engine_for_data_dir_from_env(resolved.path()) {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let kernel = match kmp_embedded::EmbeddedKernel::open_with_engine(resolved.path(), engine) {
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
                let header = match kmp_embedded::verify_bundle(&bundle) {
                    Ok(header) => header,
                    Err(error) => {
                        eprintln!("kmp-mcp: generated bundle did not verify: {error}");
                        return 2;
                    }
                };
                if let Err(error) = kmp_embedded::write_bundle_atomically(path, &bundle) {
                    eprintln!("kmp-mcp: could not write `{}`: {error}", path.display());
                    return 2;
                }
                if repair_pending
                    && let Err(error) = kmp_embedded::clear_pending_bundle_exports(resolved.path())
                {
                    eprintln!(
                        "kmp-mcp: bundle was exported, but pending markers could not be \
                         cleared: {error}"
                    );
                    return 2;
                }
                println!(
                    "{}",
                    serde_json::json!({
                        "exported_to": path.display().to_string(),
                        "data_dir": resolved.path().display().to_string(),
                        "snapshot_id": header.snapshot_id,
                        "event_count": header.event_count,
                        "content_digest": header.content_digest,
                    })
                );
                let pending = if is_project_head {
                    kmp_embedded::pending_bundle_exports(resolved.path()).len()
                } else {
                    0
                };
                if pending > 0 {
                    eprintln!(
                        "kmp-mcp: {pending} pending write marker(s) remain. Stop other KMP \
                         sessions, inspect this exported bundle, then run `kmp-mcp export \
                         --repair-pending` to acknowledge recovery safely."
                    );
                    1
                } else {
                    0
                }
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

fn print_help() {
    println!(
        "{}\n\n\
Usage:\n  kmp-mcp                         Serve MCP over stdio\n  \
kmp-mcp info                    What this binary is and which memory it opens\n  \
kmp-mcp doctor                  Diagnose the setup and name the one thing to fix\n  \
kmp-mcp document <about>        Render one about as a Markdown document\n  \
kmp-mcp snapshot <verb>         Create, verify, read or merge named snapshots\n  \
kmp-mcp uninstall [--apply]     Show what removing KMP would take, then take it\n  \
kmp-mcp export [file]           Export the append-only event log\n  \
kmp-mcp export --repair-pending Acknowledge recovery after stopping writers\n  \
kmp-mcp import [file]           Import an event-log bundle\n  \
kmp-mcp migrate <src> <dst> [--engine redb|sqlite]\n  \
kmp-mcp share-memory [data-dir] Make an existing redb store shareable\n  \
kmp-mcp viewer [addr]           Serve the local memory viewer\n  \
kmp-mcp --version               Print binary and store formats\n  \
kmp-mcp --help                  Print this help",
        kmp_mcp::banner::LARGE
    );
}

/// Named recovery points. Every operation except `create` reads only the
/// committed JSONL files; `read` imports into an isolated temporary store and
/// never opens the live one.
async fn run_snapshot_command(args: &[&str]) -> i32 {
    let Some((verb, rest)) = args.split_first() else {
        eprintln!(
            "kmp-mcp: snapshot needs create <name>, list, verify <name>, read <name> <tool> \
             <arguments-json>, or merge <left> <right> <name>"
        );
        return 2;
    };
    let resolved = match if *verb == "create" {
        kmp_embedded::resolve_data_dir_from_env()
    } else {
        kmp_embedded::locate_data_dir_from_env()
    } {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };

    match *verb {
        "create" => snapshot_create(&resolved, rest).await,
        "list" => snapshot_list(&resolved, rest),
        "verify" => snapshot_verify(&resolved, rest),
        "read" => snapshot_read(&resolved, rest).await,
        "merge" => snapshot_merge(&resolved, rest),
        other => {
            eprintln!("kmp-mcp: snapshot has no `{other}` verb");
            2
        }
    }
}

async fn snapshot_create(resolved: &kmp_embedded::ResolvedDataDir, args: &[&str]) -> i32 {
    let [name] = args else {
        eprintln!("kmp-mcp: snapshot create takes exactly one name");
        return 2;
    };
    let path = match kmp_mcp::snapshot::path_for_name(resolved, name) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let engine = match kmp_embedded::resolve_engine_for_data_dir_from_env(resolved.path()) {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let kernel = match kmp_embedded::EmbeddedKernel::open_with_engine(resolved.path(), engine) {
        Ok(kernel) => kernel,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let bundle = match kernel.store().export_named_bundle(name).await {
        Ok(bundle) => bundle,
        Err(error) => {
            eprintln!("kmp-mcp: snapshot create failed: {error}");
            return 2;
        }
    };
    match write_named_snapshot(&path, &bundle) {
        Ok(header) => {
            println!("{}", snapshot_result(&path, &header));
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            2
        }
    }
}

fn snapshot_list(resolved: &kmp_embedded::ResolvedDataDir, args: &[&str]) -> i32 {
    if !args.is_empty() {
        eprintln!("kmp-mcp: snapshot list takes no arguments");
        return 2;
    }
    match kmp_mcp::snapshot::list(resolved) {
        Ok(snapshots) => {
            let snapshots: Vec<_> = snapshots
                .into_iter()
                .map(|(path, header)| snapshot_result(&path, &header))
                .collect();
            println!("{}", serde_json::json!({"snapshots": snapshots}));
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            2
        }
    }
}

fn snapshot_verify(resolved: &kmp_embedded::ResolvedDataDir, args: &[&str]) -> i32 {
    let [name] = args else {
        eprintln!("kmp-mcp: snapshot verify takes exactly one name");
        return 2;
    };
    let path = match kmp_mcp::snapshot::path_for_name(resolved, name) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    match kmp_mcp::snapshot::read_header(&path) {
        Ok(header) => {
            println!("{}", snapshot_result(&path, &header));
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            2
        }
    }
}

async fn snapshot_read(resolved: &kmp_embedded::ResolvedDataDir, args: &[&str]) -> i32 {
    let [name, tool, raw_arguments] = args else {
        eprintln!("kmp-mcp: snapshot read takes <name> <read-tool> <arguments-json>");
        return 2;
    };
    let path = match kmp_mcp::snapshot::path_for_name(resolved, name) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let bundle = match std::fs::read_to_string(&path) {
        Ok(bundle) => bundle,
        Err(error) => {
            eprintln!("kmp-mcp: could not read `{}`: {error}", path.display());
            return 2;
        }
    };
    let arguments: serde_json::Value = match serde_json::from_str(raw_arguments) {
        Ok(serde_json::Value::Object(arguments)) => serde_json::Value::Object(arguments),
        Ok(_) => {
            eprintln!("kmp-mcp: snapshot read arguments must be a JSON object");
            return 2;
        }
        Err(error) => {
            eprintln!("kmp-mcp: snapshot read arguments are not valid JSON: {error}");
            return 2;
        }
    };
    match kmp_mcp::snapshot::read_only(&bundle, tool, arguments).await {
        Ok(response) => {
            let failed = response.get("error").is_some()
                || response["result"]["isError"].as_bool() == Some(true);
            println!("{response}");
            if failed { 1 } else { 0 }
        }
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            2
        }
    }
}

fn snapshot_merge(resolved: &kmp_embedded::ResolvedDataDir, args: &[&str]) -> i32 {
    let [left, right, name] = args else {
        eprintln!("kmp-mcp: snapshot merge takes <left> <right> <new-name>");
        return 2;
    };
    let path = |name: &str| kmp_mcp::snapshot::path_for_name(resolved, name);
    let (left_path, right_path, output_path) = match (path(left), path(right), path(name)) {
        (Ok(left), Ok(right), Ok(output)) => (left, right, output),
        (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let read = |path: &std::path::Path| {
        std::fs::read_to_string(path)
            .map_err(|error| format!("could not read `{}`: {error}", path.display()))
    };
    let (left_bundle, right_bundle) = match (read(&left_path), read(&right_path)) {
        (Ok(left), Ok(right)) => (left, right),
        (Err(error), _) | (_, Err(error)) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let merged = match kmp_embedded::merge_bundles(&left_bundle, &right_bundle, name) {
        Ok(merged) => merged,
        Err(error) => {
            eprintln!("kmp-mcp: snapshot merge refused: {error}");
            return 2;
        }
    };
    match write_named_snapshot(&output_path, &merged) {
        Ok(header) => {
            println!("{}", snapshot_result(&output_path, &header));
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            2
        }
    }
}

fn write_named_snapshot(
    path: &std::path::Path,
    bundle: &str,
) -> Result<kmp_embedded::BundleHeader, String> {
    let header = kmp_embedded::verify_bundle(bundle).map_err(|error| error.to_string())?;
    let created =
        kmp_embedded::write_bundle_if_absent(path, bundle).map_err(|error| error.to_string())?;
    if !created {
        let existing = kmp_mcp::snapshot::read_header(path)?;
        if existing.content_digest == header.content_digest {
            return Ok(existing);
        }
        return Err(format!(
            "snapshot `{}` already identifies digest {}; choose a new name instead of rewriting \
             a recovery point",
            existing.snapshot_id, existing.content_digest
        ));
    }
    Ok(header)
}

fn snapshot_result(
    path: &std::path::Path,
    header: &kmp_embedded::BundleHeader,
) -> serde_json::Value {
    serde_json::json!({
        "snapshot_id": header.snapshot_id,
        "created_at_unix_ms": header.created_at_unix_ms,
        "event_range": header.event_range,
        "event_count": header.event_count,
        "abouts": header.abouts,
        "content_digest": header.content_digest,
        "path": path.display().to_string(),
    })
}

/// `migrate <source-dir> <destination-dir>`: replays the source's history
/// into a new store this binary can open.
///
/// Both directories are explicit on purpose. This command runs precisely
/// when the environment-resolved store is the one that will not open, and
/// asking an operator to fix that by exporting an environment variable is
/// how the wrong directory gets migrated over the right one.
/// `share-memory [data-dir]` — the seven manual steps, as one command.
///
/// Two agent hosts sharing one memory needs the sqlite engine (ADR-018), and
/// getting there by hand meant: notice the binary cannot open a sqlite store,
/// reinstall with the feature, discover the live store is locked by your own
/// session so it cannot be migrated in place, snapshot it, migrate the
/// snapshot, verify nothing was lost, move the original aside, move the new
/// one in, restart. Seven steps, three of them non-obvious, and the product
/// suggested none of them.
///
/// Nothing is deleted. The original data directory is moved aside under a
/// dated name and stays exactly as it was, so this is reversible by moving it
/// back.
/// `uninstall` — what `/kmp:setup` never had an inverse for.
///
/// The dry run is the default and `--apply` is how someone says to go ahead:
/// a destructive command whose first run destroys is one people learn to fear
/// and then avoid. Exit 1 when anything was kept, so "uninstalled" is a
/// checkable claim rather than a hope.
async fn run_uninstall_command(args: &[&str]) -> i32 {
    let mut applying = false;
    let mut purge = false;
    let mut keep_memory = false;
    for argument in args {
        match *argument {
            "--apply" | "--yes" => applying = true,
            "--purge" => purge = true,
            "--keep-memory" => keep_memory = true,
            other => {
                eprintln!(
                    "kmp-mcp: uninstall has no option `{other}`; it takes --apply, --purge and \
                     --keep-memory"
                );
                return 2;
            }
        }
    }

    let roots = kmp_mcp::uninstall::Roots {
        home: std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".")),
        data_home: std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".local/share")
            }),
        working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        path_entries: std::env::var_os("PATH")
            .map(|value| std::env::split_paths(&value).collect())
            .unwrap_or_default(),
    };

    let workspace = roots.working_dir.clone();
    let pieces: Vec<_> = kmp_mcp::uninstall::survey(&roots)
        .into_iter()
        .filter(|piece| !(keep_memory && piece.kind == kmp_mcp::uninstall::PieceKind::Store))
        .collect();
    print!(
        "{}",
        kmp_mcp::uninstall::report(&pieces, &workspace, purge, applying)
    );

    if pieces.is_empty() {
        return 0;
    }
    if !applying {
        println!("Run the same command with --apply to remove what is listed.");
        return 0;
    }

    let mut kept = 0;
    for piece in &pieces {
        // Memory is handed back before it is taken. A failed save keeps the
        // store: the copy is the point, and removing memory whose rescue did
        // not happen is the one mistake this verb must not make.
        if !purge && let Some(destination) = kmp_mcp::uninstall::rescue_path(piece, &workspace) {
            match save_store(&piece.path, &destination).await {
                Ok(events) => println!(
                    "saved    {} — {events} {}",
                    destination.display(),
                    if events == 1 { "event" } else { "events" }
                ),
                Err(reason) => {
                    kept += 1;
                    println!(
                        "kept     {}\n         could not save it first: {reason}",
                        piece.path.display()
                    );
                    continue;
                }
            }
        }
        match kmp_mcp::uninstall::remove(piece) {
            Ok(()) => println!("removed  {}", piece.path.display()),
            Err(reason) => {
                kept += 1;
                println!("kept     {}\n         {reason}", piece.path.display());
            }
        }
    }
    if kept > 0 {
        println!("\n{kept} left in place. KMP is not fully removed.");
        return 1;
    }
    println!("\nEverything listed is gone.");
    0
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

/// Writes a store's whole event log to `destination`, and answers with how
/// many events it holds — a number is what makes the file believable.
async fn save_store(store: &std::path::Path, destination: &std::path::Path) -> Result<u64, String> {
    let engine = kmp_embedded::default_engine_for_data_dir(store);
    let kernel = kmp_embedded::EmbeddedKernel::open_with_engine(store, engine)
        .map_err(|error| error.to_string())?;
    let bundle = kernel
        .store()
        .export_bundle()
        .await
        .map_err(|error| error.to_string())?;
    let events = bundle
        .lines()
        .next()
        .and_then(|header| serde_json::from_str::<serde_json::Value>(header).ok())
        .and_then(|header| {
            header
                .get("event_count")
                .and_then(serde_json::Value::as_u64)
        })
        .unwrap_or_default();
    std::fs::write(destination, bundle)
        .map_err(|error| format!("could not write `{}`: {error}", destination.display()))?;
    Ok(events)
}

/// `document <about> [--out FILE]` — one about, as Markdown.
///
/// Reads the same export the bundle uses, because that is the only source
/// carrying every entry, every relation's `why` and every piece of evidence
/// as written. Rendering is a read: it opens the store, takes nothing from a
/// live session it should not, and writes only where it was told to.
async fn run_document_command(args: &[&str]) -> i32 {
    let mut about = None;
    let mut out = None;
    let mut rest = args.iter();
    while let Some(argument) = rest.next() {
        match *argument {
            "--out" | "-o" => match rest.next() {
                Some(path) => out = Some(PathBuf::from(path)),
                None => {
                    eprintln!("kmp-mcp: --out needs a file path");
                    return 2;
                }
            },
            other if about.is_none() => about = Some(other.to_string()),
            other => {
                eprintln!("kmp-mcp: document takes one about, and `{other}` is a second one");
                return 2;
            }
        }
    }
    let Some(about) = about else {
        eprintln!(
            "kmp-mcp: document needs an about — the anchor the memory was written under, like \
             `project:kmp`. `kmp-mcp info` says which memory this directory opens."
        );
        return 2;
    };

    let resolved = match kmp_embedded::resolve_data_dir_from_env() {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let engine = match kmp_embedded::resolve_engine_for_data_dir_from_env(resolved.path()) {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let kernel = match kmp_embedded::EmbeddedKernel::open_with_engine(resolved.path(), engine) {
        Ok(kernel) => kernel,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let bundle = match kernel.store().export_bundle().await {
        Ok(bundle) => bundle,
        Err(error) => {
            eprintln!("kmp-mcp: could not read the event log: {error}");
            return 2;
        }
    };
    let document = match kmp_mcp::document::render(&bundle, &about) {
        Ok(document) => document,
        Err(message) => {
            eprintln!("kmp-mcp: {message}");
            return 2;
        }
    };

    match out {
        Some(path) => {
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
                && let Err(error) = std::fs::create_dir_all(parent)
            {
                eprintln!("kmp-mcp: could not create `{}`: {error}", parent.display());
                return 2;
            }
            if let Err(error) = std::fs::write(&path, document) {
                eprintln!("kmp-mcp: could not write `{}`: {error}", path.display());
                return 2;
            }
            // stdout carries the command result only, so a script can read it.
            println!(
                "{{\"documented\":\"{about}\",\"written_to\":\"{}\"}}",
                path.display()
            );
        }
        None => print!("{document}"),
    }
    0
}

async fn run_share_memory_command(explicit_dir: Option<&str>) -> i32 {
    use kmp_embedded::{EmbeddedKernel, StorageEngine};

    // A binary without the engine cannot do any of this, and finding that out
    // after the migration would be the worst possible moment.
    if !StorageEngine::Sqlite.is_compiled() {
        eprintln!(
            "kmp-mcp: this binary was built without the sqlite engine, so it cannot share a \
             store between hosts.\n  install the shipped build with: cargo install kmp-mcp\n               (then re-run this command; nothing has been changed)"
        );
        return 2;
    }

    let data_dir = match explicit_dir {
        Some(path) => std::path::PathBuf::from(path),
        None => match kmp_embedded::resolve_data_dir_from_env() {
            Ok(resolved) => resolved.path().to_path_buf(),
            Err(error) => {
                eprintln!("kmp-mcp: cannot resolve which data dir to share: {error}");
                return 2;
            }
        },
    };
    if !data_dir.exists() {
        eprintln!(
            "kmp-mcp: no memory at `{}` yet. Start a current default build there and it is \
             shareable from the first write (or set KMP_MCP_ENGINE=sqlite explicitly).",
            data_dir.display()
        );
        return 2;
    }

    match kmp_embedded::EmbeddedKernelStore::engine_of(&data_dir) {
        Ok(StorageEngine::Sqlite) => {
            println!(
                "already shareable: `{}` is on the sqlite engine. Point both hosts at it.",
                data_dir.display()
            );
            return 0;
        }
        Ok(_) => {}
        Err(error) => {
            eprintln!(
                "kmp-mcp: cannot read the store at `{}`: {error}",
                data_dir.display()
            );
            return 2;
        }
    }

    // The live store is very likely held by the session asking for this, and
    // redb is single-writer — so the migration reads a snapshot, never the
    // original. Copying files is a read; it does not need the lock.
    // The working copies live beside the data directory rather than in a
    // temp dir: same filesystem, so installing the result is a rename within
    // one volume instead of a copy across two, and a failure leaves the
    // evidence where the operator will look for it.
    let work = data_dir.with_file_name(format!(
        "{}-share-memory-work",
        data_dir
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "kmp".to_string())
    ));
    if work.exists() {
        eprintln!(
            "kmp-mcp: `{}` is left over from an earlier run; move or remove it first. \
             Nothing has been changed.",
            work.display()
        );
        return 2;
    }
    let snapshot = work.join("snapshot");
    let shared = work.join("shared");
    if let Err(error) = copy_tree(&data_dir, &snapshot) {
        eprintln!(
            "kmp-mcp: could not snapshot `{}`: {error}",
            data_dir.display()
        );
        return 2;
    }
    println!("snapshot taken (the live store was not touched)");

    let receipt =
        match kmp_embedded::migrate_data_dir_to(&snapshot, &shared, StorageEngine::Sqlite).await {
            Ok(receipt) => receipt,
            Err(error) => {
                eprintln!("kmp-mcp: migration failed, nothing was changed: {error}");
                return 2;
            }
        };
    println!(
        "migrated: {} events, {} mutations",
        receipt.events_migrated, receipt.mutations_applied
    );

    // Verify before swapping, not after: a migration that reports success and
    // loses events would otherwise be discovered by a reader, later.
    match verify_same_log(&snapshot, &shared).await {
        Ok((events, sequence)) => {
            println!("verified: {events} events, last sequence {sequence}, on both engines");
        }
        Err(error) => {
            eprintln!(
                "kmp-mcp: the migrated store does not match the original, so nothing was \
                 changed: {error}"
            );
            return 2;
        }
    }

    let kept = data_dir.with_file_name(format!(
        "{}-redb-before-share",
        data_dir
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "kmp".to_string())
    ));
    if kept.exists() {
        eprintln!(
            "kmp-mcp: `{}` already exists, so the original cannot be moved aside safely; \
             nothing was changed",
            kept.display()
        );
        return 2;
    }
    if let Err(error) = std::fs::rename(&data_dir, &kept) {
        eprintln!("kmp-mcp: could not move the original aside: {error}");
        return 2;
    }
    if let Err(error) = copy_tree(&shared, &data_dir) {
        eprintln!(
            "kmp-mcp: could not install the shared store; the original is intact at `{}`: {error}",
            kept.display()
        );
        return 2;
    }

    let _ = std::fs::remove_dir_all(&work);
    drop(EmbeddedKernel::open(&data_dir));
    println!(
        "\n`{}` is now on the sqlite engine and two hosts can share it.\n\
         the original is kept at `{}` — nothing was deleted\n\
         restart every agent host so it opens the new store",
        data_dir.display(),
        kept.display()
    );
    0
}

/// Copies a data directory, file by file, without following the store's lock.
fn copy_tree(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Both stores must hold the same log: same length, same last sequence.
async fn verify_same_log(
    original: &std::path::Path,
    migrated: &std::path::Path,
) -> Result<(u64, u64), String> {
    let read = |dir: &std::path::Path| {
        kmp_embedded::EmbeddedKernelStore::open(dir)
            .map_err(|error| format!("could not open `{}`: {error}", dir.display()))
    };
    let before = read(original)?;
    let after = read(migrated)?;
    let before_stats = before
        .event_log_stats()
        .await
        .map_err(|error| format!("could not read the original log: {error}"))?;
    let after_stats = after
        .event_log_stats()
        .await
        .map_err(|error| format!("could not read the migrated log: {error}"))?;
    if before_stats != after_stats {
        return Err(format!(
            "original holds {} events (last sequence {}), migrated holds {} (last sequence {})",
            before_stats.0, before_stats.1, after_stats.0, after_stats.1
        ));
    }
    Ok(after_stats)
}

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
    let engine = match kmp_embedded::resolve_engine_for_data_dir_from_env(resolved.path()) {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let kernel = match kmp_embedded::EmbeddedKernel::open_with_engine(resolved.path(), engine) {
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
