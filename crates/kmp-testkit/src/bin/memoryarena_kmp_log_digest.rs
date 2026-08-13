use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    input: PathBuf,
    output: Option<PathBuf>,
    detail: bool,
    show_refs: bool,
    slow_ms: u128,
    limit_entries: Option<usize>,
}

#[derive(Debug, Default)]
struct Digest {
    lines: usize,
    json_events: usize,
    non_json_lines: Vec<String>,
    writer_entries: BTreeMap<String, WriterEntry>,
    probes: BTreeMap<ProbeKey, ProbeDigest>,
}

#[derive(Debug, Clone, Default)]
struct WriterEntry {
    entry_ref: String,
    about: Option<String>,
    entry_kind: Option<String>,
    phase: Option<String>,
    event_index: Option<u64>,
    subtask_index: Option<u64>,
    candidate_refs: Vec<String>,
    reads: Vec<ReadDigest>,
    llm: Option<LlmDecision>,
    llm_error: Option<String>,
    relation_strategy: Option<String>,
    commit_relations: Vec<RelationDecision>,
    quality: Option<RelationQuality>,
    commit_started: bool,
    commit_elapsed_ms: Option<u128>,
    verify_refs: Vec<String>,
}

impl WriterEntry {
    fn context_nodes(&self) -> BTreeSet<String> {
        self.reads
            .iter()
            .flat_map(|read| read.observed_refs.iter().cloned())
            .collect()
    }

