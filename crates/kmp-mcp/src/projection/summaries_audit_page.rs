//! One concept: summaries-audit pagination.
//!
//! The totals and the per-about counts are the stable core; the memories page
//! beneath them under the byte ceiling the caller named, behind a cursor
//! bound to the selection. No reader state is stored, and the cursor's hash
//! covers the bound arguments and every memory in the reading, so a store
//! that moved rejects a continuation rather than paging a different reading
//! as if it were the same one.
//!
//! This runs strictly *after* `summaries_audit_projection` has mapped the
//! reading, and reaches into that answer by key.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::serialized_size::serialized_len;
use super::summaries_audit_projection::{audited_summary_json, core_response};
use crate::serving::ToolError;
use crate::summaries::{AuditScope, SummaryAudit, SummaryState};

const AUDIT_CURSOR_VERSION: &str = "kmpsa1";
const DEFAULT_MAX_BYTES: usize = 10_000;
const MINIMUM_MAX_BYTES: usize = 512;

/// Renders one audit against the arguments that asked for it.
pub(crate) fn summaries_audit_page(
    audit: &SummaryAudit,
    scope: &AuditScope,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let limit = requested_byte_limit(arguments)?;
    let states = requested_states(arguments)?;
    let items = audit
        .entries()
        .iter()
        .filter(|entry| {
            states
                .as_ref()
                .is_none_or(|states| states.contains(&entry.state))
        })
        .map(|entry| {
            let value = audited_summary_json(entry);
            let bytes = serialized_len(&value);
            (value, bytes)
        })
        .collect::<Vec<_>>();

    let mut core = core_response(audit, scope);
    let selection_hash = selection_hash(&core, &items, arguments);
    let offset = page_offset(arguments, &selection_hash, items.len())?;

    let entries_cap = arguments
        .pointer("/page/entries")
        .and_then(Value::as_u64)
        .map(|entries| entries as usize)
        .unwrap_or(usize::MAX);
    let core_bytes = serialized_len(&core);
    let mut used = core_bytes;
    let mut page = Vec::new();
    let mut overran = None;
    for (value, bytes) in items.iter().skip(offset) {
        if page.len() >= entries_cap {
            break;
        }
        if used + bytes > limit {
            if page.is_empty() {
                // A memory larger than the whole ceiling still has to be
                // readable, or the continuation stalls at it forever.
                overran = Some(core_bytes + bytes);
                page.push(value.clone());
            }
            break;
        }
        used += bytes;
        page.push(value.clone());
    }

    let returned = page.len();
    let read = offset + returned;
    let has_more = read < items.len();
    let next_cursor = has_more.then(|| format!("{AUDIT_CURSOR_VERSION}:{read}:{selection_hash}"));
    core["entries"] = Value::Array(page);
    let mut page_report = json!({
        "offset": offset,
        "returned": returned,
        "total": items.len(),
        "has_more": has_more,
        "next_cursor": next_cursor
    });
    if let Some(required_bytes) = overran {
        page_report["required_bytes"] = json!(required_bytes);
        core["warnings"] = json!([format!(
            "one memory needed {required_bytes} bytes, more than the {limit}-byte ceiling this \
             call named; it was returned anyway so the reading can advance"
        )]);
    }
    core["page"] = page_report;
    core["next_actions"] = match &core["page"]["next_cursor"] {
        Value::String(cursor) => json!([{
            "tool": "kmp_summaries_audit",
            "arguments": continuation_arguments(arguments, cursor)
        }]),
        _ => json!([]),
    };
    Ok(core)
}

