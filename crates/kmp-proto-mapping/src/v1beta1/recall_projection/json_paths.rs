//! Reading and writing the array sections of a recall packet by path, and
//! recognising the keys that name a reference rather than prose.

use serde_json::Value;

pub(super) fn array_len(value: &Value, path: &[&str]) -> usize {
    let mut current = value;
    for key in path {
        let Some(next) = current.get(*key) else {
            return 0;
        };
        current = next;
    }
    current.as_array().map(Vec::len).unwrap_or(0)
}

pub(super) fn array_at_mut<'a>(value: &'a mut Value, path: &[&str]) -> Option<&'a mut Vec<Value>> {
    let mut current = value;
    for key in path {
        current = current.get_mut(*key)?;
    }
    current.as_array_mut()
}

pub(super) fn take_array(value: &mut Value, path: &[&str]) -> Vec<Value> {
    array_at_mut(value, path)
        .map(std::mem::take)
        .unwrap_or_default()
}

pub(super) fn push_array(value: &mut Value, path: &[&str], item: Value) {
    if let Some(array) = array_at_mut(value, path) {
        array.push(item);
    }
}

pub(super) fn is_reference_key(key: &str) -> bool {
    matches!(
        key,
        "id" | "ref"
            | "claim"
            | "supports"
            | "source_ref"
            | "target_ref"
            | "evidence_refs"
            | "superseded_by"
    ) || key.ends_with("_ref")
        || key.ends_with("_refs")
}
