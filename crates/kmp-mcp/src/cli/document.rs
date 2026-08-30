use std::path::PathBuf;

use super::{looks_like_option, unknown_option};

pub(super) async fn run_document_command(args: &[&str]) -> i32 {
    let mut about = None;
    let mut out = None;
    let mut positional_only = false;
    let mut rest = args.iter();
    while let Some(argument) = rest.next() {
        match *argument {
            "--" if !positional_only => positional_only = true,
            "--out" | "-o" if !positional_only => match rest.next() {
                Some(path) => out = Some(PathBuf::from(path)),
                None => {
                    eprintln!("kmp-mcp: --out needs a file path");
                    return 2;
                }
            },
            other if !positional_only && looks_like_option(other) => {
                return unknown_option("document", other);
            }
            other if about.is_none() => about = Some(other.to_string()),
            other => {
                eprintln!("kmp-mcp: document takes one about, and `{other}` is a second one");
                return 2;
            }
        }
    }
    let Some(about) = about else {
        eprintln!(
            "kmp-mcp: document needs an about — the anchor the memory was written under, like \
             `project:kmp`. `kmp-mcp info` says which memory this directory opens."
        );
        return 2;
    };

    let resolved = match kmp_embedded::resolve_data_dir_from_env() {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let engine = match kmp_embedded::resolve_engine_for_data_dir_from_env(resolved.path()) {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let kernel = match kmp_embedded::EmbeddedKernel::open_with_engine(resolved.path(), engine) {
        Ok(kernel) => kernel,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let bundle = match kernel.store().export_bundle().await {
        Ok(bundle) => bundle,
        Err(error) => {
            eprintln!("kmp-mcp: could not read the event log: {error}");
            return 2;
        }
    };
    let document = match kmp_mcp::document::render(&bundle, &about) {
        Ok(document) => document,
        Err(message) => {
            eprintln!("kmp-mcp: {message}");
            return 2;
        }
    };

    match out {
        Some(path) => {
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
                && let Err(error) = std::fs::create_dir_all(parent)
            {
                eprintln!("kmp-mcp: could not create `{}`: {error}", parent.display());
                return 2;
            }
            if let Err(error) = std::fs::write(&path, document) {
                eprintln!("kmp-mcp: could not write `{}`: {error}", path.display());
                return 2;
            }
            // stdout carries the command result only, so a script can read it.
            println!(
                "{{\"documented\":\"{about}\",\"written_to\":\"{}\"}}",
                path.display()
            );
        }
        None => print!("{document}"),
    }
    0
}
