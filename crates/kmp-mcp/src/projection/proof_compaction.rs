//! A recall's proof without what the same proof already says.
//!
//! Wake and Ask spell the same long ref again and again: a hop names its
//! endpoints in `from` and `to`, then once more in `evidence_refs` as each
//! endpoint's own detail evidence (`detail:<ref>`); that detail evidence
//! names its ref in `id` and again in `supports`. Every ref an agent copies
//! back stays verbatim where it is stated; only the repeats the reader can
//! rebuild from the same object are dropped:
//!
//! - a hop's `evidence_refs` keeps the evidence of the relation itself; the
//!   endpoints' detail evidence, `detail:<from>` and `detail:<to>`, supports
//!   the hop whenever `proof.evidence` carries it;
//! - a detail evidence item (`id` = `detail:<ref>`) that supports only that
//!   ref has no `supports`;
//! - `metadata` that only says `proof_role: stored_evidence`, the role of
//!   every evidence item that is not the memory's own text, is left out.
//!
//! No id changes, so every `evidence_refs`, `because` and continuation keeps
//! resolving; `projection.budget.used_bytes` reports the compacted bytes.

use serde_json::{Map, Value};

const DEFAULT_ROLE: &str = "stored_evidence";

/// Compacts `structured`'s proof in place.
pub(crate) fn compact_recall_proof(structured: &mut Value) {
    let before = serialized_len(structured);
    let Some(proof) = structured.get_mut("proof").and_then(Value::as_object_mut) else {
        return;
    };
    for hop in proof
        .get_mut("path")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object_mut)
    {
        without_endpoint_details(hop);
    }
    for item in proof
        .get_mut("evidence")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object_mut)
    {
        without_self_support(item);
        if item
            .get("metadata")
            .and_then(Value::as_object)
            .is_some_and(|metadata| {
                metadata.len() == 1
                    && metadata.get("proof_role") == Some(&Value::from(DEFAULT_ROLE))
            })
        {
            item.remove("metadata");
        }
    }
    let saved = before.saturating_sub(serialized_len(structured));
    if let Some(used) = structured
        .pointer_mut("/projection/budget/used_bytes")
        .filter(|used| used.is_u64())
    {
        *used = Value::from(
            used.as_u64()
                .unwrap_or_default()
                .saturating_sub(saved as u64),
        );
    }
}

fn without_endpoint_details(hop: &mut Map<String, Value>) {
    let endpoints = ["from", "to"]
        .iter()
        .filter_map(|key| hop.get(*key).and_then(Value::as_str))
        .map(|reference| format!("detail:{reference}"))
        .collect::<Vec<_>>();
    if let Some(Value::Array(refs)) = hop.get_mut("evidence_refs") {
        refs.retain(|id| {
            !id.as_str()
                .is_some_and(|id| endpoints.iter().any(|e| e == id))
        });
    }
}

fn without_self_support(item: &mut Map<String, Value>) {
    let own = match (item.get("id"), item.get("supports")) {
        (Some(Value::String(id)), Some(Value::Array(supports))) => {
            supports.len() == 1
                && supports[0]
                    .as_str()
                    .is_some_and(|reference| id.strip_prefix("detail:") == Some(reference))
        }
        _ => false,
    };
    if own {
        item.remove("supports");
    }
}

fn serialized_len(value: &Value) -> usize {
    serde_json::to_vec(value).map_or(0, |bytes| bytes.len())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn repeats_the_same_proof_states_are_dropped_and_every_id_is_kept() {
        let mut structured = json!({
            "proof": {
                "path": [{"from": "p:a", "to": "p:b", "rel": "depends_on",
                    "evidence_refs": ["detail:evidence:p:a:relation:1", "detail:p:a", "detail:p:b"]}],
                "evidence": [
                    {"id": "detail:p:a", "supports": ["p:a"], "text": "A.",
                     "metadata": {"proof_role": "entry_text"}},
                    {"id": "detail:evidence:p:a:relation:1", "supports": ["p:a", "p:b"],
                     "text": "E.", "metadata": {"proof_role": "stored_evidence"}},
                    {"id": "detail:evidence:p:a:current", "supports": ["p:a"], "text": "F.",
                     "metadata": {"proof_role": "stored_evidence", "reached_by": "x"}}
                ]
            },
            "projection": {"budget": {"used_bytes": 900}}
        });
        let before = serde_json::to_vec(&structured).expect("JSON").len();
        compact_recall_proof(&mut structured);
        let proof = &structured["proof"];
        assert_eq!(
            proof["path"][0]["evidence_refs"],
            json!(["detail:evidence:p:a:relation:1"])
        );
        assert!(proof["evidence"][0].get("supports").is_none());
        assert_eq!(proof["evidence"][0]["metadata"]["proof_role"], "entry_text");
        assert_eq!(proof["evidence"][1]["supports"], json!(["p:a", "p:b"]));
        assert!(proof["evidence"][1].get("metadata").is_none());
        assert_eq!(proof["evidence"][2]["supports"], json!(["p:a"]));
        assert_eq!(proof["evidence"][2]["metadata"]["reached_by"], "x");
        let after = serde_json::to_vec(&structured).expect("JSON").len();
        assert_eq!(
            structured["projection"]["budget"]["used_bytes"],
            900 - (before - after) as u64
        );
    }

    #[test]
    fn a_response_without_proof_is_untouched() {
        let mut structured = json!({"answer": "UNKNOWN"});
        compact_recall_proof(&mut structured);
        assert_eq!(structured, json!({"answer": "UNKNOWN"}));
    }
}