    fn relations(&self) -> &[RelationDecision] {
        self.llm
            .as_ref()
            .map(|llm| llm.relations.as_slice())
            .unwrap_or(self.commit_relations.as_slice())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReadDigest {
    tool: String,
    target_ref: Option<String>,
    elapsed_ms: Option<u128>,
    observed_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LlmDecision {
    strategy: Option<String>,
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    relations: Vec<RelationDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RelationDecision {
    rel: String,
    class: Option<String>,
    confidence: Option<String>,
    target_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct RelationQuality {
    total: usize,
    rich: usize,
    anemic: usize,
    structural: usize,
    suspect: usize,
    proof_coverage: Option<f64>,
    prior_context_coverage: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ProbeKey {
    event_index: Option<u64>,
    subtask_index: Option<u64>,
    tool: String,
    request_id: Option<u64>,
}

#[derive(Debug, Clone, Default)]
struct ProbeDigest {
    about: Option<String>,
    elapsed_ms: Option<u128>,
    observed_refs: Vec<String>,
}

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
    let digest = read_digest(&args.input)?;
    let rendered = render_digest(&digest, &args);
    if let Some(output) = args.output.as_deref() {
        let file = File::create(output)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(rendered.as_bytes())?;
        writer.flush()?;
    } else {
        print!("{rendered}");
    }
    Ok(())
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, Box<dyn Error + Send + Sync>> {
    let mut input = None;
    let mut output = None;
    let mut detail = false;
    let mut show_refs = false;
    let mut slow_ms = 10_000u128;
    let mut limit_entries = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input = Some(PathBuf::from(args.next().ok_or("--input requires path")?)),
            "--output" => {
                output = Some(PathBuf::from(args.next().ok_or("--output requires path")?));
            }
            "--detail" => detail = true,
            "--show-refs" => show_refs = true,
            "--slow-ms" => {
                slow_ms = args
                    .next()
                    .ok_or("--slow-ms requires milliseconds")?
                    .parse::<u128>()?;
            }
            "--limit-entries" => {
                limit_entries = Some(
                    args.next()
                        .ok_or("--limit-entries requires a number")?
                        .parse::<usize>()?,
                );
            }
            "--help" | "-h" => return Err(usage().into()),
            value if value.starts_with('-') => {
                return Err(format!("unknown argument: {value}\n{}", usage()).into());
            }
            value => {
                if input.is_some() {
                    return Err(format!("unexpected positional argument: {value}").into());
                }
                input = Some(PathBuf::from(value));
            }
        }
    }

    Ok(Args {
        input: input.ok_or_else(usage)?,
        output,
        detail,
        show_refs,
        slow_ms,
        limit_entries,
    })
}

fn usage() -> String {
    "usage: memoryarena_kmp_log_digest --input <stderr.jsonl|-> [--output path] [--detail] [--show-refs] [--slow-ms n] [--limit-entries n]".to_string()
}

fn read_digest(path: &PathBuf) -> Result<Digest, Box<dyn Error + Send + Sync>> {
    let mut digest = Digest::default();
    if path.as_os_str() == "-" {
        let stdin = io::stdin();
        read_lines(stdin.lock(), &mut digest)?;
    } else {
        let file = File::open(path)?;
        read_lines(BufReader::new(file), &mut digest)?;
    }
    Ok(digest)
}

fn read_lines(
    reader: impl BufRead,
    digest: &mut Digest,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for line in reader.lines() {
        let line = line?;
        digest.lines += 1;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(&line) {
            Ok(value) => {
                digest.json_events += 1;
                ingest_event(digest, &value);
            }
            Err(_) => digest.non_json_lines.push(line),
        }
    }
    Ok(())
}

fn ingest_event(digest: &mut Digest, value: &Value) {
    let Some(event) = string_field(value, "event") else {
        return;
    };
    match event {
        "memoryarena_smart_writer.entry.start" => ingest_entry_start(digest, value),
        "memoryarena_smart_writer.mcp_read.done" => ingest_writer_read_done(digest, value),
        "memoryarena_smart_writer.llm.done" => ingest_llm_done(digest, value),
        "memoryarena_smart_writer.llm.error" => ingest_llm_error(digest, value),
        "memoryarena_smart_writer.write.dry_run.done" => ingest_dry_run_done(digest, value),
        "memoryarena_smart_writer.write.commit.start" => ingest_commit_start(digest, value),
        "memoryarena_smart_writer.write.commit.done" => ingest_commit_done(digest, value),
        "memoryarena_smart_writer.write.verify.done" => ingest_verify_done(digest, value),
        "memoryarena_mcp_navigation_probe.read.done" => ingest_probe_done(digest, value),
        _ => {}
    }
}

fn ingest_entry_start(digest: &mut Digest, value: &Value) {
    let Some(entry_ref) = owned_string_field(value, "entry_ref") else {
        return;
    };
    let entry = writer_entry(digest, &entry_ref);
    entry.about = owned_string_field(value, "about");
    entry.entry_kind = owned_string_field(value, "entry_kind");
    entry.phase = owned_string_field(value, "phase");
    entry.event_index = u64_field(value, "event_index");
    entry.subtask_index = u64_field(value, "subtask_index");
    entry.candidate_refs = string_array_field(value, "candidate_refs");
}

fn ingest_writer_read_done(digest: &mut Digest, value: &Value) {
    let Some(entry_ref) = owned_string_field(value, "entry_ref") else {
        return;
    };
    let read = ReadDigest {
        tool: owned_string_field(value, "tool").unwrap_or_else(|| "unknown".to_string()),
        target_ref: owned_string_field(value, "target_ref"),
        elapsed_ms: u128_field(value, "elapsed_ms"),
        observed_refs: string_array_field(value, "observed_entry_refs"),
    };
    let entry = writer_entry(digest, &entry_ref);
    if entry.about.is_none() {
        entry.about = owned_string_field(value, "about");
    }
    entry.reads.push(read);
}

fn ingest_llm_done(digest: &mut Digest, value: &Value) {
    let Some(entry_ref) = owned_string_field(value, "entry_ref") else {
        return;
    };
    let relations = value
        .get("connect_to")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| RelationDecision {
                    rel: owned_string_field(item, "rel").unwrap_or_else(|| "unknown".to_string()),
                    class: owned_string_field(item, "class"),
                    confidence: owned_string_field(item, "confidence"),
                    target_ref: owned_string_field(item, "ref"),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    writer_entry(digest, &entry_ref).llm = Some(LlmDecision {
        strategy: owned_string_field(value, "strategy"),
        prompt_tokens: u64_field(value, "prompt_tokens"),
        completion_tokens: u64_field(value, "completion_tokens"),
        relations,
    });
}

fn ingest_llm_error(digest: &mut Digest, value: &Value) {
    let Some(entry_ref) = owned_string_field(value, "entry_ref") else {
        return;
    };
    writer_entry(digest, &entry_ref).llm_error = owned_string_field(value, "error");
}

fn ingest_dry_run_done(digest: &mut Digest, value: &Value) {
    let Some(entry_ref) = owned_string_field(value, "entry_ref") else {
        return;
    };
    let metrics = value.get("relation_quality_metrics");
    let entry = writer_entry(digest, &entry_ref);
    if entry.about.is_none() {
        entry.about = owned_string_field(value, "about");
    }
    entry.quality = metrics.map(|metrics| RelationQuality {
        total: usize_field(metrics, "relation_total").unwrap_or_default(),
        rich: usize_field(metrics, "relation_rich_count").unwrap_or_default(),
        anemic: usize_field(metrics, "relation_anemic_count").unwrap_or_default(),
        structural: usize_field(metrics, "relation_structural_count").unwrap_or_default(),
        suspect: usize_field(metrics, "relation_suspect_count").unwrap_or_default(),
        proof_coverage: f64_field(metrics, "relation_proof_coverage"),
        prior_context_coverage: f64_field(metrics, "relation_prior_context_coverage"),
    });
}

fn ingest_commit_start(digest: &mut Digest, value: &Value) {
    let Some(entry_ref) = owned_string_field(value, "entry_ref") else {
        return;
    };
    let entry = writer_entry(digest, &entry_ref);
    if entry.about.is_none() {
        entry.about = owned_string_field(value, "about");
    }
    entry.commit_started = true;
    entry.relation_strategy = owned_string_field(value, "relation_strategy");
    entry.commit_relations = relation_decisions(value);
}

fn ingest_commit_done(digest: &mut Digest, value: &Value) {
    let Some(entry_ref) = owned_string_field(value, "entry_ref") else {
        return;
    };
    let entry = writer_entry(digest, &entry_ref);
    if entry.about.is_none() {
        entry.about = owned_string_field(value, "about");
    }
    entry.commit_elapsed_ms = u128_field(value, "elapsed_ms");
}

fn ingest_verify_done(digest: &mut Digest, value: &Value) {
    let Some(entry_ref) = owned_string_field(value, "entry_ref") else {
        return;
    };
    let entry = writer_entry(digest, &entry_ref);
    if entry.about.is_none() {
        entry.about = owned_string_field(value, "about");
    }
    entry.verify_refs = string_array_field(value, "observed_entry_refs");
}

fn ingest_probe_done(digest: &mut Digest, value: &Value) {
    let key = ProbeKey {
        event_index: u64_field(value, "event_index"),
        subtask_index: u64_field(value, "subtask_index"),
        tool: owned_string_field(value, "tool").unwrap_or_else(|| "unknown".to_string()),
        request_id: u64_field(value, "request_id"),
    };
    digest.probes.insert(
        key,
        ProbeDigest {
            about: owned_string_field(value, "about"),
            elapsed_ms: u128_field(value, "elapsed_ms"),
            observed_refs: string_array_field(value, "observed_entry_refs"),
        },
    );
}

fn writer_entry<'a>(digest: &'a mut Digest, entry_ref: &str) -> &'a mut WriterEntry {
    digest
        .writer_entries
        .entry(entry_ref.to_string())
        .or_insert_with(|| WriterEntry {
            entry_ref: entry_ref.to_string(),
            ..WriterEntry::default()
        })
}

fn render_digest(digest: &Digest, args: &Args) -> String {
    let mut output = String::new();
    let entries = sorted_entries(digest);
    let stats = DigestStats::from_entries(&entries, args.slow_ms);

    output.push_str("MemoryArena KMP log digest\n");
    output.push_str(&format!(
        "source: {}\n",
        if args.input.as_os_str() == "-" {
            "stdin".to_string()
        } else {
            args.input.display().to_string()
        }
    ));
    output.push_str(&format!(
        "events: {} json / {} lines, writer_entries={}, probe_reads={}, non_json={}\n",
        digest.json_events,
        digest.lines,
        entries.len(),
        digest.probes.len(),
        digest.non_json_lines.len()
    ));
    output.push_str(&format!(
        "writer: commits={} pending={} slow>{}ms={} max_commit={}ms\n",
        stats.commits,
        stats.pending_commits,
        args.slow_ms,
        stats.slow_commits,
        stats.max_commit_ms.unwrap_or_default()
    ));
    output.push_str(&format!(
        "context_nodes_per_relation: {} max_context_nodes={} max_about_written_before={}\n",
        format_usize_counts(&stats.context_node_counts),
        stats.max_context_nodes,
        stats.max_about_written_before
    ));
    output.push_str(&format!(
        "relations: {}\n",
        format_counts(&stats.relation_counts)
    ));
    output.push_str(&format!(
        "quality: rich={} anemic={} structural={} suspect={}\n",
        stats.rich, stats.anemic, stats.structural, stats.suspect
    ));
    output.push_str(&format!(
        "strategies: {}\n",
        format_counts(&stats.strategy_counts)
    ));
    if stats.llm_errors > 0 || !digest.non_json_lines.is_empty() {
        output.push_str(&format!(
            "errors: llm_errors={} non_json_lines={}\n",
            stats.llm_errors,
            digest.non_json_lines.len()
        ));
    }

    output.push_str("\nSmart writer decisions\n");
    let mut cumulative_seen = BTreeSet::new();
    let mut about_written_entries = BTreeMap::<String, usize>::new();
    for entry in limited_entries(&entries, args.limit_entries) {
        let context_nodes = entry.context_nodes();
        cumulative_seen.extend(context_nodes.iter().cloned());
        let about_key = entry
            .about
            .as_deref()
            .unwrap_or("unknown_about")
            .to_string();
        let about_written_before = *about_written_entries.get(&about_key).unwrap_or(&0);
        render_entry(
            &mut output,
            entry,
            args,
            context_nodes.len(),
            cumulative_seen.len(),
            about_written_before,
        );
        *about_written_entries.entry(about_key).or_default() += 1;
    }

    if !digest.probes.is_empty() {
        output.push_str("\nAsk navigation growth\n");
        render_probe_growth(&mut output, digest);
        if args.detail {
            output.push_str("\nAsk navigation probe details\n");
            for (key, probe) in &digest.probes {
                render_probe(&mut output, key, probe, args);
            }
        }
    }

    if !digest.non_json_lines.is_empty() {
        output.push_str("\nNon-JSON lines\n");
        for line in &digest.non_json_lines {
            output.push_str(&format!("- {line}\n"));
        }
    }

    output
}

fn render_entry(
    output: &mut String,
    entry: &WriterEntry,
    args: &Args,
    context_nodes: usize,
    cumulative_seen: usize,
    about_written_before: usize,
) {
    let relation = entry
        .relations()
        .first()
        .map(format_relation)
        .unwrap_or_else(|| "relation=pending".to_string());
    let reads = if entry.reads.is_empty() {
        "reads=0".to_string()
    } else {
        format!(
            "reads={} [{}]",
            entry.reads.len(),
            entry
                .reads
                .iter()
                .map(|read| read.tool.as_str())
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let quality = entry
        .quality
        .as_ref()
        .map(format_quality)
        .unwrap_or_else(|| "quality=pending".to_string());
    let commit = match (entry.commit_started, entry.commit_elapsed_ms) {
        (_, Some(elapsed)) => format!("commit={elapsed}ms"),
        (true, None) => "commit=pending".to_string(),
        (false, None) => "commit=not_started".to_string(),
    };
    let strategy = entry.relation_strategy.as_deref().unwrap_or("unknown");
    let label = format_ref(&entry.entry_ref, args.show_refs);
    let kind = entry.entry_kind.as_deref().unwrap_or("unknown");

    output.push_str(&format!(
        "- {label} {kind}: strategy={strategy}; context_nodes={context_nodes}; about_written_before={about_written_before}; {relation}; {quality}; {reads}; {commit}\n"
    ));

    if let Some(error) = entry.llm_error.as_deref() {
        output.push_str(&format!("  llm_error={error}\n"));
    }
    if args.detail {
        if !entry.candidate_refs.is_empty() {
            output.push_str(&format!(
                "  candidates: {}\n",
                format_refs(&entry.candidate_refs, args.show_refs)
            ));
        }
        output.push_str(&format!(
            "  traversal context_nodes={context_nodes} cumulative_seen={cumulative_seen} about_written_before={about_written_before}\n"
        ));
        for read in &entry.reads {
            output.push_str(&format!(
                "  read {} target={} elapsed={} observed={}\n",
                read.tool,
                read.target_ref
                    .as_deref()
                    .map(|value| format_ref(value, args.show_refs))
                    .unwrap_or_else(|| "none".to_string()),
                read.elapsed_ms
                    .map(|elapsed| format!("{elapsed}ms"))
                    .unwrap_or_else(|| "unknown".to_string()),
                format_refs(&read.observed_refs, args.show_refs)
            ));
        }
        if let Some(llm) = entry.llm.as_ref() {
            output.push_str(&format!(
                "  llm tokens prompt={} completion={}\n",
                llm.prompt_tokens
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                llm.completion_tokens
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ));
        }
        if !entry.verify_refs.is_empty() {
            output.push_str(&format!(
                "  verify observed={}\n",
                format_refs(&entry.verify_refs, args.show_refs)
            ));
        }
    }
}

fn render_probe(output: &mut String, key: &ProbeKey, probe: &ProbeDigest, args: &Args) {
    let scope = probe
        .about
        .as_deref()
        .map(|about| format_ref(about, args.show_refs))
        .unwrap_or_else(|| "unknown_about".to_string());
    output.push_str(&format!(
        "- ask event={} subtask={} tool={} scope={} elapsed={} observed={}\n",
        key.event_index
            .map(|value| value.to_string())
            .unwrap_or_else(|| "?".to_string()),
        key.subtask_index
            .map(|value| value.to_string())
            .unwrap_or_else(|| "?".to_string()),
        key.tool,
        scope,
        probe
            .elapsed_ms
            .map(|elapsed| format!("{elapsed}ms"))
            .unwrap_or_else(|| "unknown".to_string()),
        if args.detail {
            format_refs(&probe.observed_refs, args.show_refs)
        } else {
            format!("{} refs", probe.observed_refs.len())
        }
    ));
}

fn render_probe_growth(output: &mut String, digest: &Digest) {
    let mut by_subtask = BTreeMap::<u64, BTreeMap<String, Vec<&ProbeDigest>>>::new();
    for (key, probe) in &digest.probes {
        let Some(subtask_index) = key.subtask_index else {
            continue;
        };
        by_subtask
            .entry(subtask_index)
            .or_default()
            .entry(key.tool.clone())
            .or_default()
            .push(probe);
    }

    for (subtask_index, by_tool) in by_subtask {
        let mut segments = Vec::new();
        for tool in ["kernel_near", "kernel_trace", "kernel_inspect"] {
            let Some(probes) = by_tool.get(tool) else {
                continue;
            };
            let observed_counts = probes
                .iter()
                .map(|probe| probe.observed_refs.len())
                .collect::<Vec<_>>();
            let elapsed_values = probes
                .iter()
                .filter_map(|probe| probe.elapsed_ms)
                .collect::<Vec<_>>();
            segments.push(format!(
                "{} observed={} avg_elapsed={}ms n={}",
                short_tool(tool),
                format_usize_range(&observed_counts),
                average_u128(&elapsed_values).unwrap_or_default(),
                probes.len()
            ));
        }
        output.push_str(&format!(
            "- subtask={subtask_index}: {}\n",
            segments.join("; ")
        ));
    }
}

fn short_tool(tool: &str) -> &str {
    tool.strip_prefix("kernel_").unwrap_or(tool)
}

fn format_usize_range(values: &[usize]) -> String {
    match (values.iter().min(), values.iter().max()) {
        (Some(min), Some(max)) if min == max => format!("{min} refs"),
        (Some(min), Some(max)) => format!("{min}-{max} refs"),
        _ => "0 refs".to_string(),
    }
}

fn average_u128(values: &[u128]) -> Option<u128> {
    if values.is_empty() {
        return None;
    }
    Some(values.iter().sum::<u128>() / values.len() as u128)
}

fn format_relation(relation: &RelationDecision) -> String {
    let target = relation
        .target_ref
        .as_deref()
        .map(|value| format_ref(value, false))
        .unwrap_or_else(|| "unknown".to_string());
    match (relation.class.as_deref(), relation.confidence.as_deref()) {
        (Some(class), Some(confidence)) => {
            format!(
                "relation={}/{}({}) -> {}",
                relation.rel, class, confidence, target
            )
        }
        (Some(class), None) => format!("relation={}/{} -> {}", relation.rel, class, target),
        _ => format!("relation={} -> {}", relation.rel, target),
    }
}

fn format_quality(quality: &RelationQuality) -> String {
    let label = if quality.suspect > 0 {
        "suspect"
    } else if quality.rich > 0 && quality.rich == quality.total {
        "rich"
    } else if quality.anemic > 0 && quality.anemic == quality.total {
        "anemic"
    } else if quality.structural > 0 && quality.structural == quality.total {
        "structural"
    } else if quality.total == 0 {
        "empty"
    } else {
        "mixed"
    };
    format!(
        "quality={} rich={}/{} anemic={} structural={} proof={} prior={}",
        label,
        quality.rich,
        quality.total,
        quality.anemic,
        quality.structural,
        format_ratio(quality.proof_coverage),
        format_ratio(quality.prior_context_coverage)
    )
}

fn format_ratio(value: Option<f64>) -> String {
    value
        .map(|value| format!("{:.0}%", value * 100.0))
        .unwrap_or_else(|| "?".to_string())
}

fn sorted_entries(digest: &Digest) -> Vec<&WriterEntry> {
    let mut entries = digest.writer_entries.values().collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        (
            left.event_index.unwrap_or(u64::MAX),
            left.subtask_index.unwrap_or(u64::MAX),
            ref_sort_key(&left.entry_ref),
        )
            .cmp(&(
                right.event_index.unwrap_or(u64::MAX),
                right.subtask_index.unwrap_or(u64::MAX),
                ref_sort_key(&right.entry_ref),
            ))
    });
    entries
}

fn limited_entries<'a>(
    entries: &'a [&'a WriterEntry],
    limit_entries: Option<usize>,
) -> Vec<&'a WriterEntry> {
    match limit_entries {
        Some(limit) => entries.iter().copied().take(limit).collect(),
        None => entries.to_vec(),
    }
}

