use std::time::Duration;

use opentelemetry::KeyValue;
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ToolErrorKind {
    Backend,
    Validation,
}

impl ToolErrorKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Backend => "backend",
            Self::Validation => "validation",
        }
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct ToolArgumentShape {
    pub(crate) dry_run: Option<bool>,
    pub(crate) strict: Option<bool>,
    pub(crate) include_raw: Option<bool>,
    pub(crate) dimension_mode: String,
    pub(crate) dimension_scope: String,
    pub(crate) abouts_count: usize,
    pub(crate) dimension_filter_count: usize,
    pub(crate) scope_ids_count: usize,
    pub(crate) memory_dimensions: usize,
    pub(crate) entries: usize,
    pub(crate) relations: usize,
    pub(crate) evidence: usize,
    pub(crate) connect_to: usize,
    pub(crate) read_context_refs: usize,
    pub(crate) trace_paths: usize,
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct ToolResultShape {
    pub(crate) warnings: usize,
    pub(crate) entries: usize,
    pub(crate) relations: usize,
    pub(crate) evidence: usize,
    pub(crate) path_length: usize,
    pub(crate) raw_refs: usize,
    pub(crate) relation_total: u64,
    pub(crate) relation_rich: u64,
    pub(crate) relation_anemic: u64,
    pub(crate) relation_structural: u64,
    pub(crate) relation_suspect: u64,
    pub(crate) prior_context_required: u64,
    pub(crate) prior_context_observed: u64,
}

pub(crate) fn record_tool_success(
    backend: &str,
    grpc_tls: &str,
    name: &str,
    arguments: &Value,
    result: &Value,
    duration: Duration,
) {
    let arguments = ToolArgumentShape::from_tool_arguments(name, arguments);
    let result = ToolResultShape::from_tool_result(result);
    record_common_metrics(name, backend, grpc_tls, "success", "none", duration);
    record_count_metrics(name, backend, &arguments, &result);
    log_tool_success(name, backend, grpc_tls, duration, &arguments, &result);
}

pub(crate) fn record_tool_error(
    backend: &str,
    grpc_tls: &str,
    name: &str,
    arguments: &Value,
    error_kind: ToolErrorKind,
    message: &str,
    duration: Duration,
) {
    let arguments = ToolArgumentShape::from_tool_arguments(name, arguments);
    record_common_metrics(
        name,
        backend,
        grpc_tls,
        "error",
        error_kind.as_str(),
        duration,
    );
    tracing::warn!(
        event = "kmp_mcp_tool",
        kmp_move = %canonical_move(name),
        backend,
        grpc_tls,
        status = "error",
        error_kind = error_kind.as_str(),
        error_hash = %stable_hash(message),
        duration_ms = duration.as_millis() as u64,
        dry_run = ?arguments.dry_run,
        strict = ?arguments.strict,
        include_raw = ?arguments.include_raw,
        dimension_mode = %arguments.dimension_mode,
        dimension_scope = %arguments.dimension_scope,
        abouts_count = arguments.abouts_count,
        dimension_filter_count = arguments.dimension_filter_count,
        scope_ids_count = arguments.scope_ids_count,
        memory_dimensions = arguments.memory_dimensions,
        entries = arguments.entries,
        relations = arguments.relations,
        evidence = arguments.evidence,
        connect_to = arguments.connect_to,
        read_context_refs = arguments.read_context_refs,
        trace_paths = arguments.trace_paths,
        "kernel mcp tool error"
    );
}

fn record_common_metrics(
    name: &str,
    backend: &str,
    grpc_tls: &str,
    status: &'static str,
    error_kind: &'static str,
    duration: Duration,
) {
    let meter = opentelemetry::global::meter("kmp");
    let attrs = [
        KeyValue::new("move", canonical_move(name).to_string()),
        KeyValue::new("backend", backend.to_string()),
        KeyValue::new("grpc_tls", grpc_tls.to_string()),
        KeyValue::new("status", status),
        KeyValue::new("error_kind", error_kind),
    ];
    meter
        .u64_counter("rehydration.kmp.tool.calls")
        .build()
        .add(1, &attrs);
    meter
        .f64_histogram("rehydration.kmp.tool.duration")
        .build()
        .record(duration.as_secs_f64(), &attrs);
}

fn record_count_metrics(
    name: &str,
    backend: &str,
    arguments: &ToolArgumentShape,
    result: &ToolResultShape,
) {
    let meter = opentelemetry::global::meter("kmp");
    let attrs = [
        KeyValue::new("move", canonical_move(name).to_string()),
        KeyValue::new("backend", backend.to_string()),
    ];
    meter
        .u64_histogram("rehydration.kmp.request.entries")
        .build()
        .record(arguments.entries as u64, &attrs);
    meter
        .u64_histogram("rehydration.kmp.request.relations")
        .build()
        .record(arguments.relations as u64, &attrs);
    meter
        .u64_histogram("rehydration.kmp.request.evidence")
        .build()
        .record(arguments.evidence as u64, &attrs);
    meter
        .u64_histogram("rehydration.kmp.result.warnings")
        .build()
        .record(result.warnings as u64, &attrs);
    meter
        .u64_histogram("rehydration.kmp.result.path_length")
        .build()
        .record(result.path_length as u64, &attrs);

    if canonical_move(name) == "kmp_write_memory" {
        record_writer_relation_metric("rich", result.relation_rich);
        record_writer_relation_metric("anemic", result.relation_anemic);
        record_writer_relation_metric("structural", result.relation_structural);
        record_writer_relation_metric("suspect", result.relation_suspect);
        meter
            .u64_histogram("rehydration.kmp.writer.read_context.required")
            .build()
            .record(result.prior_context_required, &attrs);
        meter
            .u64_histogram("rehydration.kmp.writer.read_context.observed")
            .build()
            .record(result.prior_context_observed, &attrs);
    }
}

fn record_writer_relation_metric(quality: &'static str, value: u64) {
    opentelemetry::global::meter("kmp")
        .u64_counter("rehydration.kmp.writer.relations")
        .build()
        .add(value, &[KeyValue::new("quality", quality)]);
}

fn log_tool_success(
    name: &str,
    backend: &str,
    grpc_tls: &str,
    duration: Duration,
    arguments: &ToolArgumentShape,
    result: &ToolResultShape,
) {
    tracing::info!(
        event = "kmp_mcp_tool",
        kmp_move = %canonical_move(name),
        backend,
        grpc_tls,
        status = "success",
        duration_ms = duration.as_millis() as u64,
        dry_run = ?arguments.dry_run,
        strict = ?arguments.strict,
        include_raw = ?arguments.include_raw,
        dimension_mode = %arguments.dimension_mode,
        dimension_scope = %arguments.dimension_scope,
        abouts_count = arguments.abouts_count,
        dimension_filter_count = arguments.dimension_filter_count,
        scope_ids_count = arguments.scope_ids_count,
        memory_dimensions = arguments.memory_dimensions,
        request_entries = arguments.entries,
        request_relations = arguments.relations,
        request_evidence = arguments.evidence,
        connect_to = arguments.connect_to,
        read_context_refs = arguments.read_context_refs,
        trace_paths = arguments.trace_paths,
        result_warnings = result.warnings,
        result_entries = result.entries,
        result_relations = result.relations,
        result_evidence = result.evidence,
        path_length = result.path_length,
        raw_refs = result.raw_refs,
        relation_total = result.relation_total,
        relation_rich = result.relation_rich,
        relation_anemic = result.relation_anemic,
        relation_structural = result.relation_structural,
        relation_suspect = result.relation_suspect,
        prior_context_required = result.prior_context_required,
        prior_context_observed = result.prior_context_observed,
        "kernel mcp tool completed"
    );
}

fn canonical_move(name: &str) -> &str {
    match name {
        "kernel_remember" | "kernel_ingest_context" => "kmp_ingest",
        other => other,
    }
}

fn stable_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    format!("{digest:x}").chars().take(16).collect()
}

