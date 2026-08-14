use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use kmp_interpretation::{
    CurrencyDerivationPlugin, DateDerivationPlugin, DateValuePlugin, DerivationOperand,
    DerivationOperation, DerivationRequest, DerivationResult, EvidenceFragment,
    EvidenceInterpretationInput, EvidenceValuePlugin, InterpretedValue, InterpretedValueMention,
    MathExpressionValuePlugin, MoneyValuePlugin, OperandRole, SourceCodeValuePlugin,
    UrlValuePlugin,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    task_id: String,
    task_type: String,
    #[serde(default)]
    category: Option<String>,
    subtask_index: usize,
    question: String,
    current_question_ref: String,
    ask_content: Value,
}

#[derive(Debug, Serialize)]
struct InterpretationProbeResult {
    task_id: String,
    task_type: String,
    category: Option<String>,
    subtask_index: usize,
    question: String,
    fragments: usize,
    currency_mentions: Vec<InterpretedValueMention>,
    date_mentions: Vec<InterpretedValueMention>,
    math_mentions: Vec<InterpretedValueMention>,
    source_code_mentions: Vec<InterpretedValueMention>,
    url_mentions: Vec<InterpretedValueMention>,
    currency_derivation: Option<DerivationResult>,
    date_derivation: Option<DerivationResult>,
    diagnostics: Vec<String>,
}

#[derive(Debug, Serialize)]
struct TaskTypeProbeSummary {
    asks: usize,
    asks_with_currency_mentions: usize,
    asks_with_date_mentions: usize,
    asks_with_math_mentions: usize,
    asks_with_source_code_mentions: usize,
    asks_with_url_mentions: usize,
    currency_mentions: usize,
    date_mentions: usize,
    math_mentions: usize,
    source_code_mentions: usize,
    url_mentions: usize,
    currency_derivation_attempts: usize,
    date_derivation_attempts: usize,
    derivation_errors: usize,
}

#[derive(Debug, Serialize)]
struct ProbeSummary {
    benchmark: &'static str,
    probe: &'static str,
    schema_version: &'static str,
    generated_at_unix_seconds: u64,
    run: String,
    asks: usize,
    applied_value_plugins: Vec<&'static str>,
    asks_with_currency_mentions: usize,
    asks_with_date_mentions: usize,
    asks_with_math_mentions: usize,
    asks_with_source_code_mentions: usize,
    asks_with_url_mentions: usize,
    currency_mentions: usize,
    date_mentions: usize,
    math_mentions: usize,
    source_code_mentions: usize,
    url_mentions: usize,
    currency_derivation_attempts: usize,
    date_derivation_attempts: usize,
    derivation_errors: usize,
    by_task_type: BTreeMap<String, TaskTypeProbeSummary>,
}

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
    ensure_output_dir(&args.output, args.force)?;

    let run_results = read_run_results(&args.run.join("results.jsonl"), args.limit_tasks)?;
    if run_results.is_empty() {
        return Err("MemoryArena interpretation probe has no run rows after filtering".into());
    }

    let results = probe_results(&run_results)?;
    let summary = summarize(&args, &results)?;

    write_jsonl(
        &args.output.join("interpretation_results.jsonl"),
        results.iter().map(serde_json::to_value),
    )?;
    write_json_pretty(&args.output.join("summary.json"), &summary)?;

    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn probe_results(
    run_results: &[RunAskResult],
) -> Result<Vec<InterpretationProbeResult>, Box<dyn Error + Send + Sync>> {
    let currency = CurrencyDerivationPlugin;
    let dates = DateDerivationPlugin;
    let money = MoneyValuePlugin;
    let date_values = DateValuePlugin;
    let math = MathExpressionValuePlugin;
    let source_code = SourceCodeValuePlugin;
    let url = UrlValuePlugin;
    let mut results = Vec::new();

    for run in run_results {
        let fragments = evidence_fragments(run);
        let input = EvidenceInterpretationInput::new(fragments.clone());
        let currency_output = money.interpret(&input)?;
        let date_output = date_values.interpret(&input)?;
        let math_output = math.interpret(&input)?;
        let source_code_output = source_code.interpret(&input)?;
        let url_output = url.interpret(&input)?;
        let mut diagnostics = Vec::new();

        let currency_derivation =
            match derive_currency_for_question(&currency, &run.question, &currency_output.values) {
                Ok(result) => result,
                Err(error) => {
                    diagnostics.push(error.to_string());
                    None
                }
            };
        let date_derivation =
            match derive_dates_for_question(&dates, &run.question, &date_output.values) {
                Ok(result) => result,
                Err(error) => {
                    diagnostics.push(error.to_string());
                    None
                }
            };

        results.push(InterpretationProbeResult {
            task_id: run.task_id.clone(),
            task_type: run.task_type.clone(),
            category: run.category.clone(),
            subtask_index: run.subtask_index,
            question: run.question.clone(),
            fragments: fragments.len(),
            currency_mentions: currency_output.values,
            date_mentions: date_output.values,
            math_mentions: math_output.values,
            source_code_mentions: source_code_output.values,
            url_mentions: url_output.values,
            currency_derivation,
            date_derivation,
            diagnostics,
        });
    }

    Ok(results)
}

