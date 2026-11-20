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

    let resolved = match kmp_mcp::lifecycle::resolve_memory_for_use() {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
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
        return 0;
    }

    // The demo is memory on this machine. A remote kernel is somebody's
    // shared service, and a fixture answers from canned examples: neither
    // is a store to write a worked example into.
    let server = match kmp_mcp::KernelMcpServer::try_from_env() {
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
    println!("They never enter this project's committed .kmp/memory.jsonl.");

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
    println!(
        "Ask your agent, naming KMP and the about `{}`:",
        demo::ABOUT
    );
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
    let engine = kmp_embedded::resolve_engine_for_data_dir_from_env(data_dir)
        .map_err(|error| error.to_string())?;
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
