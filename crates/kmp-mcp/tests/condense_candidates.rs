//! Card targeting through the public MCP surface and a real SQLite snapshot.
use kmp_mcp::{EmbeddedKernelMcpBackend, KernelMcpServer};
use serde_json::{Value, json};

const ABOUT: &str = "project:condense-candidates";
const ROOT: &str = "project:condense-candidates:root";
const B: &str = "project:condense-candidates:b";
const C: &str = "project:condense-candidates:c";
const SOURCE: &str = "evidence:project:condense-candidates:shared";
const EARLY: &str = "2026-07-22T10:00:00Z";

async fn call(server: &KernelMcpServer, name: &str, arguments: Value) -> Value {
    let raw = server
        .handle_json_line(
            &json!({
                "jsonrpc":"2.0", "id":1, "method":"tools/call",
                "params":{"name":name,"arguments":arguments}
            })
            .to_string(),
        )
        .await
        .expect("valid candidate fixture");
    let response: Value = serde_json::from_str(&raw).expect("valid candidate fixture");
    response["result"].clone()
}

fn content(result: &Value) -> &Value {
    assert_ne!(result["isError"], true, "{result}");
    &result["structuredContent"]
}

fn packet(generation: usize) -> Value {
    let entries = [(ROOT, 5000), (B, 2048), (C, 500)].map(|(id, size)| json!({
        "id":id,"kind":"observation","text":"x".repeat(size),
        "coordinates":[{"dimension":"task","scope_id":"read","occurred_at":EARLY,"observed_at":EARLY}]
    }));
    let relations = [B, C].map(|id| {
        json!({
            "from":ROOT,"to":id,"rel":"depends_on","class":"causal","confidence":"high",
            "why":"Both branches require the signed source.","evidence":"Signed source register.",
            "clocks":{"occurred_at":EARLY,"observed_at":EARLY}
        })
    });
    json!({
        "about":ABOUT,"idempotency_key":format!("seed-{generation}"),
        "memory":{
            "dimensions":[{"id":"read","kind":"task"}],"entries":entries,"relations":relations,
            "evidence":[
                {"id":SOURCE,"supports":[B,C],"text":"s".repeat(8192+generation),"source":"signed register","time":EARLY,
                    "support_clocks":{"observed_at":EARLY}},
                {"id":"evidence:project:condense-candidates:root","supports":[ROOT],"text":"Root register.","source":"root register","time":EARLY,
                    "support_clocks":{"observed_at":EARLY}}
            ]
        }
    })
}

async fn seed(server: &KernelMcpServer, generation: usize) {
    content(&call(server, "kmp_ingest", packet(generation)).await);
}

async fn server() -> (tempfile::TempDir, KernelMcpServer) {
    let dir = tempfile::tempdir().expect("valid candidate fixture");
    let server = KernelMcpServer::with_embedded_backend(
        EmbeddedKernelMcpBackend::open(dir.path()).expect("valid candidate fixture"),
    );
    seed(&server, 0).await;
    (dir, server)
}

fn request() -> Value {
    json!({"about":ABOUT,"from":ROOT,"to":[B,C],
        "search":{"proof":true,"compact":{"language":"es"}},
        "budget":{"max_bytes":200_000}})
}

fn write_from(candidate: &Value) -> Value {
    json!({"about":ABOUT,"ref":candidate["ref"],"scope":"node_body","language":"es",
        "source":candidate["source"],"expect":candidate["expect"],
        "actor":"candidate-test", "card":"El registro firmado respalda ambas ramas."})
}