fn derive_currency_for_question(
    plugin: &CurrencyDerivationPlugin,
    question: &str,
    mentions: &[InterpretedValueMention],
) -> Result<Option<DerivationResult>, Box<dyn Error + Send + Sync>> {
    let operation = infer_currency_operation(question);
    if operation == DerivationOperation::Unknown {
        return Ok(None);
    }
    let operands = mentions
        .iter()
        .filter(|mention| matches!(mention.value, InterpretedValue::Money { .. }))
        .map(operand_from_mention)
        .collect::<Vec<_>>();
    if operands.len() < required_operand_count(operation) {
        return Ok(None);
    }
    let request = DerivationRequest {
        question: question.to_string(),
        operation,
        unit: None,
        operands,
    };
    Ok(Some(plugin.derive(&request)?))
}

fn derive_dates_for_question(
    plugin: &DateDerivationPlugin,
    question: &str,
    mentions: &[InterpretedValueMention],
) -> Result<Option<DerivationResult>, Box<dyn Error + Send + Sync>> {
    let operation = infer_date_operation(question);
    if operation == DerivationOperation::Unknown {
        return Ok(None);
    }
    let mut operands = mentions
        .iter()
        .filter(|mention| matches!(mention.value, InterpretedValue::Date { .. }))
        .map(operand_from_mention)
        .collect::<Vec<_>>();
    if operands.len() < required_operand_count(operation) {
        return Ok(None);
    }
    if operation == DerivationOperation::Difference {
        operands.sort_by_key(|operand| date_ordinal(operand).unwrap_or_default());
        if let Some(first) = operands.first_mut() {
            first.role = Some(OperandRole::Subtrahend);
        }
        if let Some(last) = operands.last_mut() {
            last.role = Some(OperandRole::Minuend);
        }
    }
    let request = DerivationRequest {
        question: question.to_string(),
        operation,
        unit: Some("days".to_string()),
        operands,
    };
    Ok(Some(plugin.derive(&request)?))
}

fn operand_from_mention(mention: &InterpretedValueMention) -> DerivationOperand {
    let mut operand = DerivationOperand::included(mention.ref_id.clone(), mention.value.clone());
    operand.raw = Some(mention.raw.clone());
    operand
}

fn date_ordinal(operand: &DerivationOperand) -> Option<i64> {
    match operand.value.as_ref()? {
        InterpretedValue::Date { date } => Some(date.ordinal_days()),
        _ => None,
    }
}

fn infer_currency_operation(question: &str) -> DerivationOperation {
    let question = question.to_ascii_lowercase();
    if contains_any(
        &question,
        &[
            "highest-priced",
            "highest priced",
            "most expensive",
            "max price",
        ],
    ) {
        return DerivationOperation::MaxBy;
    }
    if contains_any(
        &question,
        &[
            "total",
            "combined",
            "sum",
            "spent",
            "budget",
            "cost altogether",
        ],
    ) {
        return DerivationOperation::Sum;
    }
    if contains_any(&question, &["difference", "how much more", "how much less"]) {
        return DerivationOperation::Difference;
    }
    DerivationOperation::Unknown
}

fn infer_date_operation(question: &str) -> DerivationOperation {
    let question = question.to_ascii_lowercase();
    if contains_any(&question, &["latest", "most recent", "current"]) {
        return DerivationOperation::MaxBy;
    }
    if contains_any(
        &question,
        &[
            "how long",
            "elapsed",
            "duration",
            "how many days",
            "how many weeks",
            "days between",
            "weeks between",
            "months between",
            "time between",
        ],
    ) {
        return DerivationOperation::Difference;
    }
    DerivationOperation::Unknown
}

