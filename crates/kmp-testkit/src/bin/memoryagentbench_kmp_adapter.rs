use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use kmp_testkit::{
    MemoryAgentBenchAdapterConfig, MemoryAgentBenchAdapterSummary, parse_memoryagentbench_dataset,
    prepare_memoryagentbench_items,
};
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    input: PathBuf,
    output: PathBuf,
    split: String,
    source: Option<String>,
    limit: Option<usize>,
    limit_queries: Option<usize>,
    run_id: Option<String>,
    max_context_entries: Option<usize>,
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
    split: String,
    source: Option<String>,
    limit_queries: Option<usize>,
    max_context_entries: Option<usize>,
    artifacts: ArtifactPaths,
    summary: MemoryAgentBenchAdapterSummary,
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
    let dataset = parse_memoryagentbench_dataset(&payload)?;
    let config = MemoryAgentBenchAdapterConfig {
        split: args.split.clone(),
        source: args.source.clone(),
        limit: args.limit,
        limit_queries: args.limit_queries,
        run_id: args.run_id.clone(),
        max_context_entries: args.max_context_entries,
    };
    let (prepared, summary) = prepare_memoryagentbench_items(&dataset, &config)?;

    write_jsonl(
        &args.output.join("ingest.jsonl"),
        prepared
            .iter()
            .flat_map(|item| item.ingest_events.iter())
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter(),
    )?;
    write_jsonl(
        &args.output.join("ask.jsonl"),
        prepared
            .iter()
            .flat_map(|item| item.ask_events.iter())
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter(),
    )?;
    write_jsonl(
        &args.output.join("expected.jsonl"),
        prepared
            .iter()
            .flat_map(|item| item.expected.iter())
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter(),
    )?;
    write_jsonl(
        &args.output.join("replay.jsonl"),
        prepared
            .iter()
            .map(|item| serde_json::to_value(&item.replay))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter(),
    )?;
    write_jsonl(
        &args.output.join("events.jsonl"),
        prepared
            .iter()
            .flat_map(|item| {
                let mut events = Vec::new();
                events.extend(item.ingest_events.iter().map(|event| {
                    json!({
                        "event": "ingest",
                        "item_id": event.item_id,
                        "split": event.split,
                        "source": event.source,
                        "event_index": event.event_index,
                        "phase": event.phase,
                        "about": event.about,
                        "context_entries": event.context_entries,
                        "truncated_context_entries": event.truncated_context_entries,
                        "tool": event.tool,
                        "arguments": event.arguments
                    })
                }));
                events.extend(item.ask_events.iter().map(|event| {
                    json!({
                        "event": "ask",
                        "item_id": event.item_id,
                        "split": event.split,
                        "source": event.source,
                        "event_index": event.event_index,
                        "phase": "query",
                        "query_index": event.query_index,
                        "required_ingest_events": event.required_ingest_events,
                        "available_after_event_index": event.available_after_event_index,
                        "about": event.about,
                        "question_id": event.question_id,
                        "qa_pair_id": event.qa_pair_id,
                        "question_type": event.question_type,
                        "question_date": event.question_date,
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
        benchmark: "MemoryAgentBench",
        methodology: "archive/docs/research/memoryagentbench-benchmark.md",
        source_path: args.input.display().to_string(),
        generated_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        adapter: "memoryagentbench-kmp-adapter-v1",
        run_id: args.run_id.clone(),
        split: args.split.clone(),
        source: args.source.clone(),
        limit_queries: args.limit_queries,
        max_context_entries: args.max_context_entries,
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
    let mut split = "Conflict_Resolution".to_string();
    let mut source = None;
    let mut limit = None;
    let mut limit_queries = None;
    let mut run_id = None;
    let mut max_context_entries = None;
    let mut force = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--output" => output = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--split" => split = required_flag_value(&mut args, &arg)?,
            "--source" => source = Some(required_flag_value(&mut args, &arg)?),
            "--limit" => {
                let value = required_flag_value(&mut args, &arg)?;
                limit = Some(
                    value
                        .parse::<usize>()
                        .map_err(|error| format!("invalid --limit value `{value}`: {error}"))?,
                );
            }
            "--limit-queries" => {
                let value = required_flag_value(&mut args, &arg)?;
                let parsed = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --limit-queries value `{value}`: {error}"))?;
                if parsed == 0 {
                    return Err("--limit-queries must be greater than zero".into());
                }
                limit_queries = Some(parsed);
            }
            "--run-id" => run_id = Some(required_flag_value(&mut args, &arg)?),
            "--max-context-entries" => {
                let value = required_flag_value(&mut args, &arg)?;
                let parsed = value.parse::<usize>().map_err(|error| {
                    format!("invalid --max-context-entries value `{value}`: {error}")
                })?;
                if parsed == 0 {
                    return Err("--max-context-entries must be greater than zero".into());
                }
                max_context_entries = Some(parsed);
            }
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
        split,
        source,
        limit,
        limit_queries,
        run_id,
        max_context_entries,
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
        "Usage: memoryagentbench_kmp_adapter --input <data.jsonl> --output <out-dir> [--split Conflict_Resolution|Accurate_Retrieval|Test_Time_Learning|Long_Range_Understanding] [--source SOURCE] [--limit N] [--limit-queries N] [--run-id RUN] [--max-context-entries N] [--force]"
    );
}
