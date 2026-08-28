use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use kmp_mcp::{KernelMcpGrpcTlsConfig, KernelMcpServer};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone)]
struct Args {
    run: PathBuf,
    endpoint: Option<String>,
    output: Option<PathBuf>,
    expected_run_id: Option<String>,
    limit: Option<usize>,
    offset: usize,
    log_progress_every: Option<usize>,
    progress_output: Option<PathBuf>,
    inspect: bool,
    force: bool,
}

#[derive(Debug, Default)]
struct RefInventory {
    about_refs: BTreeSet<String>,
    entry_refs: BTreeSet<String>,
    connect_refs: BTreeSet<String>,
    read_context_refs: BTreeSet<String>,
    observed_refs: BTreeSet<String>,
    evidence_refs: BTreeSet<String>,
    dimension_refs: BTreeSet<String>,
}

#[derive(Debug, Serialize)]
struct AuditSummary {
    benchmark: &'static str,
    audit: &'static str,
    generated_at_unix_seconds: u64,
    run: String,
    endpoint: Option<String>,
    expected_run_id: Option<String>,
    writer_rows: usize,
    results_rows: usize,
    counts: AuditCounts,
    run_ids: BTreeMap<String, usize>,
    mixed_run_ids: bool,
    foreign_refs: Vec<String>,
    inspect: Option<InspectSummary>,
    elapsed_ms: u128,
}

#[derive(Debug, Serialize)]
struct AuditCounts {
    about_refs: usize,
    entry_refs: usize,
    connect_refs: usize,
    read_context_refs: usize,
    observed_refs: usize,
    evidence_refs: usize,
    dimension_refs: usize,
    projected_node_refs: usize,
}

#[derive(Debug, Serialize)]
struct InspectSummary {
    total_refs: usize,
    offset: usize,
    limit: Option<usize>,
    checked_refs: usize,
    found_refs: usize,
    missing_refs: usize,
    errored_refs: usize,
    next_offset: usize,
    completed: bool,
    missing: Vec<String>,
    errors: Vec<InspectError>,
}

#[derive(Debug, Serialize)]
struct InspectError {
    r#ref: String,
    error: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
    let started = Instant::now();
    let (writer_rows, inventory) = read_writer_inventory(&args.run.join("writer_results.jsonl"))?;
    let (results_rows, ask_observed_refs) =
        read_results_observed_refs(&args.run.join("results.jsonl"))?;

    let mut inventory = inventory;
    inventory.observed_refs.extend(ask_observed_refs);

    let projected_refs = projected_refs(&inventory);
    let run_ids = run_id_counts(&projected_refs);
    let expected_run_id = args
        .expected_run_id
        .clone()
        .or_else(|| single_run_id(&run_ids));
    let foreign_refs = expected_run_id
        .as_deref()
        .map(|expected| foreign_refs(&projected_refs, expected))
        .unwrap_or_default();

    let inspect = if args.inspect {
        let endpoint = args
            .endpoint
            .as_deref()
            .ok_or("--inspect requires --endpoint")?;
        Some(inspect_refs(endpoint, &projected_refs, &inventory.about_refs, &args).await?)
    } else {
        None
    };

    let summary = AuditSummary {
        benchmark: "MemoryArena",
        audit: "memoryarena-kmp-run-audit-v1",
        generated_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        run: args.run.display().to_string(),
        endpoint: args.endpoint.clone(),
        expected_run_id,
        writer_rows,
        results_rows,
        counts: AuditCounts {
            about_refs: inventory.about_refs.len(),
            entry_refs: inventory.entry_refs.len(),
            connect_refs: inventory.connect_refs.len(),
            read_context_refs: inventory.read_context_refs.len(),
            observed_refs: inventory.observed_refs.len(),
            evidence_refs: inventory.evidence_refs.len(),
            dimension_refs: inventory.dimension_refs.len(),
            projected_node_refs: projected_refs.len(),
        },
        mixed_run_ids: run_ids.len() > 1 || !foreign_refs.is_empty(),
        run_ids,
        foreign_refs,
        inspect,
        elapsed_ms: started.elapsed().as_millis(),
    };

