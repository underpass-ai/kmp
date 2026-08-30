//! `kmp-mcp viewer [addr]` — serve ChronoLoom over the selected store.

use super::help::{looks_like_option, unknown_option};
use super::startup::spawn_viewer;

/// Standalone viewer over the env-resolved data dir (same resolution as
/// `export`/`import`). SQLite can be shared, but setting `KMP_VIEWER_ADDR` on
/// the agent session remains the direct path to its already-open kernel.
pub(super) async fn run(args: &[&str]) -> i32 {
    if args.len() > 1 {
        eprintln!("kmp-mcp: viewer takes at most one address");
        return 2;
    }
    if let Some(option) = args.first().filter(|argument| looks_like_option(argument)) {
        return unknown_option("viewer", option);
    }
    // `viewer` with no argument honours the same env the MCP mode uses.
    let addr = args
        .first()
        .copied()
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
