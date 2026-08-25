use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

use kmp_mcp::kmp_mcp_tool_names;
use kmp_testkit::{
    kernel_operator_allowed_full_tools, kernel_operator_allowed_read_tools,
    kernel_operator_is_bounded_tool_call,
};

const REPORTER: &str = "kernel-operator-contract-coverage-v1";
const ACTION_CONTRACT: &str = "kernel-operator-action-contract-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    trajectories: Option<PathBuf>,
    output: Option<PathBuf>,
    profile: Profile,
    fail_under: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Profile {
    Read,
    Full,
    WriterPreRead,
}

#[derive(Debug, Serialize)]
struct CoverageReport {
    reporter: &'static str,
    action_contract: &'static str,
    generated_at_unix_seconds: u64,
    profile: &'static str,
    trajectories: Option<String>,
    fail_under: Option<u64>,
    overall_contract_coverage: CoverageRatio,
    profile_contract_coverage: CoverageRatio,
    training_coverage: Option<TrainingCoverage>,
    required_capabilities: Vec<CapabilityRow>,
    unsupported_mcp_tools: Vec<String>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct TrainingCoverage {
    target_capability_coverage: CoverageRatio,
    target_tools: BTreeMap<String, usize>,
    target_cursor_modes: BTreeMap<String, usize>,
    target_dimension_modes: BTreeMap<String, usize>,
    target_dimension_scopes: BTreeMap<String, usize>,
    target_trace_page_modes: BTreeMap<String, usize>,
    missing_capabilities: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CapabilityRow {
    id: String,
    group: &'static str,
    required_for_profile: bool,
    contract_supported: bool,
    training_observed: Option<bool>,
}

#[derive(Debug, Serialize)]
struct CoverageRatio {
    covered: usize,
    total: usize,
    percent: f64,
}

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
    let trajectories = args.trajectories.as_deref().map(read_jsonl).transpose()?;
    let report = build_report(&args, trajectories.as_deref())?;
    let rendered = serde_json::to_string_pretty(&report)?;
    if let Some(output) = args.output.as_deref() {
        let file = File::create(output)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(rendered.as_bytes())?;
        writer.write_all(b"\n")?;
        writer.flush()?;
    }
    println!("{rendered}");
    if let Some(fail_under) = args.fail_under {
        if report.profile_contract_coverage.percent < fail_under as f64 {
            return Err(format!(
                "profile contract coverage {:.2}% is below fail-under {}%",
                report.profile_contract_coverage.percent, fail_under
            )
            .into());
        }
        if let Some(training) = &report.training_coverage
            && training.target_capability_coverage.percent < fail_under as f64
        {
            return Err(format!(
                "target capability coverage {:.2}% is below fail-under {}%",
                training.target_capability_coverage.percent, fail_under
            )
            .into());
        }
    }
    Ok(())
}

fn build_report(
    args: &Args,
    trajectories: Option<&[Value]>,
) -> Result<CoverageReport, Box<dyn Error + Send + Sync>> {
    let mcp_tools = kmp_mcp_tool_names().into_iter().collect::<BTreeSet<_>>();
    let operator_tools = match args.profile {
        Profile::Read => kernel_operator_allowed_read_tools(),
        Profile::Full => kernel_operator_allowed_full_tools(),
        Profile::WriterPreRead => kernel_operator_allowed_read_tools(),
    }
    .into_iter()
    .collect::<BTreeSet<_>>();
    let required_ids = required_capability_ids(args.profile);
    let observed = trajectories.map(observed_capabilities);
    let required_rows = required_ids
        .iter()
        .map(|capability| CapabilityRow {
            id: capability.id.to_string(),
            group: capability.group,
            required_for_profile: true,
            contract_supported: contract_supports(capability.id, &mcp_tools, &operator_tools),
            training_observed: observed
                .as_ref()
                .map(|coverage| coverage.capabilities.contains(capability.id)),
        })
        .collect::<Vec<_>>();
    let profile_supported = required_rows
        .iter()
        .filter(|row| row.contract_supported)
        .count();
    let unsupported_mcp_tools = mcp_tools
        .iter()
        .filter(|tool| !operator_tools.contains(*tool))
        .cloned()
        .collect::<Vec<_>>();
    let overall_contract_coverage = ratio(
        mcp_tools
            .iter()
            .filter(|tool| operator_tools.contains(*tool))
            .count(),
        mcp_tools.len(),
    );
    let training_coverage = observed.map(|observed| {
        let covered = required_ids
            .iter()
            .filter(|capability| observed.capabilities.contains(capability.id))
            .count();
        let missing_capabilities = required_ids
            .iter()
            .filter(|capability| !observed.capabilities.contains(capability.id))
            .map(|capability| capability.id.to_string())
            .collect();
        TrainingCoverage {
            target_capability_coverage: ratio(covered, required_ids.len()),
            target_tools: observed.target_tools,
            target_cursor_modes: observed.cursor_modes,
            target_dimension_modes: observed.dimension_modes,
            target_dimension_scopes: observed.dimension_scopes,
            target_trace_page_modes: observed.trace_page_modes,
            missing_capabilities,
        }
    });
    Ok(CoverageReport {
        reporter: REPORTER,
        action_contract: ACTION_CONTRACT,
        generated_at_unix_seconds: now_unix_seconds()?,
        profile: args.profile.name(),
        trajectories: args
            .trajectories
            .as_ref()
            .map(|path| path.display().to_string()),
        fail_under: args.fail_under,
        overall_contract_coverage,
        profile_contract_coverage: ratio(profile_supported, required_rows.len()),
        training_coverage,
        required_capabilities: required_rows,
        unsupported_mcp_tools,
        notes: vec![
            "overall_contract_coverage compares Operator tools against the entire MCP tool list; write tools can be intentionally outside a read profile.".to_string(),
            "profile_contract_coverage must be 100% before placing Operator in front of that profile.".to_string(),
            "target_capability_coverage must be 100% for the training/eval set before claiming the model has learned that profile.".to_string(),
        ],
    })
}

#[derive(Debug)]
struct Capability {
    id: &'static str,
    group: &'static str,
}

fn required_capability_ids(profile: Profile) -> Vec<Capability> {
    if profile == Profile::WriterPreRead {
        return vec![
            Capability {
                id: "mode:write_context_read",
                group: "mode",
            },
            Capability {
                id: "tool:kmp_near",
                group: "tool",
            },
            Capability {
                id: "tool:kmp_inspect",
                group: "tool",
            },
            Capability {
                id: "tool:kmp_trace",
                group: "tool",
            },
            Capability {
                id: "tool:stop",
                group: "tool",
            },
            Capability {
                id: "cursor:ref",
                group: "cursor",
            },
            Capability {
                id: "dimensions.mode:all",
                group: "dimensions",
            },
            Capability {
                id: "dimensions.scope:current_about",
                group: "dimensions",
            },
            Capability {
                id: "window:expand",
                group: "window_policy",
            },
            Capability {
                id: "window:shrink",
                group: "window_policy",
            },
            Capability {
                id: "inspect.raw:false",
                group: "security",
            },
            Capability {
                id: "trace.page:first",
                group: "pagination",
            },
            Capability {
                id: "trace.page:continue",
                group: "pagination",
            },
            Capability {
                id: "window:stop_sufficient",
                group: "window_policy",
            },
            Capability {
                id: "writer.last_tool:none",
                group: "writer_state",
            },
            Capability {
                id: "writer.last_tool:kmp_near",
                group: "writer_state",
            },
            Capability {
                id: "writer.last_tool:kmp_inspect",
                group: "writer_state",
            },
            Capability {
                id: "writer.last_tool:kmp_trace",
                group: "writer_state",
            },
            Capability {
                id: "writer.candidate_role:previous_subtask_answer",
                group: "writer_candidate",
            },
            Capability {
                id: "writer.candidate_role:same_subtask_question",
                group: "writer_candidate",
            },
            Capability {
                id: "writer.candidate_pool:ambiguous",
                group: "writer_candidate",
            },
        ];
    }
    let mut capabilities = vec![
        Capability {
            id: "tool:kmp_wake",
            group: "tool",
        },
        Capability {
            id: "tool:kmp_ask",
            group: "tool",
        },
        Capability {
            id: "tool:kmp_near",
            group: "tool",
        },
        Capability {
            id: "tool:kmp_goto",
            group: "tool",
        },
        Capability {
            id: "tool:kmp_rewind",
            group: "tool",
        },
        Capability {
            id: "tool:kmp_forward",
            group: "tool",
        },
        Capability {
            id: "tool:kmp_trace",
            group: "tool",
        },
        Capability {
            id: "tool:kmp_inspect",
            group: "tool",
        },
        Capability {
            id: "tool:stop",
            group: "tool",
        },
        Capability {
            id: "cursor:ref",
            group: "cursor",
        },
        Capability {
            id: "cursor:time",
            group: "cursor",
        },
        Capability {
            id: "cursor:sequence",
            group: "cursor",
        },
        Capability {
            id: "dimensions.mode:all",
            group: "dimensions",
        },
        Capability {
            id: "dimensions.mode:only",
            group: "dimensions",
        },
        Capability {
            id: "dimensions.mode:except",
            group: "dimensions",
        },
        Capability {
            id: "dimensions.scope:current_about",
            group: "dimensions",
        },
        Capability {
            id: "dimensions.scope:abouts",
            group: "dimensions",
        },
        Capability {
            id: "dimensions.scope:all_abouts",
            group: "dimensions",
        },
        Capability {
            id: "trace.page:first",
            group: "pagination",
        },
        Capability {
            id: "trace.page:continue",
            group: "pagination",
        },
        Capability {
            id: "window:expand",
            group: "window_policy",
        },
        Capability {
            id: "window:shrink",
            group: "window_policy",
        },
        Capability {
            id: "window:stop_sufficient",
            group: "window_policy",
        },
        Capability {
            id: "inspect.raw:false",
            group: "security",
        },
    ];
    if profile == Profile::Full {
        capabilities.extend([
            Capability {
                id: "tool:kmp_ingest",
                group: "tool",
            },
            Capability {
                id: "tool:kmp_write_memory",
                group: "tool",
            },
            Capability {
                id: "write:relation_quality",
                group: "write",
            },
            Capability {
                id: "write:read_context_proof",
                group: "write",
            },
        ]);
    }
    capabilities
}

fn contract_supports(
    id: &str,
    mcp_tools: &BTreeSet<String>,
    operator_tools: &BTreeSet<String>,
) -> bool {
    if let Some(tool) = id.strip_prefix("tool:") {
        return tool == "stop" || (mcp_tools.contains(tool) && operator_tools.contains(tool));
    }
    match id {
        "cursor:ref" | "cursor:time" | "cursor:sequence" => bounded_temporal_cursor(id),
        "dimensions.mode:all" | "dimensions.mode:only" | "dimensions.mode:except" => {
            validated_dimension_mode(id)
        }
        "dimensions.scope:current_about"
        | "dimensions.scope:abouts"
        | "dimensions.scope:all_abouts" => validated_dimension_scope(id),
        "trace.page:first" | "trace.page:continue" => bounded_trace_page(id),
        "window:expand" | "window:shrink" | "window:stop_sufficient" => true,
        "inspect.raw:false" => bounded_inspect_raw_false(),
        "mode:write_context_read"
        | "writer.last_tool:none"
        | "writer.last_tool:kmp_near"
        | "writer.last_tool:kmp_inspect"
        | "writer.last_tool:kmp_trace"
        | "writer.candidate_role:previous_subtask_answer"
        | "writer.candidate_role:same_subtask_question"
        | "writer.candidate_pool:ambiguous" => true,
        "write:relation_quality" | "write:read_context_proof" => {
            mcp_tools.contains("kmp_write_memory") && operator_tools.contains("kmp_write_memory")
        }
        _ => false,
    }
}

fn bounded_temporal_cursor(id: &str) -> bool {
    let cursor = match id {
        "cursor:ref" => serde_json::json!({ "ref": "node:1" }),
        "cursor:time" => serde_json::json!({ "time": "2026-05-14T00:00:00Z" }),
        "cursor:sequence" => serde_json::json!({ "sequence": 1 }),
        _ => return false,
    };
    let arguments = serde_json::json!({
        "about": "about:1",
        "around": cursor,
        "dimensions": { "mode": "all", "scope": "current_about" },
        "include": { "evidence": true, "raw_refs": false, "relations": true },
        "limit": { "entries": 12, "tokens": 2400 },
        "budget": { "depth": 3, "tokens": 2400 },
        "window": { "before_entries": 6, "after_entries": 0 }
    });
    kernel_operator_is_bounded_tool_call("kmp_near", &arguments)
}

fn validated_dimension_mode(id: &str) -> bool {
    let dimensions = match id {
        "dimensions.mode:all" => serde_json::json!({ "mode": "all", "scope": "current_about" }),
        "dimensions.mode:only" => {
            serde_json::json!({ "mode": "only", "include": ["agent"], "scope": "current_about" })
        }
        "dimensions.mode:except" => {
            serde_json::json!({ "mode": "except", "exclude": ["discarded"], "scope": "current_about" })
        }
        _ => return false,
    };
    action_contract_accepts_dimensions(dimensions)
}

fn validated_dimension_scope(id: &str) -> bool {
    let dimensions = match id {
        "dimensions.scope:current_about" => {
            serde_json::json!({ "mode": "all", "scope": "current_about" })
        }
        "dimensions.scope:abouts" => {
            serde_json::json!({ "mode": "all", "scope": "abouts", "abouts": ["about:1"] })
        }
        "dimensions.scope:all_abouts" => {
            serde_json::json!({ "mode": "all", "scope": "all_abouts" })
        }
        _ => return false,
    };
    action_contract_accepts_dimensions(dimensions)
}

fn action_contract_accepts_dimensions(dimensions: Value) -> bool {
    let action = serde_json::json!({
        "type": "tool_call",
        "tool": "kmp_ask",
        "arguments": {
            "about": "about:1",
            "answer_policy": "evidence_or_unknown",
            "dimensions": dimensions,
            "question": "What changed?",
            "budget": { "tokens": 2400 }
        }
    });
    kmp_testkit::kernel_operator_action_contract_error(&action).is_none()
}

fn bounded_trace_page(id: &str) -> bool {
    let page = match id {
        "trace.page:first" => serde_json::json!({ "entries": 16 }),
        "trace.page:continue" => serde_json::json!({ "entries": 16, "cursor": "page:next" }),
        _ => return false,
    };
    let arguments = serde_json::json!({
        "from": "node:1",
        "to": "node:2",
        "budget": { "depth": 2, "tokens": 2400 },
        "page": page
    });
    kernel_operator_is_bounded_tool_call("kmp_trace", &arguments)
}

fn bounded_inspect_raw_false() -> bool {
    kernel_operator_is_bounded_tool_call(
        "kmp_inspect",
        &serde_json::json!({
            "ref": "node:1",
            "include": { "details": true, "incoming": true, "outgoing": true, "raw": false }
        }),
    )
}

#[derive(Debug, Default)]
struct ObservedCoverage {
    capabilities: BTreeSet<&'static str>,
    target_tools: BTreeMap<String, usize>,
    cursor_modes: BTreeMap<String, usize>,
    dimension_modes: BTreeMap<String, usize>,
    dimension_scopes: BTreeMap<String, usize>,
    trace_page_modes: BTreeMap<String, usize>,
}

fn observed_capabilities(rows: &[Value]) -> ObservedCoverage {
    let mut observed = ObservedCoverage::default();
    for row in rows {
        let Some(action) = row.get("target_action") else {
            continue;
        };
        observe_trajectory(row, &mut observed);
        observe_action(action, &mut observed);
    }
    observed
}

fn observe_trajectory(row: &Value, observed: &mut ObservedCoverage) {
    if row.get("mode").and_then(Value::as_str) == Some("write_context_read") {
        observed.capabilities.insert("mode:write_context_read");
        observe_writer_pre_read_state(row, observed);
    }
}

fn observe_writer_pre_read_state(row: &Value, observed: &mut ObservedCoverage) {
    let Some(state) = row.get("visible_state") else {
        return;
    };
    match state.get("last_tool").and_then(Value::as_str) {
        Some("kmp_near") => {
            observed.capabilities.insert("writer.last_tool:kmp_near");
        }
        Some("kmp_inspect") => {
            observed.capabilities.insert("writer.last_tool:kmp_inspect");
        }
        Some("kmp_trace") => {
            observed.capabilities.insert("writer.last_tool:kmp_trace");
        }
        None if state.get("last_tool").is_some_and(Value::is_null) => {
            observed.capabilities.insert("writer.last_tool:none");
        }
        _ => {}
    }
    if state.get("candidate_pool").and_then(Value::as_str) == Some("ambiguous") {
        observed
            .capabilities
            .insert("writer.candidate_pool:ambiguous");
    }
    let Some(candidate_details) = state.get("candidate_ref_details").and_then(Value::as_array)
    else {
        return;
    };
    for detail in candidate_details {
        match detail.get("role").and_then(Value::as_str) {
            Some("previous_subtask_answer") => {
                observed
                    .capabilities
                    .insert("writer.candidate_role:previous_subtask_answer");
            }
            Some("same_subtask_question") => {
                observed
                    .capabilities
                    .insert("writer.candidate_role:same_subtask_question");
            }
            _ => {}
        }
    }
}

fn observe_action(action: &Value, observed: &mut ObservedCoverage) {
    match action.get("type").and_then(Value::as_str) {
        Some("stop") => {
            observed.capabilities.insert("tool:stop");
            observed.capabilities.insert("window:stop_sufficient");
            *observed.target_tools.entry("stop".to_string()).or_default() += 1;
        }
        Some("tool_call") => {}
        _ => return,
    }
    let Some(tool) = action.get("tool").and_then(Value::as_str) else {
        return;
    };
    *observed.target_tools.entry(tool.to_string()).or_default() += 1;
    if let Some(capability) = match_tool_capability(tool) {
        observed.capabilities.insert(capability);
    }
    let Some(arguments) = action.get("arguments") else {
        return;
    };
    observe_dimensions(arguments, observed);
    match tool {
        "kmp_near" => observe_cursor(arguments.get("around"), observed),
        "kmp_goto" => observe_cursor(arguments.get("at"), observed),
        "kmp_rewind" | "kmp_forward" => observe_cursor(arguments.get("from"), observed),
        "kmp_trace" => observe_trace_page(arguments, observed),
        "kmp_inspect"
            if arguments.pointer("/include/raw").and_then(Value::as_bool) == Some(false) =>
        {
            observed.capabilities.insert("inspect.raw:false");
        }
        "kmp_write_memory" => observe_write_memory(arguments, observed),
        _ => {}
    }
    observe_window(arguments, observed);
}

fn match_tool_capability(tool: &str) -> Option<&'static str> {
    match tool {
        "kmp_wake" => Some("tool:kmp_wake"),
        "kmp_ask" => Some("tool:kmp_ask"),
        "kmp_near" => Some("tool:kmp_near"),
        "kmp_goto" => Some("tool:kmp_goto"),
        "kmp_rewind" => Some("tool:kmp_rewind"),
        "kmp_forward" => Some("tool:kmp_forward"),
        "kmp_trace" => Some("tool:kmp_trace"),
        "kmp_inspect" => Some("tool:kmp_inspect"),
        "kmp_ingest" => Some("tool:kmp_ingest"),
        "kmp_write_memory" => Some("tool:kmp_write_memory"),
        _ => None,
    }
}

