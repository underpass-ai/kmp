//! Which supersessions every page repeats.
//!
//! `proof.superseded` lists every `supersedes` on the proof path, and the
//! path covers every retained memory, not only the cited ones. Kept whole in
//! the stable core it grew with the store's declared history, not with the
//! answer: on FactConsolidation-6k with its supersessions declared it reached
//! 56 entries and about 6 KB of a 10 KB page, and a budget that would carry
//! the cited proof could fail as smaller than the stable citation core.
//!
//! An ask's core keeps the supersessions that bear on what the page cites:
//! those whose replaced or replacing memory is a cited one. A cited memory
//! that was replaced, or the memory a citation replaced, is part of reading
//! the answer. Every other supersession is expansion in its own section,
//! `proof.superseded`, after the ranked evidence: it pages like any other
//! expansion, and an answered first page counts what it did not carry in
//! `projection.more_on_request`. A wake keeps every marker in its core.

use std::collections::BTreeSet;

use serde_json::Value;

use super::json_paths::{push_array, take_array};

/// The path of the supersessions in a recall packet.
pub(super) const SUPERSEDED_PATH: [&str; 2] = ["proof", "superseded"];

/// The memories the core cites: every node its core evidence supports, and
/// the node a `detail:` evidence id names.
pub(super) fn cited_nodes(core_evidence: &[Value]) -> BTreeSet<String> {
    core_evidence
        .iter()
        .flat_map(|evidence| {
            let detail = evidence
                .get("id")
                .and_then(Value::as_str)
                .and_then(|id| id.strip_prefix("detail:"))
                .map(ToString::to_string);
            let supports = evidence
                .get("supports")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(ToString::to_string);
            detail.into_iter().chain(supports)
        })
        .collect()
}

/// Leaves in the core the supersessions that touch a cited memory and
/// returns the rest, in their order, for the plan to page.
pub(super) fn split_superseded(value: &mut Value, cited: &BTreeSet<String>) -> Vec<Value> {
    let mut rest = Vec::new();
    for entry in take_array(value, &SUPERSEDED_PATH) {
        if touches(&entry, cited) {
            push_array(value, &SUPERSEDED_PATH, entry);
        } else {
            rest.push(entry);
        }
    }
    rest
}

fn touches(entry: &Value, cited: &BTreeSet<String>) -> bool {
    ["ref", "superseded_by"].into_iter().any(|key| {
        entry
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|node| cited.contains(node))
    })
}
