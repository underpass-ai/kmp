use kmp_mcp::lifecycle::LifecycleAction;

pub(crate) mod serve;

mod config;
mod document;
mod guide_verb;
mod lifecycle_verbs;
mod plugin_verb;
mod snapshot_verb;
mod summaries_verb;
mod transfer;
mod uninstall_verb;
mod viewer_verb;

use config::run_config_command;
use document::run_document_command;
use guide_verb::run_guide_command;
use lifecycle_verbs::run_lifecycle_command;
use plugin_verb::run_plugin_command;
use snapshot_verb::run_snapshot_command;
use summaries_verb::run_summaries_command;
use uninstall_verb::run_uninstall_command;
use viewer_verb::run_viewer_command;

pub(crate) async fn run_cli_command(command: &str, args: &[&str]) -> i32 {
    if is_cli_subcommand(command) && help_requested(args) {
        print_subcommand_help(command);
        return 0;
    }
    let first_argument = args.first().copied();
    match command {
        "export" | "import" => return transfer::run(command, first_argument, args).await,
        "document" => return run_document_command(args).await,
        "snapshot" => return run_snapshot_command(args).await,
        "summaries" => return run_summaries_command(args).await,
        "config" => run_config_command(args),
        "guide" => return run_guide_command(args).await,
        "plugin" => return run_plugin_command(args).await,
        "setup" => return run_lifecycle_command(LifecycleAction::Setup, args).await,
        "update" => return run_lifecycle_command(LifecycleAction::Update, args).await,
        "uninstall" => return run_uninstall_command(args).await,
        "share-memory" => {
            eprintln!("kmp-mcp: share-memory was retired; stores already use SQLite.");
            2
        }
        "viewer" => return run_viewer_command(args).await,
        "--help" | "-h" | "help" => {
            print_help();
            0
        }
        "info" => {
            print!("{}", kmp_mcp::lifecycle::info());
            0
        }
        "doctor" => {
            let (report, code) = kmp_mcp::lifecycle::doctor();
            print!("{report}");
            code
        }
        "--version" | "-V" | "version" => {
            // Format 2 is the only compiled store layout.
            use kmp_embedded::StorageEngine;
            println!(
                "kmp-mcp {} (store format {} (sqlite))",
                env!("CARGO_PKG_VERSION"),
                StorageEngine::Sqlite.format_version()
            );
            0
        }
        other => {
            eprintln!(
                "kmp-mcp: unknown command `{other}`; run without arguments for MCP \
                 stdio mode, or use `document <about> [--out FILE]` / \
                 `snapshot create|list|verify|read|merge ...` / \
                 `summaries pending [<about>] [--json]` / \
                 `config [memory-routing <mode>]` / \
                 `guide sync --plugin-root DIR [--dry-run]` / \
                 `plugin resolve-engine|notice --plugin-root DIR ...` / \
                 `setup|update [--claude] [--codex] [--version X.Y.Z] [--engine-dir DIR] \
                 [--lexical-bridge FILE | --no-lexical-bridge]` / \
                 `uninstall [--store <absolute-path> | --engine <absolute-path>] [--apply] \
                 [--purge] [--keep-memory]` / \
                 `export <file>` / `import <file>` / \
                 `viewer [addr]` / `--version` / `--help`"
            );
            2
        }
    }
}

fn is_cli_subcommand(command: &str) -> bool {
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
            | "summaries"
    )
}

fn help_requested(args: &[&str]) -> bool {
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

fn looks_like_option(argument: &str) -> bool {
    argument.starts_with('-') && argument != "-"
}

fn unknown_option(command: &str, option: &str) -> i32 {
    eprintln!("kmp-mcp {command}: unknown option `{option}`");
    eprintln!("usage: {}", subcommand_usage(command));
    2
}

fn subcommand_usage(command: &str) -> &'static str {
    match command {
        "info" => "kmp-mcp info",
        "doctor" => "kmp-mcp doctor",
        "config" => "kmp-mcp config [memory-routing <on-request|always>]",
        "document" => "kmp-mcp document <about> [--out FILE]",
        "guide" => "kmp-mcp guide sync --plugin-root DIR [--dry-run]",
        "plugin" => "kmp-mcp plugin resolve-engine|notice --plugin-root DIR",
        "setup" => {
            "kmp-mcp setup [--claude] [--codex] [--version X.Y.Z] [--engine-dir DIR] \
             [--lexical-bridge FILE | --no-lexical-bridge] [--dry-run]"
        }
        "update" => {
            "kmp-mcp update [--claude] [--codex] [--version X.Y.Z] [--engine-dir DIR] \
             [--lexical-bridge FILE | --no-lexical-bridge] [--dry-run]"
        }
        "snapshot" => "kmp-mcp snapshot create|list|verify|read|merge ...",
        "summaries" => "kmp-mcp summaries pending [<about>] [--json]",
        "uninstall" => {
            "kmp-mcp uninstall [--store <absolute-path> | --engine <absolute-path>] [--apply] \
             [--purge] [--keep-memory]"
        }
        "export" => "kmp-mcp export [file] [--about <about>]... [--repair-pending]",
        "import" => "kmp-mcp import [file]",
        "viewer" => "kmp-mcp viewer [addr]",
        _ => "kmp-mcp --help",
    }
}

fn print_subcommand_help(command: &str) {
    println!("Usage: {}", subcommand_usage(command));
    if command == "setup" || command == "update" {
        println!(
            "\nThe lexical-bridge table lets `ask` answer a question written in one language \
             from a memory written in another. It is installed once for this machine, beside \
             the stores rather than inside one, and every store reads it unless it carries a \
             table of its own. Without it `ask` matches within one language."
        );
    }
    if command == "export" {
        println!(
            "\n--about matches an opaque about exactly and may be repeated. Filtered bundles \
             preserve aggregate revisions and use bundle-local event positions starting at 1."
        );
    }
}

fn print_help() {
    println!(
        "{}\n\n\
Usage:\n  kmp-mcp                         Serve MCP over stdio\n  \
kmp-mcp info                    What this binary is and which memory it opens\n  \
kmp-mcp doctor                  Diagnose the setup and name the one thing to fix\n  \
kmp-mcp setup                   Align installed native plugins and engines\n  \
kmp-mcp update                  Update every installed KMP host as one convergence\n  \
kmp-mcp config                  Show the agent orchestration policy\n  \
kmp-mcp config memory-routing <on-request|always>\n  \
kmp-mcp guide sync --plugin-root DIR\n  \
                                Converge the two immutable shipped guides\n  \
kmp-mcp plugin resolve-engine  Select the engine matching both host manifests\n  \
kmp-mcp plugin notice          Report version drift without changing the machine\n  \
kmp-mcp document <about>        Render one about as a Markdown document\n  \
kmp-mcp snapshot <verb>         Create, verify, read or merge named snapshots\n  \
kmp-mcp summaries pending       List the memories that owe an English search summary\n  \
kmp-mcp uninstall [--store|--engine <absolute-path>] [--apply]\n  \
                                Remove one store or one engine, or preview it all\n  \
kmp-mcp export [file] [--about <about>]...  Export exact abouts or the full log\n  \
kmp-mcp export --repair-pending Acknowledge recovery after stopping writers\n  \
kmp-mcp import [file]           Import an event-log bundle\n  \
kmp-mcp viewer [addr]           Serve the local memory viewer\n  \
kmp-mcp --version               Print binary and store formats\n  \
kmp-mcp --help                  Print this help",
        kmp_mcp::banner::large(kmp_mcp::style::Style::for_stdout())
    );
}
