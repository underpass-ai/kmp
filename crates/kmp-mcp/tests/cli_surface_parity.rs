//! What the executable says about itself, pinned.
//!
//! #404 moves the CLI out of a 1,561-line `main.rs` into one module per
//! command. The tool surface is already pinned byte for byte, but the usage
//! text was not: a carve that dropped an option from a usage line, or renamed
//! a verb in the help and nowhere else, changed what every user reads and
//! nothing said so.
//!
//! The binary is run for real — the same door a person uses — and its answers
//! are compared against checked-in files. When a change is genuinely intended,
//! regenerate with
//! `KMP_BLESS_CLI_SURFACE=1 cargo test -p kmp-mcp --test cli_surface_parity`
//! and review the diff as the interface change it is.

use std::path::{Path, PathBuf};
use std::process::Command;

const BLESS: &str = "KMP_BLESS_CLI_SURFACE";

/// Every verb the executable accepts. A verb that stops being advertised, or
/// starts being advertised without a usage line, fails here.
const SUBCOMMANDS: [&str; 13] = [
    "config",
    "doctor",
    "document",
    "export",
    "guide",
    "import",
    "info",
    "plugin",
    "setup",
    "snapshot",
    "uninstall",
    "update",
    "viewer",
];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/contract/cli")
}

/// Runs the binary with a clean environment, so the pinned text is the
/// executable's own and not a reflection of this machine.
fn help(arguments: &[&str]) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kmp-mcp"));
    command.args(arguments);
    for name in ["KMP_MCP_DATA_DIR", "KMP_MCP_BACKEND", "KMP_VIEWER_ADDR"] {
        command.env_remove(name);
    }
    let output = command.output().expect("the binary should run");
    assert!(
        output.status.success(),
        "`kmp-mcp {}` exited with {}",
        arguments.join(" "),
        output.status
    );
    assert!(
        output.stderr.is_empty(),
        "help belongs on stdout: `kmp-mcp {}` also wrote {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("help is UTF-8")
}

fn pin(path: &Path, actual: &str, label: &str) {
    if std::env::var_os(BLESS).is_some() {
        std::fs::create_dir_all(path.parent().expect("fixture directory"))
            .expect("fixture directory");
        std::fs::write(path, actual).expect("write fixture");
        return;
    }
    let expected = std::fs::read_to_string(path).unwrap_or_else(|error| {
        panic!(
            "{label} has no reviewed fixture at {}: {error}\nbless it with {BLESS}=1",
            path.display()
        )
    });
    assert_eq!(actual, expected, "{label} changed; bless it with {BLESS}=1");
}

#[test]
fn the_top_level_help_matches_its_reviewed_fixture() {
    pin(
        &fixtures().join("help.txt"),
        &help(&["--help"]),
        "the top-level help",
    );
}

#[test]
fn every_subcommand_help_matches_its_reviewed_fixture() {
    for command in SUBCOMMANDS {
        pin(
            &fixtures().join(format!("{command}.txt")),
            &help(&[command, "--help"]),
            &format!("`kmp-mcp {command} --help`"),
        );
    }
}

/// A pinned set is only worth pinning while it covers the whole surface.
/// Without this, a build that stopped advertising a verb would pin a smaller
/// set and pass forever after.
#[test]
fn the_pinned_subcommands_are_the_ones_the_help_advertises() {
    let advertised = help(&["--help"]);
    for command in SUBCOMMANDS {
        assert!(
            advertised.contains(command),
            "`{command}` is pinned but the top-level help does not name it"
        );
    }
}
