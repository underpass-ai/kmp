pub(super) async fn run_snapshot_command(args: &[&str]) -> i32 {
    let Some((verb, rest)) = args.split_first() else {
        eprintln!(
            "kmp-mcp: snapshot needs create <name>, list, verify <name>, read <name> <tool> \
             <arguments-json>, or merge <left> <right> <name>"
        );
        return 2;
    };
    let resolved = match if *verb == "create" {
        kmp_embedded::resolve_data_dir_from_env()
    } else {
        kmp_embedded::locate_data_dir_from_env()
    } {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };

    match *verb {
        "create" => snapshot_create(&resolved, rest).await,
        "list" => snapshot_list(&resolved, rest),
        "verify" => snapshot_verify(&resolved, rest),
        "read" => snapshot_read(&resolved, rest).await,
        "merge" => snapshot_merge(&resolved, rest),
        other => {
            eprintln!("kmp-mcp: snapshot has no `{other}` verb");
            2
        }
    }
}

pub(super) async fn snapshot_create(
    resolved: &kmp_embedded::ResolvedDataDir,
    args: &[&str],
) -> i32 {
    let [name] = args else {
        eprintln!("kmp-mcp: snapshot create takes exactly one name");
        return 2;
    };
    let path = match kmp_mcp::snapshot::path_for_name(resolved, name) {
        Ok(path) => path,
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
    let pulse = kmp_mcp::pulse::Pulse::start("pinning this moment…");
    let exported = kernel.store().export_named_bundle(name).await;
    pulse.clear();
    let bundle = match exported {
        Ok(bundle) => bundle,
        Err(error) => {
            eprintln!("kmp-mcp: snapshot create failed: {error}");
            return 2;
        }
    };
    match write_named_snapshot(&path, &bundle) {
        Ok(header) => {
            kmp_mcp::pulse::mark_done(&format!("pinned as `{name}`"));
            println!("{}", snapshot_result(&path, &header));
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            2
        }
    }
}

pub(super) fn snapshot_list(resolved: &kmp_embedded::ResolvedDataDir, args: &[&str]) -> i32 {
    if !args.is_empty() {
        eprintln!("kmp-mcp: snapshot list takes no arguments");
        return 2;
    }
    match kmp_mcp::snapshot::list(resolved) {
        Ok(snapshots) => {
            let snapshots: Vec<_> = snapshots
                .into_iter()
                .map(|(path, header)| snapshot_result(&path, &header))
                .collect();
            println!("{}", serde_json::json!({"snapshots": snapshots}));
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            2
        }
    }
}

pub(super) fn snapshot_verify(resolved: &kmp_embedded::ResolvedDataDir, args: &[&str]) -> i32 {
    let [name] = args else {
        eprintln!("kmp-mcp: snapshot verify takes exactly one name");
        return 2;
    };
    let path = match kmp_mcp::snapshot::path_for_name(resolved, name) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    match kmp_mcp::snapshot::read_header(&path) {
        Ok(header) => {
            println!("{}", snapshot_result(&path, &header));
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            2
        }
    }
}

pub(super) async fn snapshot_read(resolved: &kmp_embedded::ResolvedDataDir, args: &[&str]) -> i32 {
    let [name, tool, raw_arguments] = args else {
        eprintln!("kmp-mcp: snapshot read takes <name> <read-tool> <arguments-json>");
        return 2;
    };
    let path = match kmp_mcp::snapshot::path_for_name(resolved, name) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let bundle = match std::fs::read_to_string(&path) {
        Ok(bundle) => bundle,
        Err(error) => {
            eprintln!("kmp-mcp: could not read `{}`: {error}", path.display());
            return 2;
        }
    };
    let arguments: serde_json::Value = match serde_json::from_str(raw_arguments) {
        Ok(serde_json::Value::Object(arguments)) => serde_json::Value::Object(arguments),
        Ok(_) => {
            eprintln!("kmp-mcp: snapshot read arguments must be a JSON object");
            return 2;
        }
        Err(error) => {
            eprintln!("kmp-mcp: snapshot read arguments are not valid JSON: {error}");
            return 2;
        }
    };
    match kmp_mcp::snapshot::read_only(&bundle, tool, arguments).await {
        Ok(response) => {
            let failed = response.get("error").is_some()
                || response["result"]["isError"].as_bool() == Some(true);
            println!("{response}");
            if failed { 1 } else { 0 }
        }
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            2
        }
    }
}

pub(super) fn snapshot_merge(resolved: &kmp_embedded::ResolvedDataDir, args: &[&str]) -> i32 {
    let [left, right, name] = args else {
        eprintln!("kmp-mcp: snapshot merge takes <left> <right> <new-name>");
        return 2;
    };
    let path = |name: &str| kmp_mcp::snapshot::path_for_name(resolved, name);
    let (left_path, right_path, output_path) = match (path(left), path(right), path(name)) {
        (Ok(left), Ok(right), Ok(output)) => (left, right, output),
        (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let read = |path: &std::path::Path| {
        std::fs::read_to_string(path)
            .map_err(|error| format!("could not read `{}`: {error}", path.display()))
    };
    let (left_bundle, right_bundle) = match (read(&left_path), read(&right_path)) {
        (Ok(left), Ok(right)) => (left, right),
        (Err(error), _) | (_, Err(error)) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let merged = match kmp_embedded::merge_bundles(&left_bundle, &right_bundle, name) {
        Ok(merged) => merged,
        Err(error) => {
            eprintln!("kmp-mcp: snapshot merge refused: {error}");
            return 2;
        }
    };
    match write_named_snapshot(&output_path, &merged) {
        Ok(header) => {
            println!("{}", snapshot_result(&output_path, &header));
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            2
        }
    }
}

pub(super) fn write_named_snapshot(
    path: &std::path::Path,
    bundle: &str,
) -> Result<kmp_embedded::BundleHeader, String> {
    let header = kmp_embedded::verify_bundle(bundle).map_err(|error| error.to_string())?;
    let created =
        kmp_embedded::write_bundle_if_absent(path, bundle).map_err(|error| error.to_string())?;
    if !created {
        let existing = kmp_mcp::snapshot::read_header(path)?;
        if existing.content_digest == header.content_digest {
            return Ok(existing);
        }
        return Err(format!(
            "snapshot `{}` already identifies digest {}; choose a new name instead of rewriting \
             a recovery point",
            existing.snapshot_id, existing.content_digest
        ));
    }
    Ok(header)
}

pub(super) fn snapshot_result(
    path: &std::path::Path,
    header: &kmp_embedded::BundleHeader,
) -> serde_json::Value {
    serde_json::json!({
        "snapshot_id": header.snapshot_id,
        "created_at_unix_ms": header.created_at_unix_ms,
        "event_range": header.event_range,
        "event_count": header.event_count,
        "abouts": header.abouts,
        "content_digest": header.content_digest,
        "path": path.display().to_string(),
    })
}
