//! Every retained read family reconstructs the same complete selected proof.
#[path = "support/guidance_fixture.rs"]
mod fixture;
#[path = "support/reviewed_writer.rs"]
mod reviewed_writer;
use fixture::*;
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[tokio::test]
async fn short_recall_temporal_trace_and_relate_preserve_complete_evidence() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let agent = open(&server, "all-read-families").await;
    let mut write = packet(&agent["context_id"]);
    write["memories"][0]["evidence"] =
        json!("S1 records the route opening on Tuesday. ".repeat(30));
    write["memories"].as_array_mut().expect("memories").push(json!({"id":"decision","kind":"decision",
        "observed_at":"2026-09-09T11:00:00Z","summary":"Schedule delivery on Tuesday.",
        "evidence":"S2 schedules delivery to match S1's route opening. ".repeat(30),
        "connect_to":[{"ref":"source","rel":"chosen_because","class":"motivational",
            "why":"S1's route opening motivates Tuesday delivery.",
            "evidence":"S2 explicitly selects Tuesday because the route opens then according to S1. ".repeat(20)}]}));
    let written = call(&server, "kmp_write_memory", write).await;
    let written = reviewed_writer::review_authored_write(&server, written).await;
    assert_eq!(written["isError"], false, "{written}");
    let refs = &written["structuredContent"]["local_refs"];
    let interval = json!({"start":"2026-09-09T00:00:00Z","end":"2026-09-10T00:00:00Z"});
    let dimensions = json!({"selectors":[{"key":"task","op":"in","values":["context-check"]}]});
    let mut queries = vec![
        (
            "kmp_wake",
            json!({"about":ABOUT,"axis":"observed","interval":interval,"dimensions":dimensions,"intent":"continue the route"}),
        ),
        (
            "kmp_ask",
            json!({"about":ABOUT,"axis":"observed","interval":interval,"dimensions":dimensions,"question":"When does the route open?","asked_as":"When does the route open?"}),
        ),
        (
            "kmp_trace",
            json!({"about":ABOUT,"from":refs["decision"],"to":refs["source"],"goal":"audit the stated reason","role":"reader"}),
        ),
        (
            "kmp_relate",
            json!({"about":ABOUT,"axis":"observed","interval":interval,"dimensions":dimensions}),
        ),
    ];
    for (tool, key, cursor) in [
        (
            "kmp_forward",
            "from",
            json!({"time":"2026-09-09T00:00:00Z"}),
        ),
        ("kmp_rewind", "from", json!({"time":"2026-09-10T00:00:00Z"})),
        ("kmp_goto", "at", json!({"ref":refs["decision"]})),
        ("kmp_near", "around", json!({"ref":refs["source"]})),
    ] {
        let mut args = json!({"about":ABOUT,"axis":"observed","interval":interval,"dimensions":dimensions,
            "include":{"evidence":true,"relations":true,"raw_refs":true},"limit":{"entries":10},"window":{"before_entries":5,"after_entries":5}});
        args[key] = cursor;
        queries.push((tool, args));
    }
    for (tool, mut args) in queries {
        args["budget"] = json!({"max_bytes":1000000,"detail":"full","depth":4});
        let full = call(&server, tool, args.clone()).await;
        assert_eq!(full["isError"], false, "{tool}: {full}");
        let full = &full["structuredContent"];
        let recall = full.get("projection").is_some();
        let section_map = if recall {
            &full["projection"]["sections"]
        } else {
            &full["page"]["sections"]
        };
        let sections: Vec<String> = match section_map.as_object() {
            Some(map) => map
                .keys()
                .map(|s| format!("/{}", s.replace('.', "/")))
                .collect(),
            None if tool == "kmp_trace" => vec!["/trace".into()],
            None => ["facts", "declared", "coordinate", "tensions", "proposed"]
                .iter()
                .map(|s| format!("/{s}"))
                .collect(),
        };
        let mut recovered: BTreeMap<String, Vec<Value>> =
            sections.iter().map(|s| (s.clone(), Vec::new())).collect();
        args["budget"]["max_bytes"] = json!(512);
        args["context_id"] = agent["context_id"].clone();
        args["purpose"] = json!("continue");
        let mut result = call(&server, tool, args).await;
        let mut collected_core = false;
        let mut pages = 0;
        loop {
            assert_eq!(result["isError"], false, "{tool}: {result}");
            let body = &result["structuredContent"];
            if body["projection"]["core_text_shortened"] != true {
                for section in &sections {
                    let key = section[1..].replace('/', ".");
                    let skip = if recall && collected_core {
                        body["projection"]["sections"][&key]["core"]
                            .as_u64()
                            .unwrap_or(0) as usize
                    } else {
                        0
                    };
                    recovered.get_mut(section).expect("collector").extend(
                        body.pointer(section)
                            .expect("section")
                            .as_array()
                            .expect("items")
                            .iter()
                            .skip(skip)
                            .cloned(),
                    );
                }
                collected_core = true;
            }
            let action = if recall {
                &body["projection"]["next_action"]
            } else if body["page"]["has_more"] == true {
                &body["next_actions"][0]
            } else {
                &Value::Null
            };
            if action.is_null() {
                break;
            }
            pages += 1;
            assert!(pages < 40, "{tool} must advance");
            assert_eq!(action["arguments"].as_object().expect("args").len(), 1);
            assert!(
                action["arguments"]["continuation"].is_string(),
                "{tool}: {action}"
            );
            assert_eq!(guidance(&result)["recommendation"]["action"], *action);
            let expanded = server.resolve_read_request(json!({"method":"tools/call","params":{"name":tool,"arguments":action["arguments"]}})).expect("resolve");
            let next = call(&server, tool, action["arguments"].clone()).await;
            let explicit = call(&server, tool, expanded["params"]["arguments"].clone()).await;
            assert_eq!(
                next["structuredContent"], explicit["structuredContent"],
                "{tool} bound call equality"
            );
            result = next;
        }
        assert!(pages > 0, "{tool} exercises a retained call");
        for (section, items) in recovered {
            assert_eq!(
                json!(items),
                *full.pointer(&section).expect("full section"),
                "{tool} {section}: complete proof equality"
            );
        }
    }
}
