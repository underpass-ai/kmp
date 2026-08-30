//! The executable's thin composition root: one argument means a CLI verb,
//! none means an MCP stdio session. Everything either path composes lives
//! under `cli/`.

mod cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli_args: Vec<String> = std::env::args().skip(1).collect();
    if let Some((command, rest)) = cli_args.split_first() {
        let rest: Vec<&str> = rest.iter().map(String::as_str).collect();
        std::process::exit(cli::run_cli_command(command, &rest).await);
    }
    cli::serve::serve().await
}
