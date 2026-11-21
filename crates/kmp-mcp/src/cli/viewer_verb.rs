use super::serve::spawn_viewer;
use super::{looks_like_option, unknown_option};

pub(super) async fn run_viewer_command(args: &[&str]) -> i32 {
    if args.len() > 1 {
        eprintln!("kmp-mcp: viewer takes at most one address");
        return 2;
    }
    if let Some(option) = args.first().filter(|argument| looks_like_option(argument)) {
        return unknown_option("viewer", option);
    }
    // `viewer` with no argument honours the same env the MCP mode uses, read
    // the same way: `off` declines the viewer rather than naming an address.
    let addr = match args.first() {
        Some(addr) => (*addr).to_string(),
        None => match kmp_mcp::viewer::viewer_addr_from_env().addr() {
            Some(addr) => addr.to_string(),
            None => {
                eprintln!(
                    "kmp-mcp: {} turns the viewer off; unset it or pass an address, for example `kmp-mcp viewer {}`",
                    kmp_viewer::VIEWER_ADDR_ENV,
                    kmp_viewer::DEFAULT_VIEWER_ADDR
                );
                return 2;
            }
        },
    };
    let resolved = match kmp_mcp::lifecycle::resolve_memory_for_use() {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let engine = kmp_embedded::default_engine_for_data_dir(resolved.path());
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
