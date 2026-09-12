//! Execute recommended routes against actual memory selections.
#[path = "support/guidance_fixture.rs"]
mod fixture;
#[path = "support/reviewed_writer.rs"]
mod reviewed_writer;
use fixture::*;
use kmp_mcp::KernelMcpServer;
use serde_json::json;

#[tokio::test]
async fn recommendations_execute_native_pages_and_stop_at_temporal_unknown() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let agent = open(&server, "recommendation-reader").await;
    let mut args = packet(&agent["context_id"]);
    args["memories"] = json!((0..12).map(|i| json!({"id":format!("source-{i}"),"kind":"observation",
        "summary":format!("Route {i} opens on Tuesday."),"evidence":format!("Source {i} records the route opening on Tuesday. ").repeat(12)})).collect::<Vec<_>>());
    let written = call(&server, "kmp_write_memory", args).await;
    let written = reviewed_writer::review_authored_write(&server, written).await;
    assert_eq!(written["isError"], false, "{written}");
    let selection = json!({"context_id":agent["context_id"],"purpose":"audit","about":ABOUT,
        "axis":"observed","interval":{"start":"2026-09-01T00:00:00Z","end":"2026-09-10T00:00:00Z"},
        "dimensions":{"selectors":[{"key":"task","op":"in","values":["context-check"]}]},
        "budget":{"max_bytes":4096}});
    let page = call(&server, "kmp_wake", selection.clone()).await;
    assert_eq!(page["isError"], false, "{page}");
    let advice = guidance(&page);
    assert_eq!(advice["signals"]["packet_partial"], true, "{page}");
    assert!(
        page["content"][0]["text"]
            .as_str()
            .expect("primary text")
            .starts_with("READ_INCOMPLETE:")
    );
    assert_eq!(
        advice["recommendation"]["reason_code"], "finish_selected_packet",
        "{advice}"
    );
    let action = &advice["recommendation"]["action"];
    let request = server.resolve_read_request(json!({"method":"tools/call","params":{"name":action["tool"],"arguments":action["arguments"]}})).expect("resolve bound continuation");
    let bound = &request["params"];
    for field in ["context_id", "purpose", "about", "axis", "interval"] {
        assert_eq!(
            bound["arguments"][field], selection[field],
            "{field}: {advice}"
        );
    }
    assert_eq!(
        bound["arguments"]["dimensions"]["selectors"],
        selection["dimensions"]["selectors"]
    );
    assert_eq!(bound["arguments"]["dimensions"]["scope"], "current_about");
    let continued = call(
        &server,
        action["tool"].as_str().expect("tool"),
        action["arguments"].clone(),
    )
    .await;
    assert_eq!(continued["isError"], false, "{continued}");
    let prior = &page["structuredContent"]["projection"];
    let next = &continued["structuredContent"]["projection"];
    assert!(
        next["page"]["offset"].as_u64() > prior["page"]["offset"].as_u64()
            || next["page"]["returned"].as_u64() > prior["page"]["returned"].as_u64()
            || (prior["core_text_shortened"] == true && next["core_text_shortened"] == false),
        "native action must make progress: {continued}"
    );
    let unknown = call(&server, "kmp_ask", json!({"about":ABOUT,"context_id":agent["context_id"],"purpose":"answer",
        "question":"When does the route open?","axis":"observed","interval":{"end":"2026-09-01T00:00:00Z"}})).await;
    assert_eq!(unknown["isError"], false, "{unknown}");
    assert_eq!(unknown["structuredContent"]["answer"], "UNKNOWN");
    assert!(
        !unknown["content"][0]["text"]
            .as_str()
            .expect("primary text")
            .starts_with("READ_INCOMPLETE:")
    );
    let advice = guidance(&unknown);
    assert_eq!(
        advice["recommendation"]["reason_code"], "unknown_in_selection",
        "{advice}"
    );
    assert_eq!(advice["recommendation"]["stop"], true);
    assert!(advice["recommendation"].get("action").is_none());
    assert_eq!(advice["usage"]["unknown"], 1);
}

