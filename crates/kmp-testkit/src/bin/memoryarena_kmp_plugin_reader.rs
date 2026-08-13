use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use kmp_interpretation::{
    ComposedEvidenceReader, EvidenceFragment, EvidenceReaderPluginConfiguration,
    EvidenceReaderRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

type TaskKey = (String, String);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    run: PathBuf,
    output: PathBuf,
    limit_tasks: Option<usize>,
    force: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct RunAskResult {
    task_type: String,
    question: String,
    current_question_ref: String,
    ask_content: Value,
}

#[derive(Debug, Serialize)]
struct ReaderSummary {
    benchmark: &'static str,
    reader: &'static str,
    schema_version: &'static str,
    generated_at_unix_seconds: u64,
    run: String,
    results: usize,
    changed_answers: usize,
    value_plugins: Vec<&'static str>,
    derivation_plugins: Vec<&'static str>,
    plugin_configuration: EvidenceReaderPluginConfiguration,
    value_mentions: usize,
    derivation_results: usize,
    diagnostics: usize,
    by_task_type: BTreeMap<String, TaskTypeReaderSummary>,
}

#[derive(Debug, Default, Serialize)]
struct TaskTypeReaderSummary {
    results: usize,
    changed_answers: usize,
    value_mentions: usize,
    derivation_results: usize,
    diagnostics: usize,
}

#[derive(Debug)]
struct PluginReaderResult {
    output: Value,
    task_type: String,
    changed_answer: bool,
    value_mentions: usize,
    derivation_results: usize,
    diagnostics: usize,
}

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
    ensure_output_dir(&args.output, args.force)?;

    let rows = read_selected_run_results(&args.run.join("results.jsonl"), args.limit_tasks)?;
    if rows.is_empty() {
        return Err("MemoryArena plugin reader has no run rows after filtering".into());
    }

    let reader = ComposedEvidenceReader::kernel_default();
    let mut results = Vec::new();
    for row in rows {
        results.push(read_row_with_plugins(&reader, row)?);
    }
    let summary = summarize(&args, &reader, &results)?;

    write_jsonl(
        &args.output.join("results.jsonl"),
        results.iter().map(|result| Ok(result.output.clone())),
    )?;
    write_jsonl(
        &args.output.join("hypotheses.jsonl"),
        results.iter().map(|result| {
            let task_id = result
                .output
                .get("task_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let subtask_index = result
                .output
                .get("subtask_index")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            let hypothesis = result
                .output
                .get("ask_answer")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Ok(json!({
                "task_id": task_id,
                "subtask_index": subtask_index,
                "hypothesis": hypothesis
            }))
        }),
    )?;
    write_json_pretty(&args.output.join("summary.json"), &summary)?;

    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn read_row_with_plugins(
    reader: &ComposedEvidenceReader,
    row: Value,
) -> Result<PluginReaderResult, Box<dyn Error + Send + Sync>> {
    let run = serde_json::from_value::<RunAskResult>(row.clone())?;
    let fragments = evidence_fragments(&run);
    let request = EvidenceReaderRequest::new(kmp_interpretation::EvidenceInterpretationInput::new(
        fragments,
    ));
    let plugin_output = reader.read(&request)?;

    let deterministic_answer = plugin_output
        .derivation_results
        .iter()
        .find_map(|result| result.answer.as_deref())
        .map(str::trim)
        .filter(|answer| !answer.is_empty())
        .map(ToString::to_string);

    let mut output = row
        .as_object()
        .cloned()
        .ok_or("run result row must be a JSON object")?;
    let changed_answer = if let Some(answer) = deterministic_answer {
        output.insert(
            "kernel_ask_answer".to_string(),
            output.get("ask_answer").cloned().unwrap_or(Value::Null),
        );
        output.insert("ask_answer".to_string(), Value::String(answer));
        true
    } else {
        false
    };
    output.insert(
        "plugin_reader".to_string(),
        serde_json::to_value(&plugin_output)?,
    );

    Ok(PluginReaderResult {
        output: Value::Object(output),
        task_type: run.task_type,
        changed_answer,
        value_mentions: plugin_output.values.len(),
        derivation_results: plugin_output.derivation_results.len(),
        diagnostics: plugin_output.diagnostics.len(),
    })
}

fn evidence_fragments(run: &RunAskResult) -> Vec<EvidenceFragment> {
    let mut fragments = Vec::new();
    fragments.push(EvidenceFragment::new(
        run.current_question_ref.clone(),
        run.question.clone(),
    ));

    if let Some(evidence) = run
        .ask_content
        .pointer("/proof/evidence")
        .and_then(Value::as_array)
    {
        for item in evidence {
            let Some(text) = item.get("text").and_then(Value::as_str) else {
                continue;
            };
            let ref_id = item
                .get("id")
                .and_then(Value::as_str)
                .or_else(|| item.get("source").and_then(Value::as_str))
                .unwrap_or("proof:evidence");
            let mut fragment = EvidenceFragment::new(ref_id, text);
            fragment.source = item
                .get("source")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            fragments.push(fragment);
        }
    }

    if let Some(because) = run.ask_content.get("because").and_then(Value::as_array) {
        for item in because {
            let Some(text) = item.get("evidence").and_then(Value::as_str) else {
                continue;
            };
            let ref_id = item
                .get("ref")
                .and_then(Value::as_str)
                .unwrap_or("because:evidence");
            fragments.push(EvidenceFragment::new(ref_id, text));
        }
    }

    dedupe_fragments(fragments)
}

fn dedupe_fragments(fragments: Vec<EvidenceFragment>) -> Vec<EvidenceFragment> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();
    for fragment in fragments {
        if seen.insert((fragment.ref_id.clone(), fragment.text.clone())) {
            deduped.push(fragment);
        }
    }
    deduped
}

fn summarize(
    args: &Args,
    reader: &ComposedEvidenceReader,
    results: &[PluginReaderResult],
) -> Result<ReaderSummary, Box<dyn Error + Send + Sync>> {
    let mut by_task_type = BTreeMap::<String, TaskTypeReaderSummary>::new();
    for result in results {
        let entry = by_task_type.entry(result.task_type.clone()).or_default();
        entry.results += 1;
        entry.changed_answers += usize::from(result.changed_answer);
        entry.value_mentions += result.value_mentions;
        entry.derivation_results += result.derivation_results;
        entry.diagnostics += result.diagnostics;
    }

    Ok(ReaderSummary {
        benchmark: "MemoryArena",
        reader: "memoryarena-kmp-plugin-reader-v1",
        schema_version: "memoryarena-plugin-reader-summary-v1",
        generated_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        run: args.run.display().to_string(),
        results: results.len(),
        changed_answers: results
            .iter()
            .filter(|result| result.changed_answer)
            .count(),
        value_plugins: reader.value_plugin_ids(),
        derivation_plugins: reader.derivation_plugin_ids(),
        plugin_configuration: reader.configuration(),
        value_mentions: results.iter().map(|result| result.value_mentions).sum(),
        derivation_results: results.iter().map(|result| result.derivation_results).sum(),
        diagnostics: results.iter().map(|result| result.diagnostics).sum(),
        by_task_type,
    })
}

fn read_selected_run_results(
    path: &Path,
    limit_tasks: Option<usize>,
) -> Result<Vec<Value>, Box<dyn Error + Send + Sync>> {
    let selected_task_keys = selected_task_keys(path, limit_tasks)?;
    let run_results = read_jsonl(path)?
        .into_iter()
        .filter(|value| {
            selected_task_keys.as_ref().is_none_or(|keys| {
                let task_type = value.get("task_type").and_then(Value::as_str);
                let task_id = value.get("task_id").and_then(Value::as_str);
                task_type.zip(task_id).is_some_and(|(task_type, task_id)| {
                    keys.contains(&task_key(task_type, task_id))
                })
            })
        })
        .collect::<Vec<_>>();
    Ok(run_results)
}

fn selected_task_keys(
    path: &Path,
    limit_tasks: Option<usize>,
) -> Result<Option<BTreeSet<TaskKey>>, Box<dyn Error + Send + Sync>> {
    let Some(limit) = limit_tasks else {
        return Ok(None);
    };
    let mut selected = BTreeSet::new();
    for value in read_jsonl(path)? {
        let task_type = required_string(&value, "task_type")?;
        let task_id = required_string(&value, "task_id")?;
        selected.insert(task_key(&task_type, &task_id));
        if selected.len() >= limit {
            break;
        }
    }
    Ok(Some(selected))
}

fn task_key(task_type: &str, task_id: &str) -> TaskKey {
    (task_type.to_string(), task_id.to_string())
}

fn read_jsonl(path: &Path) -> Result<Vec<Value>, Box<dyn Error + Send + Sync>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut values = Vec::new();
    for (line_index, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        values.push(serde_json::from_str::<Value>(&line).map_err(|error| {
            format!(
                "invalid JSONL at {}:{}: {error}",
                path.display(),
                line_index + 1
            )
        })?);
    }
    Ok(values)
}