    if let Some(output) = args.output.as_deref() {
        write_output(output, &summary, args.force)?;
    }
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn read_writer_inventory(
    path: &Path,
) -> Result<(usize, RefInventory), Box<dyn Error + Send + Sync>> {
    let mut inventory = RefInventory::default();
    let mut rows = 0usize;
    for value in read_jsonl(path)? {
        rows += 1;
        collect_string_field(&value, "about", &mut inventory.about_refs);
        collect_string_field(&value, "entry_ref", &mut inventory.entry_refs);
        collect_path_strings(
            &value,
            &["write_request", "connect_to"],
            "ref",
            &mut inventory.connect_refs,
        );
        collect_array_strings(
            &value,
            &["write_request", "read_context", "temporal_refs"],
            &mut inventory.read_context_refs,
        );
        collect_array_strings(
            &value,
            &["write_request", "read_context", "inspected_refs"],
            &mut inventory.read_context_refs,
        );
        collect_pre_read_observed_refs(&value, &mut inventory.observed_refs);
        collect_array_strings_from_items(
            &value,
            &["dry_run_content", "ingest_preview", "memory", "evidence"],
            "id",
            &mut inventory.evidence_refs,
        );
        collect_coordinate_scope_refs(&value, &mut inventory.dimension_refs);
    }
    Ok((rows, inventory))
}

fn read_results_observed_refs(
    path: &Path,
) -> Result<(usize, BTreeSet<String>), Box<dyn Error + Send + Sync>> {
    if !path.exists() {
        return Ok((0, BTreeSet::new()));
    }
    let mut rows = 0usize;
    let mut refs = BTreeSet::new();
    for value in read_jsonl(path)? {
        rows += 1;
        collect_array_strings(&value, &["observed_refs"], &mut refs);
        collect_array_strings(&value, &["mcp_navigation", "observed_refs"], &mut refs);
    }
    Ok((rows, refs))
}

fn read_jsonl(path: &Path) -> Result<Vec<Value>, Box<dyn Error + Send + Sync>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut values = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        values.push(serde_json::from_str(&line).map_err(|error| {
            format!(
                "failed to parse {} line {}: {error}",
                path.display(),
                index + 1
            )
        })?);
    }
    Ok(values)
}

fn collect_string_field(value: &Value, field: &str, out: &mut BTreeSet<String>) {
    if let Some(text) = value.get(field).and_then(Value::as_str) {
        out.insert(text.to_string());
    }
}

fn collect_path_strings(value: &Value, path: &[&str], field: &str, out: &mut BTreeSet<String>) {
    for item in path_value(value, path)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        if let Some(text) = item.get(field).and_then(Value::as_str) {
            out.insert(text.to_string());
        }
    }
}

fn collect_array_strings(value: &Value, path: &[&str], out: &mut BTreeSet<String>) {
    for item in path_value(value, path)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        if let Some(text) = item.as_str() {
            out.insert(text.to_string());
        }
    }
}

fn collect_array_strings_from_items(
    value: &Value,
    path: &[&str],
    field: &str,
    out: &mut BTreeSet<String>,
) {
    for item in path_value(value, path)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        if let Some(text) = item.get(field).and_then(Value::as_str) {
            out.insert(text.to_string());
        }
    }
}

fn collect_pre_read_observed_refs(value: &Value, out: &mut BTreeSet<String>) {
    for call in value
        .get("pre_read_calls")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        collect_array_strings(call, &["observed_refs"], out);
    }
}

fn collect_coordinate_scope_refs(value: &Value, out: &mut BTreeSet<String>) {
    let Some(about) = value.get("about").and_then(Value::as_str) else {
        return;
    };
    let entries = path_value(
        value,
        &["dry_run_content", "ingest_preview", "memory", "entries"],
    )
    .and_then(Value::as_array)
    .map(Vec::as_slice)
    .unwrap_or(&[]);
    for entry in entries {
        let coordinates = entry
            .get("coordinates")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        for coordinate in coordinates {
            if let Some(scope_id) = coordinate.get("scope_id").and_then(Value::as_str) {
                out.insert(format!("about:{about}:dimension:{scope_id}"));
            }
        }
    }
}