impl ToolArgumentShape {
    pub(crate) fn from_tool_arguments(name: &str, arguments: &Value) -> Self {
        let dimensions = arguments.get("dimensions");
        let mut shape = Self {
            dry_run: tool_dry_run(name, arguments),
            strict: arguments
                .get("options")
                .and_then(|options| options.get("strict"))
                .and_then(Value::as_bool),
            include_raw: include_raw(name, arguments),
            dimension_mode: dimensions
                .and_then(|value| value.get("mode"))
                .and_then(Value::as_str)
                .unwrap_or("all")
                .to_string(),
            dimension_scope: dimensions
                .and_then(|value| value.get("scope"))
                .and_then(Value::as_str)
                .unwrap_or("current_about")
                .to_string(),
            abouts_count: array_len_at(dimensions, &["abouts"]),
            dimension_filter_count: dimension_filter_count(dimensions),
            scope_ids_count: array_len_at(dimensions, &["scope_ids"]),
            memory_dimensions: array_len_at(arguments.get("memory"), &["dimensions"]),
            entries: array_len_at(arguments.get("memory"), &["entries"]),
            relations: array_len_at(arguments.get("memory"), &["relations"]),
            evidence: array_len_at(arguments.get("memory"), &["evidence"]),
            connect_to: array_len_at(Some(arguments), &["connect_to"]),
            read_context_refs: 0,
            trace_paths: array_len_at(arguments.get("read_context"), &["trace_paths"]),
        };
        shape.read_context_refs = read_context_ref_count(arguments.get("read_context"));
        shape
    }
}

