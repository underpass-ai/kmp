//! Every binary a test spawns lives in a throwaway home (#910).
//!
//! A spawned `kmp-mcp` remembers each store it opens in
//! `$XDG_DATA_HOME/kmp/known-stores.jsonl`, and with no project and no
//! `KMP_MCP_DATA_DIR` it falls back to the user store under the same home.
//! Inheriting the developer's environment therefore wrote temp stores into
//! the real index and could reach real memory. This command starts from a
//! home under the test target directory instead; a test that needs its own
//! layout still sets these variables afterwards, and the later value wins.

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;

/// `program` with `HOME`, `XDG_DATA_HOME` and `XDG_CONFIG_HOME` pointed at
/// a home private to this test process.
#[allow(dead_code)]
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let home = home();
    let mut command = Command::new(program);
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
