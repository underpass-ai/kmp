//! `kmp-mcp import --from <store-dir|bundle-file> --about <about>...`: bring
//! exact abouts of another workspace's memory into the store `kmp-mcp`
//! resolves here, without replacing what this store already holds.

use std::path::{Path, PathBuf};

use super::{looks_like_option, unknown_option};

/// Whether `import` was asked for the live-store form rather than the
/// empty-store replay of one bundle.
pub(super) fn requested(args: &[&str]) -> bool {
    args.iter()
        .take_while(|argument| **argument != "--")
        .any(|argument| matches!(*argument, "--from" | "--about"))
}

pub(super) async fn run(args: &[&str]) -> i32 {
    let (from, abouts) = match parse(args) {
        Ok(parsed) => parsed,
        Err(code) => return code,
    };
    let source = match read_source(&from, &abouts).await {
        Ok(source) => source,
        Err(error) => {
            eprintln!("kmp-mcp: import refused; nothing was written. {error}");
            return 2;
        }
    };

    // The destination is opened exactly as plain `import` opens it: the store
    // this directory resolves to, under the engine it was created with.
    let resolved = match kmp_mcp::lifecycle::resolve_memory_for_use() {
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

    let pulse = kmp_mcp::pulse::Pulse::start("bringing those abouts in…");
    let imported = kernel
        .store()
        .import_abouts(
            &source,
            &abouts,
            kmp_application::projection_mutations_for_context_event,
        )
        .await;
    pulse.clear();
    match imported {
        Ok(report) => {
            kmp_mcp::pulse::mark_done(&match report.events_imported {
                0 => "already here — nothing to add".to_string(),
                count => format!("in — {} appended", super::transfer::events(count)),
            });
            println!(
                "{}",
                serde_json::json!({
                    "data_dir": resolved.path().display().to_string(),
                    "from": from.display().to_string(),
                    "abouts": report.abouts.keys().collect::<Vec<_>>(),
                    "events_imported": report.events_imported,
                    "mutations_applied": report.mutations_applied,
                    "aggregates": report
                        .abouts
                        .iter()
                        .map(|(about, outcome)| (about.clone(), serde_json::Value::from(outcome.as_str())))
                        .collect::<serde_json::Map<_, _>>(),
                })
            );
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp: import failed: {error}");
            2
        }
    }
}

fn parse(args: &[&str]) -> Result<(PathBuf, Vec<String>), i32> {
    let mut from = None;
    let mut abouts = Vec::new();
    let mut arguments = args.iter();
    while let Some(argument) = arguments.next() {
        match *argument {
            "--from" => match arguments.next() {
                Some(path) if !path.is_empty() => {
                    if from.replace(PathBuf::from(path)).is_some() {
                        eprintln!("kmp-mcp: import takes one --from");
                        return Err(2);
                    }
                }
                _ => {
                    eprintln!("kmp-mcp: import --from needs a store directory or a bundle file");
                    return Err(2);
                }
            },
            "--about" => match arguments.next() {
                Some(about) if !about.is_empty() => abouts.push((*about).to_string()),
                _ => {
                    eprintln!("kmp-mcp: import --about needs a non-empty about");
                    return Err(2);
                }
            },
            other if looks_like_option(other) => return Err(unknown_option("import", other)),
            other => {
                eprintln!(
                    "kmp-mcp: import --from takes no bundle path `{other}`; name the source with \
                     --from"
                );
                return Err(2);
            }
        }
    }
    let Some(from) = from else {
        eprintln!(
            "kmp-mcp: import --about needs --from <store-dir|bundle-file>: the memory the \
             abouts come from"
        );
        return Err(2);
    };
    if abouts.is_empty() {
        eprintln!(
            "kmp-mcp: import --from needs at least one --about; a live store only takes the \
             abouts you name"
        );
        return Err(2);
    }
    Ok((from, abouts))
}

/// The source as a bundle: a file is read as one; a store is opened
/// read-only and exported through the filtered export path, which refuses an
/// about the store does not hold before anything else happens.
async fn read_source(from: &Path, abouts: &[String]) -> Result<String, String> {
    if from.is_file() {
        return std::fs::read_to_string(from)
            .map_err(|error| format!("could not read `{}`: {error}", from.display()));
    }
    if !from.is_dir() {
        return Err(format!(
            "`{}` is neither a KMP store directory nor a bundle file",
            from.display()
        ));
    }
    // A workspace path is accepted for the store it keeps in `.kernel/`.
    let data_dir = if from.join("FORMAT_VERSION").is_file() {
        from.to_path_buf()
    } else if from.join(".kernel").join("FORMAT_VERSION").is_file() {
        from.join(".kernel")
    } else {
        return Err(format!(
            "`{}` is not a KMP store: neither it nor its `.kernel/` has a FORMAT_VERSION",
            from.display()
        ));
    };
    let store = kmp_embedded::EmbeddedKernelStore::open_read_only(&data_dir)
        .map_err(|error| error.to_string())?;
    store
        .export_bundle_for_abouts(abouts)
        .await
        .map_err(|error| error.to_string())
}
