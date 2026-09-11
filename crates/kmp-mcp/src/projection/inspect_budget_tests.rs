use super::*;

const SECTIONS: [(&str, &str); 4] = [
    ("evidence", "/evidence"),
    ("outgoing", "/links/outgoing"),
    ("incoming", "/links/incoming"),
    ("raw", "/raw"),
];

fn inspection(count: usize, body: &str) -> Value {
    json!({
        "summary": "Inspection — 原文 🦀",
        "object": {"ref": "hub\"\\\n", "kind": "decision", "text": body,
            "metadata": {"empty": "", "numbers": [i64::MIN, u64::MAX, -0.0, 1.25, 1e20]}},
        "evidence": (0..count).map(|i| json!({
            "id": format!("source:{i}"), "text": body,
            "source": "signed\tcanonical\rsource", "supports": ["hub", "other"]
        })).collect::<Vec<_>>(),
        "links": {
            "outgoing": (0..count).map(|i| json!({"from_ref": "hub", "to_ref": format!("out:{i}"),
                "rel": "depends_on", "why": body})).collect::<Vec<_>>(),
            "incoming": (0..count).map(|i| json!({"from_ref": format!("in:{i}"), "to_ref": "hub",
                "rel": "supports", "why": body})).collect::<Vec<_>>()
        },
        "raw": (0..count).map(|i| json!({"ref": format!("raw:{i}"), "detail": body,
            "version": i})).collect::<Vec<_>>(),
        "quality": {"nodes": 1, "relationships": 2 * count, "details": 1, "truncated": false},
        "warnings": ["source contains \u{0000}\n\r\t\"\\ and UTF-8 ñ"]
    })
}

fn encoded(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("JSON encodes")
}

// Independent statement of the existing kmpi1 wire contract. The oracle hashes
// the original input, not the emptied template or the candidate's cached sizes.
fn original_selection_hash(value: &Value, arguments: &Value) -> String {
    let mut arguments = arguments.clone();
    arguments.as_object_mut().unwrap().remove("page");
    if let Some(budget) = arguments.get_mut("budget") {
        budget.as_object_mut().unwrap().remove("max_bytes");
        if budget.as_object().unwrap().is_empty() {
            arguments.as_object_mut().unwrap().remove("budget");
        }
    }
    let mut core = value.clone();
    for (_, path) in SECTIONS {
        *core.pointer_mut(path).unwrap() = json!([]);
    }
    let mut hash = Sha256::new();
    hash.update(b"kmpi1\0");
    hash.update(encoded(&arguments));
    hash.update(b"\0");
    hash.update(encoded(&core));
    for (section, path) in SECTIONS {
        for item in value.pointer(path).unwrap().as_array().unwrap() {
            hash.update(b"\0");
            hash.update(section.as_bytes());
            hash.update(b"\0");
            hash.update(encoded(item));
        }
    }
    format!("{:x}", hash.finalize())
}

