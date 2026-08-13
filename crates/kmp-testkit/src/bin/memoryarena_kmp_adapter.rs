use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use kmp_testkit::{
    MemoryArenaAdapterConfig, MemoryArenaAdapterSummary, parse_memoryarena_dataset,
    prepare_memoryarena_items,
};
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    input: PathBuf,
    output: PathBuf,
    task_type: String,
    category: Option<String>,
    limit: Option<usize>,
    run_id: Option<String>,
    force: bool,
}

#[derive(Debug, Serialize)]
struct Manifest {
    benchmark: &'static str,
    methodology: &'static str,
    source_path: String,
    generated_at_unix_seconds: u64,
    adapter: &'static str,
    run_id: Option<String>,
    task_type: String,
    artifacts: ArtifactPaths,
    summary: MemoryArenaAdapterSummary,
}

#[derive(Debug, Serialize)]
struct ArtifactPaths {
    events_jsonl: &'static str,
    ingest_jsonl: &'static str,
    ask_jsonl: &'static str,
    expected_jsonl: &'static str,
    replay_jsonl: &'static str,
    summary_json: &'static str,
}

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
    ensure_output_dir(&args.output, args.force)?;

    let payload = fs::read_to_string(&args.input)?;
    let dataset = parse_memoryarena_dataset(&payload)?;
    let config = MemoryArenaAdapterConfig {
        task_type: args.task_type.clone(),
        category: args.category.clone(),
        limit: args.limit,
        run_id: args.run_id.clone(),
    };
    let (prepared, summary) = prepare_memoryarena_items(&dataset, &config)?;

    write_jsonl(
        &args.output.join("ingest.jsonl"),
        prepared
            .iter()
            .flat_map(|task| task.ingest_events.iter())
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter(),
    )?;
    write_jsonl(
        &args.output.join("ask.jsonl"),
        prepared
            .iter()
            .flat_map(|task| task.ask_events.iter())
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter(),
    )?;
    write_jsonl(
        &args.output.join("expected.jsonl"),
        prepared
            .iter()
            .flat_map(|task| task.expected.iter())
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter(),
    )?;
    write_jsonl(
        &args.output.join("replay.jsonl"),
        prepared
            .iter()
            .map(|task| serde_json::to_value(&task.replay))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter(),
    )?;
    write_jsonl(
        &args.output.join("events.jsonl"),
        prepared
            .iter()
            .flat_map(|task| {
                let mut events = Vec::new();
                events.extend(task.ingest_events.iter().map(|event| {
                    json!({
                        "event": "ingest",
                        "task_id": event.task_id,
                        "task_type": event.task_type,
                        "category": event.category,
                        "event_index": event.event_index,
                        "phase": event.phase,
                        "subtask_index": event.subtask_index,
                        "about": event.about,
                        "tool": event.tool,
                        "arguments": event.arguments
                    })
                }));
                events.extend(task.ask_events.iter().map(|event| {
                    json!({
                        "event": "ask",
                        "task_id": event.task_id,
                        "task_type": event.task_type,
                        "category": event.category,
                        "event_index": event.event_index,
                        "phase": "ask",
                        "subtask_index": event.subtask_index,
                        "required_ingest_events": event.required_ingest_events,
                        "available_after_event_index": event.available_after_event_index,
                        "about": event.about,
                        "tool": event.tool,
                        "arguments": event.arguments
                    })
                }));
                events.sort_by_key(|event| {
                    event
                        .get("event_index")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or_default()
                });
                events
            })
            .collect::<Vec<_>>()
            .into_iter(),
    )?;
    write_json_pretty(&args.output.join("summary.json"), &summary)?;

    let manifest = Manifest {
        benchmark: "MemoryArena",
        methodology: "docs/product/kernel-roadmap-milestones.md",
        source_path: args.input.display().to_string(),
        generated_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        adapter: "memoryarena-kmp-adapter-v1",
        run_id: args.run_id.clone(),
        task_type: args.task_type.clone(),
        artifacts: ArtifactPaths {
            events_jsonl: "events.jsonl",
            ingest_jsonl: "ingest.jsonl",
            ask_jsonl: "ask.jsonl",
            expected_jsonl: "expected.jsonl",
            replay_jsonl: "replay.jsonl",
            summary_json: "summary.json",
        },
        summary,
    };
    write_json_pretty(&args.output.join("manifest.json"), &manifest)?;

    println!("{}", serde_json::to_string_pretty(&manifest)?);
    Ok(())
}

fn ensure_output_dir(output: &Path, force: bool) -> Result<(), Box<dyn Error + Send + Sync>> {
    if output.exists() {
        if !output.is_dir() {
            return Err(format!("output path is not a directory: {}", output.display()).into());
        }
        if !force && output.read_dir()?.next().is_some() {
            return Err(format!(
                "output directory is not empty: {} (use --force to overwrite known artifact files)",
                output.display()
            )
            .into());
        }
    }
    fs::create_dir_all(output)?;
    Ok(())
}

fn write_jsonl(
    path: &Path,
    values: impl Iterator<Item = serde_json::Value>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    for value in values {
        serde_json::to_writer(&mut writer, &value)?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    Ok(())
}

fn write_json_pretty<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, Box<dyn Error + Send + Sync>> {
    let mut input = None;
    let mut output = None;
    let mut task_type = "memoryarena".to_string();
    let mut category = None;
    let mut limit = None;
    let mut run_id = None;
    let mut force = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--output" => output = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--task-type" => task_type = required_flag_value(&mut args, &arg)?,
            "--category" => category = Some(required_flag_value(&mut args, &arg)?),
            "--limit" => {
                let value = required_flag_value(&mut args, &arg)?;
                limit = Some(
                    value
                        .parse::<usize>()
                        .map_err(|error| format!("invalid --limit value `{value}`: {error}"))?,
                );
            }
            "--run-id" => run_id = Some(required_flag_value(&mut args, &arg)?),
            "--force" => force = true,
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument `{other}`").into()),
        }
    }

    Ok(Args {
        input: input.ok_or("--input is required")?,
        output: output.ok_or("--output is required")?,
        task_type,
        category,
        limit,
        run_id,
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

fn print_usage() {
    println!(
        "Usage: memoryarena_kmp_adapter --input <data.jsonl> --output <out-dir> --task-type <progressive_search|group_travel_planner|...> [--category CATEGORY] [--limit N] [--run-id RUN] [--force]"
    );
}
