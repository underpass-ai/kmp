//! Everything the `kmp-mcp` executable does that the library does not.
//!
//! One concept per module: one file per command, plus `help` for what the
//! executable says about itself, `startup` for bringing the server up from the
//! environment, and `stdio` for the loop that serves MCP once it is up.
//!
//! This tree belongs to the binary, not to `kmp-mcp` the library. `main.rs`
//! declares it and nothing else does, so a maintenance verb can grow here
//! without widening what the published crate promises.

mod config;
mod document;
mod guide;
mod help;
mod lifecycle;
mod plugin;
mod snapshot;
pub mod startup;
pub mod stdio;
mod transfer;
mod uninstall;
mod viewer;

use help::{help_requested, print_help, print_subcommand_help};
use kmp_embedded::StorageEngine;
use kmp_mcp::lifecycle::LifecycleAction;

/// Non-MCP maintenance surface (everything is a process — no library):
/// `export <file>` and `import <file>` move the append-only event log between
/// embedded stores, and `viewer [addr]` serves the local web viewer over the
/// store; stdout carries the command result only.
pub async fn run(command: &str, args: &[&str]) -> i32 {
    if is_cli_subcommand(command) && help_requested(args) {
        print_subcommand_help(command);
        return 0;
    }
    match command {
        "export" | "import" => transfer::run(command, args).await,
        "document" => document::run(args).await,
        "snapshot" => snapshot::run(args).await,
        "config" => config::run(args),
        "guide" => guide::run(args).await,
        "plugin" => plugin::run(args).await,
        "setup" => lifecycle::run(LifecycleAction::Setup, args).await,
        "update" => lifecycle::run(LifecycleAction::Update, args).await,
        "uninstall" => uninstall::run(args).await,
        "viewer" => viewer::run(args).await,
        "share-memory" => {
            eprintln!("kmp-mcp: share-memory was retired; stores already use SQLite.");
            2
        }
        "--help" | "-h" | "help" => {
            print_help();
            0
        }
        "info" => {
            print!("{}", kmp_mcp::diagnostics::info());
            0
        }
        "doctor" => {
            let (report, code) = kmp_mcp::diagnostics::doctor();
            print!("{report}");
            code
        }
        "--version" | "-V" | "version" => {
            // Format 2 is the only compiled store layout.
            println!(
                "kmp-mcp {} (store format {} (sqlite))",
                env!("CARGO_PKG_VERSION"),
                StorageEngine::Sqlite.format_version()
            );
            0
        }
        other => {
            help::unknown_command(other);
            2
        }
    }
}

pub(super) fn is_cli_subcommand(command: &str) -> bool {
    matches!(
        command,
        "info"
            | "doctor"
            | "config"
            | "document"
            | "guide"
            | "plugin"
            | "setup"
            | "update"
            | "snapshot"
            | "uninstall"
            | "export"
            | "import"
            | "viewer"
    )
}