#[derive(Debug, Default)]
struct DigestStats {
    commits: usize,
    pending_commits: usize,
    slow_commits: usize,
    max_commit_ms: Option<u128>,
    rich: usize,
    anemic: usize,
    structural: usize,
    suspect: usize,
    llm_errors: usize,
    max_context_nodes: usize,
    max_about_written_before: usize,
    context_node_counts: BTreeMap<usize, usize>,
    relation_counts: BTreeMap<String, usize>,
    strategy_counts: BTreeMap<String, usize>,
}

impl DigestStats {
    fn from_entries(entries: &[&WriterEntry], slow_ms: u128) -> Self {
        let mut stats = DigestStats::default();
        let mut about_written_entries = BTreeMap::<String, usize>::new();
        for entry in entries {
            let context_nodes = entry.context_nodes().len();
            stats.max_context_nodes = stats.max_context_nodes.max(context_nodes);
            *stats.context_node_counts.entry(context_nodes).or_default() += 1;
            let about_key = entry
                .about
                .as_deref()
                .unwrap_or("unknown_about")
                .to_string();
            let about_written_before = *about_written_entries.get(&about_key).unwrap_or(&0);
            stats.max_about_written_before =
                stats.max_about_written_before.max(about_written_before);
            *about_written_entries.entry(about_key).or_default() += 1;

            if entry.commit_started && entry.commit_elapsed_ms.is_none() {
                stats.pending_commits += 1;
            }
            if let Some(elapsed) = entry.commit_elapsed_ms {
                stats.commits += 1;
                stats.max_commit_ms = Some(stats.max_commit_ms.unwrap_or_default().max(elapsed));
                if elapsed > slow_ms {
                    stats.slow_commits += 1;
                }
            }
            if let Some(quality) = entry.quality.as_ref() {
                stats.rich += quality.rich;
                stats.anemic += quality.anemic;
                stats.structural += quality.structural;
                stats.suspect += quality.suspect;
            }
            if entry.llm_error.is_some() {
                stats.llm_errors += 1;
            }
            if let Some(strategy) = entry.relation_strategy.as_deref() {
                *stats
                    .strategy_counts
                    .entry(strategy.to_string())
                    .or_default() += 1;
            }
            if let Some(llm) = entry.llm.as_ref() {
                for relation in &llm.relations {
                    *stats
                        .relation_counts
                        .entry(relation.rel.to_string())
                        .or_default() += 1;
                }
            } else {
                for relation in &entry.commit_relations {
                    *stats
                        .relation_counts
                        .entry(relation.rel.to_string())
                        .or_default() += 1;
                }
            }
        }
        stats
    }
}

