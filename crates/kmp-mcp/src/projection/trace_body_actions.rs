//! Native, copyable actions for the bodies a bounded read did not deliver.
//!
//! A capability that reports a manifest, a deferred ref and an exact cost, and
//! then leaves the agent to rebuild the call, has not delivered the action —
//! it has delivered homework. Everything here is a complete `kmp_trace` call:
//! the same bound query, the ceiling the caller chose, the next refs of the
//! selection, and the manifest the response was computed from.
//!
//! **These continue a suffix; they do not promise a whole selection.** The
//! response carries no memory of earlier calls, so the only honest frontier is
//! the last ref of the manifest order that *this* response actually loaded.
//! Everything after it is offered; everything before it the consumer already
//! holds, or deliberately skipped, and either way it is named by its own
//! `body_state`. A consumer joins responses by ref against one manifest. No
//! action here says a selection is complete, and the end of one page is not
//! the end of a selection.
//!
//! What is deliberately not here: paging. A continuation walks one projection
//! of one selection; these ask for different bodies of the same selection.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use kmp_domain::MAX_EXPANSION_REFS;

/// One object of the manifest order, as this response reports it.
struct Slot {
    reference: String,
    loaded: bool,
    /// `Some` when the store holds a body this response did not deliver.
    pending_cost: Option<u64>,
}

fn slots(value: &Value) -> Vec<Slot> {
    value["objects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|object| {
            let reference = object["ref"].as_str()?.to_string();
            let state = object["body_state"].as_str()?;
            let pending_cost = matches!(
                state,
                "deferred_budget" | "not_requested" | "compact"
            )
            .then(|| {
                object["required_record_bytes"]
                    .as_u64()
                    .or_else(|| object["descriptor"]["record_bytes"].as_u64())
            })
            .flatten();
            Some(Slot {
                reference,
                loaded: state == "loaded",
                pending_cost,
            })
        })
        .collect()
}

/// Where a suffix continuation may start.
///
/// One ref past the last body this response really loaded. A ref the caller
/// asked for and did not get — because it did not fit — never advances the
/// frontier, so the next action offers it again instead of walking past it.
/// With nothing loaded the frontier is the start of what the caller asked
/// about: the beginning of the selection, or the first ref it named.
fn frontier(slots: &[Slot], named: Option<&BTreeSet<String>>) -> usize {
    let considered = |slot: &Slot| named.is_none_or(|named| named.contains(&slot.reference));
    let last_loaded = slots
        .iter()
        .enumerate()
        .filter(|(_, slot)| slot.loaded && considered(slot))
        .map(|(index, _)| index)
        .next_back();
    match (last_loaded, named) {
        (Some(index), _) => index + 1,
        (None, None) => 0,
        (None, Some(_)) => slots
            .iter()
            .position(considered)
            .unwrap_or(slots.len()),
    }
}

/// The next batch of the suffix, and the allowance it carries.
///
/// Bounded twice: by `MAX_EXPANSION_REFS`, because a batch the parser would
/// refuse is not a copyable action, and by the ceiling the caller chose, which
/// is carried through unchanged. A record larger than that ceiling on its own
/// is skipped here and kept explicitly pending: the rest of the suffix is
/// still recoverable, and the large one is offered separately, at a price the
/// caller can see and refuse.
fn next_batch(suffix: &[&Slot], ceiling: Option<u64>) -> (Vec<String>, u64) {
    let mut refs = Vec::new();
    let mut total = 0;
    for slot in suffix {
        let Some(cost) = slot.pending_cost else {
            continue;
        };
        if refs.len() == MAX_EXPANSION_REFS {
            break;
        }
        match ceiling {
            Some(ceiling) if cost > ceiling => continue,
            Some(ceiling) if total + cost > ceiling => break,
            _ => {}
        }
        refs.push(slot.reference.clone());
        total += cost;
    }
    (refs, ceiling.unwrap_or(total))
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

fn expansion(query: &Value, refs: Vec<String>, manifest: &str, allowance: u64) -> Value {
    let call = with_search(query, |search| {
        search.insert("proof_refs".into(), json!(refs));
        search.insert("expect_selection".into(), json!(manifest));
        search.insert("max_body_record_bytes".into(), json!(allowance));
    });
    json!({"tool": "kmp_trace", "arguments": call})
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
    let manifest = manifest.to_string();
    let slots = slots(value);
    let named: Option<BTreeSet<String>> = arguments
        .pointer("/search/proof_refs")
        .and_then(Value::as_array)
        .map(|refs| {
            refs.iter()
                .filter_map(|reference| reference.as_str().map(str::to_string))
                .collect()
        })
        .filter(|refs: &BTreeSet<String>| !refs.is_empty());
    let ceiling = arguments
        .pointer("/search/max_body_record_bytes")
        .and_then(Value::as_u64);
    let suffix: Vec<&Slot> = slots[frontier(&slots, named.as_ref()).min(slots.len())..]
        .iter()
        .collect();
    let query = bound_query(arguments);

    let (refs, allowance) = next_batch(&suffix, ceiling);
    if !refs.is_empty() && allowance > 0 {
        value["proof"]["expand_bodies"] = expansion(&query, refs, &manifest, allowance);
    }
    // A record the caller's ceiling cannot hold is never folded into the batch
    // above and never raises that ceiling behind its back. It is offered on
    // its own, priced exactly, so taking it is a decision and not a surprise.
    if let Some(oversized) = ceiling.and_then(|ceiling| {
        suffix
            .iter()
            .find(|slot| slot.pending_cost.is_some_and(|cost| cost > ceiling))
    }) {
        let cost = oversized.pending_cost.expect("oversized cost");
        value["proof"]["expand_oversized_body"] = expansion(
            &query,
            vec![oversized.reference.clone()],
            &manifest,
            cost,
        );
    }
}

#[cfg(test)]
#[path = "trace_body_actions_tests.rs"]
mod tests;
