//! What a `memories` packet wrote over a ref the caller chose.
//!
//! `memories[].ref` is the update path: the record's text, coordinates,
//! labels, metadata and generated evidence take the place of whatever that
//! ref held. That is a legitimate move, and it is also the move a writer
//! reached for when all it wanted was a link. Naming it in the result — the
//! ref, the evidence this write put on it and the observation it now carries
//! — keeps a replacement from reading like an attachment.

use std::collections::BTreeSet;

use serde_json::{Value, json};

pub(super) fn replaced_memories(ingest: &Value, supplied: &BTreeSet<String>) -> Vec<Value> {
    if supplied.is_empty() {
        return Vec::new();
    }
    let evidence = ingest["memory"]["evidence"].as_array();
    ingest["memory"]["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let reference = entry["id"].as_str()?;
            if !supplied.contains(reference) {
                return None;
            }
            let owned = format!("evidence:{reference}:");
            Some(json!({
                "ref": reference,
                "evidence": evidence
                    .into_iter()
                    .flatten()
                    .filter(|item| item["id"]
                        .as_str()
                        .is_some_and(|id| id.starts_with(&owned)))
                    .map(|item| item["id"].clone())
                    .collect::<Vec<_>>(),
                "observed_at": entry["coordinates"][0]["observed_at"]
            }))
        })
        .collect()
}
