//! The MCP stdio loop.
//!
//! One concept: reading JSON-RPC lines from stdin and writing answers to
//! stdout. Blank lines are skipped and a request the server chooses not to
//! answer produces no line, because stdout is the protocol and anything else
//! on it is a parse error for the host.

use std::io::{self, BufRead, Write};

use kmp_mcp::KernelMcpServer;

pub async fn serve(server: &KernelMcpServer) -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut stdin = stdin.lock();
    let mut line = Vec::new();
    loop {
        line.clear();
        if stdin.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }

        if let Some(response) = server.handle_json_bytes(&line).await {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}
