use serde_json::{Value, json};

use kmp_proto::v1beta1::{TemporalDirection, TemporalMoveResponse};

use super::rendering::*;
use super::serialized_size::serialized_len;

use crate::serving::tool_error::ToolError;
use kmp_proto_mapping::v1beta1::recall_projection::requested_byte_limit;

pub(crate) fn temporal_from_response(response: TemporalMoveResponse) -> Value {
    let next_action = temporal_next_action(&response);
    json!({
        "summary": response.summary,
        "next_action": next_action,
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

fn temporal_next_action(response: &TemporalMoveResponse) -> Value {
    let Some(page) = response.page.as_ref().filter(|page| page.has_more) else {
        return Value::Null;
    };
    let direction = response
        .temporal
        .as_ref()
        .and_then(|state| TemporalDirection::try_from(state.direction).ok());
    // `Near` names its continuation with the entries it returned, because it is
    // an anchor rather than a self-paginator and carries no cursor of its own.
    // Demanding one here would silence the single direction designed without it.
    let cursor_bound = !matches!(direction, Some(TemporalDirection::Near));
    if cursor_bound && page.next_cursor.trim().is_empty() {
        return Value::Null;
    }
    let unchanged = "Keep about, axis, dimensions, include, depth, budget, and limit unchanged.";
    let action = match direction {
        Some(TemporalDirection::Goto) => format!(
            "Continue the earlier history with kmp_rewind using from.ref=\"{}\". {unchanged} Do not pass this cursor back to kmp_goto.",
            page.next_cursor
        ),
        Some(TemporalDirection::Near) => {
            let (Some(first), Some(last)) = (
                response.entries.first().map(|entry| entry.r#ref.as_str()),
                response.entries.last().map(|entry| entry.r#ref.as_str()),
            ) else {
                // Nothing to anchor on, and near has no cursor to fall back to.
                return Value::Null;
            };
            format!(
                "kmp_near is an anchor, not a self-paginator. Continue earlier with kmp_rewind using from.ref=\"{first}\" and later with kmp_forward using from.ref=\"{last}\". {unchanged} Do not pass page.next_cursor back to kmp_near."
            )
        }
        Some(TemporalDirection::Rewind) => format!(
            "Continue with kmp_rewind using from.ref=\"{}\". {unchanged}",
            page.next_cursor
        ),
        Some(TemporalDirection::Forward) => format!(
            "Continue with kmp_forward using from.ref=\"{}\". {unchanged}",
            page.next_cursor
        ),
        _ => return Value::Null,
    };
    Value::String(action)
}

pub(crate) fn enforce_temporal_output_budget(
    mut value: Value,
    arguments: &Value,
) -> Result<Value, ToolError> {
    // A `max_bytes` the caller cannot have meant is the caller's to fix, and
    // arrives here as untyped text from the shared parser.
    let limit = requested_byte_limit(arguments).map_err(ToolError::invalid_argument)?;
    if serialized_len(&value) <= limit {
        return Ok(value);
    }

    let entries = value["entries"].as_array().cloned().unwrap_or_default();
    let total = entries.len();

    // Largest prefix that fits. Probing on a copy, because truncating the
    // response in place makes every probe narrower than the last and walks
    // the search down to nothing.
    let (mut low, mut high) = (0usize, total);
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        let mut probe = value.clone();
        truncate_entries(&mut probe, &entries, middle, total);
        if serialized_len(&probe) <= limit {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    truncate_entries(&mut value, &entries, low, total);

    if low == 0 && (total > 0 || serialized_len(&value) > limit) {
        // A ceiling below this response's floor stopped being an error in
        // #441, the same contract recall adopted in #439: the envelope goes
        // out with zero entries and the warning names the number to raise.
        // `page` already says nothing was returned, so the floor cannot be
        // mistaken for a complete read.
        let floor_bytes = serialized_len(&value);
        if let Some(warnings) = value["warnings"].as_array_mut() {
            warnings.push(serde_json::json!(format!(
                "budget.max_bytes {limit} is below this response's stable floor; returned \
                 the {floor_bytes}-byte floor instead — raise max_bytes past it to see more"
            )));
        }
    }
    Ok(value)
}

fn truncate_entries(value: &mut Value, entries: &[Value], keep: usize, total: usize) {
    let keep = keep.min(total);
    value["entries"] = Value::Array(entries[..keep].to_vec());
    value["page"]["returned"] = json!(keep);
    value["page"]["total"] =
        json!(total.max(value["page"]["total"].as_u64().unwrap_or_default() as usize));
    if keep < total {
        value["page"]["has_more"] = json!(true);
        restate_trimmed_page(value, entries, keep);
    }
}

/// `summary` and `next_action` were written for the untrimmed response. Left
/// alone they describe entries this page no longer carries, so a partial read
/// announces itself as complete and offers nothing to continue with.
fn restate_trimmed_page(value: &mut Value, entries: &[Value], keep: usize) {
    value["summary"] = json!(format!(
        "Returned {keep} temporal {}, trimmed to fit budget.max_bytes.",
        if keep == 1 { "entry" } else { "entries" }
    ));

    // A trim always drops the tail of the array, whichever verb produced it, so
    // the boundary is the last entry kept — not the cursor the kernel would
    // have handed out for its own pagination.
    let Some(boundary) = keep
        .checked_sub(1)
        .and_then(|index| entries.get(index))
        .and_then(|entry| entry["ref"].as_str())
        .filter(|reference| !reference.trim().is_empty())
    else {
        return;
    };
    let boundary = boundary.to_string();

    // `rewind` walks newest to oldest, so its dropped tail is the older end.
    let (dropped, verb) = match value["temporal"]["direction"].as_str() {
        Some("rewind") => ("before", "kmp_rewind"),
        _ => ("after", "kmp_forward"),
    };
    value["page"]["next_cursor"] = json!(boundary);
    value["next_action"] = json!(format!(
        "This page was trimmed to fit budget.max_bytes, dropping the entries {dropped} \
         `{boundary}`. Continue with {verb} using from.ref=\"{boundary}\", or repeat the call \
         with a larger budget.max_bytes. Keep about, axis, dimensions, include and depth \
         unchanged."
    ));
    if let Some(warnings) = value["warnings"].as_array_mut() {
        warnings.push(json!(
            "temporal response trimmed to budget.max_bytes; page.next_cursor and next_action \
             continue from the last entry on this page"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::test_support::fixtures::byte_len;
    use crate::projection::test_support::fixtures::coordinate;
    #[allow(unused_imports)]
    use kmp_proto::v1beta1::*;
    #[allow(unused_imports)]
    use serde_json::{Value, json};

    fn temporal_value(entries: usize, text_len: usize) -> serde_json::Value {
        temporal_value_for("forward", entries, text_len)
    }
    fn temporal_value_for(direction: &str, entries: usize, text_len: usize) -> serde_json::Value {
        serde_json::json!({
            "summary": format!("Returned {entries} temporal entries."),
            "next_action": serde_json::Value::Null,
            "temporal": {"direction": direction, "axis": "occurred"},
            "entries": (0..entries)
                .map(|index| serde_json::json!({
                    "ref": format!("project:t:entry:{index}"),
                    "text": "x".repeat(text_len)
                }))
                .collect::<Vec<_>>(),
            "page": {"returned": entries, "total": entries, "has_more": false, "next_cursor": null},
            "warnings": []
        })
    }

    #[test]
    fn a_response_inside_the_ceiling_is_returned_untouched() {
        let value = temporal_value(3, 20);
        let arguments = serde_json::json!({"budget": {"max_bytes": 9000}});
        let bounded = enforce_temporal_output_budget(value.clone(), &arguments).expect("fits");
        assert_eq!(bounded, value);
    }
    #[test]
    fn a_trimmed_response_counts_only_the_entries_it_carries() {
        let bounded = enforce_temporal_output_budget(
            temporal_value(6, 200),
            &serde_json::json!({"budget": {"max_bytes": 900}}),
        )
        .expect("a trimmed page is still a page");

        let returned = bounded["entries"].as_array().expect("entries").len();
        assert!(returned < 6, "the budget should have trimmed: {bounded}");
        assert_eq!(bounded["page"]["returned"], returned);
        assert_eq!(bounded["page"]["has_more"], true);
        assert!(
            bounded["summary"]
                .as_str()
                .expect("summary")
                .starts_with(&format!("Returned {returned} temporal")),
            "summary must count what the page carries: {bounded}"
        );
    }
    #[test]
    fn a_trimmed_response_always_offers_a_way_to_continue() {
        for direction in ["goto", "near", "rewind", "forward"] {
            let bounded = enforce_temporal_output_budget(
                temporal_value_for(direction, 6, 200),
                &serde_json::json!({"budget": {"max_bytes": 900}}),
            )
            .expect("a trimmed page is still a page");

            assert_eq!(bounded["page"]["has_more"], true, "{direction}: {bounded}");
            let last = bounded["entries"]
                .as_array()
                .expect("entries")
                .last()
                .expect("a trimmed page keeps at least one entry")["ref"]
                .as_str()
                .expect("ref")
                .to_string();
            assert_eq!(bounded["page"]["next_cursor"], last, "{direction}");
            let action = bounded["next_action"].as_str().unwrap_or_default();
            assert!(action.contains(&last), "{direction}: {bounded}");
            // rewind walks newest to oldest, so its dropped tail is the older end.
            let verb = if direction == "rewind" {
                "kmp_rewind"
            } else {
                "kmp_forward"
            };
            assert!(action.contains(verb), "{direction}: {bounded}");
        }
    }
    #[test]
    fn a_budget_that_cannot_hold_one_entry_returns_the_floor_and_says_so() {
        let bounded = enforce_temporal_output_budget(
            temporal_value(6, 4_000),
            &serde_json::json!({"budget": {"max_bytes": 600}}),
        )
        .expect("the floor is returned, not an error");

        assert_eq!(bounded["entries"].as_array().expect("entries").len(), 0);
        assert_eq!(bounded["page"]["returned"], 0);
        let warnings = bounded["warnings"].as_array().expect("warnings");
        assert!(
            warnings.iter().any(|warning| warning
                .as_str()
                .is_some_and(|text| { text.contains("stable floor") && text.contains("600") })),
            "the floor names the number to raise: {warnings:?}"
        );
    }
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
        assert!(
            value["next_action"]
                .as_str()
                .is_some_and(|action| action.contains("kmp_forward")
                    && action.contains("from.ref=\"claim:target\""))
        );
    }
    #[test]
    fn temporal_next_actions_name_the_tool_that_can_consume_each_cursor() {
        let response = |direction| TemporalMoveResponse {
            temporal: Some(TemporalState {
                direction: direction as i32,
                ..Default::default()
            }),
            entries: vec![
                TemporalEntry {
                    r#ref: "claim:first".to_string(),
                    ..Default::default()
                },
                TemporalEntry {
                    r#ref: "claim:last".to_string(),
                    ..Default::default()
                },
            ],
            page: Some(kmp_proto::v1beta1::PageInfo {
                returned: 2,
                total: 5,
                has_more: true,
                next_cursor: "claim:boundary".to_string(),
            }),
            ..Default::default()
        };

        let goto = temporal_from_response(response(TemporalDirection::Goto));
        let goto_action = goto["next_action"].as_str().expect("goto action");
        assert!(goto_action.contains("kmp_rewind"));
        assert!(goto_action.contains("from.ref=\"claim:boundary\""));
        assert!(goto_action.contains("Do not pass this cursor back to kmp_goto"));

        let near = temporal_from_response(response(TemporalDirection::Near));
        let near_action = near["next_action"].as_str().expect("near action");
        assert!(near_action.contains("kmp_rewind using from.ref=\"claim:first\""));
        assert!(near_action.contains("kmp_forward using from.ref=\"claim:last\""));
        assert!(near_action.contains("Do not pass page.next_cursor back to kmp_near"));

        for (direction, tool) in [
            (TemporalDirection::Rewind, "kmp_rewind"),
            (TemporalDirection::Forward, "kmp_forward"),
        ] {
            let value = temporal_from_response(response(direction));
            let action = value["next_action"].as_str().expect("move action");
            assert!(action.contains(tool));
            assert!(action.contains("from.ref=\"claim:boundary\""));
        }

        let mut complete = response(TemporalDirection::Goto);
        complete.page.as_mut().expect("page").has_more = false;
        assert!(temporal_from_response(complete)["next_action"].is_null());
    }

    #[test]
    fn an_oversized_response_is_cut_to_the_ceiling_the_caller_named() {
        // The reported shape: max_bytes 9000, a response at roughly twice it.
        let value = temporal_value(8, 2_000);
        assert!(byte_len(&value) > 9_000, "the fixture has to be over");
        let arguments = serde_json::json!({"budget": {"max_bytes": 9000}});

        let bounded = enforce_temporal_output_budget(value, &arguments).expect("fits after");
        assert!(
            byte_len(&bounded) <= 9_000,
            "returned {} bytes against a 9000 ceiling",
            byte_len(&bounded)
        );
    }
    #[test]
    fn a_cut_response_says_it_was_cut() {
        let value = temporal_value(8, 2_000);
        let arguments = serde_json::json!({"budget": {"max_bytes": 9000}});
        let bounded = enforce_temporal_output_budget(value, &arguments).expect("fits after");

        let returned = bounded["entries"].as_array().expect("entries").len();
        assert!(returned < 8, "something had to go");
        // Silence here is the failure this exists to prevent: a partial walk
        // that reads as a complete one.
        assert_eq!(bounded["page"]["returned"], returned);
        assert_eq!(bounded["page"]["total"], 8);
        assert_eq!(bounded["page"]["has_more"], true);
    }
    #[test]
    fn the_default_ceiling_applies_when_the_caller_names_none() {
        // The tool publishes anthropic/maxResultSizeChars 10_000 and callers
        // plan around it, so an unasked response must respect it too.
        let value = temporal_value(40, 1_000);
        let bounded =
            enforce_temporal_output_budget(value, &serde_json::json!({})).expect("fits after");
        assert!(byte_len(&bounded) <= 10_000);
    }
    #[test]
    fn a_budget_a_caller_cannot_have_meant_is_the_callers_to_fix() {
        for arguments in [
            serde_json::json!({"budget": {"max_bytes": 12}}),
            serde_json::json!({"budget": {"max_bytes": "lots"}}),
        ] {
            let error = enforce_temporal_output_budget(temporal_value(1, 10), &arguments)
                .expect_err("a budget below the floor is not a budget");
            assert_eq!(
                error.code,
                crate::serving::tool_error_code::ToolErrorCode::InvalidArgument,
                "a bad max_bytes is the caller's, not the kernel's"
            );
        }
    }
    #[test]
    fn an_envelope_below_the_floor_returns_the_floor_and_says_so() {
        let mut value = temporal_value(0, 0);
        value["summary"] = serde_json::json!("x".repeat(2_000));
        let arguments = serde_json::json!({"budget": {"max_bytes": 512}});

        let bounded = enforce_temporal_output_budget(value, &arguments)
            .expect("even an oversized envelope is returned as the floor");
        let warnings = bounded["warnings"].as_array().expect("warnings");
        assert!(
            warnings.iter().any(|warning| warning
                .as_str()
                .is_some_and(|text| { text.contains("stable floor") && text.contains("512") })),
            "{warnings:?}"
        );
    }
}