fn required_operand_count(operation: DerivationOperation) -> usize {
    match operation {
        DerivationOperation::Difference => 2,
        DerivationOperation::Sum | DerivationOperation::Average | DerivationOperation::MaxBy => 2,
        DerivationOperation::List => 1,
        DerivationOperation::Count => 1,
        DerivationOperation::Unknown => usize::MAX,
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
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
    results: &[InterpretationProbeResult],
) -> Result<ProbeSummary, Box<dyn Error + Send + Sync>> {
    let mut by_task_type = BTreeMap::<String, TaskTypeProbeSummary>::new();
    for result in results {
        let entry = by_task_type
            .entry(result.task_type.clone())
            .or_insert_with(empty_task_type_summary);
        entry.asks += 1;
        if !result.currency_mentions.is_empty() {
            entry.asks_with_currency_mentions += 1;
        }
        if !result.date_mentions.is_empty() {
            entry.asks_with_date_mentions += 1;
        }
        if !result.math_mentions.is_empty() {
            entry.asks_with_math_mentions += 1;
        }
        if !result.source_code_mentions.is_empty() {
            entry.asks_with_source_code_mentions += 1;
        }
        if !result.url_mentions.is_empty() {
            entry.asks_with_url_mentions += 1;
        }
        entry.currency_mentions += result.currency_mentions.len();
        entry.date_mentions += result.date_mentions.len();
        entry.math_mentions += result.math_mentions.len();
        entry.source_code_mentions += result.source_code_mentions.len();
        entry.url_mentions += result.url_mentions.len();
        if result.currency_derivation.is_some() {
            entry.currency_derivation_attempts += 1;
        }
        if result.date_derivation.is_some() {
            entry.date_derivation_attempts += 1;
        }
        entry.derivation_errors += result.diagnostics.len();
    }

    Ok(ProbeSummary {
        benchmark: "MemoryArena",
        probe: "memoryarena-kmp-interpretation-probe-v1",
        schema_version: "memoryarena-interpretation-probe-summary-v1",
        generated_at_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        run: args.run.display().to_string(),
        asks: results.len(),
        applied_value_plugins: vec![
            "money-value-v1",
            "date-value-v1",
            "math-expression-value-v1",
            "source-code-value-v1",
            "url-value-v1",
        ],
        asks_with_currency_mentions: results
            .iter()
            .filter(|result| !result.currency_mentions.is_empty())
            .count(),
        asks_with_date_mentions: results
            .iter()
            .filter(|result| !result.date_mentions.is_empty())
            .count(),
        asks_with_math_mentions: results
            .iter()
            .filter(|result| !result.math_mentions.is_empty())
            .count(),
        asks_with_source_code_mentions: results
            .iter()
            .filter(|result| !result.source_code_mentions.is_empty())
            .count(),
        asks_with_url_mentions: results
            .iter()
            .filter(|result| !result.url_mentions.is_empty())
            .count(),
        currency_mentions: results
            .iter()
            .map(|result| result.currency_mentions.len())
            .sum(),
        date_mentions: results
            .iter()
            .map(|result| result.date_mentions.len())
            .sum(),
        math_mentions: results
            .iter()
            .map(|result| result.math_mentions.len())
            .sum(),
        source_code_mentions: results
            .iter()
            .map(|result| result.source_code_mentions.len())
            .sum(),
        url_mentions: results.iter().map(|result| result.url_mentions.len()).sum(),
        currency_derivation_attempts: results
            .iter()
            .filter(|result| result.currency_derivation.is_some())
            .count(),
        date_derivation_attempts: results
            .iter()
            .filter(|result| result.date_derivation.is_some())
            .count(),
        derivation_errors: results.iter().map(|result| result.diagnostics.len()).sum(),
        by_task_type,
    })
}

fn empty_task_type_summary() -> TaskTypeProbeSummary {
    TaskTypeProbeSummary {
        asks: 0,
        asks_with_currency_mentions: 0,
        asks_with_date_mentions: 0,
        asks_with_math_mentions: 0,
        asks_with_source_code_mentions: 0,
        asks_with_url_mentions: 0,
        currency_mentions: 0,
        date_mentions: 0,
        math_mentions: 0,
        source_code_mentions: 0,
        url_mentions: 0,
        currency_derivation_attempts: 0,
        date_derivation_attempts: 0,
        derivation_errors: 0,
    }
}

fn read_run_results(
    path: &Path,
    limit_tasks: Option<usize>,
) -> Result<Vec<RunAskResult>, Box<dyn Error + Send + Sync>> {
    let selected_task_keys = selected_task_keys(path, limit_tasks)?;
    let run_results = read_jsonl(path)?
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|result: &RunAskResult| {
            selected_task_keys
                .as_ref()
                .is_none_or(|keys| keys.contains(&task_key(&result.task_type, &result.task_id)))
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
        "Usage: memoryarena_kmp_interpretation_probe --run <runner-output-dir> --output <probe-output-dir> [--limit-tasks N] [--force]"
    );
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn evidence_fragments_include_question_and_proof_evidence() {
        let run = RunAskResult {
            task_id: "task-1".to_string(),
            task_type: "bundled_shopping".to_string(),
            category: None,
            subtask_index: 1,
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
                    "ref": "detail:1",
                    "evidence": "Closed on 2026-05-06."
                }]
            }),
        };

        let fragments = evidence_fragments(&run);

        assert_eq!(fragments.len(), 3);
        assert!(
            fragments
                .iter()
                .any(|fragment| fragment.ref_id == "question:1")
        );
        assert!(
            fragments
                .iter()
                .any(|fragment| fragment.ref_id == "evidence:1")
        );
        assert!(
            fragments
                .iter()
                .any(|fragment| fragment.ref_id == "detail:1")
        );
    }
}
