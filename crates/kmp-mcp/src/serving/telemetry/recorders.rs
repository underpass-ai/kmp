//! The metrics and logs one tool call leaves behind. Counts, durations,
//! labels and keyed fingerprints — the message, the question and the stored
//! text never reach telemetry.

use std::time::Duration;

use opentelemetry::KeyValue;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::call_fingerprints::CallFingerprints;
use super::call_origin::CallOrigin;
use super::call_source::CallSource;
use super::feedback_codes::FeedbackCodes;
use super::recall_outcome::RecallOutcome;
use super::tool_argument_shape::ToolArgumentShape;
use super::tool_error_kind::ToolErrorKind;
use super::tool_result_shape::ToolResultShape;

pub(crate) fn record_call_success(
    source: CallSource<'_>,
    backend: &str,
    grpc_tls: &str,
    name: &str,
    arguments: &Value,
    result: &Value,
    duration: Duration,
) {
    let prints = source.origin.fingerprints(name, arguments, source.salt);
    let outcome = RecallOutcome::from_tool_result(name, result);
    let arguments = ToolArgumentShape::from_tool_arguments(name, arguments);
    let result = ToolResultShape::from_tool_result(result);
    record_common_metrics(name, backend, grpc_tls, "success", "none", duration);
    record_count_metrics(name, backend, &arguments, &result);
    log_tool_success(
        name,
        backend,
        grpc_tls,
        duration,
        &arguments,
        &result,
        &Provenance {
            origin: source.origin,
            prints: &prints,
            outcome: outcome.as_ref(),
        },
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn record_call_error(
    source: CallSource<'_>,
    backend: &str,
    grpc_tls: &str,
    name: &str,
    arguments: &Value,
    error_kind: ToolErrorKind,
    message: &str,
    feedback: &[Value],
    duration: Duration,
) {
    let prints = source.origin.fingerprints(name, arguments, source.salt);
    let feedback = FeedbackCodes::from_feedback(feedback);
    let origin = source.origin;
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
        duration_us = duration.as_micros() as u64,
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
        client_name = origin.client.as_ref().map(|client| client.name.as_str()),
        client_version = origin.client.as_ref().map(|client| client.version.as_str()),
        is_continuation = origin.is_continuation,
        subject_fingerprint = prints.subject.as_deref(),
        context_fingerprint = prints.context.as_deref(),
        feedback_count = feedback.as_ref().map(|codes| codes.count),
        feedback_codes = feedback.as_ref().map(|codes| codes.entries.as_str()),
        "kernel mcp tool error"
    );
}

/// What a success line says about its call beyond the shapes: where it
/// came from and, for a wake or an ask, how it came out.
struct Provenance<'a> {
    origin: &'a CallOrigin,
    prints: &'a CallFingerprints,
    outcome: Option<&'a RecallOutcome>,
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
    provenance: &Provenance<'_>,
) {
    let origin = provenance.origin;
    let outcome = provenance.outcome;
    tracing::info!(
        event = "kmp_mcp_tool",
        kmp_move = %canonical_move(name),
        backend,
        grpc_tls,
        status = "success",
        duration_ms = duration.as_millis() as u64,
        duration_us = duration.as_micros() as u64,
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
        client_name = origin.client.as_ref().map(|client| client.name.as_str()),
        client_version = origin.client.as_ref().map(|client| client.version.as_str()),
        is_continuation = origin.is_continuation,
        subject_fingerprint = provenance.prints.subject.as_deref(),
        context_fingerprint = provenance.prints.context.as_deref(),
        answer_status = outcome.and_then(|o| o.answer_status.as_deref()),
        unknown_reason = outcome.and_then(|o| o.unknown_reason.as_deref()),
        confidence = outcome.and_then(|o| o.confidence.as_deref()),
        anchored = outcome.and_then(|o| o.anchored),
        citations = outcome.map(|o| o.citations),
        citations_reached_by = outcome.map(|o| o.reached_by.as_str()),
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
    use std::time::Duration;

    use serde_json::json;

    use super::{CallOrigin, CallSource, record_call_error, record_call_success, stable_hash};
    use crate::serving::telemetry::ToolErrorKind;
    use crate::serving::telemetry::captured_log::CapturedLog;

    fn record_tool_success(
        backend: &str,
        grpc_tls: &str,
        name: &str,
        arguments: &serde_json::Value,
        result: &serde_json::Value,
        duration: Duration,
    ) {
        let origin = CallOrigin::default();
        let source = CallSource {
            origin: &origin,
            salt: None,
        };
        record_call_success(source, backend, grpc_tls, name, arguments, result, duration);
    }

    fn record_tool_error(
        backend: &str,
        grpc_tls: &str,
        name: &str,
        arguments: &serde_json::Value,
        error_kind: ToolErrorKind,
        message: &str,
        duration: Duration,
    ) {
        let origin = CallOrigin::default();
        let source = CallSource {
            origin: &origin,
            salt: None,
        };
        record_call_error(
            source,
            backend,
            grpc_tls,
            name,
            arguments,
            error_kind,
            message,
            &[],
            duration,
        );
    }

    #[test]
    fn every_tool_line_carries_its_duration_in_microseconds() {
        let (log, _guard) = CapturedLog::start("kmp_mcp=info");
        let duration = Duration::from_micros(2_345);

        record_tool_success(
            "embedded",
            "off",
            "kmp_ask",
            &json!({}),
            &json!({}),
            duration,
        );
        record_tool_error(
            "embedded",
            "off",
            "kmp_ask",
            &json!({}),
            ToolErrorKind::Backend,
            "denied",
            duration,
        );

        let lines = log.events("kmp_mcp_tool");
        assert_eq!(lines.len(), 2);
        for line in lines {
            assert_eq!(line["fields"]["duration_us"], 2_345);
            assert_eq!(line["fields"]["duration_ms"], 2);
        }
    }

    #[test]
    fn an_ask_line_says_how_it_came_out_and_where_from_without_its_text() {
        use crate::serving::telemetry::{CallOrigin, CallSource, FingerprintSalt, McpClient};
        use crate::serving::telemetry::{
            TELEMETRY_SALT_FILE, record_call_error, record_call_success,
        };

        let dir = tempfile::tempdir().expect("dir");
        let salt =
            FingerprintSalt::load_or_create(&dir.path().join(TELEMETRY_SALT_FILE)).expect("salt");
        let client = McpClient::from_initialize(&json!({
            "params": {"clientInfo": {"name": "codex-mcp-client", "version": "0.154.0"}}
        }));
        let sent = json!({"continuation": "call-1", "context_id": "ctx-secret"});
        let origin = CallOrigin::read(&sent, client);
        let source = CallSource {
            origin: &origin,
            salt: Some(&salt),
        };
        let resolved = json!({"about": "a", "question": "Which secret rollout failed?"});
        let (log, _guard) = CapturedLog::start("kmp_mcp=info");

        record_call_success(
            source,
            "embedded",
            "off",
            "kmp_ask",
            &resolved,
            &json!({"structuredContent": {
                "answer": "the secret answer",
                "answer_status": "partial",
                "proof": {"confidence": "medium", "evidence": [
                    {"id": "e", "text": "secret evidence"},
                    {"id": "f", "text": "secret", "metadata": {"reached_by": "lifecycle"}}
                ]}
            }}),
            Duration::from_micros(10),
        );
        record_call_error(
            source,
            "embedded",
            "off",
            "kmp_ask",
            &resolved,
            ToolErrorKind::Backend,
            "secret failure",
            &[json!({"code": "LABELS_REQUIRED", "field": "labels", "reason": "secret"})],
            Duration::from_micros(10),
        );

        let lines = log.events("kmp_mcp_tool");
        assert_eq!(lines.len(), 2);
        let fields = &lines[0]["fields"];
        assert_eq!(fields["client_name"], "codex-mcp-client");
        assert_eq!(fields["client_version"], "0.154.0");
        assert_eq!(fields["is_continuation"], true);
        assert_eq!(fields["answer_status"], "partial");
        assert_eq!(fields["confidence"], "medium");
        assert_eq!(fields["anchored"], true);
        assert_eq!(fields["citations"], 2);
        assert_eq!(fields["citations_reached_by"], "direct:1,lifecycle:1");
        assert!(fields.get("unknown_reason").is_none());
        let subject = fields["subject_fingerprint"].as_str().expect("subject");
        assert_eq!(
            subject,
            salt.fingerprint_text("question", "Which secret rollout failed?")
                .expect("print")
        );
        assert_eq!(lines[1]["fields"]["subject_fingerprint"], subject);
        assert_eq!(lines[1]["fields"]["feedback_count"], 1);
        assert_eq!(
            lines[1]["fields"]["feedback_codes"],
            "LABELS_REQUIRED@labels"
        );
        assert_eq!(
            fields["context_fingerprint"],
            salt.fingerprint("context", "ctx-secret").as_str()
        );
        for line in &lines {
            let text = line.to_string();
            assert!(!text.contains("secret"), "{text}");
            assert!(!text.contains("call-1"), "{text}");
        }
    }

    #[test]
    fn a_line_without_an_origin_adds_no_field() {
        let (log, _guard) = CapturedLog::start("kmp_mcp=info");
        record_tool_success(
            "embedded",
            "off",
            "kmp_inspect",
            &json!({}),
            &json!({}),
            Duration::from_micros(1),
        );
        let fields = &log.events("kmp_mcp_tool")[0]["fields"];
        for absent in [
            "client_name",
            "is_continuation",
            "subject_fingerprint",
            "context_fingerprint",
            "answer_status",
            "citations",
        ] {
            assert!(fields.get(absent).is_none(), "{absent}");
        }
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
