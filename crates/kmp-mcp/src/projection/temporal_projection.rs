use serde_json::{Value, json};

use kmp_proto::v1beta1::TemporalMoveResponse;

use super::rendering::*;

use crate::serving::tool_error::ToolError;

pub(crate) fn temporal_from_response(response: TemporalMoveResponse) -> Value {
    json!({
        "summary": response.summary,
        "temporal": response
            .temporal
            .as_ref()
            .map(temporal_state_json)
            .unwrap_or(Value::Null),
        "coverage": response
            .coverage
            .as_ref()
            .map(|coverage| {
                json!({
                    "requested": coverage
                        .requested
                        .as_ref()
                        .map(dimension_selection_json)
                        .unwrap_or(Value::Null),
                    "included": coverage.included,
                    "missing": coverage.missing,
                    "dimensions": coverage
                        .dimensions
                        .iter()
                        .map(dimension_coverage_json)
                        .collect::<Vec<_>>()
                })
            })
            .unwrap_or_else(|| json!({
                "requested": Value::Null,
                "included": Vec::<String>::new(),
                "missing": Vec::<String>::new(),
                "dimensions": Vec::<Value>::new()
        })),
        "entries": response.entries.iter().map(temporal_entry_json).collect::<Vec<_>>(),
        "page": response
            .page
            .as_ref()
            .map(page_info_json)
            .unwrap_or_else(empty_page_info_json),
        "raw_refs": response.raw_refs.iter().map(raw_memory_ref_json).collect::<Vec<_>>(),
        "proof": response.proof.as_ref().map(proof_json).unwrap_or_else(empty_proof_json),
        "quality": optional_quality_json(response.quality.as_ref()),
        "warnings": response.warnings
    })
}

pub(crate) fn enforce_temporal_output_budget(
    value: Value,
    arguments: &Value,
) -> Result<Value, ToolError> {
    super::temporal_page::TemporalPage::project(value, arguments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::test_support::fixtures::coordinate;
    #[allow(unused_imports)]
    use kmp_proto::v1beta1::*;
    #[allow(unused_imports)]
    use serde_json::{Value, json};

    #[test]
    fn maps_temporal_response_to_kmp_json_names() {
        let response = TemporalMoveResponse {
            summary: "Returned 1 temporal entry.".to_string(),
            temporal: Some(TemporalState {
                direction: TemporalDirection::Forward as i32,
                axis: TemporalAxis::Observed as i32,
                requested: Some(TemporalCursor {
                    r#ref: "claim:source".to_string(),
                    time: None,
                    sequence: None,
                }),
                resolved: Some(coordinate()),
            }),
            coverage: Some(kmp_proto::v1beta1::TemporalCoverage {
                requested: Some(DimensionSelection {
                    mode: DimensionSelectionMode::Only as i32,
                    include: vec!["timeline".to_string()],
                    exclude: Vec::new(),
                    scope: DimensionScopeMode::CurrentAbout as i32,
                    abouts: Vec::new(),
                    scope_ids: vec!["timeline:main".to_string()],
                    selectors: Vec::new(),
                }),
                included: vec!["timeline".to_string()],
                missing: Vec::new(),
                dimensions: vec![kmp_proto::v1beta1::DimensionCoverage {
                    dimension: "timeline".to_string(),
                    returned: 1,
                    present: true,
                }],
            }),
            entries: vec![TemporalEntry {
                r#ref: "claim:target".to_string(),
                kind: "claim".to_string(),
                text: "Target".to_string(),
                coordinates: vec![coordinate()],
                metadata: [("window".to_string(), "10:00-10:20".to_string())]
                    .into_iter()
                    .collect(),
            }],
            proof: None,
            warnings: Vec::new(),
            raw_refs: Vec::new(),
            page: Some(kmp_proto::v1beta1::PageInfo {
                returned: 1,
                total: 2,
                has_more: true,
                next_cursor: "claim:target".to_string(),
            }),
            quality: Some(kmp_proto::v1beta1::ResponseQuality {
                nodes: 1,
                relationships: 0,
                details: 1,
                detail_coverage: 1.0,
                causal_density: 0.0,
                truncated: true,
            }),
        };

        let value = temporal_from_response(response);

        assert_eq!(value["temporal"]["direction"], "forward");
        assert_eq!(value["entries"][0]["ref"], "claim:target");
        assert_eq!(value["entries"][0]["coordinates"][0]["scope_id"], "scope");
        assert_eq!(value["entries"][0]["metadata"]["window"], "10:00-10:20");
        assert_eq!(value["coverage"]["requested"]["scope"], "current_about");
        assert_eq!(
            value["coverage"]["requested"]["scope_ids"][0],
            "timeline:main"
        );
        assert_eq!(value["coverage"]["dimensions"][0]["dimension"], "timeline");
        assert_eq!(value["coverage"]["dimensions"][0]["returned"], 1);
        assert_eq!(value["coverage"]["dimensions"][0]["present"], true);
        assert_eq!(value["quality"]["nodes"], 1);
        assert_eq!(value["quality"]["details"], 1);
        assert_eq!(value["quality"]["detail_coverage"], 1.0);
        assert_eq!(value["quality"]["truncated"], true);
        assert_eq!(value["page"]["returned"], 1);
        assert_eq!(value["page"]["total"], 2);
        assert_eq!(value["page"]["has_more"], true);
        assert_eq!(value["page"]["next_cursor"], "claim:target");
    }
}
