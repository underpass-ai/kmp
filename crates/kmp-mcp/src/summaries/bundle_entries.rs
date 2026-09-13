//! One concept: turning an exported bundle into the history of every memory
//! entry it carries.
//!
//! This is the only place that knows the export's wire shape — `changes`,
//! `entity_kind`, `payload_json`. Everything downstream reads
//! [`EntryHistory`] values and never a `serde_json::Value`, which is what
//! lets the judgement and its weakness signals be exercised against
//! constructed histories rather than against JSON.

use std::collections::HashMap;

use serde_json::Value;

use super::about_lexicon::AboutLexicon;
use super::audit_scope::AuditScope;
use super::entry_history::EntryHistory;
use super::entry_revision::EntryRevision;

/// Every write of every memory entry the bundle carries, in the order the
/// store met them, narrowed to the abouts `scope` covers.
pub(crate) fn entry_histories(
    bundle: &str,
    scope: &AuditScope,
) -> Result<Vec<EntryHistory>, String> {
    let mut histories: Vec<EntryHistory> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (number, line) in bundle.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let event: Value = serde_json::from_str(line)
            .map_err(|error| format!("bundle line {} is not JSON: {error}", number + 1))?;
        let Some(root) = event.get("root_node_id").and_then(Value::as_str) else {
            continue;
        };
        if !scope.covers(root) {
            continue;
        }
        for change in event["changes"].as_array().into_iter().flatten() {
            if change["entity_kind"].as_str() != Some("memory_entry") {
                continue;
            }
            let Some(id) = change["entity_id"].as_str() else {
                continue;
            };
            let payload: Value = change["payload_json"]
                .as_str()
                .and_then(|raw| serde_json::from_str(raw).ok())
                .unwrap_or(Value::Null);
            let revision = EntryRevision::from_payload(&payload);
            match index.get(id) {
                Some(&at) => histories[at].record(revision),
                None => {
                    index.insert(id.to_string(), histories.len());
                    histories.push(EntryHistory::new(
                        root.to_string(),
                        id.to_string(),
                        revision,
                    ));
                }
            }
        }
    }
    Ok(histories)
}

/// The lexical field of each about, built from what its memories are as they
/// now stand.
pub(crate) fn about_lexicons(histories: &[EntryHistory]) -> HashMap<String, AboutLexicon> {
    let mut lexicons: HashMap<String, AboutLexicon> = HashMap::new();
    for history in histories {
        let latest = history.latest();
        lexicons
            .entry(history.about.clone())
            .or_default()
            .admit(&latest.text, latest.summary.as_deref());
    }
    lexicons
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(about: &str, id: &str, text: &str, summary: Option<&str>) -> String {
        let mut payload = serde_json::json!({"id": id, "kind": "decision", "text": text});
        if let Some(summary) = summary {
            payload["metadata"] = serde_json::json!({"summary_en": summary});
        }
        serde_json::json!({
            "root_node_id": about,
            "changes": [{
                "entity_kind": "memory_entry",
                "entity_id": id,
                "payload_json": payload.to_string()
            }]
        })
        .to_string()
    }

    #[test]
    fn every_write_of_an_entry_is_kept_in_the_order_the_store_met_them() {
        let bundle = [
            event("project:a", "project:a:e1", "primer texto", None),
            event(
                "project:a",
                "project:a:e1",
                "segundo texto",
                Some("a summary"),
            ),
            event("project:a", "project:a:e2", "otro hecho", None),
        ]
        .join("\n");

        let histories = entry_histories(&bundle, &AuditScope::AllAbouts).expect("parses");

        assert_eq!(histories.len(), 2);
        assert_eq!(histories[0].reference, "project:a:e1");
        assert_eq!(histories[0].latest().text, "segundo texto");
        assert_eq!(histories[1].reference, "project:a:e2");
    }

    #[test]
    fn a_scope_narrows_before_anything_is_decoded() {
        let bundle = [
            event("project:a", "project:a:e1", "uno", None),
            event("project:b", "project:b:e1", "dos", None),
        ]
        .join("\n");

        let histories =
            entry_histories(&bundle, &AuditScope::CurrentAbout("project:b".to_string()))
                .expect("parses");

        assert_eq!(histories.len(), 1);
        assert_eq!(histories[0].about, "project:b");
    }

    #[test]
    fn a_change_that_is_not_a_memory_entry_is_not_a_memory() {
        let bundle = serde_json::json!({
            "root_node_id": "project:a",
            "changes": [{
                "entity_kind": "memory_dimension",
                "entity_id": "label:v1:project%3Aa:work:work%3Amain",
                "payload_json": "{}"
            }]
        })
        .to_string();

        assert!(
            entry_histories(&bundle, &AuditScope::AllAbouts)
                .expect("parses")
                .is_empty()
        );
    }

    #[test]
    fn a_line_that_is_not_json_is_named() {
        let error = entry_histories("{\"root_node_id\":\"a\"}\nnot json", &AuditScope::AllAbouts)
            .expect_err("bad line");

        assert!(error.starts_with("bundle line 2 is not JSON"), "{error}");
    }

    #[test]
    fn each_about_gets_its_own_lexical_field_from_what_its_memories_now_are() {
        let bundle = [
            event("project:a", "project:a:e1", "primer texto", None),
            event("project:a", "project:a:e1", "la válvula se congeló", None),
            event("project:b", "project:b:e1", "el menú se publicó", None),
        ]
        .join("\n");
        let histories = entry_histories(&bundle, &AuditScope::AllAbouts).expect("parses");

        let lexicons = about_lexicons(&histories);

        assert_eq!(lexicons.len(), 2);
        assert_eq!(lexicons["project:a"].entries(), 1);
        // The superseded first text is not in the field: the latest write is
        // what a question would land on.
        assert_eq!(
            lexicons["project:a"].other_entries_reached("primer", false),
            0
        );
        assert_eq!(
            lexicons["project:a"].other_entries_reached("valvula", false),
            1
        );
    }
}
