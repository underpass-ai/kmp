use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use kmp_testkit::{
    LongMemEvalAdapterConfig, LongMemEvalAdapterSummary, LongMemEvalEvidenceLabels,
    parse_longmemeval_dataset, prepare_longmemeval_items,
};
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    input: PathBuf,
    output: PathBuf,
    limit: Option<usize>,
    per_question_type_limit: Option<usize>,
    question_type: Option<String>,
    run_id: Option<String>,
    evidence_labels: Option<PathBuf>,
    include_abstention: bool,
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
    artifacts: ArtifactPaths,
    summary: LongMemEvalAdapterSummary,
}

#[derive(Debug, Serialize)]
struct ArtifactPaths {
    ingest_jsonl: &'static str,
    ask_jsonl: &'static str,
    expected_jsonl: &'static str,
    summary_json: &'static str,
}

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
    ensure_output_dir(&args.output, args.force)?;

    let payload = fs::read_to_string(&args.input)?;
    let dataset = parse_longmemeval_dataset(&payload)?;
    let generated_evidence = args
        .evidence_labels
        .as_deref()
        .map(read_evidence_labels)
        .transpose()?;
    let config = LongMemEvalAdapterConfig {
        limit: args.limit,
        per_question_type_limit: args.per_question_type_limit,
        question_type: args.question_type.clone(),
        include_abstention: args.include_abstention,
        strict_temporal: true,
        run_id: args.run_id.clone(),
        generated_evidence,
    };
    let (prepared, summary) = prepare_longmemeval_items(&dataset, &config)?;

    write_jsonl(
        &args.output.join("ingest.jsonl"),
        prepared.iter().map(|item| {
            json!({
                "tool": "kmp_ingest",
                "question_id": item.question_id,
                "question_type": item.question_type,
                "about": item.about,
                "arguments": item.ingest
            })
        }),
    )?;
    write_jsonl(
        &args.output.join("ask.jsonl"),
        prepared.iter().map(|item| {
            json!({
                "tool": "kmp_ask",
                "question_id": item.question_id,
                "question_type": item.question_type,
                "about": item.about,
                "arguments": item.ask
            })
        }),
    )?;
    let expected_values = prepared
        .iter()
        .map(|item| serde_json::to_value(&item.expected))
        .collect::<Result<Vec<_>, _>>()?;
    write_jsonl(
        &args.output.join("expected.jsonl"),
        expected_values.into_iter(),
    )?;

    write_json_pretty(&args.output.join("summary.json"), &summary)?;

    let manifest = Manifest {
        benchmark: "LongMemEval",
        methodology: "https://github.com/underpass-ai/kmp/blob/v0.5.0/archive/docs/research/benchmark-methodology-v1.md",
        source_path: args.input.display().to_string(),
        generated_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        adapter: "longmemeval-kmp-adapter-v1",
        run_id: args.run_id.clone(),
        artifacts: ArtifactPaths {
            ingest_jsonl: "ingest.jsonl",
            ask_jsonl: "ask.jsonl",
            expected_jsonl: "expected.jsonl",
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

fn read_evidence_labels(
    path: &Path,
) -> Result<BTreeMap<String, LongMemEvalEvidenceLabels>, Box<dyn Error + Send + Sync>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut labels = BTreeMap::new();
    for (line_index, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let item = serde_json::from_str::<LongMemEvalEvidenceLabels>(&line).map_err(|error| {
            format!(
                "invalid evidence labels JSONL at {}:{}: {error}",
                path.display(),
                line_index + 1
            )
        })?;
        if labels.insert(item.question_id.clone(), item).is_some() {
            return Err(format!(
                "duplicate evidence labels for question_id at {}:{}",
                path.display(),
                line_index + 1
            )
            .into());
        }
    }
    Ok(labels)
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, Box<dyn Error + Send + Sync>> {
    let mut input = None;
    let mut output = None;
    let mut limit = None;
    let mut per_question_type_limit = None;
    let mut question_type = None;
    let mut run_id = None;
    let mut evidence_labels = None;
    let mut include_abstention = true;
    let mut force = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--output" => output = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--limit" => {
                let value = required_flag_value(&mut args, &arg)?;
                limit = Some(
                    value
                        .parse::<usize>()
                        .map_err(|error| format!("invalid --limit value `{value}`: {error}"))?,
                );
            }
            "--per-question-type-limit" => {
                let value = required_flag_value(&mut args, &arg)?;
                per_question_type_limit = Some(value.parse::<usize>().map_err(|error| {
                    format!("invalid --per-question-type-limit value `{value}`: {error}")
                })?);
            }
            "--question-type" => question_type = Some(required_flag_value(&mut args, &arg)?),
            "--run-id" => run_id = Some(required_flag_value(&mut args, &arg)?),
            "--evidence-labels" => {
                evidence_labels = Some(PathBuf::from(required_flag_value(&mut args, &arg)?));
            }
            "--exclude-abstention" => include_abstention = false,
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
        limit,
        per_question_type_limit,
        question_type,
        run_id,
        evidence_labels,
        include_abstention,
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
    eprintln!(
        "Usage: longmemeval_kmp_adapter --input <longmemeval.json> --output <artifact-dir> [--limit N] [--per-question-type-limit N] [--question-type TYPE] [--run-id RUN] [--evidence-labels labels.jsonl] [--exclude-abstention] [--force]"
    );
}