#[tokio::test]
async fn every_page_ranks_the_shared_source_and_its_identity_can_be_used_directly() {
    let (_dir, server) = server().await;
    let whole = call(&server, "kmp_trace", request()).await;
    let expected = content(&whole)["proof"]["condense_candidates"].clone();
    assert_eq!(expected["items"][0]["ref"], SOURCE);
    assert_eq!(expected["items"][0]["shared_by"], 2);
    assert_eq!(expected["items"][0]["body_bytes"], 8192);
    assert_eq!(expected["below_floor"], 2);
    assert_eq!(content(&whole)["proof"]["body_bytes"], 0);
    assert_eq!(content(&whole)["proof"]["complete_groups"], json!([]));

    let mut paged = request();
    paged["page"] = json!({"entries":1});
    let mut pages = 0;
    loop {
        let result = call(&server, "kmp_trace", paged).await;
        let value = content(&result);
        assert_eq!(value["proof"]["condense_candidates"], expected);
        pages += 1;
        assert!(pages < 50);
        if !value["page"]["has_more"]
            .as_bool()
            .expect("valid candidate fixture")
        {
            break;
        }
        paged = value["next_actions"][0]["arguments"].clone();
    }
    assert!(pages > 2);

    // Read the canonical body before authoring. No descriptor transcribing.
    let mut expansion = request();
    expansion["search"]["proof_refs"] = json!([SOURCE]);
    expansion["search"]["max_body_record_bytes"] = expected["items"][0]["record_bytes"].clone();
    expansion["search"]["expect_selection"] = content(&whole)["proof"]["manifest_id"].clone();
    let expanded = call(&server, "kmp_trace", expansion).await;
    let source = content(&expanded)["objects"]
        .as_array()
        .expect("valid candidate fixture")
        .iter()
        .find(|o| o["ref"] == SOURCE)
        .expect("valid candidate fixture");
    assert_eq!(
        source["text"]
            .as_str()
            .expect("valid candidate fixture")
            .len(),
        8192
    );
    let candidate = &expected["items"][0];
    assert_eq!(
        candidate["source"]["revision"],
        source["descriptor"]["revision"]
    );
    assert_eq!(
        candidate["source"]["record_digest"],
        source["descriptor"]["record_digest"]
    );
    let written = call(&server, "kmp_condense", write_from(candidate)).await;
    content(&written);
    let compact = call(&server, "kmp_trace", request()).await;
    let candidates = &content(&compact)["proof"]["condense_candidates"];
    assert_eq!(candidates["valid"], 1);
    assert!(
        candidates["items"]
            .as_array()
            .expect("valid candidate fixture")
            .iter()
            .all(|c| c["ref"] != SOURCE)
    );

    let mut other_language = request();
    other_language["search"]["compact"]["language"] = json!("en");
    let english = call(&server, "kmp_trace", other_language).await;
    let first = &content(&english)["proof"]["condense_candidates"]["items"][0];
    assert_eq!(first["ref"], SOURCE);
    assert_eq!(first["expect"], json!({"absent":true}));

    // The same stored card is unavailable on a historical read.
    let mut historical = request();
    historical["axis"] = json!("occurred");
    historical["as_of"] = json!({"time":"2026-08-01T00:00:00Z"});
    let past = call(&server, "kmp_trace", historical).await;
    let candidates = &content(&past)["proof"]["condense_candidates"];
    assert_eq!(candidates["after_cut"], 1, "{past}");
    assert!(
        candidates["items"]
            .as_array()
            .expect("valid candidate fixture")
            .iter()
            .all(|c| c["ref"] != SOURCE)
    );
}

#[tokio::test]
async fn moved_source_is_refused_and_a_stale_card_carries_the_exact_cas_revision() {
    let (_dir, server) = server().await;
    let before = call(&server, "kmp_trace", request()).await;
    let old = content(&before)["proof"]["condense_candidates"]["items"][0].clone();
    content(&call(&server, "kmp_condense", write_from(&old)).await);
    seed(&server, 1).await;
    let refused = call(&server, "kmp_condense", write_from(&old)).await;
    assert_eq!(refused["isError"], true);
    assert_eq!(refused["structuredContent"]["error"]["code"], "conflict");
    assert!(
        refused.to_string().contains("body of this entry moved"),
        "{refused}"
    );
    let now = call(&server, "kmp_trace", request()).await;
    let fresh = &content(&now)["proof"]["condense_candidates"]["items"][0];
    assert_eq!(fresh["ref"], SOURCE);
    assert_eq!(fresh["card_status"], "stale");
    assert_eq!(fresh["expect"], json!({"card_revision":1}));
    assert_ne!(fresh["source"], old["source"]);
    content(&call(&server, "kmp_condense", write_from(fresh)).await);
    let concurrent = call(&server, "kmp_condense", write_from(fresh)).await;
    assert_eq!(concurrent["isError"], true);
    assert_eq!(concurrent["structuredContent"]["error"]["code"], "conflict");
    assert!(concurrent.to_string().contains("stored card revision 2"));
}