fn observe_dimensions(arguments: &Value, observed: &mut ObservedCoverage) {
    let Some(dimensions) = arguments.get("dimensions") else {
        return;
    };
    if let Some(mode) = dimensions.get("mode").and_then(Value::as_str) {
        *observed
            .dimension_modes
            .entry(mode.to_string())
            .or_default() += 1;
        match mode {
            "all" => {
                observed.capabilities.insert("dimensions.mode:all");
            }
            "only" => {
                observed.capabilities.insert("dimensions.mode:only");
            }
            "except" => {
                observed.capabilities.insert("dimensions.mode:except");
            }
            _ => {}
        }
    }
    if let Some(scope) = dimensions.get("scope").and_then(Value::as_str) {
        *observed
            .dimension_scopes
            .entry(scope.to_string())
            .or_default() += 1;
        match scope {
            "current_about" => {
                observed
                    .capabilities
                    .insert("dimensions.scope:current_about");
            }
            "abouts" => {
                observed.capabilities.insert("dimensions.scope:abouts");
            }
            "all_abouts" => {
                observed.capabilities.insert("dimensions.scope:all_abouts");
            }
            _ => {}
        }
    }
}

fn observe_cursor(cursor: Option<&Value>, observed: &mut ObservedCoverage) {
    let Some(cursor) = cursor else {
        return;
    };
    let mode = if cursor.get("ref").is_some() {
        Some("ref")
    } else if cursor.get("time").is_some() {
        Some("time")
    } else if cursor.get("sequence").is_some() {
        Some("sequence")
    } else {
        None
    };
    if let Some(mode) = mode {
        *observed.cursor_modes.entry(mode.to_string()).or_default() += 1;
        match mode {
            "ref" => {
                observed.capabilities.insert("cursor:ref");
            }
            "time" => {
                observed.capabilities.insert("cursor:time");
            }
            "sequence" => {
                observed.capabilities.insert("cursor:sequence");
            }
            _ => {}
        }
    }
}

