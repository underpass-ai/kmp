use serde_json::{Map, Value, json};

use kmp_proto::v1beta1::InspectResponse;

use super::rendering::*;

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

#[cfg(test)]
mod tests {
    use crate::projection::inspect_budget::enforce_inspect_output_budget;
    use crate::projection::test_support::fixtures::byte_len;

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
    fn an_inspect_floor_over_the_ceiling_is_returned_and_says_so() {
        let mut value = inspect_value();
        value["object"]["text"] = serde_json::json!("core".repeat(600));
        let bounded = enforce_inspect_output_budget(
            value,
            &serde_json::json!({"budget": {"max_bytes": 512}}),
        )
        .expect("the stable object floor is returned, not an error");

        assert!(bounded["object"]["text"].as_str().is_some());
        let warnings = bounded["warnings"].as_array().expect("warnings");
        assert!(
            warnings
                .iter()
                .any(|warning| warning.as_str().is_some_and(|text| {
                    text.contains("stable floor")
                        && text.contains("512")
                        && text.contains("full response requires")
                })),
            "{warnings:?}"
        );
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
}
