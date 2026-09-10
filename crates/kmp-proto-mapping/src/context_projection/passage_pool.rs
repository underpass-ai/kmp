//! Exact display-text sharing. Equal wording never merges source records.
use std::collections::BTreeMap;

use serde_json::{Value, json};

/// Share only typed prose slots in native memory packets. Metadata, references,
/// timestamps, cursors, actions and arbitrary raw JSON remain opaque.
pub(super) fn share(packets: &mut Value) -> BTreeMap<String, String> {
    let mut counts = BTreeMap::<String, usize>::new();
    visit(packets, &mut |slot| {
        if let Some(text) = slot.as_str() {
            *counts.entry(text.to_owned()).or_default() += 1;
        }
    });
    let mut replacements = BTreeMap::<String, String>::new();
    let mut passages = BTreeMap::new();
    for (text, count) in counts {
        if count < 2 {
            continue;
        }
        let id = format!("p{}", passages.len() + 1);
        let literal_bytes = json!(&text).to_string().len();
        let use_bytes = json!({"passage": &id}).to_string().len();
        let entry_bytes = json!({&id: &text}).to_string().len();
        // Include the table entry and comma. Never spend more to abbreviate.
        if count * literal_bytes > count * use_bytes + entry_bytes + 1 {
            replacements.insert(text.clone(), id.clone());
            passages.insert(id, text);
        }
    }
    visit(packets, &mut |slot| {
        if let Some(id) = slot.as_str().and_then(|text| replacements.get(text)) {
            *slot = json!({"passage": id});
        }
    });
    passages
}

pub(super) fn has_references(packets: &mut Value) -> bool {
    let mut found = false;
    visit(packets, &mut |slot| {
        found |= slot
            .as_object()
            .is_some_and(|object| object.len() == 1 && object.contains_key("passage"));
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
        if object.len() != 1 || !object.contains_key("passage") {
            return;
        }
        let id = object["passage"].as_str().unwrap_or_default();
        if let Some(text) = passages.get(id) {
            *slot = json!(text);
        } else {
            failure = Some(format!("unresolved response-local passage {id:?}"));
        }
    });
    failure.map_or(Ok(()), Err)
}

fn visit(packets: &mut Value, f: &mut impl FnMut(&mut Value)) {
    for packet in packets.as_array_mut().into_iter().flatten() {
        if let Some(text) = packet.pointer_mut("/object/text") {
            f(text);
        }
        for (array, fields) in [
            ("/entries", &["text"][..]),
            ("/evidence", &["text"][..]),
            ("/proof/evidence", &["text"][..]),
            ("/proof/path", &["why", "evidence"][..]),
            ("/trace", &["why", "evidence"][..]),
            ("/facts", &["text", "why", "evidence"][..]),
            ("/declared", &["why", "evidence"][..]),
            ("/coordinate", &["why", "evidence"][..]),
            ("/tensions", &["why", "evidence"][..]),
            ("/proposed", &["why", "evidence"][..]),
            ("/links/incoming", &["why", "evidence"][..]),
            ("/links/outgoing", &["why", "evidence"][..]),
        ] {
            for record in packet
                .pointer_mut(array)
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                for field in fields {
                    if let Some(slot) = record.get_mut(*field) {
                        f(slot);
                    }
                }
            }
        }
    }
}