fn observe_trace_page(arguments: &Value, observed: &mut ObservedCoverage) {
    let Some(page) = arguments.get("page") else {
        return;
    };
    if page.get("cursor").is_some() {
        *observed
            .trace_page_modes
            .entry("continue".to_string())
            .or_default() += 1;
        observed.capabilities.insert("trace.page:continue");
    } else {
        *observed
            .trace_page_modes
            .entry("first".to_string())
            .or_default() += 1;
        observed.capabilities.insert("trace.page:first");
    }
}

fn observe_window(arguments: &Value, observed: &mut ObservedCoverage) {
    let before = arguments
        .pointer("/window/before_entries")
        .and_then(Value::as_u64);
    let entries = arguments.pointer("/limit/entries").and_then(Value::as_u64);
    if before.is_some_and(|value| value > 6) || entries.is_some_and(|value| value > 12) {
        observed.capabilities.insert("window:expand");
    }
    if before.is_some_and(|value| value < 6) || entries.is_some_and(|value| value < 12) {
        observed.capabilities.insert("window:shrink");
    }
}

fn observe_write_memory(arguments: &Value, observed: &mut ObservedCoverage) {
    let relation_refs = arguments
        .get("connect_to")
        .and_then(Value::as_array)
        .map(|links| {
            links
                .iter()
                .filter_map(|link| link.get("ref").and_then(Value::as_str))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let relation_quality_observed = arguments
        .get("connect_to")
        .and_then(Value::as_array)
        .is_some_and(|links| {
            !links.is_empty()
                && links.iter().all(|link| {
                    ["ref", "rel", "class", "why", "evidence"]
                        .iter()
                        .all(|field| {
                            link.get(*field)
                                .and_then(Value::as_str)
                                .is_some_and(|value| !value.is_empty())
                        })
                })
        });
    if relation_quality_observed {
        observed.capabilities.insert("write:relation_quality");
    }

    let read_context_refs = write_read_context_refs(arguments.get("read_context"));
    if relation_refs
        .iter()
        .any(|relation_ref| read_context_refs.contains(*relation_ref))
    {
        observed.capabilities.insert("write:read_context_proof");
    }
}

fn write_read_context_refs(read_context: Option<&Value>) -> BTreeSet<&str> {
    let mut refs = BTreeSet::new();
    let Some(read_context) = read_context else {
        return refs;
    };
    for field in ["inspected_refs", "temporal_refs", "wake_refs", "ask_refs"] {
        if let Some(values) = read_context.get(field).and_then(Value::as_array) {
            for value in values {
                if let Some(ref_id) = value.as_str() {
                    refs.insert(ref_id);
                }
            }
        }
    }
    if let Some(paths) = read_context.get("trace_paths").and_then(Value::as_array) {
        for path in paths {
            for field in ["from", "to"] {
                if let Some(ref_id) = path.get(field).and_then(Value::as_str) {
                    refs.insert(ref_id);
                }
            }
            if let Some(values) = path.get("refs").and_then(Value::as_array) {
                for value in values {
                    if let Some(ref_id) = value.as_str() {
                        refs.insert(ref_id);
                    }
                }
            }
        }
    }
    refs
}

fn ratio(covered: usize, total: usize) -> CoverageRatio {
    let percent = if total == 0 {
        100.0
    } else {
        covered as f64 * 100.0 / total as f64
    };
    CoverageRatio {
        covered,
        total,
        percent,
    }
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, Box<dyn Error + Send + Sync>> {
    let mut trajectories = None;
    let mut output = None;
    let mut profile = Profile::Read;
    let mut fail_under = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--trajectories" => {
                trajectories = Some(PathBuf::from(next_arg(&mut args, "--trajectories")?));
            }
            "--output" => output = Some(PathBuf::from(next_arg(&mut args, "--output")?)),
            "--profile" => profile = parse_profile(&next_arg(&mut args, "--profile")?)?,
            "--fail-under" => fail_under = Some(next_arg(&mut args, "--fail-under")?.parse()?),
            "--help" | "-h" => return Err(usage().into()),
            value if value.starts_with('-') => {
                return Err(format!("unknown argument: {value}\n{}", usage()).into());
            }
            value => {
                if trajectories.is_some() {
                    return Err(format!("unexpected positional argument: {value}").into());
                }
                trajectories = Some(PathBuf::from(value));
            }
        }
    }
    Ok(Args {
        trajectories,
        output,
        profile,
        fail_under,
    })
}

fn parse_profile(value: &str) -> Result<Profile, Box<dyn Error + Send + Sync>> {
    match value {
        "read" => Ok(Profile::Read),
        "full" => Ok(Profile::Full),
        "writer-pre-read" => Ok(Profile::WriterPreRead),
        other => {
            Err(format!("unknown profile `{other}`; expected read|full|writer-pre-read").into())
        }
    }
}

impl Profile {
    fn name(self) -> &'static str {
        match self {
            Profile::Read => "read",
            Profile::Full => "full",
            Profile::WriterPreRead => "writer-pre-read",
        }
    }
}

fn next_arg(
    args: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    args.next()
        .ok_or_else(|| format!("{name} requires a value").into())
}

fn usage() -> String {
    "usage: kernel_operator_contract_coverage [--trajectories trajectories.jsonl] [--profile read|full|writer-pre-read] [--fail-under pct] [--output summary.json]".to_string()
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

fn now_unix_seconds() -> Result<u64, Box<dyn Error + Send + Sync>> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
