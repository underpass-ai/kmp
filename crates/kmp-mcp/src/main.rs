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
/// backend. Sharing this process's kernel keeps the viewer on the exact live
/// facade without a second store connection or read model.
async fn server_from_env() -> Result<KernelMcpServer, StartupFailure> {
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
fn backend_is_embedded_from_env() -> bool {
    let configured_backend = std::env::var(MCP_BACKEND_ENV).ok();
    let endpoint = std::env::var(kmp_mcp::GRPC_ENDPOINT_ENV).ok();
    match configured_backend.as_deref().map(str::trim) {
        Some(value) if !value.is_empty() => value.eq_ignore_ascii_case("embedded"),
        _ => endpoint.is_none_or(|value| value.trim().is_empty()),
    }
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
        "config" => return run_config_command(args),
        "uninstall" => return run_uninstall_command(args).await,
        "migrate" => return run_migrate_command(args).await,
        "share-memory" => {
            eprintln!(
                "kmp-mcp: share-memory was retired; new stores already use SQLite. \
                 Migrate a legacy format-1 store with `kmp-mcp migrate <source-dir> \
                 <destination-dir>`."
            );
            return 2;
        }
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
            // Format 2 is the active SQLite layout. Format 1 remains readable
            // only for the compatibility and migration promise.
            use kmp_embedded::StorageEngine;
            println!(
                "kmp-mcp {} (store formats {} (legacy read), {} (sqlite))",
                env!("CARGO_PKG_VERSION"),
                StorageEngine::Redb.format_version(),
                StorageEngine::Sqlite.format_version()
            );
            return 0;
        }
        other => {
            eprintln!(
                "kmp-mcp: unknown command `{other}`; run without arguments for MCP \
                 stdio mode, or use `document <about> [--out FILE]` / \
                 `snapshot create|list|verify|read|merge ...` / \
                 `config [ask-fallback-languages <tags>]` / \
                 `uninstall [--apply] [--purge] [--keep-memory]` / \
                 `export <file>` / `import <file>` / \
                 `migrate <source-dir> <destination-dir>` / \
                 `viewer [addr]` / `--version` / `--help`"
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
        "export" => {
            // The pulse holds the line only while the store is actually
            // read; it is erased before anything else prints.
            let pulse = kmp_mcp::pulse::Pulse::start("saving your memory…");
            let exported = store.export_bundle().await;
            pulse.clear();
            match exported {
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
                        && let Err(error) =
                            kmp_embedded::clear_pending_bundle_exports(resolved.path())
                    {
                        eprintln!(
                            "kmp-mcp: bundle was exported, but pending markers could not be \
                         cleared: {error}"
                        );
                        return 2;
                    }
                    kmp_mcp::pulse::mark_done(&match header.event_count {
                        0 => "saved — an empty log, ready to grow".to_string(),
                        count => format!("saved — {}, every one in order", events(count)),
                    });
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
            }
        }
        _ => {
            let bundle = match std::fs::read_to_string(path) {
                Ok(bundle) => bundle,
                Err(error) => {
                    eprintln!("kmp-mcp: could not read `{}`: {error}", path.display());
                    return 2;
                }
            };
            let pulse = kmp_mcp::pulse::Pulse::start("bringing your memory back…");
            let imported = store
                .import_bundle(
                    &bundle,
                    kmp_application::projection_mutations_for_context_event,
                )
                .await;
            pulse.clear();
            match imported {
                Ok(report) => {
                    kmp_mcp::pulse::mark_done(&match report.events_imported {
                        0 => "back — nothing to replay yet".to_string(),
                        count => format!("back — {} replayed", events(count)),
                    });
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
kmp-mcp config                  Show the agent orchestration policy\n  \
kmp-mcp config ask-fallback-languages <tags|none>\n  \
kmp-mcp document <about>        Render one about as a Markdown document\n  \
kmp-mcp snapshot <verb>         Create, verify, read or merge named snapshots\n  \
kmp-mcp uninstall [--apply]     Show what removing KMP would take, then take it\n  \
kmp-mcp export [file]           Export the append-only event log\n  \
kmp-mcp export --repair-pending Acknowledge recovery after stopping writers\n  \
kmp-mcp import [file]           Import an event-log bundle\n  \
kmp-mcp migrate <src> <dst>     Migrate a legacy store to SQLite\n  \
kmp-mcp viewer [addr]           Serve the local memory viewer\n  \
kmp-mcp --version               Print binary and store formats\n  \
kmp-mcp --help                  Print this help",
        kmp_mcp::banner::large(kmp_mcp::style::Style::for_stdout())
    );
}

fn run_config_command(args: &[&str]) -> i32 {
    match args {
        [] => match kmp_mcp::agent_policy::load() {
            Ok(policy) => {
                print!("{}", kmp_mcp::agent_policy::display(&policy));
                0
            }
            Err(error) => {
                eprintln!("kmp-mcp: agent policy is invalid: {error}");
                2
            }
        },
        ["ask-fallback-languages" | "--ask-fallback-languages", value] => {
            let languages = match kmp_mcp::agent_policy::parse_cli_languages(value) {
                Ok(languages) => languages,
                Err(error) => {
                    eprintln!("kmp-mcp: {error}");
                    return 2;
                }
            };
            match kmp_mcp::agent_policy::store(&languages) {
                Ok(policy) => {
                    print!("{}", kmp_mcp::agent_policy::display(&policy));
                    0
                }
                Err(error) => {
                    eprintln!("kmp-mcp: could not store agent policy: {error}");
                    2
                }
            }
        }
        _ => {
            eprintln!(
                "kmp-mcp: config takes no arguments, or `ask-fallback-languages <comma-separated-tags|none>`"
            );
            2
        }
    }
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
    let pulse = kmp_mcp::pulse::Pulse::start("pinning this moment…");
    let exported = kernel.store().export_named_bundle(name).await;
    pulse.clear();
    let bundle = match exported {
        Ok(bundle) => bundle,
        Err(error) => {
            eprintln!("kmp-mcp: snapshot create failed: {error}");
            return 2;
        }
    };
    match write_named_snapshot(&path, &bundle) {
        Ok(header) => {
            kmp_mcp::pulse::mark_done(&format!("pinned as `{name}`"));
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

    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let data_home =
        kmp_embedded::user_data_home().unwrap_or_else(|| home.join(".local").join("share"));
    let roots = kmp_mcp::uninstall::Roots {
        home,
        data_home,
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
        kmp_mcp::uninstall::report(
            &pieces,
            &workspace,
            purge,
            applying,
            kmp_mcp::style::Style::for_stdout()
        )
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

/// `1 event`, `12 events`. The pulse's closing line quotes a count, and a
/// count that reads wrong undoes the care that line exists to show.
fn events(count: u64) -> String {
    if count == 1 {
        "1 event".to_string()
    } else {
        format!("{count} events")
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

async fn run_migrate_command(args: &[&str]) -> i32 {
    let (Some(source), Some(destination)) = (args.first(), args.get(1)) else {
        eprintln!("kmp-mcp: migrate requires <source-dir> <destination-dir>");
        return 2;
    };
    if let Some(unexpected) = args.get(2) {
        eprintln!(
            "kmp-mcp: migrate has no engine option; SQLite is the only destination \
             engine (unexpected argument `{unexpected}`)"
        );
        return 2;
    }
    let pulse = kmp_mcp::pulse::Pulse::start("replaying history onto the new engine…");
    let migrated = kmp_embedded::migrate_data_dir_to(
        std::path::Path::new(source),
        std::path::Path::new(destination),
        kmp_embedded::StorageEngine::Sqlite,
    )
    .await;
    pulse.clear();
    match migrated {
        Ok(receipt) => {
            kmp_mcp::pulse::mark_done(&match receipt.events_migrated {
                0 => "moved — nothing to replay yet".to_string(),
                count => format!("moved — {} replayed", events(count)),
            });
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

/// Standalone viewer over the env-resolved data dir (same resolution as
/// `export`/`import`). A redb store must be idle; SQLite can be shared, but
/// setting `KMP_VIEWER_ADDR` on the agent session remains the direct path to
/// its already-open kernel.
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