#[tokio::test]
async fn noncompact_reads_and_refused_expansions_do_not_offer_candidates() {
    let (_dir, server) = server().await;
    let mut plain = request();
    plain["search"]
        .as_object_mut()
        .expect("valid candidate fixture")
        .remove("compact");
    let read = call(&server, "kmp_trace", plain).await;
    assert!(content(&read)["proof"].get("condense_candidates").is_none());
    let mut unknown = request();
    unknown["search"]["proof_refs"] = json!(["foreign:ref"]);
    let compact = call(&server, "kmp_trace", request()).await;
    unknown["search"]["expect_selection"] = content(&compact)["proof"]["manifest_id"].clone();
    unknown["search"]["max_body_record_bytes"] = json!(100_000);
    let refused = call(&server, "kmp_trace", unknown).await;
    assert!(
        content(&refused)["proof"]
            .get("condense_candidates")
            .is_none()
    );
}

#[tokio::test]
async fn seek_counts_complete_structural_groups_before_compact_body_delivery() {
    let (_dir, server) = server().await;
    let mut seek = request();
    seek.as_object_mut()
        .expect("valid candidate fixture")
        .remove("to");
    seek["search"]["seek"] = json!(["depends_on"]);
    let result = call(&server, "kmp_trace", seek).await;
    let value = content(&result);
    assert_eq!(
        value["groups"]
            .as_array()
            .expect("valid candidate fixture")
            .len(),
        2,
        "{value}"
    );
    let candidates = &value["proof"]["condense_candidates"]["items"];
    assert_eq!(candidates[0]["ref"], SOURCE);
    assert_eq!(candidates[0]["shared_by"], 2);
    assert_eq!(value["proof"]["complete_groups"], json!([]));
}

#[tokio::test]
async fn seek_does_not_count_groups_with_missing_witnesses_as_shared_proof() {
    let (_dir, server) = server().await;
    let mut seek = request();
    seek.as_object_mut()
        .expect("valid candidate fixture")
        .remove("to");
    seek["search"]["seek"] = json!(["depends_on"]);
    seek["search"]["same_labels"] = json!(["event"]);
    let result = call(&server, "kmp_trace", seek).await;
    let value = content(&result);
    assert_eq!(value["seek"]["status"], "review_required", "{value}");
    assert_eq!(value["groups"].as_array().expect("groups").len(), 2);
    let candidates = value["proof"]["condense_candidates"]["items"]
        .as_array()
        .expect("candidates");
    assert!(!candidates.is_empty());
    assert!(
        candidates.iter().all(|c| c["shared_by"] == 0),
        "{candidates:?}"
    );
}

#[tokio::test]
async fn historical_seek_does_not_count_groups_with_unknown_relation_clocks() {
    let dir = tempfile::tempdir().expect("temporary store");
    let server = KernelMcpServer::with_embedded_backend(
        EmbeddedKernelMcpBackend::open(dir.path()).expect("SQLite backend"),
    );
    let mut seed = packet(0);
    for edge in seed["memory"]["relations"]
        .as_array_mut()
        .expect("relations")
    {
        edge.as_object_mut().expect("relation").remove("clocks");
    }
    content(&call(&server, "kmp_ingest", seed).await);
    let mut seek = request();
    seek.as_object_mut().expect("request").remove("to");
    seek["search"]["seek"] = json!(["depends_on"]);
    seek["axis"] = json!("occurred");
    seek["as_of"] = json!({"time":"2026-08-01T00:00:00Z"});
    let result = call(&server, "kmp_trace", seek).await;
    let value = content(&result);
    assert_eq!(value["seek"]["status"], "review_required", "{value}");
    let groups = value["groups"].as_array().expect("groups");
    assert_eq!(groups.len(), 2);
    assert!(groups.iter().all(|g| g["clock_unknown"] == true));
    let candidates = value["proof"]["condense_candidates"]["items"]
        .as_array()
        .expect("candidates");
    assert!(!candidates.is_empty());
    assert!(
        candidates.iter().all(|c| c["shared_by"] == 0),
        "{candidates:?}"
    );
}

#[tokio::test]
async fn material_selection_counts_only_returned_routes() {
    let (_dir, server) = server().await;
    let mut selected = request();
    selected["search"]["select"] = json!({"max_material_nodes":2,"max_paths":1});
    let result = call(&server, "kmp_trace", selected).await;
    let value = content(&result);
    assert_eq!(value["routes"].as_array().expect("routes").len(), 1);
    assert_eq!(value["search"]["material"]["candidate_count"], 2);
    let candidates = value["proof"]["condense_candidates"]["items"]
        .as_array()
        .expect("candidates");
    assert!(!candidates.is_empty());
    assert!(
        candidates.iter().all(|c| c["shared_by"] == 1),
        "{candidates:?}"
    );
}
