//! Writing one nested argument of a call the server proposes back (a page
//! cursor, a byte allowance) without trusting the caller's shapes.
//!
//! `arguments["page"]["cursor"] = …` panics when the caller sent `"page": 1`,
//! and a panic ended the stdio server for the whole session (B2 of the 1.0
//! review). The boundary now refuses such a call before it gets here; this is
//! the second line: a slot that is not an object becomes one.

use serde_json::{Map, Value};

/// Sets `arguments[outer][inner] = value`, making `arguments` and
/// `arguments[outer]` objects first when they are anything else.
pub(crate) fn set_nested(arguments: &mut Value, outer: &str, inner: &str, value: Value) {
    let root = object_at(arguments);
    let slot = root
        .entry(outer.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    object_at(slot).insert(inner.to_string(), value);
}

fn object_at(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    match value {
        Value::Object(object) => object,
        _ => unreachable!("replaced by an object above"),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_slot_of_another_type_becomes_an_object() {
        for page in [json!(1), json!("c"), json!([1]), Value::Null] {
            let mut arguments = json!({"about": "a", "page": page});
            set_nested(&mut arguments, "page", "cursor", json!("x"));
            assert_eq!(arguments, json!({"about": "a", "page": {"cursor": "x"}}));
        }
        let mut arguments = json!({"page": {"entries": 2}});
        set_nested(&mut arguments, "page", "cursor", json!("x"));
        assert_eq!(arguments, json!({"page": {"entries": 2, "cursor": "x"}}));
        let mut arguments = json!(7);
        set_nested(&mut arguments, "budget", "max_bytes", json!(512));
        assert_eq!(arguments, json!({"budget": {"max_bytes": 512}}));
    }
}
