//! The demo bundle shipped in the plugin must stay loadable.
//!
//! `plugins/kmp/demo/checkout-latency.jsonl` is exported from a real store
//! rather than hand-written, but a bundle in a file does not follow the code
//! that reads it. Anything that moves the event format, the bundle header or
//! the projection derivation would leave `/kmp:demo` broken for every new
//! install while every other test stayed green — the failure would surface in
//! front of the one person we most wanted to impress.
//!
//! So the shipped file is imported here on every run, and the shape is
//! asserted rather than the byte count: the incident's story is what the demo
//! is for, and a bundle that loads but has lost its causal chain is no more
//! use than one that does not load.

use std::path::PathBuf;

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_application::projection_mutations_for_context_event;
use kmp_domain::{ContextEventStore, GraphNeighborhoodReader, NodeRelationshipReader};

const ABOUT: &str = "incident:checkout-latency";
const ROLE: &str = "memory";

fn bundle_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins/kmp/demo/checkout-latency.jsonl")
}

#[tokio::test]
async fn the_shipped_demo_bundle_imports_and_keeps_its_story() {
    let bundle =
        std::fs::read_to_string(bundle_path()).expect("the demo bundle ships with the plugin");
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let store = EmbeddedKernelStore::open(data_dir.path()).expect("store opens");

    let report = store
        .import_bundle(&bundle, projection_mutations_for_context_event)
        .await
        .expect("the shipped bundle imports into an empty store");

    assert_eq!(
        report.events_imported, 8,
        "the incident is eight events long"
    );
    assert!(
        report.rebuild.mutations_applied > 0,
        "projections must be rebuilt from the events, not shipped alongside them"
    );

    let revision = store
        .current_revision(ABOUT, ROLE)
        .await
        .expect("revision reads");
    assert_eq!(revision, 8);

    // The anchor and its neighbourhood: what /kmp:demo tells the user to wake.
    // Assert the entries by name rather than by count — the neighbourhood also
    // carries evidence and dimension nodes, and a count would break on any
    // change to how those are materialized without telling us anything about
    // the incident.
    let neighborhood = store
        .load_neighborhood(ABOUT, 1)
        .await
        .expect("neighborhood reads")
        .expect("the incident anchor exists");
    let present: Vec<&str> = neighborhood
        .neighbors
        .iter()
        .map(|node| node.node_id.as_str())
        .collect();
    for entry in [
        "obs:p99-tripled",
        "obs:pool-saturated",
        "decision:rollback-pool-size",
        "obs:rollback-did-not-help",
        "obs:retry-storm",
        "decision:cap-retries",
        "success:p99-recovered",
        "constraint:retry-budget",
    ] {
        let node_id = format!("{ABOUT}:{entry}");
        assert!(
            present.contains(&node_id.as_str()),
            "the incident lost `{entry}`; the demo tells a different story now"
        );
    }

    // The wrong turn is the demo. If this edge ever disappears, the bundle
    // still loads and the demo stops being worth running.
    let rollback = store
        .load_node_relationships(&format!("{ABOUT}:decision:rollback-pool-size"))
        .await
        .expect("relationships read")
        .expect("the rolled-back decision exists");
    let contradiction = rollback
        .incoming
        .iter()
        .find(|relation| relation.relation_type == "contradicts")
        .expect("the rollback is contradicted by what happened after it");
    assert_eq!(
        contradiction.source_node_id,
        format!("{ABOUT}:obs:rollback-did-not-help")
    );
    let explanation = &contradiction.explanation;
    assert!(
        explanation.rationale().is_some_and(|why| !why.is_empty()),
        "the contradiction carries its why: it is the whole point of the example"
    );
    assert!(
        explanation
            .evidence()
            .is_some_and(|proof| !proof.is_empty()),
        "and its proof"
    );
}
