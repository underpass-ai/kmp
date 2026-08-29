//! The two guides shipped in the plugin must remain importable and useful.
//!
//! The regular format-2 bundle is the empty-store fast path used by setup.
//! Later guide versions converge through public ingest, but both paths are
//! generated from the same requests and must preserve the agent/human split.

use std::path::PathBuf;

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_application::projection_mutations_for_context_event;
use kmp_domain::{ContextEventStore, GraphNeighborhoodReader, NodeRelationshipReader};

const ROLE: &str = "memory";

fn bundle_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins/kmp/guide/memory.jsonl")
}

#[tokio::test]
async fn shipped_guides_import_and_keep_distinct_audiences() {
    let bundle = std::fs::read_to_string(bundle_path()).expect("the guide bundle ships");
    let header: serde_json::Value =
        serde_json::from_str(bundle.lines().next().expect("the bundle has a header"))
            .expect("the guide header is JSON");
    assert_eq!(header["bundle_format"], 2);
    assert_eq!(header["event_count"], 2);
    assert_eq!(header["kernel_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        header["abouts"],
        serde_json::json!(["guide:kmp", "guide:kmp-agent"])
    );

    let data_dir = tempfile::tempdir().expect("temp data dir");
    let store = EmbeddedKernelStore::open(data_dir.path()).expect("store opens");
    let report = store
        .import_bundle(&bundle, projection_mutations_for_context_event)
        .await
        .expect("the shipped guide bundle imports into an empty store");
    assert_eq!(report.events_imported, 2);
    assert!(report.rebuild.mutations_applied > 0);

    for about in ["guide:kmp", "guide:kmp-agent"] {
        assert_eq!(
            store.current_revision(about, ROLE).await.expect("revision"),
            1,
            "each audience is one independently addressable guide event"
        );
        assert!(
            store
                .load_neighborhood(about, 1)
                .await
                .expect("guide neighborhood reads")
                .is_some(),
            "{about} has a navigable projection"
        );
    }

    let agent_verb = store
        .load_node_relationships("guide:kmp-agent:verb:wake")
        .await
        .expect("agent relationships read")
        .expect("the agent wake rule exists");
    assert!(
        agent_verb
            .incoming
            .iter()
            .chain(agent_verb.outgoing.iter())
            .any(|relation| relation
                .explanation
                .rationale()
                .is_some_and(|why| !why.is_empty())
                && relation
                    .explanation
                    .evidence()
                    .is_some_and(|proof| !proof.is_empty())),
        "the operational verb is linked by an explained, evidenced relation"
    );

    let live_tool = store
        .load_node_relationships("guide:kmp-agent:tool:kmp_write_memory")
        .await
        .expect("live-tool relationships read")
        .expect("the agent guide includes the live write tool");
    assert!(
        live_tool
            .outgoing
            .iter()
            .any(|relation| relation.target_node_id == "guide:kmp-agent:verb:write"),
        "the exact MCP contract grounds the human-readable write verb"
    );

    let human_welcome = store
        .load_node_relationships("guide:kmp:welcome")
        .await
        .expect("human relationships read")
        .expect("the human welcome exists");
    assert!(
        human_welcome
            .incoming
            .iter()
            .any(|relation| relation.source_node_id == "guide:kmp:basic:memory"),
        "the visual human guide starts with its short basic path"
    );
}