#[tokio::test]
async fn a_declared_relation_yields_an_executable_audit_and_history_requires_a_clock() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let agent = open(&server, "relation-reader").await;
    let mut args = packet(&agent["context_id"]);
    args["memories"].as_array_mut().expect("memories").push(json!({"id":"decision","kind":"decision",
        "summary":"Schedule delivery on Tuesday.","evidence":"S2 schedules delivery to match the opening in S1.",
        "connect_to":[{"ref":"@source","rel":"chosen_because","class":"causal",
            "why":"Delivery requires an open route.","evidence":"S2 explicitly cites the route opening in S1."}]}));
    let written = call(&server, "kmp_write_memory", args).await;
    let written = reviewed_writer::review_authored_write(&server, written).await;
    assert_eq!(written["isError"], false, "{written}");
    let mut inspect = json!({"about":ABOUT,"context_id":agent["context_id"],"purpose":"audit",
        "ref":written["structuredContent"]["local_refs"]["decision"]});
    let result = call(&server, "kmp_inspect", inspect.clone()).await;
    assert_eq!(result["isError"], false, "{result}");
    let advice = guidance(&result);
    assert_eq!(
        advice["recommendation"]["reason_code"], "audit_declared_relation",
        "{advice}"
    );
    let action = &advice["recommendation"]["action"];
    assert_eq!(action["tool"], "kmp_trace");
    assert_eq!(
        action["arguments"]["from"],
        written["structuredContent"]["local_refs"]["decision"]
    );
    assert_eq!(
        action["arguments"]["to"],
        written["structuredContent"]["local_refs"]["source"]
    );
    let traced = call(&server, "kmp_trace", action["arguments"].clone()).await;
    assert_eq!(traced["isError"], false, "{traced}");
    assert!(
        traced["structuredContent"]
            .to_string()
            .contains("Delivery requires an open route.")
    );
    inspect["purpose"] = json!("history");
    let historical = call(&server, "kmp_inspect", inspect).await;
    let advice = guidance(&historical);
    assert_eq!(
        advice["recommendation"]["reason_code"], "history_needs_clock",
        "{advice}"
    );
    let action = &advice["recommendation"]["action"];
    assert_eq!(
        call(&server, "kmp_guide", action["arguments"].clone()).await["isError"],
        false
    );
}

#[tokio::test]
async fn recall_audit_uses_the_canonical_claim_ref_from_both_evidence_shapes() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let agent = open(&server, "recall-auditor").await;
    let written = call(&server, "kmp_write_memory", packet(&agent["context_id"])).await;
    let written = reviewed_writer::review_authored_write(&server, written).await;
    assert_eq!(written["isError"], false, "{written}");
    for tool in ["kmp_wake", "kmp_ask"] {
        let mut args = json!({"about":ABOUT,"context_id":agent["context_id"],"purpose":"audit"});
        if tool == "kmp_ask" {
            args["question"] = json!("When does the route open?");
        }
        let result = call(&server, tool, args).await;
        assert_eq!(result["isError"], false, "{result}");
        let advice = guidance(&result);
        assert_eq!(
            advice["recommendation"]["reason_code"], "inspect_retrieved_claim",
            "{tool}: {advice}"
        );
        assert_eq!(advice["recommendation"]["changes_selection"], true);
        let action = &advice["recommendation"]["action"];
        assert_eq!(
            action["arguments"]["ref"],
            written["structuredContent"]["local_refs"]["source"]
        );
        let inspected = call(&server, "kmp_inspect", action["arguments"].clone()).await;
        assert_eq!(inspected["isError"], false, "{inspected}");
    }
}

#[tokio::test]
async fn trace_body_option_refusals_offer_reader_card_help_without_replacing_other_lessons() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let agent = open(&server, "trace-help-reader").await;
    let context = &agent["context_id"];

    let body_option = call(
        &server,
        "kmp_trace",
        json!({
            "about": ABOUT, "from": "source", "to": "target",
            "search": {"proof": true, "max_body_record_bytes": 0},
            "context_id": context
        }),
    )
    .await;
    assert_eq!(body_option["isError"], true, "{body_option}");
    let reader_cards = &body_option["structuredContent"]["help"]["examples"][0];
    assert_eq!(
        reader_cards["arguments"]["ref"],
        "guide:kmp-agent:example:reader-cards"
    );
    let lesson = call(
        &server,
        reader_cards["tool"].as_str().expect("tool"),
        reader_cards["arguments"].clone(),
    )
    .await;
    assert_eq!(lesson["isError"], false, "{lesson}");

    let destination = call(
        &server,
        "kmp_trace",
        json!({
            "about": ABOUT, "from": "source", "to": "target",
            "search": {"max_states": 0}, "context_id": context
        }),
    )
    .await;
    assert_eq!(destination["isError"], true, "{destination}");
    assert_eq!(
        destination["structuredContent"]["help"]["examples"][0]["arguments"]["ref"],
        "guide:kmp-agent:example:decision-history"
    );

    let seek = call(
        &server,
        "kmp_trace",
        json!({
            "about": ABOUT, "from": "source", "search": {"seek": []},
            "context_id": context
        }),
    )
    .await;
    assert_eq!(seek["isError"], true, "{seek}");
    assert_eq!(
        seek["structuredContent"]["help"]["examples"][0]["arguments"]["ref"],
        "guide:kmp-agent:example:evidence-seek"
    );
}
