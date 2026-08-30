//! What the executable says about itself.
//!
//! One concept: usage text and the argument shapes that are wrong before any
//! command runs. Every command's usage line lives here together, because a
//! reader comparing two of them should not have to open two files.

pub(super) fn help_requested(args: &[&str]) -> bool {
    for argument in args {
        if *argument == "--" {
            break;
        }
        if matches!(*argument, "--help" | "-h") {
            return true;
        }
    }
    false
}

pub(super) fn looks_like_option(argument: &str) -> bool {
    argument.starts_with('-') && argument != "-"
}

pub(super) fn unknown_option(command: &str, option: &str) -> i32 {
    eprintln!("kmp-mcp {command}: unknown option `{option}`");
    eprintln!("usage: {}", subcommand_usage(command));
    2
}

pub(super) fn subcommand_usage(command: &str) -> &'static str {
    match command {
        "info" => "kmp-mcp info",
        "doctor" => "kmp-mcp doctor",
        "config" => "kmp-mcp config [ask-fallback-languages <tags|none>]",
        "document" => "kmp-mcp document <about> [--out FILE]",
        "guide" => "kmp-mcp guide sync --plugin-root DIR [--dry-run]",
        "plugin" => "kmp-mcp plugin resolve-engine|notice --plugin-root DIR",
        "setup" => {
            "kmp-mcp setup [--claude] [--codex] [--version X.Y.Z] [--engine-dir DIR] [--dry-run]"
        }
        "update" => {
            "kmp-mcp update [--claude] [--codex] [--version X.Y.Z] [--engine-dir DIR] [--dry-run]"
        }
        "snapshot" => "kmp-mcp snapshot create|list|verify|read|merge ...",
        "uninstall" => {
            "kmp-mcp uninstall [--store <absolute-path>] [--apply] [--purge] [--keep-memory]"
        }
        "export" => "kmp-mcp export [file] [--about <about>]... [--repair-pending]",
        "import" => "kmp-mcp import [file]",
        "viewer" => "kmp-mcp viewer [addr]",
        _ => "kmp-mcp --help",
    }
}

pub(super) fn print_subcommand_help(command: &str) {
    println!("Usage: {}", subcommand_usage(command));
    if command == "export" {
        println!(
            "\n--about matches an opaque about exactly and may be repeated. Filtered bundles \
             preserve aggregate revisions and use bundle-local event positions starting at 1."
        );
    }
}

pub(super) fn print_help() {
    println!(
        "{}\n\n\
Usage:\n  kmp-mcp                         Serve MCP over stdio\n  \
kmp-mcp info                    What this binary is and which memory it opens\n  \
kmp-mcp doctor                  Diagnose the setup and name the one thing to fix\n  \
kmp-mcp setup                   Align installed native plugins and engines\n  \
kmp-mcp update                  Update every installed KMP host as one convergence\n  \
kmp-mcp config                  Show the agent orchestration policy\n  \
kmp-mcp config ask-fallback-languages <tags|none>\n  \
kmp-mcp guide sync --plugin-root DIR\n  \
                                Converge the two immutable shipped guides\n  \
kmp-mcp plugin resolve-engine  Select the engine matching both host manifests\n  \
kmp-mcp plugin notice          Report version drift without changing the machine\n  \
kmp-mcp document <about>        Render one about as a Markdown document\n  \
kmp-mcp snapshot <verb>         Create, verify, read or merge named snapshots\n  \
kmp-mcp uninstall [--store <absolute-path>] [--apply]\n  \
                                Remove one store, or preview the whole installation\n  \
kmp-mcp export [file] [--about <about>]...  Export exact abouts or the full log\n  \
kmp-mcp export --repair-pending Acknowledge recovery after stopping writers\n  \
kmp-mcp import [file]           Import an event-log bundle\n  \
kmp-mcp viewer [addr]           Serve the local memory viewer\n  \
kmp-mcp --version               Print binary and store formats\n  \
kmp-mcp --help                  Print this help",
        kmp_mcp::banner::large(kmp_mcp::style::Style::for_stdout())
    );
}

/// Names every verb, because a mistyped one is the moment a person most needs
/// the list and least wants to run a second command to see it.
pub(super) fn unknown_command(command: &str) {
    eprintln!(
        "kmp-mcp: unknown command `{command}`; run without arguments for MCP \
         stdio mode, or use `document <about> [--out FILE]` / \
         `snapshot create|list|verify|read|merge ...` / \
         `config [ask-fallback-languages <tags>]` / \
         `guide sync --plugin-root DIR [--dry-run]` / \
         `plugin resolve-engine|notice --plugin-root DIR ...` / \
         `setup|update [--claude] [--codex] [--version X.Y.Z] [--engine-dir DIR]` / \
         `uninstall [--store <absolute-path>] [--apply] [--purge] [--keep-memory]` / \
         `export <file>` / `import <file>` / \
         `viewer [addr]` / `--version` / `--help`"
    );
}
