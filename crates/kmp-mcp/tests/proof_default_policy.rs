//! Semantic defaults do not retime explicit or restored canonical declarations.
#[path = "support/write_neighborhood_fixture.rs"]
mod fixture;

use fixture::{call, scratch};
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:proof-default-policy";
const FROM: &str = "project:proof-default-policy:observation:claim";
const TO: &str = "project:proof-default-policy:observation:check";
const OLD: &str = "2026-09-01T10:00:00Z";
const LATE: &str = "2026-09-02T10:00:00Z";

fn packet() -> Value {
    let entries = [FROM, TO].map(|id| {
        json!({"id":id,"kind":"observation","text":"Recorded source.",
        "coordinates":[{"dimension":"task","scope_id":"audit","observed_at":OLD}]})
    });
    json!({"about":ABOUT,"idempotency_key":"canonical-proof",
        "provenance":{"source_kind":"agent","source_agent":"investigator","observed_at":LATE},
        "memory":{"dimensions":[{"id":"audit","kind":"task"}],
        "entries":entries,
        "relations":[{"from":FROM,"to":TO,"rel":"verified_by","class":"evidential","confidence":"high",
            "why":"The review connects the recorded check to the claim.","evidence":"Review R3 makes the connection."}],
        "evidence":[{"id":"evidence:project:proof-default-policy:log","supports":[FROM],
            "text":"The original log.","source":"fixture:R1","time":OLD}]}})
}

#[tokio::test]
async fn proof_defaults_preserve_canonical_inheritance_explicit_and_restored_clocks() {
    // None = absent object: inherit the packet. Empty object with the semantic
    // policy = this declaration has no authored observation: exact ingestion.
    for policy in [false, true] {
        for supplied in [
            None,
            Some(json!({})),
            Some(json!({"observed_at":OLD})),
            Some(json!({"ingested_at":OLD})),
        ] {
            let dir = scratch();
            let server = KernelMcpServer::embedded(dir.path()).expect("server");
            let mut args = packet();
            args["default_observation_to_ingestion"] = json!(policy);
            if let Some(clocks) = &supplied {
                args["memory"]["relations"][0]["clocks"] = clocks.clone();
                args["memory"]["evidence"][0]["support_clocks"] = clocks.clone();
            }
            call(&server, "kmp_ingest", args).await;
            let inspected = call(
                &server,
                "kmp_inspect",
                json!({"about":ABOUT,"ref":FROM,"budget":{"max_bytes":100000}}),
            )
            .await;
            let link = inspected["links"]["outgoing"]
                .as_array()
                .expect("links")
                .iter()
                .find(|r| r["rel"] == "verified_by")
                .expect("link");
            let source = &inspected["evidence"][0];
            assert_eq!(source["time"], OLD);
            for clocks in [&link["clocks"], &source["support_clocks"]] {
                if supplied
                    .as_ref()
                    .is_some_and(|c| c["ingested_at"].is_string())
                {
                    assert!(clocks.get("observed_at").is_none(), "{clocks}");
                    assert_eq!(clocks["ingested_at"], OLD);
                } else if supplied
                    .as_ref()
                    .is_some_and(|c| c["observed_at"].is_string())
                {
                    assert_eq!(clocks["observed_at"], OLD);
                } else if policy && supplied.is_some() {
                    assert_eq!(clocks["observed_at"], clocks["ingested_at"]);
                } else {
                    assert_eq!(clocks["observed_at"], LATE);
                }
            }
        }
    }
}

#[tokio::test]
async fn explicit_relation_coordinate_remains_authoritative_under_semantic_defaults() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let mut args = packet();
    args["default_observation_to_ingestion"] = json!(true);
    args["memory"]["relations"][0]["clocks"] = json!({});
    args["memory"]["relations"][0]["coordinate"] =
        json!({"dimension":"task","scope_id":"audit","observed_at":OLD});
    call(&server, "kmp_ingest", args).await;
    let inspected = call(
        &server,
        "kmp_inspect",
        json!({"about":ABOUT,"ref":FROM,"budget":{"max_bytes":100000}}),
    )
    .await;
    let link = inspected["links"]["outgoing"]
        .as_array()
        .expect("links")
        .iter()
        .find(|r| r["rel"] == "verified_by")
        .expect("link");
    assert_eq!(link["clocks"]["observed_at"], OLD);
}