fn path_value<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for part in path {
        current = current.get(*part)?;
    }
    Some(current)
}

fn projected_refs(inventory: &RefInventory) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    refs.extend(inventory.about_refs.iter().cloned());
    refs.extend(inventory.entry_refs.iter().cloned());
    refs.extend(inventory.connect_refs.iter().cloned());
    refs.extend(inventory.evidence_refs.iter().cloned());
    refs.extend(inventory.dimension_refs.iter().cloned());
    refs.extend(
        inventory
            .observed_refs
            .iter()
            .filter(|reference| {
                reference.starts_with("memoryarena:") || reference.starts_with("about:")
            })
            .cloned(),
    );
    refs.extend(
        inventory
            .read_context_refs
            .iter()
            .filter(|reference| {
                reference.starts_with("memoryarena:") || reference.starts_with("about:")
            })
            .cloned(),
    );
    refs
}

fn run_id_counts(refs: &BTreeSet<String>) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for reference in refs {
        if let Some(run_id) = extract_run_id(reference) {
            *counts.entry(run_id).or_insert(0) += 1;
        }
    }
    counts
}

fn single_run_id(counts: &BTreeMap<String, usize>) -> Option<String> {
    if counts.len() == 1 {
        counts.keys().next().cloned()
    } else {
        None
    }
}

fn foreign_refs(refs: &BTreeSet<String>, expected_run_id: &str) -> Vec<String> {
    refs.iter()
        .filter(|reference| {
            extract_run_id(reference)
                .as_deref()
                .is_some_and(|run_id| run_id != expected_run_id)
        })
        .cloned()
        .collect()
}

fn extract_run_id(reference: &str) -> Option<String> {
    let start = reference.find("run:")? + "run:".len();
    let rest = &reference[start..];
    let end = rest.find(":task_type")?;
    Some(rest[..end].to_string())
}

async fn inspect_refs(
    endpoint: &str,
    refs: &BTreeSet<String>,
    about_refs: &BTreeSet<String>,
    args: &Args,
) -> Result<InspectSummary, Box<dyn Error + Send + Sync>> {
    let server = KernelMcpServer::grpc_with_tls(
        endpoint,
        KernelMcpGrpcTlsConfig::from_env_for_endpoint(Some(endpoint)),
    );
    let selected_refs = refs
        .iter()
        .skip(args.offset)
        .take(args.limit.unwrap_or(usize::MAX))
        .cloned()
        .collect::<Vec<_>>();
    let mut progress = ProgressLogger::new(args)?;
    let mut id = 1u64;
    let mut found = 0usize;
    let mut missing = Vec::new();
    let mut errors = Vec::new();
    let started = Instant::now();

    for (index, reference) in selected_refs.iter().enumerate() {
        let processed = index + 1;
        let about = about_refs
            .iter()
            .filter(|about| reference.starts_with(about.as_str()))
            .max_by_key(|about| about.len())
            .ok_or_else(|| format!("no about boundary found for ref `{reference}`"))?;
        let outcome = inspect_one_ref(&server, id, about, reference).await?;
        id = id.checked_add(1).ok_or("request id overflow")?;
        match outcome {
            InspectRefOutcome::Found => found += 1,
            InspectRefOutcome::Missing => missing.push(reference.clone()),
            InspectRefOutcome::Error(error) => errors.push(InspectError {
                r#ref: reference.clone(),
                error,
            }),
        }
        progress.log(InspectProgress {
            processed,
            selected: selected_refs.len(),
            total: refs.len(),
            offset: args.offset,
            limit: args.limit,
            reference,
            found,
            missing: missing.len(),
            errors: errors.len(),
            page_completed: false,
            started: &started,
            force: false,
        })?;
    }
    progress.log(InspectProgress {
        processed: selected_refs.len(),
        selected: selected_refs.len(),
        total: refs.len(),
        offset: args.offset,
        limit: args.limit,
        reference: selected_refs.last().map(String::as_str).unwrap_or(""),
        found,
        missing: missing.len(),
        errors: errors.len(),
        page_completed: true,
        started: &started,
        force: true,
    })?;

    Ok(InspectSummary {
        total_refs: refs.len(),
        offset: args.offset,
        limit: args.limit,
        checked_refs: selected_refs.len(),
        found_refs: found,
        missing_refs: missing.len(),
        errored_refs: errors.len(),
        next_offset: args.offset.saturating_add(selected_refs.len()),
        completed: args.offset.saturating_add(selected_refs.len()) >= refs.len(),
        missing,
        errors,
    })
}