fn format_counts(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    counts
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_usize_counts(counts: &BTreeMap<usize, usize>) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    counts
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_refs(refs: &[String], show_refs: bool) -> String {
    if refs.is_empty() {
        return "none".to_string();
    }
    refs.iter()
        .map(|value| format_ref(value, show_refs))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_ref(value: &str, show_ref: bool) -> String {
    if show_ref {
        return value.to_string();
    }
    ref_alias(value).unwrap_or_else(|| compact_unknown_ref(value))
}

fn ref_alias(value: &str) -> Option<String> {
    let parts = value.split(':').collect::<Vec<_>>();
    let task = value_after(&parts, "task")?;
    let subtask = value_after(&parts, "subtask");
    if let Some(subtask) = subtask {
        let role = parts.last().copied().unwrap_or("entry");
        return Some(format!("t{task}/s{subtask}/{role}"));
    }
    if let Some(dimension) = value_after(&parts, "dimension") {
        return Some(format!("t{task}/dim:{dimension}"));
    }
    Some(format!("t{task}/about"))
}

fn compact_unknown_ref(value: &str) -> String {
    let mut parts = value.rsplit(':').take(3).collect::<Vec<_>>();
    parts.reverse();
    parts.join(":")
}

fn ref_sort_key(value: &str) -> String {
    ref_alias(value).unwrap_or_else(|| value.to_string())
}

fn value_after<'a>(parts: &'a [&str], key: &str) -> Option<&'a str> {
    parts
        .iter()
        .position(|part| *part == key)
        .and_then(|index| parts.get(index + 1).copied())
}

fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn owned_string_field(value: &Value, key: &str) -> Option<String> {
    string_field(value, key).map(ToString::to_string)
}

fn u64_field(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

fn u128_field(value: &Value, key: &str) -> Option<u128> {
    u64_field(value, key).map(u128::from)
}

fn usize_field(value: &Value, key: &str) -> Option<usize> {
    u64_field(value, key).and_then(|value| usize::try_from(value).ok())
}

fn f64_field(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

fn string_array_field(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn relation_decisions(value: &Value) -> Vec<RelationDecision> {
    value
        .get("connect_to")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| RelationDecision {
                    rel: owned_string_field(item, "rel").unwrap_or_else(|| "unknown".to_string()),
                    class: owned_string_field(item, "class"),
                    confidence: owned_string_field(item, "confidence"),
                    target_ref: owned_string_field(item, "ref"),
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_alias_compacts_memoryarena_refs() {
        assert_eq!(
            ref_alias("memoryarena:run:r1:task_type:progressive_search:task:2:subtask:6:answer")
                .as_deref(),
            Some("t2/s6/answer")
        );
        assert_eq!(
            ref_alias("memoryarena:run:r1:task_type:progressive_search:task:2").as_deref(),
            Some("t2/about")
        );
        assert_eq!(
            ref_alias(
                "about:memoryarena:run:r1:task_type:progressive_search:task:2:dimension:episode-1"
            )
            .as_deref(),
            Some("t2/dim:episode-1")
        );
    }

    #[test]
    fn digest_renders_compact_writer_decision() {
        let mut digest = Digest::default();
        let entry = "memoryarena:run:r1:task_type:progressive_search:task:1:subtask:10:question";
        for event in [
            serde_json::json!({
                "event": "memoryarena_smart_writer.entry.start",
                "entry_ref": entry,
                "entry_kind": "subtask_question",
                "event_index": 29,
                "subtask_index": 10,
                "candidate_refs": ["memoryarena:run:r1:task_type:progressive_search:task:1:subtask:9:answer"]
            }),
            serde_json::json!({
                "event": "memoryarena_smart_writer.mcp_read.done",
                "entry_ref": entry,
                "tool": "kernel_near",
                "target_ref": "memoryarena:run:r1:task_type:progressive_search:task:1:subtask:9:answer",
                "elapsed_ms": 702,
                "observed_entry_refs": ["memoryarena:run:r1:task_type:progressive_search:task:1:subtask:9:answer"]
            }),
            serde_json::json!({
                "event": "memoryarena_smart_writer.llm.done",
                "entry_ref": entry,
                "strategy": "llm",
                "prompt_tokens": 864,
                "completion_tokens": 172,
                "connect_to": [{
                    "rel": "depends_on",
                    "class": "causal",
                    "confidence": "high",
                    "ref": "memoryarena:run:r1:task_type:progressive_search:task:1:subtask:9:answer"
                }]
            }),
            serde_json::json!({
                "event": "memoryarena_smart_writer.write.dry_run.done",
                "entry_ref": entry,
                "relation_quality_metrics": {
                    "relation_total": 1,
                    "relation_rich_count": 1,
                    "relation_anemic_count": 0,
                    "relation_structural_count": 0,
                    "relation_suspect_count": 0,
                    "relation_proof_coverage": 1.0,
                    "relation_prior_context_coverage": 1.0
                }
            }),
            serde_json::json!({
                "event": "memoryarena_smart_writer.write.commit.start",
                "entry_ref": entry,
                "relation_strategy": "llm"
            }),
            serde_json::json!({
                "event": "memoryarena_smart_writer.write.commit.done",
                "entry_ref": entry,
                "elapsed_ms": 11514
            }),
        ] {
            digest.json_events += 1;
            ingest_event(&mut digest, &event);
        }

        let rendered = render_digest(
            &digest,
            &Args {
                input: "-".into(),
                output: None,
                detail: false,
                show_refs: false,
                slow_ms: 10_000,
                limit_entries: None,
            },
        );

        assert!(rendered.contains("writer_entries=1"));
        assert!(rendered.contains("slow>10000ms=1"));
        assert!(rendered.contains("t1/s10/question subtask_question"));
        assert!(rendered.contains("relation=depends_on/causal(high) -> t1/s9/answer"));
        assert!(rendered.contains("quality=rich"));
        assert!(rendered.contains("commit=11514ms"));
    }

    #[test]
    fn read_lines_keeps_non_json_diagnostics() {
        let mut digest = Digest::default();
        read_lines("not json\n".as_bytes(), &mut digest).expect("read should work");
        assert_eq!(digest.lines, 1);
        assert_eq!(digest.non_json_lines, vec!["not json"]);
    }
}
