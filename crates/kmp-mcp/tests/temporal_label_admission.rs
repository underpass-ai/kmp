//! #711: late label assertions cannot rewrite historical entry or proof selection.
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:label-cut";
const FACT: &str = "project:label-cut:observation:fact";
const SOURCE: &str = "project:label-cut:observation:source";
const EARLY: &str = "2026-09-01T10:00:00Z";
const CUT: &str = "2026-09-01T12:00:00Z";
const LATE: &str = "2026-09-02T10:00:00Z";

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let response = server
        .handle_json_line(
            &json!({"jsonrpc":"2.0","id":1,
        "method":"tools/call","params":{"name":tool,"arguments":arguments}})
            .to_string(),
        )
        .await
        .expect("response");
    let response: Value = serde_json::from_str(&response).expect("JSON");
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(response["result"]["isError"], false, "{response}");
    response["result"]["structuredContent"].clone()
}

async fn seed(server: &KernelMcpServer) {
    let entries = [(FACT, EARLY), (SOURCE, LATE)]
        .into_iter()
        .map(|(id, env_at)| {
            json!({
                "id":id,"kind":"observation","text":format!("The register records {id}."),
                "coordinates":[{"dimension":"task","scope_id":"register","observed_at":EARLY},
                    {"dimension":"env","scope_id":"prod","observed_at":env_at}]
            })
        })
        .collect::<Vec<_>>();
    let evidence = [FACT, SOURCE]
        .into_iter()
        .enumerate()
        .map(|(i, id)| {
            json!({
                "id":format!("evidence:{ABOUT}:{i}"),"text":format!("Register proves {id}."),
                "source":"fixture:labels","time":EARLY,"supports":[id]
            })
        })
        .collect::<Vec<_>>();
    call(server,"kmp_ingest",json!({"about":ABOUT,"idempotency_key":"label-cut:seed",
        "provenance":{"source_kind":"agent","source_agent":"test","observed_at":EARLY},
        "memory":{"dimensions":[{"kind":"task","id":"register"},{"kind":"env","id":"prod"}],
            "entries":entries,"evidence":evidence,"relations":[{"from":FACT,"to":SOURCE,
                "rel":"verified_by","class":"evidential","confidence":"high","why":"The register explicitly names this verifier.",
                "evidence":"The fact is verified by the source.","clocks":{"observed_at":EARLY}}]}})).await;
}

fn query() -> Value {
    json!({"about":ABOUT,"at":{"time":CUT},"axis":"observed","refs":[FACT],
        "include":{"dependencies":true},"limit":{"entries":20},"budget":{"max_bytes":1_000_000}})
}

fn assert_no_late_coordinates(entry: &Value) {
    for coordinate in entry["coordinates"].as_array().expect("coordinates") {
        assert_ne!(coordinate["observed_at"], LATE, "{entry}");
    }
}

#[tokio::test]
async fn entry_and_dependency_predicates_and_coordinates_obey_goto_cut() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    seed(&server).await;
    let full = call(&server, "kmp_goto", query()).await;
    assert_eq!(full["proof"]["entries"][0]["ref"], SOURCE, "{full}");
    assert_no_late_coordinates(&full["entries"][0]);
    assert_no_late_coordinates(&full["proof"]["entries"][0]);
    assert_eq!(
        full["proof"]["entries"][0]["coordinates"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    let mut filtered = query();
    filtered["dimensions"] = json!({"mode":"only","include":["task"],
        "selectors":[{"key":"env","op":"in","values":["prod"]}]});
    let response = call(&server, "kmp_goto", filtered.clone()).await;
    assert_eq!(response["entries"][0]["ref"], FACT);
    assert_eq!(
        response["entries"][0]["coordinates"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        response["proof"]["entries"],
        json!([]),
        "future label cannot qualify a dependency"
    );
    filtered["refs"] = json!([SOURCE]);
    assert_eq!(
        call(&server, "kmp_goto", filtered.clone()).await["entries"],
        json!([])
    );
    filtered["dimensions"]["selectors"] = json!([{"key":"env","op":"notexists"}]);
    assert_eq!(
        call(&server, "kmp_goto", filtered.clone()).await["entries"][0]["ref"],
        SOURCE
    );
    filtered["dimensions"]["selectors"] = json!([{"key":"env","op":"in","values":["prod"]}]);
    filtered["at"]["time"] = json!(LATE);
    assert_eq!(
        call(&server, "kmp_goto", filtered).await["entries"][0]["ref"],
        SOURCE
    );
}

#[tokio::test]
async fn historical_response_continuations_reconstruct_the_same_coordinates_and_proof() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    seed(&server).await;
    let full = call(&server, "kmp_goto", query()).await;
    let mut args = query();
    args["page"] = json!({"entries":1});
    let mut page = call(&server, "kmp_goto", args).await;
    let pointers = full["page"]["sections"]
        .as_object()
        .expect("sections")
        .keys()
        .map(|name| format!("/{}", name.replace('.', "/")))
        .collect::<Vec<_>>();
    let mut joined = full.clone();
    for pointer in &pointers {
        *joined.pointer_mut(pointer).expect("section") = json!([]);
    }
    let mut count = 0;
    loop {
        count += 1;
        for pointer in &pointers {
            joined
                .pointer_mut(pointer)
                .expect("section")
                .as_array_mut()
                .expect("array")
                .extend(
                    page.pointer(pointer)
                        .expect("section")
                        .as_array()
                        .expect("array")
                        .clone(),
                );
        }
        if page["page"]["has_more"] == false {
            break;
        }
        assert!(count < 30, "continuation must advance");
        let action = &page["next_actions"][0];
        let next = call(
            &server,
            action["tool"].as_str().expect("tool"),
            action["arguments"].clone(),
        )
        .await;
        assert!(next["page"]["offset"].as_u64() > page["page"]["offset"].as_u64());
        page = next;
    }
    assert!(count > 1);
    for pointer in &pointers {
        assert_eq!(joined.pointer(pointer), full.pointer(pointer), "{pointer}");
    }
}