impl ToolResultShape {
    pub(crate) fn from_tool_result(result: &Value) -> Self {
        let structured = result.get("structuredContent").unwrap_or(result);
        let metrics = structured.get("relation_quality_metrics");
        let memory = structured.get("memory").or_else(|| {
            structured
                .get("ingest_result")
                .and_then(|value| value.get("memory"))
        });
        let accepted = memory.and_then(|value| value.get("accepted"));
        let temporal = structured.get("temporal");
        let inspect_object = structured.get("object");

        Self {
            warnings: array_len_at(Some(structured), &["warnings"]),
            entries: first_non_zero(&[
                number_at(accepted, &["entries"]) as usize,
                array_len_at(Some(structured), &["entries"]),
                array_len_at(temporal, &["entries"]),
            ]),
            relations: first_non_zero(&[
                number_at(accepted, &["relations"]) as usize,
                array_len_at(Some(structured), &["relations"]),
                array_len_at(Some(structured), &["relation_quality"]),
            ]),
            evidence: first_non_zero(&[
                number_at(accepted, &["evidence"]) as usize,
                array_len_at(Some(structured), &["because"]),
                array_len_at(Some(structured), &["evidence"]),
            ]),
            path_length: first_non_zero(&[
                array_len_at(Some(structured), &["trace"]),
                array_len_at(structured.get("proof"), &["path"]),
            ]),
            raw_refs: first_non_zero(&[
                array_len_at(Some(structured), &["raw_refs"]),
                array_len_at(Some(structured), &["raw"]),
                inspect_object
                    .and_then(|object| object.get("raw_refs"))
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or_default(),
            ]),
            relation_total: number_at(metrics, &["relation_total"]),
            relation_rich: number_at(metrics, &["relation_rich_count"]),
            relation_anemic: number_at(metrics, &["relation_anemic_count"]),
            relation_structural: number_at(metrics, &["relation_structural_count"]),
            relation_suspect: number_at(metrics, &["relation_suspect_count"]),
            prior_context_required: number_at(metrics, &["relation_prior_context_required_count"]),
            prior_context_observed: number_at(metrics, &["relation_prior_context_observed_count"]),
        }
    }
}

fn tool_dry_run(name: &str, arguments: &Value) -> Option<bool> {
    match canonical_move(name) {
        "kmp_write_memory" => arguments
            .get("options")
            .and_then(|options| options.get("dry_run"))
            .and_then(Value::as_bool),
        "kmp_ingest" => arguments.get("dry_run").and_then(Value::as_bool),
        _ => None,
    }
}

fn include_raw(name: &str, arguments: &Value) -> Option<bool> {
    match canonical_move(name) {
        "kmp_inspect" => arguments
            .get("include")
            .and_then(|include| include.get("raw"))
            .and_then(Value::as_bool),
        "kmp_goto" | "kmp_near" | "kmp_rewind" | "kmp_forward" => arguments
            .get("include")
            .and_then(|include| include.get("raw_refs"))
            .and_then(Value::as_bool),
        _ => None,
    }
}

fn read_context_ref_count(read_context: Option<&Value>) -> usize {
    let Some(read_context) = read_context else {
        return 0;
    };
    array_len_at(Some(read_context), &["inspected_refs"])
        + array_len_at(Some(read_context), &["temporal_refs"])
        + array_len_at(Some(read_context), &["wake_refs"])
        + array_len_at(Some(read_context), &["ask_refs"])
        + read_context
            .get("trace_paths")
            .and_then(Value::as_array)
            .map(|paths| {
                paths
                    .iter()
                    .map(|path| {
                        2 + path
                            .get("refs")
                            .and_then(Value::as_array)
                            .map(Vec::len)
                            .unwrap_or_default()
                    })
                    .sum::<usize>()
            })
            .unwrap_or_default()
}

