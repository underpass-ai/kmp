//! The metrics and logs one tool call leaves behind. Counts, durations
//! and a stable hash — the message itself never reaches telemetry.

use std::time::Duration;

use opentelemetry::KeyValue;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::tool_argument_shape::ToolArgumentShape;
use super::tool_error_kind::ToolErrorKind;
use super::tool_result_shape::ToolResultShape;

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

pub(super) fn canonical_move(name: &str) -> &str {
    match name {
        "kernel_remember" | "kernel_ingest_context" => "kmp_ingest",
        other => other,
    }
}

pub(super) fn stable_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    format!("{digest:x}").chars().take(16).collect()
}

#[cfg(test)]
mod tests {
    use super::stable_hash;

    #[test]
    fn error_hash_is_stable_and_does_not_expose_message() {
        let message = "KernelMemoryService.Inspect failed for `private-ref`: denied";
        let hash = stable_hash(message);

        assert_eq!(hash, stable_hash(message));
        assert_eq!(hash.len(), 16);
        assert!(!hash.contains("private-ref"));
    }
}
