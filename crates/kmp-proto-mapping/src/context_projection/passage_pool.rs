//! Exact display-text sharing. Equal wording never merges source records.
use std::collections::BTreeMap;

use serde_json::{Value, json};

/// Share only typed prose slots in native memory packets. Metadata, references,
/// timestamps, cursors, actions and arbitrary raw JSON remain opaque.
pub(super) fn share(packets: &mut Value) -> BTreeMap<String, String> {
    let mut passages = BTreeMap::new();
    share_into(packets, &mut passages);
    passages
}

pub(super) fn share_into(packets: &mut Value, passages: &mut BTreeMap<String, String>) {
    let mut counts = BTreeMap::<String, usize>::new();
    visit(packets, &mut |slot| {
        if let Some(text) = slot.as_str() {
            *counts.entry(text.to_owned()).or_default() += 1;
        }
    });
    let mut replacements = BTreeMap::<String, String>::new();
    for (text, count) in counts {
        let existing = passages
            .iter()
            .find_map(|(id, literal)| (literal == &text).then(|| id.clone()));
        if count < 2 && existing.is_none() {
            continue;
        }
        let id = existing
            .clone()
            .unwrap_or_else(|| format!("p{}", passages.len() + 1));
        let literal_bytes = json!(&text).to_string().len();
        let use_bytes = json!({"passage": &id}).to_string().len();
        let entry_bytes = if existing.is_some() {
            0
        } else {
            json!({&id: &text}).to_string().len() + 1
        };
        // Include the table entry and comma. Never spend more to abbreviate.
        if count * literal_bytes > count * use_bytes + entry_bytes {
            replacements.insert(text.clone(), id.clone());
            passages.insert(id, text);
        }
    }
    visit(packets, &mut |slot| {
        if let Some(id) = slot.as_str().and_then(|text| replacements.get(text)) {
            *slot = json!({"passage": id});
        }
    });
}

pub(super) fn has_references(packets: &mut Value) -> bool {
    let mut found = false;
    visit(packets, &mut |slot| {
        found |= slot
            .as_object()
            .is_some_and(|object| object.contains_key("passage"));
    });
    found
}

/// The same typed slots make reconstruction unambiguous even when a source
/// happens to contain JSON or a literal string such as "p1".
pub(super) fn restore(
    packets: &mut Value,
    passages: &BTreeMap<String, String>,
) -> Result<(), String> {
    let mut failure = None;
    visit(packets, &mut |slot| {
        let Some(object) = slot.as_object() else {
            return;
        };
        if !object.contains_key("passage") {
            return;
        }
        if object.len() != 1 {
            failure = Some("invalid passage reference shape".into());
            return;
        }
        let ids = if let Some(id) = object["passage"].as_str() {
            Some(vec![id])
        } else {
            object["passage"]
                .as_array()
                .filter(|parts| !parts.is_empty())
                .and_then(|parts| parts.iter().map(Value::as_str).collect::<Option<Vec<_>>>())
        };
        let Some(ids) = ids else {
            failure = Some("invalid passage reference names".into());
            return;
        };
        let mut restored = String::new();
        for id in ids {
            let Some(text) = passages.get(id) else {
                failure = Some(format!("unresolved response-local passage {id:?}"));
                return;
            };
            restored.push_str(text);
        }
        *slot = json!(restored);
    });
    failure.map_or(Ok(()), Err)
}

fn visit(packets: &mut Value, f: &mut impl FnMut(&mut Value)) {
    for packet in packets.as_array_mut().into_iter().flatten() {
        if let Some(text) = packet.pointer_mut("/object/text") {
            f(text);
        }
        for (array, fields) in ARRAYS {
            for record in packet
                .pointer_mut(array)
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                for field in *fields {
                    if let Some(slot) = record.get_mut(*field) {
                        f(slot);
                    }
                }
            }
        }
    }
}

const ARRAYS: &[(&str, &[&str])] = &[
    ("/entries", &["text"]),
    ("/evidence", &["text"]),
    ("/proof/evidence", &["text"]),
    ("/proof/path", &["why", "evidence"]),
    ("/trace", &["why", "evidence"]),
    ("/facts", &["text", "why", "evidence"]),
    ("/declared", &["why", "evidence"]),
    ("/coordinate", &["why", "evidence"]),
    ("/tensions", &["why", "evidence"]),
    ("/proposed", &["why", "evidence"]),
    ("/links/incoming", &["why", "evidence"]),
    ("/links/outgoing", &["why", "evidence"]),
];

pub(super) fn is_prose_pointer(pointer: &str) -> bool {
    if pointer == "/object/text" {
        return true;
    }
    let Some((prefix, field)) = pointer.rsplit_once('/') else {
        return false;
    };
    let Some((array, index)) = prefix.rsplit_once('/') else {
        return false;
    };
    index.parse::<usize>().is_ok_and(|i| i.to_string() == index)
        && ARRAYS
            .iter()
            .any(|(path, fields)| *path == array && fields.contains(&field))
}
