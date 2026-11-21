//! `kmp-mcp demo [--no-viewer] [--dry-run]`: write the worked example into
//! the store that resolves here and open ChronoLoom on it, so a fresh
//! install has memory to look at and questions to ask in its first minute.
use kmp_mcp::demo;

use super::serve::spawn_viewer;
use super::{looks_like_option, unknown_option};

pub(super) async fn run_demo_command(args: &[&str]) -> i32 {
    let mut viewer = true;
    let mut dry_run = false;
    for argument in args {
        match *argument {
            "--no-viewer" => viewer = false,
            "--dry-run" => dry_run = true,
            other if looks_like_option(other) => return unknown_option("demo", other),
            other => {
                eprintln!("kmp-mcp demo: unexpected argument `{other}`");
                eprintln!("usage: {}", super::subcommand_usage("demo"));
                return 2;
            }
        }
    }

    // Where the demo goes is decided by locating, which writes nothing: a
    // preview must leave no trace, and the real run prepares only the store
    // the demo actually writes (N1 of the 1.0 review).
    let located = match kmp_embedded::locate_data_dir_from_env() {
        Ok(located) => located,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let destination = match DemoDestination::for_located(located) {
        Ok(destination) => destination,
        Err(error) => {
            eprintln!("kmp-mcp demo: {error}");
            return 2;
        }
    };
    let resolved = destination.store.clone();
    let entries = match demo::entry_count() {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!("kmp-mcp demo: {error}");
            return 2;
        }
    };
    if dry_run {
        println!(
            "KMP demo: would write {entries} memories about {} into {} (chosen by {}) and \
             open ChronoLoom on them",
            demo::ABOUT,
            resolved.path().display(),
            resolved.rule_name()
        );
        destination.explain();
        return 0;
    }

    // The demo is memory on this machine. A remote kernel is somebody's
    // shared service, and a fixture answers from canned examples: neither
    // is a store to write a worked example into.
    let server = match kmp_mcp::KernelMcpServer::try_from_env_with_store(Some(resolved.clone())) {
        Ok(server) if server.backend_name() == "embedded" => server,
        Ok(server) => {
            eprintln!(
                "kmp-mcp demo: the demo writes the local embedded store, and this environment \
                 selects the {} backend; unset {} to run it here",
                server.backend_name(),
                kmp_mcp::MCP_BACKEND_ENV
            );
            return 2;
        }
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let pulse = kmp_mcp::pulse::Pulse::start("writing the worked example…");
    let written = demo::seed(&server).await;
    drop(pulse);
    // Release this process's claim on the store before the viewer opens it.
    drop(server);
    if let Err(error) = written {
        eprintln!("kmp-mcp demo: {error}");
        return 2;
    }
    println!(
        "KMP demo: {entries} memories about {} are in {} (chosen by {}).",
        demo::ABOUT,
        resolved.path().display(),
        resolved.rule_name()
    );
    destination.explain();

    let url = if viewer {
        match open_viewer(resolved.path()).await {
            Ok(url) => Some(url),
            Err(error) => {
                eprintln!("kmp-mcp demo: {error}; continuing without the viewer");
                None
            }
        }
    } else {
        None
    };
    println!();
    if let Some(url) = &url {
        println!("ChronoLoom is open on it: {url}");
    }
    match &destination.project_store {
        None => println!(
            "Ask your agent, naming KMP and the about `{}`:",
            demo::ABOUT
        ),
        Some(_) => println!(
            "Ask an agent whose session opens {} (one started outside a project, or with \
             KMP_MCP_DATA_DIR={}), naming KMP and the about `{}`:",
            resolved.path().display(),
            resolved.path().display(),
            demo::ABOUT
        ),
    }
    for ask in demo::ASKS {
        println!("  • {ask}");
    }
    if url.is_some() {
        println!();
        eprintln!("kmp-mcp: serving the viewer until this process is stopped (Ctrl-C)");
        std::future::pending::<()>().await;
    }
    0
}

async fn open_viewer(data_dir: &std::path::Path) -> Result<String, String> {
    let addr = kmp_mcp::viewer::viewer_addr_from_env();
    let Some(addr) = addr.addr() else {
        return Err(format!(
            "{} declines the viewer",
            kmp_viewer::VIEWER_ADDR_ENV
        ));
    };
    let engine = kmp_embedded::default_engine_for_data_dir(data_dir);
    let kernel = kmp_embedded::EmbeddedKernel::open_with_engine(data_dir, engine)
        .map_err(|error| error.to_string())?;
    match spawn_viewer(&kernel, addr).await {
        Ok(url) => Ok(url),
        // The default address is a preference: another session may hold it.
        Err(message) if !kmp_mcp::viewer::viewer_addr_from_env().was_asked_for() => {
            eprintln!("kmp-mcp: {message}; choosing a free per-session loopback port instead");
            spawn_viewer(&kernel, "127.0.0.1:0").await
        }
        Err(message) => Err(message),
    }
    .inspect(|_| {
        // The viewer task borrows nothing from the kernel handle after
        // spawning; keep the kernel alive for the life of this process.
        std::mem::forget(kernel);
    })
}

/// Where the demo writes, and the project store it stays out of.
///
/// A team commits its project's memory, and its snapshots copy every about
/// the project store holds; a worked example has no place there. So the
/// demo never writes a project store: there it goes where a directory
/// outside any project would — the saved selection, else the per-user
/// default — and says so. `KMP_MCP_DATA_DIR` still decides when set.
struct DemoDestination {
    store: kmp_embedded::ResolvedDataDir,
    /// The project store this directory resolves to, when the demo stayed
    /// out of it.
    project_store: Option<std::path::PathBuf>,
}

impl DemoDestination {
    fn for_located(located: kmp_embedded::ResolvedDataDir) -> Result<Self, String> {
        use kmp_embedded::ResolvedDataDir;
        if !matches!(
            located,
            ResolvedDataDir::Project(_) | ResolvedDataDir::Worktree { .. }
        ) {
            return Ok(Self {
                store: located,
                project_store: None,
            });
        }
        let store = match kmp_embedded::memory_selection::saved_selection() {
            Ok(Some(selection)) => ResolvedDataDir::Saved(selection.path().to_path_buf()),
            Ok(None) => ResolvedDataDir::UserDefault(
                kmp_embedded::user_data_home()
                    .ok_or("no user data directory resolves (XDG_DATA_HOME or HOME)")?
                    .join("kmp")
                    .join("default"),
            ),
            Err(error) => {
                return Err(format!(
                    "this directory is in a project, and the demo never writes a project's \
                     memory; the user store it would use instead is unusable: {error}"
                ));
            }
        };
        Ok(Self {
            store,
            project_store: Some(located.path().to_path_buf()),
        })
    }

    fn explain(&self) {
        match &self.project_store {
            None => println!("They never enter a project's committed .kmp/memory.jsonl."),
            Some(project) => println!(
                "Not in this project's store {}: the demo never writes a project's memory, so \
                 it stays out of its committed bundle and snapshots.",
                project.display()
            ),
        }
    }
}
