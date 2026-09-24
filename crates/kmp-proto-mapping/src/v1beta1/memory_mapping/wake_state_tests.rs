//! A resume packet states the memories it resumes, not the bookkeeping that
//! ties sources to them, and never turns a historical link into a task.
use super::responses::wake_response_from_result;
use kmp_application::{GetContextResult, queries::render_graph_bundle};
use kmp_domain::{
    BundleMetadata, BundleNode, BundleRelationship, CaseId, KmpBundle, RelationExplanation,
    RelationSemanticClass, Role, TemporalSelection,
};
use kmp_proto::v1beta1::WakeResponse;
use std::collections::BTreeMap;

fn node(id: &str, kind: &str, summary: &str) -> BundleNode {
    BundleNode::new(id, kind, id, summary, "ACTIVE", Vec::new(), BTreeMap::new())
}

fn edge(source: &str, target: &str, rel: &str, class: RelationSemanticClass) -> BundleRelationship {
    BundleRelationship::new(
        source,
        target,
        rel,
        RelationExplanation::new(class).with_rationale(format!("{source} {rel} {target}")),
    )
}

/// Two live memories, one replaced decision, one historical `updates_state`
/// link and six sources whose `supports` edges outnumber the memories.
fn wake() -> WakeResponse {
    let structural = RelationSemanticClass::Structural;
    let evidential = RelationSemanticClass::Evidential;
    let entries = [
        (
            "decision:cache-v2",
            "decision",
            "Serve reads from the v2 cache.",
        ),
        (
            "decision:cache-v1",
            "decision",
            "Serve reads from the v1 cache.",
        ),
        (
            "observation:latency",
            "observation",
            "p95 latency fell to 40 ms.",
        ),
    ];
    let sources = (0..6)
        .map(|index| format!("evidence:source-{index}"))
        .collect::<Vec<_>>();
    let mut nodes = vec![node("lane", "lane", "")];
    nodes.extend(entries.iter().map(|(id, kind, text)| node(id, kind, text)));
    nodes.extend(
        sources
            .iter()
            .map(|id| node(id, "memory_evidence", "a source")),
    );
    let mut edges = entries
        .iter()
        .map(|(id, _, _)| edge("lane", id, "contains_entry", structural))
        .collect::<Vec<_>>();
    edges.extend(sources.iter().enumerate().map(|(index, id)| {
        let target = entries[index % entries.len()].0;
        edge(id, target, "supports", evidential)
    }));
    edges.push(edge(
        "decision:cache-v2",
        "decision:cache-v1",
        "supersedes",
        RelationSemanticClass::Causal,
    ));
    edges.push(edge(
        "observation:latency",
        "decision:cache-v2",
        "updates_state",
        RelationSemanticClass::Causal,
    ));
    let bundle = KmpBundle::new(
        CaseId::new("about:x").expect("about"),
        Role::new("reader").expect("role"),
        node("about:x", "about", ""),
        nodes,
        edges,
        Vec::new(),
        BundleMetadata::initial("test"),
    )
    .expect("bundle");
    let rendered = render_graph_bundle(&bundle);
    let result = GetContextResult {
        read_revision: None,
        bundle,
        rendered,
        requested_scopes: Vec::new(),
        served_at: std::time::SystemTime::UNIX_EPOCH,
        timing: None,
    };
    wake_response_from_result("resume", None, result, &TemporalSelection::Frontier).expect("wake")
}

fn current_state(response: &WakeResponse) -> &[String] {
    &response.wake.as_ref().expect("wake packet").current_state
}

fn position(state: &[String], needle: &str) -> Option<usize> {
    state.iter().position(|line| line.contains(needle))
}

#[test]
fn live_memories_lead_the_state_ahead_of_support_bookkeeping() {
    let response = wake();
    let state = current_state(&response);

    let v2 = position(state, "Serve reads from the v2 cache.").expect("live decision in state");
    let latency = position(state, "p95 latency fell to 40 ms.")
        .unwrap_or_else(|| panic!("live observation in state: {state:#?}"));
    if let Some(support) = position(state, "--supports-->") {
        assert!(v2 < support && latency < support, "{state:#?}");
    }
}

#[test]
fn a_replaced_decision_does_not_read_as_current_before_its_replacement() {
    let response = wake();
    let state = current_state(&response);

    let v2 = position(state, "Serve reads from the v2 cache.").expect("live decision in state");
    if let Some(v1) = position(state, "Serve reads from the v1 cache.") {
        assert!(v2 < v1, "{state:#?}");
    }
}

#[test]
fn a_historical_relation_is_not_a_next_action() {
    let response = wake();
    let packet = response.wake.as_ref().expect("wake packet");

    assert!(packet.next_actions.is_empty(), "{:?}", packet.next_actions);
    let next = response
        .summary
        .lines()
        .find(|line| line.starts_with("Next:"))
        .expect("the summary still states its next action line");
    assert!(
        !next.contains("updates_state") && !next.contains("supersedes"),
        "{next}"
    );
}
