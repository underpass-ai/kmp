//! Every binary a test spawns lives in a throwaway home (#910).
//!
//! A spawned `kmp-mcp` remembers each store it opens in
//! `$XDG_DATA_HOME/kmp/known-stores.jsonl`, with no project and no
//! `KMP_MCP_DATA_DIR` it falls back to the user store under the same home,
//! and every serve start collects old plugin versions from the Claude Code
//! and Codex caches it finds there. Inheriting the developer's environment
//! therefore wrote temp stores into the real index, could reach real memory
//! and pruned the real plugin caches. This command starts from a home under
//! the test target directory instead; a test that needs its own layout still
//! sets these variables afterwards, and the later value wins.

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;

/// Variables that point a spawned binary somewhere outside `HOME`, or pick
/// a store or engine for it. They are removed so that every location is
/// derived from the isolated `HOME`, or from the one a test sets itself.
pub const INHERITED_LOCATIONS: &[&str] = &[
    "CODEX_HOME",
    "CLAUDE_CONFIG_DIR",
    "HERMES_HOME",
    "PI_CODING_AGENT_DIR",
    "XDG_CACHE_HOME",
    "XDG_STATE_HOME",
    "KMP_MCP_DATA_DIR",
    "KMP_MCP_BACKEND",
    "KMP_MCP_ENGINE",
    "KMP_MCP_BIN",
    "KMP_INSTALL_DIR",
    "KMP_PLUGIN_ROOT",
    "KMP_KERNEL_GRPC_ENDPOINT",
];

/// `program` with `HOME`, `XDG_DATA_HOME` and `XDG_CONFIG_HOME` pointed at
/// a home private to this test process, and every other inherited location
/// removed.
#[allow(dead_code)]
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let home = home();
    let mut command = Command::new(program);
    for name in INHERITED_LOCATIONS {
        command.env_remove(name);
    }
    command
        .env("HOME", &home)
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_CONFIG_HOME", home.join(".config"));
    command
}

/// The data home an in-process server under test keeps its own state in.
#[allow(dead_code)]
pub fn data_home() -> PathBuf {
    home().join(".local/share")
}

fn home() -> PathBuf {
    let home = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("isolated-home")
        .join(std::process::id().to_string());
    std::fs::create_dir_all(&home).expect("isolated test home");
    home
}