fn required_string(value: &Value, field: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("missing string field `{field}`").into())
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

fn write_jsonl<I>(path: &Path, values: I) -> Result<(), Box<dyn Error + Send + Sync>>
where
    I: Iterator<Item = Result<Value, serde_json::Error>>,
{
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    for value in values {
        serde_json::to_writer(&mut writer, &value?)?;
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
    let mut run = None;
    let mut output = None;
    let mut limit_tasks = None;
    let mut force = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--run" => run = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--output" => output = Some(PathBuf::from(required_flag_value(&mut args, &arg)?)),
            "--limit-tasks" => {
                let value = required_flag_value(&mut args, &arg)?;
                let parsed = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --limit-tasks value `{value}`: {error}"))?;
                if parsed == 0 {
                    return Err("--limit-tasks must be greater than zero".into());
                }
                limit_tasks = Some(parsed);
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
        run: run.ok_or("--run is required")?,
        output: output.ok_or("--output is required")?,
        limit_tasks,
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
        "Usage: memoryarena_kmp_plugin_reader --run <runner-output-dir> --output <reader-output-dir> [--limit-tasks N] [--force]"
    );
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn evidence_fragments_include_question_and_proof_evidence() {
        let run = RunAskResult {
            task_type: "bundled_shopping".to_string(),
            question: "Total budget is $70.".to_string(),
            current_question_ref: "question:1".to_string(),
            ask_content: json!({
                "proof": {
                    "evidence": [{
                        "id": "evidence:1",
                        "source": "source:1",
                        "text": "Previous price was $12."
                    }]
                },
                "because": [{
                    "ref": "because:1",
                    "evidence": "Asked on 2026-05-06."
                }]
            }),
        };

        let fragments = evidence_fragments(&run);

        assert_eq!(fragments.len(), 3);
        assert_eq!(fragments[0].ref_id, "question:1");
        assert_eq!(fragments[1].ref_id, "evidence:1");
        assert_eq!(fragments[1].source.as_deref(), Some("source:1"));
        assert_eq!(fragments[2].ref_id, "because:1");
    }
}