enum InspectRefOutcome {
    Found,
    Missing,
    Error(String),
}

async fn inspect_one_ref(
    server: &KernelMcpServer,
    id: u64,
    about: &str,
    reference: &str,
) -> Result<InspectRefOutcome, Box<dyn Error + Send + Sync>> {
    let request = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": "kmp_inspect",
            "arguments": {
                "about": about,
                "ref": reference,
                "include": {
                    "incoming": false,
                    "outgoing": false,
                    "details": false,
                    "raw": false
                }
            }
        }
    });
    let response = server
        .handle_json_line(&request.to_string())
        .await
        .ok_or("kmp_inspect returned no JSON-RPC response")?;
    let value = serde_json::from_str::<Value>(&response)?;
    if let Some(error) = value.get("error") {
        return Ok(InspectRefOutcome::Error(error.to_string()));
    }
    let Some(result) = value.get("result") else {
        return Ok(InspectRefOutcome::Error(
            "missing JSON-RPC result".to_string(),
        ));
    };
    if result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(InspectRefOutcome::Missing);
    }
    Ok(InspectRefOutcome::Found)
}

struct ProgressLogger {
    every: Option<usize>,
    file: Option<BufWriter<File>>,
}

struct InspectProgress<'a> {
    processed: usize,
    selected: usize,
    total: usize,
    offset: usize,
    limit: Option<usize>,
    reference: &'a str,
    found: usize,
    missing: usize,
    errors: usize,
    page_completed: bool,
    started: &'a Instant,
    force: bool,
}

impl ProgressLogger {
    fn new(args: &Args) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let file = match args.progress_output.as_deref() {
            Some(path) => {
                if path.exists() && !args.force {
                    return Err(format!(
                        "progress output exists: {} (use --force)",
                        path.display()
                    )
                    .into());
                }
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                Some(BufWriter::new(File::create(path)?))
            }
            None => None,
        };
        Ok(Self {
            every: args.log_progress_every,
            file,
        })
    }

    fn log(&mut self, progress: InspectProgress<'_>) -> Result<(), Box<dyn Error + Send + Sync>> {
        if !progress.page_completed && progress.processed == progress.selected {
            return Ok(());
        }
        let should_log = progress.force
            || progress.page_completed
            || self
                .every
                .is_some_and(|every| every > 0 && progress.processed.is_multiple_of(every));
        if !should_log {
            return Ok(());
        }
        let event = json!({
            "event": "memoryarena_kmp_run_audit.inspect.progress",
            "processed": progress.processed,
            "selected": progress.selected,
            "total": progress.total,
            "offset": progress.offset,
            "limit": progress.limit,
            "next_offset": progress.offset.saturating_add(progress.processed),
            "ref": if progress.reference.is_empty() {
                Value::Null
            } else {
                Value::String(progress.reference.to_string())
            },
            "found_refs": progress.found,
            "missing_refs": progress.missing,
            "errored_refs": progress.errors,
            "page_completed": progress.page_completed,
            "completed": progress.offset.saturating_add(progress.processed) >= progress.total,
            "elapsed_ms": progress.started.elapsed().as_millis(),
        });
        eprintln!("{event}");
        if let Some(file) = self.file.as_mut() {
            serde_json::to_writer(&mut *file, &event)?;
            file.write_all(b"\n")?;
            file.flush()?;
        }
        Ok(())
    }
}

