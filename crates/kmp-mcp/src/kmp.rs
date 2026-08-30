mod inspect_budget;
mod rendering;

pub(crate) use inspect_budget::enforce_inspect_output_budget;

use rendering::{
    dimension_coverage_json, dimension_selection_json, empty_page_info_json, empty_proof_json,
    insert_optional_string, memory_evidence_json, memory_relation_json, optional_quality_json,
    page_info_json, proof_json, raw_memory_ref_json, temporal_axis_label, temporal_coordinate_json,
    temporal_entry_json, temporal_state_json,
};

use kmp_proto::v1beta1::{
    AskResponse, IngestResponse, InspectResponse, ProjectVisualResponse, TemporalDirection,
    TemporalMoveResponse, TraceResponse, VisualLevelOfDetail, WakeResponse,
};
use serde_json::{Map, Value, json};

use kmp_application::queries::cl100k_estimator::Cl100kEstimator;
use kmp_domain::TokenEstimator;

use crate::ingest::KmpIngestPlan;
use crate::tool_error::ToolError;
use kmp_proto_mapping::v1beta1::recall_projection::{
    ProjectionOutcome, project_recall_output, requested_byte_limit,
};

pub(crate) fn ingest_from_response(response: IngestResponse) -> Value {
    let memory = response.memory.as_ref();
    let accepted = memory.and_then(|memory| memory.accepted.as_ref());

    json!({
        "summary": response.summary,
        "memory": {
            "about": memory.map(|memory| memory.about.as_str()).unwrap_or(""),
            "memory_id": memory.map(|memory| memory.memory_id.as_str()).unwrap_or(""),
            "accepted": {
                "entries": accepted.map(|accepted| accepted.entries).unwrap_or_default(),
                "relations": accepted.map(|accepted| accepted.relations).unwrap_or_default(),
                "evidence": accepted.map(|accepted| accepted.evidence).unwrap_or_default()
            },
            "read_after_write_ready": memory
                .map(|memory| memory.read_after_write_ready)
                .unwrap_or(false)
        },
        "warnings": response.warnings
    })
}

pub(crate) fn dry_run_ingest_from_plan(plan: &KmpIngestPlan) -> Value {
    json!({
        "summary": format!(
            "Ingested {} {}, {} {}, and {} {} for {}.",
            plan.accepted.entries,
            plural(plan.accepted.entries, "entry", "entries"),
            plan.accepted.relations,
            plural(plan.accepted.relations, "relation", "relations"),
            plan.accepted.evidence,
            plural(plan.accepted.evidence, "evidence item", "evidence items"),
            plan.about
        ),
        "memory": {
            "about": plan.about,
            "memory_id": plan.memory_id,
            "accepted": {
                "entries": plan.accepted.entries,
                "relations": plan.accepted.relations,
                "evidence": plan.accepted.evidence
            },
            "read_after_write_ready": false
        },
        "warnings": [
            "dry_run=true; validated memory without sending a KernelMemoryService.Ingest call"
        ]
    })
}

pub(crate) fn wake_from_response(response: WakeResponse) -> Value {
    kmp_proto_mapping::v1beta1::recall_projection::wake_value(&response)
}

pub(crate) fn ask_from_response(response: AskResponse) -> Value {
    kmp_proto_mapping::v1beta1::recall_projection::ask_value(&response)
}

/// Applies the caller's budget to the JSON that the MCP host actually sees.
///
/// The application renderer ranks/selects memory and budgets its prose. This
/// host gateway then preserves the cited core and adds a fixed, pageable
/// expansion prefix under a normative serialized-byte ceiling.
#[cfg(test)]
pub(crate) fn enforce_recall_output_budget(
    value: Value,
    arguments: &Value,
    default_tokens: u32,
) -> Value {
    let estimator = Cl100kEstimator::new();
    enforce_recall_output_budget_with_estimator(value, arguments, default_tokens, &estimator)
}

#[cfg(test)]
fn enforce_recall_output_budget_with_estimator(
    value: Value,
    arguments: &Value,
    default_tokens: u32,
    estimator: &dyn TokenEstimator,
) -> Value {
    try_enforce_recall_output_budget_with_estimator(value, arguments, default_tokens, estimator)
        .expect("test recall cursor should be valid")
}

pub(crate) fn try_enforce_recall_output_budget(
    value: Value,
    arguments: &Value,
    default_tokens: u32,
) -> Result<Value, ToolError> {
    let estimator = Cl100kEstimator::new();
    try_enforce_recall_output_budget_with_estimator(value, arguments, default_tokens, &estimator)
}

fn try_enforce_recall_output_budget_with_estimator(
    value: Value,
    arguments: &Value,
    default_tokens: u32,
    estimator: &dyn TokenEstimator,
) -> Result<Value, ToolError> {
    match project_recall_output(value, arguments, default_tokens, estimator)? {
        ProjectionOutcome::Projected(value) => Ok(value),
        ProjectionOutcome::CoreTooLarge => Err(ToolError::invalid_argument(
            "recall projection byte budget is smaller than the stable citation core; increase \
             budget.max_bytes",
        )),
    }
}

#[cfg(test)]
fn array_len(value: &Value, path: &[&str]) -> usize {
    let mut current = value;
    for key in path {
        let Some(next) = current.get(*key) else {
            return 0;
        };
        current = next;
    }
    current.as_array().map(Vec::len).unwrap_or_default()
}

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

