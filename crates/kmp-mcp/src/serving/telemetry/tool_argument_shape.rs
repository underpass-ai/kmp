use serde_json::Value;

use super::recorders::canonical_move;
use super::shape_reading::array_len_at;

/// What a call asked for, as counts and flags — never its text or refs.
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

fn tool_dry_run(name: &str, arguments: &Value) -> Option<bool> {
    match canonical_move(name) {
        "kmp_write_memory" | "kmp_relabel" => arguments
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::ToolArgumentShape;

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
}
