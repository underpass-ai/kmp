//! The `kmp-mcp` executable.
//!
//! A composition root and nothing else: it reads argv, and either hands the
//! rest to a maintenance command or brings the server up and serves MCP over
//! stdio. Every decision it used to hold lives in `cli`.

mod cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let Some((command, rest)) = arguments.split_first() {
        let rest: Vec<&str> = rest.iter().map(String::as_str).collect();
        std::process::exit(cli::run(command, &rest).await);
    }

    let _log_guard = cli::startup::init_tracing();
    let server = match cli::startup::server_from_env().await {
        Ok(server) => server,
        Err(failure) => {
            cli::startup::report_failure(&failure);
            std::process::exit(2);
        }
    };
    cli::startup::announce(&server);
    cli::stdio::serve(&server).await?;
    Ok(())
}