pub(crate) fn trace_from_response(response: TraceResponse) -> Value {
    json!({
        "summary": response.summary,
        "trace": response.trace.iter().map(memory_relation_json).collect::<Vec<_>>(),
        "page": response
            .page
            .as_ref()
            .map(page_info_json)
            .unwrap_or_else(empty_page_info_json),
        "quality": optional_quality_json(response.quality.as_ref()),
        "warnings": response.warnings
    })
}

pub(crate) fn visual_projection_from_response(response: ProjectVisualResponse) -> Value {
    json!({
        "contract": response.contract,
        "about": response.about,
        "axis": temporal_axis_label(response.axis),
        "level_of_detail": match VisualLevelOfDetail::try_from(response.level_of_detail) {
            Ok(VisualLevelOfDetail::Episode) => "episode",
            Ok(VisualLevelOfDetail::Moment) => "moment",
            _ => "atlas",
        },
        "range": {
            "from": response.from.map(|value| value.to_string()),
            "to": response.to.map(|value| value.to_string()),
        },
        "bins": response.bins.into_iter().map(|bin| json!({
            "dimension": bin.dimension,
            "from": bin.from.map(|value| value.to_string()),
            "to": bin.to.map(|value| value.to_string()),
            "total": bin.entries,
            "by_kind": bin.by_kind,
        })).collect::<Vec<_>>(),
        "clusters": response.clusters.into_iter().map(|cluster| json!({
            "id": cluster.id,
            "dimension": cluster.dimension,
            "from": cluster.from.map(|value| value.to_string()),
            "to": cluster.to.map(|value| value.to_string()),
            "total": cluster.entries,
            "refs": cluster.refs,
            "by_kind": cluster.by_kind,
        })).collect::<Vec<_>>(),
        "entries": response.entries.iter().map(|entry| json!({
            "ref_id": entry.r#ref,
            "kind": entry.kind,
            "text": entry.text,
            "coordinates": entry.coordinates.iter().map(temporal_coordinate_json).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "by_kind": response.by_kind,
        "relations": response.relations.iter().map(memory_relation_json).collect::<Vec<_>>(),
        "metrics": response.metrics.into_iter().map(|metric| json!({
            "name": metric.name,
            "value": metric.value,
            "unit": metric.unit,
            "scope": metric.scope,
        })).collect::<Vec<_>>(),
        "coverage": response.coverage.map(|coverage| json!({
            "included": coverage.included,
            "missing": coverage.missing,
            "dimensions": coverage.dimensions.iter().map(dimension_coverage_json).collect::<Vec<_>>(),
        })),
        "revision": response.revision,
        "content_hash": response.content_hash,
        "page": response.page.as_ref().map(page_info_json).unwrap_or_else(empty_page_info_json),
        "truncated": response.truncated,
        "missing": response.missing,
    })
}

pub(crate) fn inspect_from_response(response: InspectResponse) -> Value {
    let object = response.object.as_ref().map_or_else(
        || {
            json!({
                "ref": "",
                "kind": "",
                "text": "",
                "metadata": {}
            })
        },
        |object| {
            let mut value = Map::new();
            value.insert("ref".to_string(), json!(object.r#ref));
            value.insert("kind".to_string(), json!(object.kind));
            value.insert("text".to_string(), json!(object.text));
            value.insert("metadata".to_string(), json!(object.metadata));
            insert_optional_string(&mut value, "source", &object.source);
            Value::Object(value)
        },
    );
    let links = response.links.as_ref();

    json!({
        "summary": response.summary,
        "object": object,
        "links": {
            "incoming": links
                .map(|links| links.incoming.iter().map(memory_relation_json).collect::<Vec<_>>())
                .unwrap_or_default(),
            "outgoing": links
                .map(|links| links.outgoing.iter().map(memory_relation_json).collect::<Vec<_>>())
                .unwrap_or_default()
        },
        "evidence": response.evidence.iter().map(memory_evidence_json).collect::<Vec<_>>(),
        "raw": response.raw.iter().map(raw_memory_ref_json).collect::<Vec<_>>(),
        "quality": optional_quality_json(response.quality.as_ref()),
        "warnings": response.warnings
    })
}

fn plural<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 { singular } else { plural }
}

/// Keeps a temporal response inside the byte ceiling the caller named.
///
/// The four temporal verbs declared the full `budget` object — `max_bytes`
/// included, described as normative — and applied none of it. A `max_bytes:
/// 9000` request came back at 17.3 KB, past the caller's ceiling and past the
/// tool's own published `anthropic/maxResultSizeChars` of 10,000. Two limits,
/// both advertised, neither enforced.
///
/// Declaring a limit and not enforcing it is worse than declaring none: every
/// one of these verbs exists to be called by a model with a finite context,
/// so the agent plans around the published number and takes no precautions of
/// its own. An oversized result does not degrade a turn, it ends it.
///
/// Entries are dropped from the end — the far side of the move, the least
/// recent thing the caller asked to walk toward — and `page` says so, which is
/// the same channel a naturally-partial temporal read already uses.
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

    if low == 0 && total > 0 {
        // Zero entries plus `has_more` is not a page the caller can walk, and
        // there is no boundary entry to continue from. Say which number to
        // raise rather than returning something unusable.
        return Err(ToolError::invalid_argument(format!(
            "this temporal response does not fit budget.max_bytes={limit} with even one entry; \
             raise budget.max_bytes"
        )));
    }
    if low == 0 && serialized_len(&value) > limit {
        // The envelope alone is over.
        return Err(ToolError::invalid_argument(format!(
            "this temporal response does not fit budget.max_bytes={limit} even with no entries; \
             raise budget.max_bytes"
        )));
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

fn serialized_len(value: &Value) -> usize {
    serde_json::to_string(value)
        .map(|text| text.len())
        .unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::{enforce_inspect_output_budget, enforce_temporal_output_budget};

    use kmp_proto::v1beta1::{
        DimensionScopeMode, DimensionSelection, DimensionSelectionMode, MemoryConfidence,
        MemoryEvidence, MemoryRelation, MemorySemanticClass, TemporalAxis, TemporalCoordinate,
        TemporalCursor, TemporalEntry,
    };

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

    fn byte_len(value: &serde_json::Value) -> usize {
        serde_json::to_string(value).expect("serializes").len()
    }

    fn inspect_value() -> serde_json::Value {
        serde_json::json!({
            "summary": "one hub",
            "object": {"ref": "hub", "kind": "decision", "text": "stable object", "metadata": {}},
            "links": {
                "incoming": (0..5).map(|index| serde_json::json!({
                    "from_ref": format!("incoming:{index}"),
                    "to_ref": "hub",
                    "rel": "supports",
                    "why": "i".repeat(120)
                })).collect::<Vec<_>>(),
                "outgoing": (0..4).map(|index| serde_json::json!({
                    "from_ref": "hub",
                    "to_ref": format!("outgoing:{index}"),
                    "rel": "depends_on",
                    "why": "o".repeat(120)
                })).collect::<Vec<_>>()
            },
            "evidence": (0..8).map(|index| serde_json::json!({
                "id": format!("evidence:{index}"),
                "supports": ["hub"],
                "text": "e".repeat(350),
                "source": format!("source:{index}")
            })).collect::<Vec<_>>(),
            "raw": [{"ref": "hub", "kind": "decision", "detail": "r".repeat(300)}],
            "quality": {"nodes": 1, "relationships": 9, "details": 1, "truncated": false},
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
    fn a_budget_that_cannot_hold_one_entry_names_the_number_to_raise() {
        let error = enforce_temporal_output_budget(
            temporal_value(6, 4_000),
            &serde_json::json!({"budget": {"max_bytes": 600}}),
        )
        .expect_err("no entry fits, so there is no page to walk");

        assert!(
            format!("{error:?}").contains("raise budget.max_bytes"),
            "{error:?}"
        );
    }

    #[test]
    fn inspect_pages_an_oversized_hub_without_repeats_or_gaps() {
        let value = inspect_value();
        let full = enforce_inspect_output_budget(
            value.clone(),
            &serde_json::json!({
                "about": "project:test",
                "ref": "hub",
                "budget": {"max_bytes": 100_000}
            }),
        )
        .expect("full inspect");
        assert_eq!(full["page"]["required_bytes"], byte_len(&full));

        let mut arguments = serde_json::json!({
            "about": "project:test",
            "ref": "hub",
            "budget": {"max_bytes": 2_400}
        });
        let mut evidence = Vec::new();
        let mut outgoing = Vec::new();
        let mut incoming = Vec::new();
        let mut raw = Vec::new();
        for page_index in 0..20 {
            let page = enforce_inspect_output_budget(value.clone(), &arguments)
                .expect("oversized inspect is a successful page");
            assert_eq!(page["object"]["ref"], "hub");
            assert!(byte_len(&page) <= 2_400, "page {page_index}: {page}");
            assert_eq!(page["page"]["required_bytes"], byte_len(&full));
            evidence.extend(
                page["evidence"]
                    .as_array()
                    .expect("evidence")
                    .iter()
                    .map(|item| item["id"].as_str().expect("evidence id").to_string()),
            );
            outgoing.extend(
                page["links"]["outgoing"]
                    .as_array()
                    .expect("outgoing")
                    .iter()
                    .map(|item| item["to_ref"].as_str().expect("outgoing id").to_string()),
            );
            incoming.extend(
                page["links"]["incoming"]
                    .as_array()
                    .expect("incoming")
                    .iter()
                    .map(|item| item["from_ref"].as_str().expect("incoming id").to_string()),
            );
            raw.extend(
                page["raw"]
                    .as_array()
                    .expect("raw")
                    .iter()
                    .map(|item| item["ref"].as_str().expect("raw ref").to_string()),
            );
            if !page["page"]["has_more"].as_bool().expect("has_more") {
                break;
            }
            arguments["page"] = serde_json::json!({
                "cursor": page["page"]["next_cursor"].as_str().expect("next cursor")
            });
        }

        assert_eq!(
            evidence,
            (0..8)
                .map(|index| format!("evidence:{index}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            outgoing,
            (0..4)
                .map(|index| format!("outgoing:{index}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            incoming,
            (0..5)
                .map(|index| format!("incoming:{index}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(raw, ["hub"]);
    }

    #[test]
    fn inspect_errors_only_when_the_stable_object_floor_cannot_fit() {
        let mut value = inspect_value();
        value["object"]["text"] = serde_json::json!("core".repeat(600));
        let error = enforce_inspect_output_budget(
            value,
            &serde_json::json!({"budget": {"max_bytes": 512}}),
        )
        .expect_err("the stable object itself is larger than the ceiling");

        assert_eq!(
            error.code,
            crate::tool_error::ToolErrorCode::InvalidArgument
        );
        assert!(error.message.contains("object floor"), "{error}");
        assert!(error.message.contains("full response requires"), "{error}");
    }

    #[test]
    fn inspect_cursor_is_bound_to_the_selection_but_not_the_byte_ceiling() {
        let value = inspect_value();
        let first_arguments = serde_json::json!({
            "about": "project:test",
            "ref": "hub",
            "include": {"incoming": true, "outgoing": true, "details": true, "raw": true},
            "budget": {"max_bytes": 2_400}
        });
        let first =
            enforce_inspect_output_budget(value.clone(), &first_arguments).expect("first page");
        let cursor = first["page"]["next_cursor"]
            .as_str()
            .expect("partial cursor");

        let raised = enforce_inspect_output_budget(
            value.clone(),
            &serde_json::json!({
                "about": "project:test",
                "ref": "hub",
                "include": {"incoming": true, "outgoing": true, "details": true, "raw": true},
                "budget": {"max_bytes": 100_000},
                "page": {"cursor": cursor}
            }),
        )
        .expect("the continuation may raise its byte ceiling");
        assert!(raised["page"]["offset"].as_u64().unwrap_or_default() > 0);

        let error = enforce_inspect_output_budget(
            value,
            &serde_json::json!({
                "about": "project:test",
                "ref": "hub",
                "include": {"incoming": false, "outgoing": true, "details": true, "raw": true},
                "budget": {"max_bytes": 2_400},
                "page": {"cursor": cursor}
            }),
        )
        .expect_err("a changed selection cannot consume the cursor");
        assert!(error.message.contains("does not match"), "{error}");
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
                crate::tool_error::ToolErrorCode::InvalidArgument,
                "a bad max_bytes is the caller's, not the kernel's"
            );
        }
    }

    #[test]
    fn an_envelope_that_cannot_fit_says_which_number_to_raise() {
        let mut value = temporal_value(0, 0);
        value["summary"] = serde_json::json!("x".repeat(2_000));
        let arguments = serde_json::json!({"budget": {"max_bytes": 512}});

        let error = enforce_temporal_output_budget(value, &arguments)
            .expect_err("nothing can be dropped to make this fit");
        assert!(error.message.contains("budget.max_bytes"), "{error}");
        assert_eq!(
            error.code,
            crate::tool_error::ToolErrorCode::InvalidArgument
        );
    }

    use kmp_proto::v1beta1::{
        AnswerReason, MemoryBudget, MemoryDetailLevel, Proof, TemporalState, WakeClaim, WakePacket,
    };

    use super::*;

    #[test]
    fn maps_typed_ask_response_without_inventing_null_answer() {
        let response = AskResponse {
            summary: "No deterministic memory answer found.".to_string(),
            answer: String::new(),
            because: vec![AnswerReason {
                claim: "claim".to_string(),
                evidence: "evidence".to_string(),
                r#ref: "evidence:1".to_string(),
            }],
            proof: Some(Proof {
                path: vec![relation()],
                evidence: vec![evidence()],
                conflicts: Vec::new(),
                superseded: Vec::new(),
                expired: Vec::new(),
                missing: vec!["generative_answer".to_string()],
                frontier_size: 1,
                matched_terms: vec!["deterministic".to_string()],
                matched_relations: vec!["supports".to_string()],
                confidence: MemoryConfidence::Medium as i32,
            }),
            warnings: Vec::new(),
            projection: None,
            truncation: None,
        };

        let value = ask_from_response(response);

        assert_eq!(value["answer"], Value::Null);
        assert_eq!(value["because"][0]["ref"], "evidence:1");
        assert_eq!(value["proof"]["path"][0]["from"], "claim:source");
        assert_eq!(value["proof"]["confidence"], "medium");
        assert_eq!(value["proof"]["frontier_size"], 1);
    }

    #[test]
    fn three_reason_packet_serializes_each_evidence_body_once() {
        let bodies = (0..3)
            .map(|index| {
                format!(
                    "Evidence body {index} establishes the selected claim with exact temporal and provenance detail. {}",
                    "grounded context remains canonical here. ".repeat(24)
                )
            })
            .collect::<Vec<_>>();
        let reasons = bodies
            .iter()
            .enumerate()
            .map(|(index, body)| AnswerReason {
                claim: format!("claim:{index}"),
                evidence: body.clone(),
                r#ref: format!("detail:evidence:{index}"),
            })
            .collect::<Vec<_>>();
        let evidence = bodies
            .iter()
            .enumerate()
            .map(|(index, body)| MemoryEvidence {
                id: format!("detail:evidence:{index}"),
                supports: vec![format!("claim:{index}")],
                text: body.clone(),
                source: format!("source:{index}"),
                time: None,
                metadata: Default::default(),
            })
            .collect::<Vec<_>>();
        let path = (0..12)
            .map(|index| {
                let evidence_index = index % 3;
                MemoryRelation {
                    source_ref: format!("evidence:{evidence_index}"),
                    target_ref: format!("claim:{evidence_index}"),
                    rel: "supports".to_string(),
                    semantic_class: MemorySemanticClass::Evidential as i32,
                    why: "Evidence supports the selected memory claim.".to_string(),
                    evidence: bodies[evidence_index].clone(),
                    confidence: MemoryConfidence::High as i32,
                    sequence: Some(index as u32 + 1),
                    explanation: None,
                    evidence_refs: Vec::new(),
                }
            })
            .collect::<Vec<_>>();
        let legacy_answer = bodies
            .iter()
            .map(|body| format!("- {body}"))
            .collect::<Vec<_>>()
            .join("\n");
        let response = AskResponse {
            summary: "Deterministic memory answer from 3 evidence items.".to_string(),
            answer: legacy_answer.clone(),
            because: reasons.clone(),
            proof: Some(Proof {
                path: path.clone(),
                evidence: evidence.clone(),
                conflicts: Vec::new(),
                superseded: Vec::new(),
                expired: Vec::new(),
                missing: Vec::new(),
                frontier_size: 0,
                matched_terms: vec!["canonical".to_string()],
                matched_relations: vec!["supports".to_string()],
                confidence: MemoryConfidence::High as i32,
            }),
            warnings: Vec::new(),
            projection: None,
            truncation: None,
        };
        let legacy = json!({
            "summary": response.summary,
            "answer": legacy_answer,
            "because": reasons.iter().map(|reason| json!({
                "claim": reason.claim,
                "evidence": reason.evidence,
                "ref": reason.r#ref
            })).collect::<Vec<_>>(),
            "proof": {
                "path": path.iter().map(memory_relation_json).collect::<Vec<_>>(),
                "evidence": evidence.iter().map(memory_evidence_json).collect::<Vec<_>>(),
                "conflicts": [],
                "superseded": [],
                "missing": [],
                "frontier_size": 0,
                "matched_terms": ["canonical"],
                "matched_relations": ["supports"],
                "confidence": "high"
            },
            "warnings": []
        });

        let outputs = (0..3)
            .map(|_| {
                serde_json::to_string(&ask_from_response(response.clone()))
                    .expect("normalized ask should serialize")
            })
            .collect::<Vec<_>>();
        let normalized = &outputs[0];
        let legacy = serde_json::to_string(&legacy).expect("legacy ask should serialize");
        let estimator = Cl100kEstimator::new();
        let normalized_tokens = estimator.estimate_tokens(normalized);
        let legacy_tokens = estimator.estimate_tokens(&legacy);
        let normalized_value: Value =
            serde_json::from_str(normalized).expect("normalized ask should parse");
        let legacy_value: Value = serde_json::from_str(&legacy).expect("legacy ask should parse");
        let canonical_body_bytes = bodies.iter().map(String::len).sum::<usize>();
        // Legacy owns every body in answer, because, proof.evidence, and four
        // supporting path hops. The normalized packet owns the registry copy.
        let legacy_owned_body_bytes = canonical_body_bytes * 7;
        let normalized_owned_body_bytes = canonical_body_bytes;
        let samples = 250;
        let legacy_started = std::time::Instant::now();
        for _ in 0..samples {
            std::hint::black_box(
                serde_json::to_vec(std::hint::black_box(&legacy_value))
                    .expect("legacy ask should serialize repeatedly"),
            );
        }
        let legacy_serialization = legacy_started.elapsed();
        let normalized_started = std::time::Instant::now();
        for _ in 0..samples {
            std::hint::black_box(
                serde_json::to_vec(std::hint::black_box(&normalized_value))
                    .expect("normalized ask should serialize repeatedly"),
            );
        }
        let normalized_serialization = normalized_started.elapsed();

        println!(
            "three-reason normalization: {} -> {} bytes; {} -> {} advisory cl100k tokens; owned evidence-body bytes {} -> {}; {samples} serializations {:?} -> {:?}",
            legacy.len(),
            normalized.len(),
            legacy_tokens,
            normalized_tokens,
            legacy_owned_body_bytes,
            normalized_owned_body_bytes,
            legacy_serialization,
            normalized_serialization
        );

        assert!(outputs.windows(2).all(|pair| pair[0] == pair[1]));
        for body in &bodies {
            assert_eq!(normalized.matches(body).count(), 1, "{body}");
        }
        assert!(
            normalized.len() * 100 <= legacy.len() * 40,
            "normalized={} legacy={}",
            normalized.len(),
            legacy.len()
        );
        assert!(normalized.len() < 10_000, "{} bytes", normalized.len());

        let canonical_ids = normalized_value["proof"]["evidence"]
            .as_array()
            .expect("canonical evidence registry")
            .iter()
            .filter_map(|item| item["id"].as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert!(
            normalized_value["because"]
                .as_array()
                .expect("reasons")
                .iter()
                .all(|reason| reason.get("evidence").is_none()
                    && reason["ref"]
                        .as_str()
                        .is_some_and(|evidence_ref| canonical_ids.contains(evidence_ref)))
        );
        assert!(
            normalized_value["proof"]["path"]
                .as_array()
                .expect("proof path")
                .iter()
                .all(|relation| relation.get("evidence").is_none()
                    && relation["evidence_refs"]
                        .as_array()
                        .is_some_and(|refs| !refs.is_empty()
                            && refs.iter().all(|evidence_ref| {
                                evidence_ref.as_str().is_some_and(|evidence_ref| {
                                    canonical_ids.contains(evidence_ref)
                                })
                            })))
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
    fn maps_wake_and_ignores_transport_budget_types() {
        let response = WakeResponse {
            summary: "Wake summary".to_string(),
            wake: Some(WakePacket {
                objective: "continue".to_string(),
                current_state: vec!["state".to_string()],
                causal_spine: vec![WakeClaim {
                    claim: "claim".to_string(),
                    because: "because".to_string(),
                    evidence_ref: "evidence:1".to_string(),
                }],
                open_loops: Vec::new(),
                next_actions: Vec::new(),
                guardrails: Vec::new(),
            }),
            proof: None,
            resume_cursor: Some(TemporalCursor {
                r#ref: "decision:latest".to_string(),
                time: Some(prost_types::Timestamp {
                    seconds: 1_786_924_800,
                    nanos: 0,
                }),
                sequence: Some(3),
            }),
            warnings: Vec::new(),
            projection: None,
            truncation: None,
        };
        let _budget = MemoryBudget {
            tokens: 1,
            detail: MemoryDetailLevel::Full as i32,
            depth: 1,
            max_entries: 0,
            max_bytes: 0,
        };

        let value = wake_from_response(response);

        assert_eq!(value["wake"]["current_state"][0], "state");
        assert_eq!(
            value["wake"]["causal_spine"][0]["evidence_ref"],
            "evidence:1"
        );
        // The bookmark a caller carries to kmp_forward, so catching up is
        // one more call rather than a rewind that exists to find a timestamp.
        assert_eq!(value["resume_cursor"]["ref"], "decision:latest");
        assert_eq!(value["resume_cursor"]["sequence"], 3);
    }

    #[test]
    fn compact_output_filters_structure_and_honours_the_serialized_byte_limit() {
        let path = (0..40)
            .map(|index| {
                json!({
                    "from": format!("about:root:{index}"),
                    "to": format!("entry:{index}"),
                    "rel": "contains_entry",
                    "class": "structural",
                    "why": "Repeated structural boilerplate that must not fill compact output",
                    "confidence": "high"
                })
            })
            .collect::<Vec<_>>();
        let evidence = (0..20)
            .map(|index| {
                json!({
                    "id": format!("evidence:{index}"),
                    "supports": [format!("entry:{index}")],
                    "text": "Long evidence text repeated to exercise final MCP packet budgeting",
                    "source": format!("source:{index}")
                })
            })
            .collect::<Vec<_>>();
        let value = json!({
            "summary": "Recall summary",
            "wake": {"current_state": ["state"], "causal_spine": []},
            "proof": {
                "path": path,
                "evidence": evidence,
                "missing": [],
                "frontier_size": 0,
                "confidence": "medium"
            },
            "warnings": []
        });
        let arguments = json!({"budget": {"tokens": 1, "max_bytes": 2_000, "detail": "compact"}});

        let bounded = enforce_recall_output_budget(value, &arguments, 1600);
        assert!(
            serde_json::to_vec(&bounded)
                .expect("bounded projection")
                .len()
                <= 2_000
        );
        assert!(
            bounded["proof"]["path"]
                .as_array()
                .expect("proof path remains typed")
                .iter()
                .all(|relation| relation["class"] != "structural")
        );
        assert_eq!(bounded["truncation"]["truncated"], true);
    }

    #[test]
    fn transport_omissions_do_not_change_the_graph_frontier() {
        let value = json!({
            "summary": "answer",
            "answer": "UNKNOWN",
            "because": [],
            "proof": {
                "path": [
                    {"from": "root", "to": "claim:one", "rel": "contains_entry", "class": "structural"},
                    {"from": "root", "to": "claim:two", "rel": "contains_entry", "class": "structural"}
                ],
                "evidence": [],
                "conflicts": [],
                "superseded": [],
                "missing": ["unexplored:one", "unexplored:two"],
                "frontier_size": 2,
                "confidence": "high"
            },
            "warnings": []
        });
        let low_budget = enforce_recall_output_budget(
            value.clone(),
            &json!({"budget": {"tokens": 500, "detail": "compact"}}),
            2_400,
        );
        let high_budget = enforce_recall_output_budget(
            value,
            &json!({"budget": {"tokens": 1_200, "detail": "full"}}),
            2_400,
        );

        for result in [&low_budget, &high_budget] {
            assert_eq!(result["proof"]["frontier_size"], 2);
        }
        assert_eq!(low_budget["truncation"]["truncated"], true);
        assert!(high_budget.get("truncation").is_none());
        assert_eq!(low_budget["projection"]["excluded_by_detail"], 4);
        assert_eq!(high_budget["projection"]["excluded_by_detail"], 0);
    }

    #[test]
    fn max_entries_caps_ask_reasons_as_well_as_proof_evidence() {
        let reasons = (0..5)
            .map(|index| json!({"evidence": format!("answer {index}")}))
            .collect::<Vec<_>>();
        let evidence = (0..5)
            .map(|index| json!({"id": format!("evidence:{index}"), "text": "answer"}))
            .collect::<Vec<_>>();
        let value = json!({
            "summary": "answer",
            "answer": "unbounded",
            "because": reasons,
            "proof": {"path": [], "evidence": evidence, "missing": [], "frontier_size": 0},
            "warnings": []
        });

        let bounded = enforce_recall_output_budget(
            value,
            &json!({"budget": {"tokens": 1000, "max_entries": 2}}),
            2400,
        );

        assert_eq!(bounded["because"].as_array().expect("reasons").len(), 2);
        assert_eq!(
            bounded["proof"]["evidence"]
                .as_array()
                .expect("evidence")
                .len(),
            2
        );
    }

    #[test]
    fn projection_envelope_is_budgeted_without_losing_the_cited_core() {
        let value = recall_budget_fixture();
        let mut compact_source = value.clone();
        compact_source["proof"]["path"]
            .as_array_mut()
            .expect("proof path")
            .retain(|relation| relation["class"] != "structural");
        let byte_limit = serde_json::to_vec(&compact_source)
            .expect("compact source should serialize")
            .len();

        // This is the exact cliff from #94. The projection envelope is now
        // reserved before filling the fixed expansion prefix.
        let bounded = enforce_recall_output_budget(
            value,
            &json!({"budget": {"tokens": 1, "max_bytes": byte_limit, "detail": "compact"}}),
            2_400,
        );

        assert!(
            serde_json::to_vec(&bounded)
                .expect("bounded projection")
                .len()
                <= byte_limit
        );
        assert_eq!(bounded["because"].as_array().expect("reasons").len(), 3);
        assert_eq!(bounded["truncation"]["truncated"], true);
        assert_eq!(
            bounded["projection"]["contract"],
            "kmp.recall.projection.v1"
        );
        assert!(!bounded["warnings"].as_array().expect("warnings").is_empty());
    }

    #[test]
    fn ask_budget_sweep_preserves_the_ceiling_and_monotone_cited_core() {
        let value = recall_budget_fixture();
        for detail in ["compact", "balanced", "full"] {
            let mut previous_reasons = 0;
            let mut previous_evidence = 0;
            for byte_limit in (4_000..=10_000).step_by(100) {
                let bounded = enforce_recall_output_budget(
                    value.clone(),
                    &json!({
                        "budget": {
                            "tokens": 1,
                            "max_bytes": byte_limit,
                            "detail": detail,
                            "max_entries": 12
                        }
                    }),
                    2_400,
                );
                let reasons = array_len(&bounded, &["because"]);
                let evidence = array_len(&bounded, &["proof", "evidence"]);

                assert!(
                    serde_json::to_vec(&bounded)
                        .expect("projection should serialize")
                        .len()
                        <= byte_limit,
                    "{detail} exceeded the normative byte ceiling {byte_limit}"
                );
                assert!(
                    reasons >= previous_reasons,
                    "{detail} lost cited reasons when the byte budget grew to {byte_limit}"
                );
                assert!(
                    evidence >= previous_evidence,
                    "{detail} lost proof evidence when the byte budget grew to {byte_limit}"
                );
                if bounded
                    .pointer("/truncation/truncated")
                    .and_then(Value::as_bool)
                    == Some(true)
                {
                    assert_eq!(
                        bounded["projection"]["contract"],
                        "kmp.recall.projection.v1"
                    );
                    assert!(!bounded["warnings"].as_array().expect("warnings").is_empty());
                }
                previous_reasons = reasons;
                previous_evidence = evidence;
            }
        }
    }

    #[test]
    fn wake_budget_sweep_preserves_the_ceiling_and_monotone_state() {
        let value = wake_budget_fixture();
        let mut previous_state = 0;

        for byte_limit in (4_000..=10_000).step_by(100) {
            let bounded = enforce_recall_output_budget(
                value.clone(),
                &json!({"budget": {"tokens": 1, "max_bytes": byte_limit, "detail": "balanced"}}),
                2_400,
            );
            let retained_state = [
                "current_state",
                "causal_spine",
                "open_loops",
                "next_actions",
                "guardrails",
            ]
            .into_iter()
            .map(|section| array_len(&bounded, &["wake", section]))
            .sum::<usize>();

            assert!(
                serde_json::to_vec(&bounded)
                    .expect("projection should serialize")
                    .len()
                    <= byte_limit
            );
            assert!(
                retained_state >= previous_state,
                "wake lost state when the byte budget grew to {byte_limit}"
            );
            if bounded
                .pointer("/truncation/truncated")
                .and_then(Value::as_bool)
                == Some(true)
            {
                assert_eq!(
                    bounded["projection"]["contract"],
                    "kmp.recall.projection.v1"
                );
                assert!(!bounded["warnings"].as_array().expect("warnings").is_empty());
            }
            previous_state = retained_state;
        }
    }

    #[test]
    fn ask_budget_shortens_text_without_dropping_stable_citations() {
        let reasons = (0..5)
            .map(|index| {
                json!({
                    "claim": format!("claim:{index}"),
                    "evidence": format!("reason {index} {}", "evidence ".repeat(4_000)),
                    "ref": format!("evidence:{index}")
                })
            })
            .collect::<Vec<_>>();
        let answer = reasons
            .iter()
            .map(|reason| format!("- {}", reason["evidence"].as_str().expect("evidence")))
            .collect::<Vec<_>>()
            .join("\n");
        let value = json!({
            "summary": "Deterministic memory answer from 5 evidence items for: why?",
            "answer": answer,
            "because": reasons,
            "proof": {
                "path": [],
                "evidence": [],
                "conflicts": [],
                "superseded": [],
                "missing": [],
                "frontier_size": 0,
                "confidence": "high"
            },
            "warnings": []
        });

        for arguments in [
            json!({}),
            json!({"budget": {"detail": "compact"}}),
            json!({"budget": {"tokens": 400}}),
        ] {
            let bounded = enforce_recall_output_budget(value.clone(), &arguments, 2_400);

            assert!(
                serde_json::to_vec(&bounded)
                    .expect("projection should serialize")
                    .len()
                    <= 10_000
            );
            assert!(
                bounded["answer"]
                    .as_str()
                    .is_some_and(|answer| !answer.is_empty()),
                "the payload must survive the budget gate"
            );
            assert_eq!(bounded["because"].as_array().expect("citations").len(), 5);
            assert!(
                bounded["because"]
                    .as_array()
                    .expect("citations")
                    .iter()
                    .enumerate()
                    .all(|(index, reason)| reason["ref"] == format!("evidence:{index}"))
            );
            assert!(bounded["proof"].is_object());
            assert_eq!(bounded["proof"]["confidence"], "high");
            assert_eq!(bounded["truncation"]["truncated"], true);
            assert!(bounded["truncation"]["omitted"].is_object());
            assert_eq!(bounded["projection"]["core_text_shortened"], true);
            assert!(bounded["truncation"].get("omitted_items").is_none());
            assert!(
                bounded["proof"]["missing"]
                    .as_array()
                    .expect("missing")
                    .is_empty()
            );
        }
    }

    #[test]
    fn wake_budget_keeps_the_wake_shape_when_retained_text_is_too_large() {
        let value = json!({
            "summary": "Objective: keep working.",
            "wake": {
                "objective": format!("objective {}", "detail ".repeat(4_000)),
                "current_state": [format!("state {}", "detail ".repeat(4_000))],
                "causal_spine": [{
                    "claim": "claim",
                    "because": format!("because {}", "detail ".repeat(4_000)),
                    "evidence_ref": "evidence:1"
                }],
                "open_loops": [],
                "next_actions": [],
                "guardrails": []
            },
            "proof": {
                "path": [],
                "evidence": [],
                "conflicts": [],
                "superseded": [],
                "missing": [],
                "frontier_size": 0,
                "confidence": "medium"
            },
            "resume_cursor": {"ref": "decision:latest"},
            "warnings": []
        });

        let bounded = enforce_recall_output_budget(value, &json!({}), 1_600);
        assert!(serde_json::to_vec(&bounded).expect("bounded wake").len() <= 10_000);
        assert!(bounded["wake"].is_object());
        assert!(bounded["proof"].is_object());
        assert_eq!(bounded["resume_cursor"]["ref"], "decision:latest");
        assert_eq!(bounded["truncation"]["truncated"], true);
        assert_eq!(bounded["projection"]["core_text_shortened"], true);
        assert_eq!(
            bounded["wake"]["causal_spine"][0]["evidence_ref"],
            "evidence:1"
        );
    }

    fn recall_budget_fixture() -> Value {
        let reasons = (0..3)
            .map(|index| {
                json!({
                    "claim": format!("claim:{index}"),
                    "evidence": format!("Reason {index}: {}", "specific evidence detail ".repeat(6)),
                    "ref": format!("evidence:{index}")
                })
            })
            .collect::<Vec<_>>();
        let answer = reasons
            .iter()
            .map(|reason| format!("- {}", reason["evidence"].as_str().expect("evidence")))
            .collect::<Vec<_>>()
            .join("\n");
        let mut path = vec![json!({
            "from": "about:root",
            "to": "claim:0",
            "rel": "contains_entry",
            "class": "structural",
            "why": "structural bookkeeping",
            "confidence": "high"
        })];
        path.extend((0..8).map(|index| {
            json!({
                "from": format!("claim:{index}"),
                "to": format!("evidence:{}", index % 3),
                "rel": "supports",
                "class": "evidential",
                "why": format!("Semantic proof hop {index}: {}", "relationship detail ".repeat(4)),
                "evidence": format!("hop evidence {index}: {}", "grounded detail ".repeat(4)),
                "confidence": "high"
            })
        }));
        let evidence = (0..3)
            .map(|index| {
                json!({
                    "id": format!("evidence:{index}"),
                    "supports": [format!("claim:{index}")],
                    "text": format!("Reason {index}: {}", "specific evidence detail ".repeat(6)),
                    "source": format!("source:{index}")
                })
            })
            .collect::<Vec<_>>();

        json!({
            "summary": "Deterministic memory answer from 3 evidence items.",
            "answer": answer,
            "because": reasons,
            "proof": {
                "path": path,
                "evidence": evidence,
                "conflicts": [],
                "superseded": [],
                "missing": [],
                "frontier_size": 0,
                "matched_terms": ["truncation"],
                "matched_relations": ["supports"],
                "confidence": "high"
            },
            "warnings": []
        })
    }

    fn wake_budget_fixture() -> Value {
        let state = (0..6)
            .map(|index| format!("state {index}: {}", "specific retained detail ".repeat(4)))
            .collect::<Vec<_>>();
        let claims = (0..6)
            .map(|index| {
                json!({
                    "claim": format!("claim {index}: {}", "specific retained detail ".repeat(4)),
                    "because": format!("reason {index}: {}", "grounded detail ".repeat(4)),
                    "evidence_ref": format!("evidence:{index}")
                })
            })
            .collect::<Vec<_>>();

        json!({
            "summary": "Wake summary",
            "wake": {
                "objective": "continue truncation work",
                "current_state": state,
                "causal_spine": claims,
                "open_loops": ["open loop 0", "open loop 1", "open loop 2"],
                "next_actions": ["next action 0", "next action 1", "next action 2"],
                "guardrails": ["guardrail 0", "guardrail 1", "guardrail 2"]
            },
            "proof": {
                "path": [],
                "evidence": [],
                "conflicts": [],
                "superseded": [],
                "missing": [],
                "frontier_size": 0,
                "confidence": "high"
            },
            "resume_cursor": {"ref": "decision:latest"},
            "warnings": []
        })
    }

    fn relation() -> MemoryRelation {
        MemoryRelation {
            source_ref: "claim:source".to_string(),
            target_ref: "claim:target".to_string(),
            rel: "supports".to_string(),
            semantic_class: MemorySemanticClass::Evidential as i32,
            why: "why".to_string(),
            evidence: "evidence".to_string(),
            confidence: MemoryConfidence::High as i32,
            sequence: Some(1),
            explanation: None,
            evidence_refs: Vec::new(),
        }
    }

    fn evidence() -> MemoryEvidence {
        MemoryEvidence {
            id: "evidence:1".to_string(),
            supports: vec!["claim:target".to_string()],
            text: "Evidence".to_string(),
            source: "source".to_string(),
            time: None,
            metadata: Default::default(),
        }
    }

    fn coordinate() -> TemporalCoordinate {
        TemporalCoordinate {
            dimension: "timeline".to_string(),
            scope_id: "scope".to_string(),
            occurred_at: None,
            observed_at: None,
            ingested_at: None,
            valid_from: None,
            valid_until: None,
            sequence: Some(2),
            rank: None,
            metadata: Default::default(),
        }
    }
}
