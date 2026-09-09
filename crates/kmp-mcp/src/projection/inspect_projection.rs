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

    #[test]
    fn inspect_can_reuse_a_large_object_and_reconstruct_every_expansion() {
        use serde_json::json;

        let mut value = inspect_value();
        value["object"]["text"] = json!("Canonical original — 原文. ".repeat(140));
        value["object"]["metadata"] = json!({"source_revision": "original:3"});
        value["object"]["source"] = json!("signed source");
        let full = enforce_inspect_output_budget(
            value.clone(),
            &json!({
                "about": "project:test", "ref": "hub", "budget": {"max_bytes": 100_000}
            }),
        )
        .expect("complete inspection");
        let mut args = json!({
            "about": "project:test", "ref": "hub", "budget": {"max_bytes": 2_400}
        });
        let first = enforce_inspect_output_budget(value.clone(), &args).expect("first page");
        assert_eq!(first["object"], full["object"]);
        assert_eq!(first["page"]["returned"], 0);
        assert!(first.get("object_reused").is_none());

        let mut accumulated = first.clone();
        let mut next = first["page"]["next_cursor"].clone();
        for _ in 0..30 {
            args["page"] = json!({"cursor": next, "repeat_object": false});
            let page = enforce_inspect_output_budget(value.clone(), &args).expect("continuation");
            assert_eq!(page["object"], json!({"ref": "hub"}));
            assert_eq!(page["object_reused"], true);
            assert_eq!(
                page["page"]["required_bytes"],
                full["page"]["required_bytes"]
            );
            assert!(byte_len(&page) <= 2_400);
            assert!(
                page["page"]["returned"]
                    .as_u64()
                    .expect("returned item count")
                    > 0
            );
            for path in ["/evidence", "/links/incoming", "/links/outgoing", "/raw"] {
                accumulated
                    .pointer_mut(path)
                    .expect("accumulated expansion section")
                    .as_array_mut()
                    .expect("expansion array")
                    .extend(
                        page.pointer(path)
                            .expect("page expansion section")
                            .as_array()
                            .expect("expansion array")
                            .iter()
                            .cloned(),
                    );
            }
            if page["page"]["has_more"] == false {
                next = serde_json::Value::Null;
                break;
            }
            next = page["page"]["next_cursor"].clone();
        }
        assert!(next.is_null(), "the inspection must finish");
        for path in ["/object", "/evidence", "/links", "/raw"] {
            assert_eq!(accumulated.pointer(path), full.pointer(path), "{path}");
        }
    }

    #[test]
    fn inspect_reuse_rejects_a_changed_object_or_proof() {
        use serde_json::json;

        let value = inspect_value();
        let mut args =
            json!({"about": "project:test", "ref": "hub", "budget": {"max_bytes": 2_400}});
        let first =
            enforce_inspect_output_budget(value.clone(), &args).expect("first inspection page");
        args["page"] = json!({"cursor": first["page"]["next_cursor"], "repeat_object": false});
        for path in [
            "/object/text",
            "/object/metadata",
            "/evidence/0/text",
            "/links/outgoing/0/why",
        ] {
            let mut changed = value.clone();
            *changed
                .pointer_mut(path)
                .expect("selected object or proof field") = json!("changed after the first page");
            let error =
                enforce_inspect_output_budget(changed.clone(), &args).expect_err("stale selection");
            assert_eq!(error.code, crate::serving::ToolErrorCode::Conflict);
            let restart = &error.feedback[0]["action"];
            assert_eq!(restart["tool"], "kmp_inspect");
            assert!(restart["arguments"].get("page").is_none());
            let fresh = enforce_inspect_output_budget(changed.clone(), &restart["arguments"])
                .expect("returned restart is executable");
            assert_eq!(fresh["object"], changed["object"], "{path}");
            assert!(fresh.get("object_reused").is_none());
        }
        assert!(enforce_inspect_output_budget(value, &args).is_ok());
    }

    #[test]
    fn inspect_reuse_requires_a_valid_continuation() {
        use serde_json::json;

        for page in [
            json!({"repeat_object": false}),
            json!({"repeat_object": false, "cursor": ""}),
        ] {
            let error = enforce_inspect_output_budget(
                inspect_value(),
                &json!({
                    "about": "project:test", "ref": "hub", "page": page
                }),
            )
            .expect_err("no first-page object can be reused");
            assert!(error.message.contains("cursor"));
        }
    }
}