fn dimension_filter_count(dimensions: Option<&Value>) -> usize {
    array_len_at(dimensions, &["include"]) + array_len_at(dimensions, &["exclude"])
}

fn array_len_at(root: Option<&Value>, path: &[&str]) -> usize {
    value_at(root, path)
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default()
}

fn number_at(root: Option<&Value>, path: &[&str]) -> u64 {
    value_at(root, path)
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn value_at<'a>(root: Option<&'a Value>, path: &[&str]) -> Option<&'a Value> {
    let mut current = root?;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn first_non_zero(values: &[usize]) -> usize {
    values.iter().copied().find(|value| *value > 0).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ToolArgumentShape, ToolResultShape, stable_hash};

    #[test]
    fn writer_argument_shape_counts_without_text_or_refs() {
        let shape = ToolArgumentShape::from_tool_arguments(
            "kmp_write_memory",
            &json!({
                "about": "incident:mobile-login",
                "intent": "record_decision",
                "options": {
                    "dry_run": true,
                    "strict": true
                },
                "current": {
                    "kind": "decision",
                    "summary": "Do not log this",
                    "evidence": "Do not log this either"
                },
                "connect_to": [
                    {"ref": "node:a", "rel": "chosen_because", "class": "motivational"},
                    {"ref": "node:b", "rel": "follows", "class": "procedural"}
                ],
                "read_context": {
                    "inspected_refs": ["node:a"],
                    "ask_refs": ["node:b"],
                    "trace_paths": [
                        {"from": "node:a", "to": "node:c", "refs": ["node:b"]}
                    ]
                }
            }),
        );

        assert_eq!(shape.dry_run, Some(true));
        assert_eq!(shape.strict, Some(true));
        assert_eq!(shape.connect_to, 2);
        assert_eq!(shape.trace_paths, 1);
        assert_eq!(shape.read_context_refs, 5);
    }

    #[test]
    fn ingest_argument_shape_counts_canonical_memory_sections() {
        let shape = ToolArgumentShape::from_tool_arguments(
            "kmp_ingest",
            &json!({
                "about": "incident:mobile-login",
                "dry_run": false,
                "memory": {
                    "dimensions": [{"id": "conversation", "kind": "session"}],
                    "entries": [{"id": "a"}, {"id": "b"}],
                    "relations": [{"from": "a", "to": "b"}],
                    "evidence": [{"id": "e1"}]
                }
            }),
        );

        assert_eq!(shape.dry_run, Some(false));
        assert_eq!(shape.memory_dimensions, 1);
        assert_eq!(shape.entries, 2);
        assert_eq!(shape.relations, 1);
        assert_eq!(shape.evidence, 1);
    }

    #[test]
    fn result_shape_extracts_writer_metrics_and_ingest_counts() {
        let shape = ToolResultShape::from_tool_result(&json!({
            "structuredContent": {
                "accepted": true,
                "relation_quality_metrics": {
                    "relation_total": 3,
                    "relation_rich_count": 2,
                    "relation_anemic_count": 1,
                    "relation_structural_count": 0,
                    "relation_suspect_count": 0,
                    "relation_prior_context_required_count": 2,
                    "relation_prior_context_observed_count": 2
                },
                "ingest_result": {
                    "memory": {
                        "accepted": {
                            "entries": 2,
                            "relations": 3,
                            "evidence": 3
                        }
                    }
                },
                "warnings": []
            }
        }));

        assert_eq!(shape.entries, 2);
        assert_eq!(shape.relations, 3);
        assert_eq!(shape.evidence, 3);
        assert_eq!(shape.relation_total, 3);
        assert_eq!(shape.relation_rich, 2);
        assert_eq!(shape.prior_context_required, 2);
        assert_eq!(shape.prior_context_observed, 2);
    }

    #[test]
    fn error_hash_is_stable_and_does_not_expose_message() {
        let message = "KernelMemoryService.Inspect failed for `private-ref`: denied";
        let hash = stable_hash(message);

        assert_eq!(hash, stable_hash(message));
        assert_eq!(hash.len(), 16);
        assert!(!hash.contains("private-ref"));
    }
}
