//! Short graph-reference uses; canonical record definitions remain explicit.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn share(packets: &mut Value) -> BTreeMap<String, String> {
    let mut definitions = BTreeSet::new();
    for packet in packets.as_array().into_iter().flatten() {
        if let Some(reference) = packet.pointer("/object/ref").and_then(Value::as_str) {
            definitions.insert(reference.to_owned());
        }
        for (path, key) in [
            ("/entries", "ref"),
            ("/facts", "ref"),
            ("/evidence", "id"),
            ("/proof/evidence", "id"),
        ] {
            for record in packet
                .pointer(path)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(reference) = record[key].as_str() {
                    definitions.insert(reference.to_owned());
                }
            }
        }
    }
    let mut counts = BTreeMap::<String, usize>::new();
    visit(packets, &mut |slot| {
        if let Some(reference) = slot.as_str() {
            *counts.entry(reference.into()).or_default() += 1;
        }
    });
    let mut replacements = BTreeMap::new();
    let mut citations = BTreeMap::new();
    for (reference, count) in counts {
        if count < 2 || !definitions.contains(&reference) {
            continue;
        }
        let id = format!("c{}", citations.len() + 1);
        let literal = json!(&reference).to_string().len();
        let usage = json!({"citation":&id}).to_string().len();
        let entry = json!({&id:&reference}).to_string().len();
        if count * literal > count * usage + entry + 1 {
            replacements.insert(reference.clone(), id.clone());
            citations.insert(id, reference);
        }
    }
    visit(packets, &mut |slot| {
        if let Some(id) = slot
            .as_str()
            .and_then(|reference| replacements.get(reference))
        {
            *slot = json!({"citation":id});
        }
    });
    citations
}

pub(super) fn has_references(packets: &mut Value) -> bool {
    let mut found = false;
    visit(packets, &mut |slot| {
        found |= slot
            .as_object()
            .is_some_and(|object| object.len() == 1 && object.contains_key("citation"));
    });
    found
}

pub(super) fn restore(
    packets: &mut Value,
    citations: &BTreeMap<String, String>,
) -> Result<(), String> {
    let mut failure = None;
    visit(packets, &mut |slot| {
        let Some(object) = slot.as_object() else {
            return;
        };
        if object.len() != 1 || !object.contains_key("citation") {
            return;
        }
        let id = object["citation"].as_str().unwrap_or_default();
        if let Some(reference) = citations.get(id) {
            *slot = json!(reference);
        } else {
            failure = Some(format!("unresolved response-local citation {id:?}"));
        }
    });
    failure.map_or(Ok(()), Err)
}

fn visit(packets: &mut Value, f: &mut impl FnMut(&mut Value)) {
    for packet in packets.as_array_mut().into_iter().flatten() {
        for path in ["/evidence", "/proof/evidence"] {
            for evidence in packet
                .pointer_mut(path)
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                for reference in evidence
                    .get_mut("supports")
                    .and_then(Value::as_array_mut)
                    .into_iter()
                    .flatten()
                {
                    f(reference);
                }
            }
        }
        for path in [
            "/proof/path",
            "/trace",
            "/links/incoming",
            "/links/outgoing",
            "/declared",
            "/coordinate",
            "/tensions",
            "/proposed",
        ] {
            for relation in packet
                .pointer_mut(path)
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                for key in ["from", "to", "from_ref", "to_ref", "ref", "other"] {
                    if let Some(reference) = relation.get_mut(key) {
                        f(reference);
                    }
                }
                for reference in relation
                    .get_mut("evidence_refs")
                    .and_then(Value::as_array_mut)
                    .into_iter()
                    .flatten()
                {
                    f(reference);
                }
            }
        }
    }
}
