//! A string key that two JSON values share exactly when they are equal, so a
//! projected page can be matched back to its originals through a hash map
//! instead of rendering and comparing every original for every item.

use std::fmt::Write;

use serde_json::Value;

/// Two values have the same key if and only if `left == right`.
///
/// Every token is self-delimiting (strings are length-prefixed, scalars end
/// in `,`), object keys are sorted whatever the map's iteration order, and
/// numbers keep serde_json's own equality: an integer never equals a float,
/// and `-0.0 == 0.0`.
pub(super) fn canonical_json_key(value: &Value) -> String {
    let mut key = String::new();
    push_value(&mut key, value);
    key
}

fn push_value(key: &mut String, value: &Value) {
    match value {
        Value::Null => key.push_str("n,"),
        Value::Bool(true) => key.push_str("t,"),
        Value::Bool(false) => key.push_str("f,"),
        Value::Number(number) => {
            if let Some(unsigned) = number.as_u64() {
                let _ = write!(key, "u{unsigned},");
            } else if let Some(signed) = number.as_i64() {
                let _ = write!(key, "i{signed},");
            } else {
                let float = number.as_f64().unwrap_or_default();
                let float = if float == 0.0 { 0.0 } else { float };
                let _ = write!(key, "d{float:?},");
            }
        }
        Value::String(text) => push_string(key, text),
        Value::Array(items) => {
            key.push('[');
            for item in items {
                push_value(key, item);
            }
            key.push(']');
        }
        Value::Object(map) => {
            let mut entries = map.iter().collect::<Vec<_>>();
            entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
            key.push('{');
            for (name, item) in entries {
                push_string(key, name);
                push_value(key, item);
            }
            key.push('}');
        }
    }
}

fn push_string(key: &mut String, text: &str) {
    let _ = write!(key, "s{}:", text.len());
    key.push_str(text);
}

#[cfg(test)]
#[path = "canonical_json_key_tests.rs"]
mod tests;