fn write_output(
    path: &Path,
    summary: &AuditSummary,
    force: bool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if path.exists() && !force {
        return Err(format!("output exists: {} (use --force)", path.display()).into());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, summary)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, Box<dyn Error + Send + Sync>> {
    let mut run = None;
    let mut endpoint = None;
    let mut output = None;
    let mut expected_run_id = None;
    let mut limit = None;
    let mut offset = 0usize;
    let mut log_progress_every = None;
    let mut progress_output = None;
    let mut inspect = false;
    let mut force = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--run" => run = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--endpoint" => endpoint = Some(required_flag_value(&mut args, &arg)?),
            "--output" => output = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--expected-run-id" => expected_run_id = Some(required_flag_value(&mut args, &arg)?),
            "--limit" => {
                limit = Some(parse_positive_usize(
                    &required_flag_value(&mut args, &arg)?,
                    &arg,
                )?);
            }
            "--offset" => offset = required_flag_value(&mut args, &arg)?.parse()?,
            "--log-progress-every" => {
                log_progress_every = Some(parse_positive_usize(
                    &required_flag_value(&mut args, &arg)?,
                    &arg,
                )?);
            }
            "--progress-output" => {
                progress_output = Some(PathBuf::from(required_flag_value(&mut args, &arg)?));
            }
            "--inspect" => inspect = true,
            "--force" => force = true,
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument `{other}`").into()),
        }
    }

    Ok(Args {
        run: run.ok_or("--run is required")?,
        endpoint,
        output,
        expected_run_id,
        limit,
        offset,
        log_progress_every,
        progress_output,
        inspect,
        force,
    })
}

fn required_flag_value(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value").into())
}

fn parse_positive_usize(value: &str, flag: &str) -> Result<usize, Box<dyn Error + Send + Sync>> {
    let parsed = value
        .parse::<usize>()
        .map_err(|error| format!("invalid {flag} value `{value}`: {error}"))?;
    if parsed == 0 {
        return Err(format!("{flag} must be greater than zero").into());
    }
    Ok(parsed)
}

fn print_usage() {
    eprintln!(
        "Usage: memoryarena_kmp_run_audit --run <runner-output-dir> [--endpoint http://host --inspect] [--expected-run-id RUN] [--output audit.json] [--limit N] [--offset N] [--log-progress-every N] [--progress-output progress.jsonl] [--force]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_run_id_handles_about_and_scope_refs() {
        assert_eq!(
            extract_run_id(
                "memoryarena:run:run-a:task_type:progressive_search:task:0:subtask:1:question"
            )
            .as_deref(),
            Some("run-a")
        );
        assert_eq!(
            extract_run_id(
                "about:memoryarena:run:run-a:task_type:progressive_search:task:0:dimension:x"
            )
            .as_deref(),
            Some("run-a")
        );
        assert_eq!(
            extract_run_id("memoryarena:process:run:run-a:task_type:progressive_search:task:0")
                .as_deref(),
            Some("run-a")
        );
    }

    #[test]
    fn foreign_refs_detects_mixed_runs() {
        let refs = BTreeSet::from([
            "memoryarena:run:a:task_type:t:task:0".to_string(),
            "memoryarena:run:b:task_type:t:task:0".to_string(),
        ]);

        assert_eq!(
            foreign_refs(&refs, "a"),
            vec!["memoryarena:run:b:task_type:t:task:0"]
        );
    }

    #[test]
    fn parse_args_accepts_paged_progress_audit_flags() {
        let args = parse_args(
            [
                "--run",
                "/tmp/run",
                "--limit",
                "100",
                "--offset",
                "200",
                "--log-progress-every",
                "25",
                "--progress-output",
                "/tmp/progress.jsonl",
            ]
            .into_iter()
            .map(ToString::to_string),
        )
        .expect("args should parse");

        assert_eq!(args.run, PathBuf::from("/tmp/run"));
        assert_eq!(args.limit, Some(100));
        assert_eq!(args.offset, 200);
        assert_eq!(args.log_progress_every, Some(25));
        assert_eq!(
            args.progress_output.as_deref(),
            Some(Path::new("/tmp/progress.jsonl"))
        );
    }

    #[test]
    fn parse_positive_usize_rejects_zero() {
        let error = parse_positive_usize("0", "--limit").expect_err("zero should fail");
        assert!(
            error
                .to_string()
                .contains("--limit must be greater than zero")
        );
    }
}