/// The states a caller kept, or every state when it named none.
fn requested_states(arguments: &Value) -> Result<Option<Vec<SummaryState>>, ToolError> {
    let Some(states) = arguments.get("states") else {
        return Ok(None);
    };
    let Some(states) = states.as_array() else {
        return Err(ToolError::invalid_argument(
            "states is an array of summary states",
        ));
    };
    states
        .iter()
        .map(|state| {
            state.as_str().and_then(SummaryState::parse).ok_or_else(|| {
                ToolError::invalid_argument(format!(
                    "`{state}` is not a summary state; they are {}",
                    SummaryState::ALL.map(SummaryState::as_str).join(", ")
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn requested_byte_limit(arguments: &Value) -> Result<usize, ToolError> {
    let Some(requested) = arguments.pointer("/budget/max_bytes") else {
        return Ok(DEFAULT_MAX_BYTES);
    };
    let requested = requested
        .as_u64()
        .filter(|bytes| *bytes >= MINIMUM_MAX_BYTES as u64)
        .ok_or_else(|| {
            ToolError::invalid_argument(format!(
                "budget.max_bytes is an integer of at least {MINIMUM_MAX_BYTES}"
            ))
        })?;
    Ok(requested as usize)
}

/// Binds the cursor to the arguments that selected this reading and to every
/// memory in it.
fn selection_hash(core: &Value, items: &[(Value, usize)], arguments: &Value) -> String {
    let mut bound = arguments.clone();
    if let Some(bound) = bound.as_object_mut() {
        bound.remove("page");
        let empty_budget = bound
            .get_mut("budget")
            .and_then(Value::as_object_mut)
            .is_some_and(|budget| {
                budget.remove("max_bytes");
                budget.is_empty()
            });
        if empty_budget {
            bound.remove("budget");
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(AUDIT_CURSOR_VERSION.as_bytes());
    hasher.update(b"\0");
    hasher.update(serde_json::to_vec(&bound).expect("audit arguments serialize"));
    hasher.update(b"\0");
    hasher.update(serde_json::to_vec(&core["totals"]).expect("audit totals serialize"));
    for (value, _) in items {
        hasher.update(b"\0");
        hasher.update(serde_json::to_vec(value).expect("audited memory serializes"));
    }
    format!("{:x}", hasher.finalize())
}

fn page_offset(arguments: &Value, selection_hash: &str, total: usize) -> Result<usize, ToolError> {
    let Some(cursor) = arguments.pointer("/page/cursor") else {
        return Ok(0);
    };
    let malformed = || {
        ToolError::invalid_argument("invalid page.cursor: malformed summaries-audit continuation")
    };
    let cursor = cursor
        .as_str()
        .filter(|cursor| !cursor.is_empty())
        .ok_or_else(malformed)?;
    let mut parts = cursor.split(':');
    if parts.next() != Some(AUDIT_CURSOR_VERSION) {
        return Err(malformed());
    }
    let offset = parts
        .next()
        .and_then(|offset| offset.parse::<usize>().ok())
        .ok_or_else(malformed)?;
    let hash = parts.next().ok_or_else(malformed)?;
    if parts.next().is_some() {
        return Err(malformed());
    }
    if hash != selection_hash {
        return Err(ToolError::invalid_argument(
            "the store's summaries moved since this cursor was issued; read the audit again \
             from its first page",
        ));
    }
    if offset > total {
        return Err(malformed());
    }
    Ok(offset)
}

/// The complete call that continues this reading: every bound argument as it
/// was, with the cursor set.
fn continuation_arguments(arguments: &Value, cursor: &str) -> Value {
    let mut next = arguments.clone();
    if let Some(next) = next.as_object_mut() {
        let entries = next
            .get("page")
            .and_then(|page| page.get("entries"))
            .cloned();
        let mut page = json!({"cursor": cursor});
        if let Some(entries) = entries {
            page["entries"] = entries;
        }
        next.insert("page".to_string(), page);
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(about: &str, id: &str, text: &str, summary: Option<&str>) -> String {
        let mut payload = json!({"id": id, "kind": "decision", "text": text});
        if let Some(summary) = summary {
            payload["metadata"] = json!({"summary_en": summary, "summary_en_by": "agent:a"});
        }
        json!({
            "root_node_id": about,
            "changes": [{
                "entity_kind": "memory_entry",
                "entity_id": id,
                "payload_json": payload.to_string()
            }]
        })
        .to_string()
    }

    fn bundle(memories: usize) -> String {
        (1..=memories)
            .map(|number| {
                event(
                    "project:a",
                    &format!("project:a:e{number}"),
                    &format!("El punto {number} del despliegue quedó anotado sin resolver."),
                    None,
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn audit(bundle: &str) -> SummaryAudit {
        SummaryAudit::read(bundle, &AuditScope::CurrentAbout("project:a".to_string()))
            .expect("bundle parses")
    }

    fn render(bundle: &str, arguments: Value) -> Value {
        summaries_audit_page(
            &audit(bundle),
            &AuditScope::CurrentAbout("project:a".to_string()),
            &arguments,
        )
        .expect("the audit renders")
    }

    #[test]
    fn the_totals_describe_the_selection_and_the_entries_describe_the_page() {
        let rendered = render(
            &bundle(6),
            json!({"about": "project:a", "page": {"entries": 2}}),
        );

        assert_eq!(rendered["totals"]["entries"], 6);
        assert_eq!(rendered["entries"].as_array().expect("entries").len(), 2);
        assert_eq!(rendered["page"]["returned"], 2);
        assert_eq!(rendered["page"]["total"], 6);
        assert_eq!(rendered["page"]["has_more"], true);
        assert_eq!(rendered["next_actions"][0]["tool"], "kmp_summaries_audit");
    }

    #[test]
    fn every_page_together_reconstructs_the_whole_reading_and_the_last_one_stops() {
        let bundle = bundle(7);
        let mut arguments = json!({"about": "project:a", "page": {"entries": 3}});
        let mut read = Vec::new();
        loop {
            let rendered = render(&bundle, arguments.clone());
            for entry in rendered["entries"].as_array().expect("entries") {
                read.push(entry["ref"].as_str().expect("ref").to_string());
            }
            match rendered["next_actions"]
                .as_array()
                .expect("actions")
                .first()
            {
                None => {
                    assert_eq!(rendered["page"]["has_more"], false);
                    assert!(rendered["page"]["next_cursor"].is_null());
                    break;
                }
                Some(action) => arguments = action["arguments"].clone(),
            }
        }

        assert_eq!(read.len(), 7);
        assert_eq!(read[0], "project:a:e1");
        assert_eq!(read[6], "project:a:e7");
    }

    #[test]
    fn a_cursor_from_a_different_reading_is_refused_rather_than_paged() {
        let first = render(
            &bundle(4),
            json!({"about": "project:a", "page": {"entries": 1}}),
        );
        let cursor = first["page"]["next_cursor"]
            .as_str()
            .expect("a continuation")
            .to_string();

        let error = summaries_audit_page(
            &audit(&bundle(5)),
            &AuditScope::CurrentAbout("project:a".to_string()),
            &json!({"about": "project:a", "page": {"entries": 1, "cursor": cursor}}),
        )
        .expect_err("the reading moved");

        assert!(error.message.contains("read the audit again"), "{error:?}");
    }

    #[test]
    fn a_malformed_cursor_is_refused_by_shape_before_it_is_trusted() {
        for cursor in ["", "kmpsa1", "kmpsa1:x:abc", "other:0:abc", "kmpsa1:0:a:b"] {
            let error = summaries_audit_page(
                &audit(&bundle(2)),
                &AuditScope::CurrentAbout("project:a".to_string()),
                &json!({"about": "project:a", "page": {"cursor": cursor}}),
            )
            .expect_err("a malformed cursor pages nothing");
            assert!(error.message.contains("malformed"), "{cursor}: {error:?}");
        }
    }

    #[test]
    fn a_states_filter_narrows_the_page_and_leaves_the_totals_alone() {
        let bundle = [
            event(
                "project:a",
                "project:a:e1",
                "El despliegue se retrasó.",
                None,
            ),
            event(
                "project:a",
                "project:a:e2",
                "The rollout slipped because the auditors had not signed off.",
                None,
            ),
        ]
        .join("\n");

        let rendered = render(
            &bundle,
            json!({"about": "project:a", "states": ["missing"]}),
        );

        assert_eq!(rendered["totals"]["entries"], 2);
        assert_eq!(rendered["totals"]["not_required"], 1);
        assert_eq!(rendered["entries"].as_array().expect("entries").len(), 1);
        assert_eq!(rendered["entries"][0]["state"], "missing");
        assert_eq!(rendered["page"]["total"], 1);
    }

    #[test]
    fn an_unknown_state_is_refused_with_the_vocabulary_named() {
        let error = summaries_audit_page(
            &audit(&bundle(1)),
            &AuditScope::CurrentAbout("project:a".to_string()),
            &json!({"about": "project:a", "states": ["weak"]}),
        )
        .expect_err("weak is not a state");

        assert!(error.message.contains("not_required"), "{error:?}");
    }

    #[test]
    fn a_byte_ceiling_below_the_published_floor_is_refused() {
        let error = summaries_audit_page(
            &audit(&bundle(1)),
            &AuditScope::CurrentAbout("project:a".to_string()),
            &json!({"about": "project:a", "budget": {"max_bytes": 8}}),
        )
        .expect_err("a ceiling nothing fits under");

        assert!(error.message.contains("at least 512"), "{error:?}");
    }

    /// A memory that cannot fit the ceiling is still returned, because the
    /// alternative is a continuation that can never advance past it.
    #[test]
    fn one_memory_larger_than_the_ceiling_is_returned_with_a_warning() {
        let long = "El comité revisó la migración ".repeat(60);
        let bundle = event("project:a", "project:a:e1", &long, None);

        let rendered = render(
            &bundle,
            json!({"about": "project:a", "budget": {"max_bytes": 512}}),
        );

        assert_eq!(rendered["entries"].as_array().expect("entries").len(), 1);
        assert!(rendered["page"]["required_bytes"].as_u64().expect("bytes") > 512);
        assert!(
            rendered["warnings"][0]
                .as_str()
                .expect("a warning")
                .contains("returned anyway")
        );
    }

    #[test]
    fn an_empty_selection_pages_nothing_and_offers_no_continuation() {
        let rendered = render("", json!({"about": "project:a"}));

        assert_eq!(rendered["page"]["total"], 0);
        assert_eq!(rendered["page"]["has_more"], false);
        assert_eq!(rendered["next_actions"], json!([]));
    }
}
