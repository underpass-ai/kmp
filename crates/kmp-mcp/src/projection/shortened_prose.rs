//! Shortens the prose of one Trace/Relate item so a page whose next item is
//! larger than `budget.max_bytes` still returns it and advances, instead of
//! returning nothing: its refs, relation, class and clocks stay whole, and
//! only the fields people write in are cut, each marked with an ellipsis.
use serde_json::Value;

/// The fields a writer fills with prose. Everything else is an address, a
/// type or a clock and is never cut.
const PROSE: &[&str] = &[
    "why",
    "evidence",
    "motivation",
    "method",
    "text",
    "summary",
    "rationale",
    "chosen_because",
    "excerpt",
    "body",
];

/// Cuts every prose string of `value` longer than `max_chars` to that many
/// characters and an ellipsis; returns how many characters it removed.
pub(super) fn shorten_prose(value: &mut Value, max_chars: usize) -> usize {
    match value {
        Value::Array(items) => items
            .iter_mut()
            .map(|item| shorten_prose(item, max_chars))
            .sum(),
        Value::Object(object) => object
            .iter_mut()
            .map(|(key, value)| match value {
                Value::String(text) if PROSE.contains(&key.as_str()) => {
                    let total = text.chars().count();
                    if total <= max_chars {
                        0
                    } else {
                        let mut kept = text.chars().take(max_chars).collect::<String>();
                        kept.push('…');
                        *text = kept;
                        total - max_chars
                    }
                }
                other => shorten_prose(other, max_chars),
            })
            .sum(),
        _ => 0,
    }
}

/// The longest prose string of `value`, in characters.
pub(super) fn longest_prose(value: &Value) -> usize {
    match value {
        Value::Array(items) => items.iter().map(longest_prose).max().unwrap_or(0),
        Value::Object(object) => object
            .iter()
            .map(|(key, value)| match value {
                Value::String(text) if PROSE.contains(&key.as_str()) => text.chars().count(),
                other => longest_prose(other),
            })
            .max()
            .unwrap_or(0),
        _ => 0,
    }
}