#[test]
fn template_bytes_equal_real_json_for_all_slices_and_object_modes() {
    for count in [0, 1, 2] {
        for body in ["", "雪 🦀 \"quotes\" \\ \n\r\t\u{0000}\u{001f}"] {
            let original = inspection(count, body);
            let mut template = original.clone();
            let mut items = inspect_page_items(&mut template);
            let args = json!({"ref": original["object"]["ref"], "budget": {"max_bytes": 9999}});
            let (hash, core_bytes) = inspect_selection_hash(&template, &mut items, &args);
            assert_eq!(hash, original_selection_hash(&original, &args));
            assert_eq!(
                core_bytes + inspect_expansion_bytes(&items, 0, items.len()),
                encoded(&original).len()
            );
            template["object"] = Value::Null;
            for reuse in [false, true] {
                let object = if reuse {
                    json!({"ref": original["object"]["ref"]})
                } else {
                    original["object"].clone()
                };
                let mut core = template.clone();
                if reuse {
                    core["object_reused"] = json!(true);
                }
                for offset in 0..=items.len() {
                    for keep in 0..=items.len() - offset {
                        for required in [999, 1000, 9999, 10000] {
                            let mut page = render_inspect_page(
                                &core, &items, offset, keep, &hash, required, &args,
                            );
                            // Include mutable floor/progress fields: they stay in the
                            // serialized template and must never be cached as constants.
                            page["page"]["minimum_progress_bytes"] = json!(10000);
                            page["warnings"]
                                .as_array_mut()
                                .unwrap()
                                .push(json!(format!("{required}-byte floor — \n")));
                            let measured =
                                inspect_page_serialized_len(&page, &items, encoded(&object).len());
                            let materialized =
                                materialize_inspect_page(page, object.clone(), items.clone());
                            assert_eq!(
                                measured,
                                encoded(&materialized).len(),
                                "count={count}, offset={offset}, keep={keep}, reuse={reuse}"
                            );
                            let selected: Vec<_> = SECTIONS
                                .into_iter()
                                .flat_map(|(name, path)| {
                                    original
                                        .pointer(path)
                                        .unwrap()
                                        .as_array()
                                        .unwrap()
                                        .iter()
                                        .map(move |value| (name, value))
                                })
                                .skip(offset)
                                .take(keep)
                                .collect();
                            for (name, path) in SECTIONS {
                                let expected: Vec<_> = selected
                                    .iter()
                                    .filter(|(section, _)| *section == name)
                                    .map(|(_, value)| (*value).clone())
                                    .collect();
                                assert_eq!(materialized.pointer(path).unwrap(), &json!(expected));
                            }
                            assert_eq!(materialized["object"], object);
                            assert_eq!(
                                materialized["quality"]["relationships"],
                                materialized["links"]["outgoing"].as_array().unwrap().len()
                                    + materialized["links"]["incoming"].as_array().unwrap().len()
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn exact_whole_item_boundary_changes_only_when_real_bytes_fit() {
    let original = inspection(3, &"é\\\n\"".repeat(400));
    let mut template = original.clone();
    let mut items = inspect_page_items(&mut template);
    let mut args = json!({"ref": original["object"]["ref"], "budget": {"max_bytes": 512}});
    let (hash, _) = inspect_selection_hash(&template, &mut items, &args);
    let full = enforce_inspect_output_budget(
        original.clone(),
        &json!({"ref": original["object"]["ref"], "budget": {"max_bytes": 1000000}}),
    )
    .unwrap();
    let required = encoded(&full).len();
    template["object"] = Value::Null;
    // Solve against REAL materialized JSON, independently of the sizing helper.
    let expected = loop {
        let page = materialize_inspect_page(
            render_inspect_page(&template, &items, 0, 1, &hash, required, &args),
            original["object"].clone(),
            items.clone(),
        );
        let size = encoded(&page).len();
        if args["budget"]["max_bytes"] == size {
            break page;
        }
        args["budget"]["max_bytes"] = json!(size);
    };
    let exact = enforce_inspect_output_budget(original.clone(), &args).unwrap();
    assert_eq!(encoded(&exact), encoded(&expected));
    assert_eq!(exact["page"]["returned"], 1);
    args["budget"]["max_bytes"] = json!(encoded(&expected).len() - 1);
    let below = enforce_inspect_output_budget(original, &args).unwrap();
    assert_eq!(below["page"]["returned"], 0);
    assert!(below["page"]["minimum_progress_bytes"].as_u64().is_some());
}

#[test]
fn exact_floor_actions_and_reused_pages_reconstruct_original_sections() {
    for count in [0, 2] {
        let original = inspection(count, &"é原文 \" \\ \n\u{0000}".repeat(200));
        let full = enforce_inspect_output_budget(
            original.clone(),
            &json!({"ref": original["object"]["ref"], "budget": {"max_bytes": 1000000}}),
        )
        .unwrap();
        assert_eq!(full["page"]["required_bytes"], encoded(&full).len());
        for reuse in [false, true] {
            for allowance in [512, 2400, 9999, 10000] {
                let mut args =
                    json!({"ref": original["object"]["ref"], "budget": {"max_bytes": allowance}});
                let mut collected =
                    json!({"evidence": [], "links": {"outgoing": [], "incoming": []}, "raw": []});
                let mut completed = false;
                for index in 0..40 {
                    let page = enforce_inspect_output_budget(original.clone(), &args).unwrap();
                    let actual_size = encoded(&page).len();
                    assert_eq!(
                        page["page"]["required_bytes"],
                        full["page"]["required_bytes"]
                    );
                    if actual_size > args["budget"]["max_bytes"].as_u64().unwrap() as usize {
                        assert!(page["warnings"].as_array().unwrap().iter().any(|warning| {
                            warning
                                .as_str()
                                .unwrap()
                                .contains(&format!("{actual_size}-byte floor"))
                        }));
                    }
                    assert_eq!(
                        page["object"],
                        if reuse && index > 0 {
                            json!({"ref": original["object"]["ref"]})
                        } else {
                            original["object"].clone()
                        }
                    );
                    for (_, path) in SECTIONS {
                        collected
                            .pointer_mut(path)
                            .unwrap()
                            .as_array_mut()
                            .unwrap()
                            .extend(
                                page.pointer(path)
                                    .unwrap()
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .cloned(),
                            );
                    }
                    if page["page"]["has_more"] == false {
                        assert_eq!(page["next_actions"], json!([]));
                        completed = true;
                        break;
                    }
                    let offset = page["page"]["offset"].as_u64().unwrap()
                        + page["page"]["returned"].as_u64().unwrap();
                    assert_eq!(
                        page["page"]["next_cursor"],
                        format!(
                            "kmpi1:{offset}:{}",
                            original_selection_hash(&original, &args)
                        )
                    );
                    args = page["next_actions"][0]["arguments"].clone();
                    if reuse {
                        args["page"]["repeat_object"] = json!(false);
                    }
                }
                assert!(completed, "bounded native page traversal");
                for (_, path) in SECTIONS {
                    assert_eq!(collected.pointer(path), original.pointer(path));
                }
            }
        }
    }
}

#[test]
fn reused_cursor_still_binds_source_identity_body_and_shared_supports() {
    let original = inspection(2, "source body");
    let mut args = json!({"ref": original["object"]["ref"], "budget": {"max_bytes": 512}});
    let first = enforce_inspect_output_budget(original.clone(), &args).unwrap();
    args["page"] = json!({"cursor": first["page"]["next_cursor"], "repeat_object": false});
    for path in [
        "/object/text",
        "/evidence/0/text",
        "/evidence/0/source",
        "/evidence/0/supports/1",
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(path).unwrap() = json!("changed source revision");
        let error = enforce_inspect_output_budget(changed, &args).expect_err("stale source cursor");
        assert_eq!(error.code, crate::serving::ToolErrorCode::Conflict);
        assert_eq!(error.feedback[0]["code"], "READ_SELECTION_CHANGED");
        assert!(
            error.feedback[0]["action"]["arguments"]
                .get("page")
                .is_none()
        );
    }
}
