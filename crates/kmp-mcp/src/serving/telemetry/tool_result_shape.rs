use serde_json::Value;

use super::shape_reading::{array_len_at, first_non_zero, number_at};

/// What a call answered with, as counts — never its content.
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::ToolResultShape;

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
}
