//! Native, copyable actions for the bodies a bounded read did not deliver.
//!
//! A capability that reports a manifest, a deferred ref and an exact cost, and
//! then leaves the agent to rebuild the call, has not delivered the action —
//! it has delivered homework. Everything here is a complete `kmp_trace` call:
//! the same bound query, the ceiling the caller chose, the refs the kernel
//! says remain, and the manifest the response was computed from.
//!
//! Nothing here decides *which* refs remain. That was the defect: this layer
//! sees one page, and a page holding no pending ref offered nothing, so refs
//! on the other pages were never named by any action and following the whole
//! protocol still lost bodies. The kernel now plans the batch over the
//! complete proof table before pagination and hands it over in
//! `proof.expansion_plan`; this turns that plan into arguments and nothing
//! more.
//!
//! **These continue a suffix; they do not promise a whole selection.** A
//! consumer joins responses by ref against one manifest. The absence of an
//! action on a page is not the end of a selection, and an arbitrary named
//! subset proves nothing about the refs before it.
//!
//! What is deliberately not here: paging. A continuation walks one projection
//! of one selection; these ask for different bodies of the same selection.

use serde_json::{Map, Value, json};

/// The bound query, without what belongs to one delivery rather than to the
/// selection.
///
/// The page *size* stays: it is the caller's shape for the response, and
/// dropping it silently re-partitioned every expansion into the default. Only
/// the cursor goes, because a position in one projection means nothing in
/// another.
fn bound_query(arguments: &Value) -> Value {
    let mut query = arguments.clone();
    if let Some(search) = query.get_mut("search").and_then(Value::as_object_mut) {
        search.remove("proof_refs");
        search.remove("expect_selection");
    }
    if let Some(page) = query.get_mut("page").and_then(Value::as_object_mut) {
        page.remove("cursor");
        if page.is_empty() {
            query
                .as_object_mut()
                .expect("trace arguments")
                .remove("page");
        }
    }
    if let Some(object) = query.as_object_mut() {
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

fn expansion(query: &Value, refs: Vec<&str>, manifest: &str, allowance: u64) -> Value {
    let call = with_search(query, |search| {
        search.insert("proof_refs".into(), json!(refs));
        search.insert("expect_selection".into(), json!(manifest));
        search.insert("max_body_record_bytes".into(), json!(allowance));
    });
    json!({"tool": "kmp_trace", "arguments": call})
}

/// Attaches the kernel's plan as executable calls, and the fresh read to a
/// refused response.
pub(crate) fn attach(value: &mut Value, arguments: &Value) {
    if value.get("expansion_refusal").is_some() {
        // The only executable move left: read the selection again and take the
        // manifest it returns. Naming refs or an expectation of the selection
        // that was just refused would repeat the refusal.
        let fresh = bound_query(arguments);
        value["expansion_refusal"]["fresh_read"] = json!({"tool": "kmp_trace", "arguments": fresh});
        return;
    }
    let Some(manifest) = value
        .pointer("/proof/manifest_id")
        .and_then(Value::as_str)
        .filter(|manifest| !manifest.is_empty())
        .map(str::to_string)
    else {
        return;
    };
    let Some(plan) = value.pointer("/proof/expansion_plan").cloned() else {
        return;
    };
    let query = bound_query(arguments);

    let refs: Vec<&str> = plan["refs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let allowance = plan["record_bytes"].as_u64().unwrap_or_default();
    if !refs.is_empty() && allowance > 0 {
        value["proof"]["expand_bodies"] = expansion(&query, refs, &manifest, allowance);
    }
    // A record the caller's ceiling cannot hold is never folded into the batch
    // and never raises that ceiling behind its back. It is offered on its own,
    // priced exactly, so taking it is a decision and not a surprise.
    if let (Some(oversized), Some(bytes)) = (
        plan["oversized_ref"].as_str().filter(|r| !r.is_empty()),
        plan["oversized_record_bytes"].as_u64(),
    ) {
        value["proof"]["expand_oversized_body"] =
            expansion(&query, vec![oversized], &manifest, bytes);
    }
}

#[cfg(test)]
#[path = "trace_body_actions_tests.rs"]
mod tests;
