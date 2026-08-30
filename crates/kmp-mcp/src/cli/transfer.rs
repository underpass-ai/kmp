use std::path::{Path, PathBuf};

use super::{looks_like_option, unknown_option};

/// `export` and `import`: the whole event log, in order, as one file.
pub(super) async fn run(command: &str, first_argument: Option<&str>, args: &[&str]) -> i32 {
    let (path, repair_pending, abouts) = if command == "export" {
        let mut path = None;
        let mut repair_pending = false;
        let mut abouts = Vec::new();
        let mut arguments = args.iter();
        while let Some(argument) = arguments.next() {
            match *argument {
                "--repair-pending" => repair_pending = true,
                "--about" => match arguments.next() {
                    Some(about) if !about.is_empty() => abouts.push((*about).to_string()),
                    Some(_) => {
                        eprintln!("kmp-mcp: export --about needs a non-empty about");
                        return 2;
                    }
                    None => {
                        eprintln!("kmp-mcp: export --about needs an about");
                        return 2;
                    }
                },
                other if looks_like_option(other) => {
                    return unknown_option("export", other);
                }
                other => {
                    if path.replace(other).is_some() {
                        eprintln!(
                            "kmp-mcp: export takes at most one file path, repeatable --about, and \
                             optional --repair-pending"
                        );
                        return 2;
                    }
                }
            }
        }
        (path, repair_pending, abouts)
    } else {
        if args.len() > 1 {
            eprintln!("kmp-mcp: import takes at most one file path");
            return 2;
        }
        if let Some(argument) = first_argument
            && looks_like_option(argument)
        {
            return unknown_option("import", argument);
        }
        (first_argument, false, Vec::new())
    };

    if command == "export"
        && path
            .map(Path::new)
            .and_then(Path::file_name)
            .is_some_and(|name| name.to_string_lossy().starts_with('-'))
    {
        let path = path.expect("checked export path is present");
        eprintln!(
            "kmp-mcp: export refuses destination `{path}` because its basename begins with `-`; \
             choose an unambiguous file name"
        );
        return 2;
    }

    let resolved = match kmp_embedded::resolve_data_dir_from_env() {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    // No path means the project's committed copy. Only a project-scoped store
    // has one — an explicit data dir or the per-user default belongs to no
    // repository, and picking a file for them would write memory somewhere
    // the operator did not choose.
    let path = match path.map(PathBuf::from) {
        Some(path) => path,
        None => match kmp_embedded::project_bundle_path(&resolved) {
            Some(path) => path,
            None => {
                eprintln!(
                    "kmp-mcp: {command} needs a bundle file path here. The default \
                     `{}` is the project's committed memory, and this store is not \
                     project-scoped: it resolved to `{}` by the `{}` rule.",
                    kmp_embedded::PROJECT_BUNDLE_PATH,
                    resolved.path().display(),
                    resolved.rule_name()
                );
                return 2;
            }
        },
    };
    let path = path.as_path();
    let is_project_head = kmp_embedded::project_bundle_path(&resolved).as_deref() == Some(path);
    if repair_pending && !abouts.is_empty() {
        eprintln!(
            "kmp-mcp: --repair-pending requires a full-store export; it cannot be combined \
             with --about"
        );
        return 2;
    }
    if repair_pending && !is_project_head {
        eprintln!(
            "kmp-mcp: --repair-pending applies only to the project head \
             `.kmp/memory.jsonl`, not an explicit export path"
        );
        return 2;
    }
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
    let store = kernel.store();

    match command {
        "export" => {
            // The pulse holds the line only while the store is actually
            // read; it is erased before anything else prints.
            let pulse = kmp_mcp::pulse::Pulse::start("saving your memory…");
            let exported = if abouts.is_empty() {
                store.export_bundle().await
            } else {
                store.export_bundle_for_abouts(&abouts).await
            };
            pulse.clear();
            match exported {
                Ok(bundle) => {
                    // `.kmp/` will not exist on the first save, and failing on
                    // that would make the convention useless exactly once per
                    // repository — at the moment someone tries it.
                    if let Some(parent) = path.parent()
                        && !parent.as_os_str().is_empty()
                        && let Err(error) = std::fs::create_dir_all(parent)
                    {
                        eprintln!("kmp-mcp: could not create `{}`: {error}", parent.display());
                        return 2;
                    }
                    let header = match kmp_embedded::verify_bundle(&bundle) {
                        Ok(header) => header,
                        Err(error) => {
                            eprintln!("kmp-mcp: generated bundle did not verify: {error}");
                            return 2;
                        }
                    };
                    if let Err(error) = kmp_embedded::write_bundle_atomically(path, &bundle) {
                        eprintln!("kmp-mcp: could not write `{}`: {error}", path.display());
                        return 2;
                    }
                    if repair_pending
                        && let Err(error) =
                            kmp_embedded::clear_pending_bundle_exports(resolved.path())
                    {
                        eprintln!(
                            "kmp-mcp: bundle was exported, but pending markers could not be \
                         cleared: {error}"
                        );
                        return 2;
                    }
                    kmp_mcp::pulse::mark_done(&match header.event_count {
                        0 => "saved — an empty log, ready to grow".to_string(),
                        count => format!("saved — {}, every one in order", events(count)),
                    });
                    println!(
                        "{}",
                        serde_json::json!({
                            "exported_to": path.display().to_string(),
                            "data_dir": resolved.path().display().to_string(),
                            "snapshot_id": header.snapshot_id,
                            "event_count": header.event_count,
                            "abouts": header.abouts,
                            "content_digest": header.content_digest,
                        })
                    );
                    let pending = if is_project_head {
                        kmp_embedded::pending_bundle_exports(resolved.path()).len()
                    } else {
                        0
                    };
                    if pending > 0 {
                        eprintln!(
                            "kmp-mcp: {pending} pending write marker(s) remain. Stop other KMP \
                         sessions, inspect this exported bundle, then run `kmp-mcp export \
                         --repair-pending` to acknowledge recovery safely."
                        );
                        1
                    } else {
                        0
                    }
                }
                Err(error) => {
                    eprintln!("kmp-mcp: export failed: {error}");
                    2
                }
            }
        }
        _ => {
            let bundle = match std::fs::read_to_string(path) {
                Ok(bundle) => bundle,
                Err(error) => {
                    eprintln!("kmp-mcp: could not read `{}`: {error}", path.display());
                    return 2;
                }
            };
            let pulse = kmp_mcp::pulse::Pulse::start("bringing your memory back…");
            let imported = store
                .import_bundle(
                    &bundle,
                    kmp_application::projection_mutations_for_context_event,
                )
                .await;
            pulse.clear();
            match imported {
                Ok(report) => {
                    kmp_mcp::pulse::mark_done(&match report.events_imported {
                        0 => "back — nothing to replay yet".to_string(),
                        count => format!("back — {} replayed", events(count)),
                    });
                    println!(
                        "{{\"events_imported\":{},\"mutations_applied\":{}}}",
                        report.events_imported, report.rebuild.mutations_applied
                    );
                    0
                }
                Err(error) => {
                    eprintln!("kmp-mcp: import failed: {error}");
                    2
                }
            }
        }
    }
}

pub(super) fn events(count: u64) -> String {
    if count == 1 {
        "1 event".to_string()
    } else {
        format!("{count} events")
    }
}
