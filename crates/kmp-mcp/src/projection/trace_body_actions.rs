//! Native, copyable actions for the bodies a bounded read did not deliver.
//!
//! A capability that reports a manifest, a deferred ref and an exact cost, and
//! then leaves the agent to rebuild the call, has not delivered the action —
//! it has delivered homework. Everything here is a complete `kmp_trace` call:
//! the same bound query, the same ceiling, the refs still pending, and the
//! manifest the response was computed from.
//!
//! What is deliberately not here: paging. A continuation walks one projection
//! of one selection; these ask for different bodies of the same selection, and
//! conflating them is how a consumer ends up joining chunks that never
//! belonged together.

use serde_json::{Map, Value, json};

/// Refs whose canonical body this response withheld but the store holds, in
/// the order the response listed them, with what each one costs to expand.
fn pending_refs(value: &Value) -> Vec<(String, u64)> {
    value["objects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|object| {
            matches!(
                object["body_state"].as_str(),
                Some("deferred_budget" | "not_requested" | "compact")
            )
        })
        .filter_map(|object| {
            let reference = object["ref"].as_str()?.to_string();
            let cost = object["required_record_bytes"]
                .as_u64()
                .or_else(|| object["descriptor"]["record_bytes"].as_u64())?;
            Some((reference, cost))
        })
        .collect()
}

/// The batch this action names, and the allowance it carries.
///
/// A named batch pays only for what it names, so the allowance is the exact
/// sum of the records in it — never a round number, and never the whole
/// selection. Under a ceiling the caller chose, the batch is the longest
/// prefix that fits; when not even the first record fits, the action names
/// that one record with its exact requirement, so the reader can see the
/// price and decide. Offering it is not reading it.
fn next_batch(pending: &[(String, u64)], declared: Option<u64>) -> (Vec<String>, u64) {
    let mut refs = Vec::new();
    let mut allowance = 0;
    for (reference, cost) in pending {
        match declared {
            Some(ceiling) if allowance + cost > ceiling => break,
            _ => {}
        }
        refs.push(reference.clone());
        allowance += cost;
    }
    match pending.first() {
        Some((reference, cost)) if refs.is_empty() => (vec![reference.clone()], *cost),
        _ => (refs, allowance),
    }
}

/// The bound query, without anything that belongs to one delivery rather than
/// to the selection: no cursor, no requested subset, no stale expectation.
fn bound_query(arguments: &Value) -> Value {
    let mut query = arguments.clone();
    if let Some(search) = query.get_mut("search").and_then(Value::as_object_mut) {
        search.remove("proof_refs");
        search.remove("expect_selection");
    }
    if let Some(object) = query.as_object_mut() {
        object.remove("page");
        object.remove("continuation");
    }
    query
}

fn with_search(query: &Value, mutate: impl FnOnce(&mut Map<String, Value>)) -> Value {
    let mut call = query.clone();
    let search = call
        .as_object_mut()
        .expect("trace arguments")
        .entry("search")
        .or_insert_with(|| json!({}));
    mutate(search.as_object_mut().expect("search object"));
    call
}

/// Attaches the next batch to a delivered response, and the fresh read to a
/// refused one.
pub(crate) fn attach(value: &mut Value, arguments: &Value) {
    if value.get("expansion_refusal").is_some() {
        // The only executable move left: read the selection again and take the
        // manifest it returns. Naming refs or an expectation of the selection
        // that was just refused would repeat the refusal.
        let fresh = bound_query(arguments);
        value["expansion_refusal"]["fresh_read"] =
            json!({"tool": "kmp_trace", "arguments": fresh});
        return;
    }
    let Some(manifest) = value
        .pointer("/proof/manifest_id")
        .and_then(Value::as_str)
        .filter(|manifest| !manifest.is_empty())
    else {
        return;
    };
    let pending = pending_refs(value);
    if pending.is_empty() {
        return;
    }
    let manifest = manifest.to_string();
    let declared = arguments
        .pointer("/search/max_body_record_bytes")
        .and_then(Value::as_u64);
    let (refs, allowance) = next_batch(&pending, declared);
    if refs.is_empty() || allowance == 0 {
        return;
    }
    let batch = with_search(&bound_query(arguments), |search| {
        search.insert("proof_refs".into(), json!(refs));
        search.insert("expect_selection".into(), json!(manifest));
        search.insert("max_body_record_bytes".into(), json!(allowance));
    });
    value["proof"]["expand_bodies"] = json!({"tool": "kmp_trace", "arguments": batch});
}

#[cfg(test)]
#[path = "trace_body_actions_tests.rs"]
mod tests;
