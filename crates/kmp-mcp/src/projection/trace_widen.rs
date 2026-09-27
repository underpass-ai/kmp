use serde_json::{Value, json};

/// The largest allowance a bounded target search accepts
/// (`TraceSearchLimits::validate`).
const WIDEST: [(&str, u32); 4] = [
    ("max_nodes", 4096),
    ("max_edges", 32768),
    ("max_depth", 1024),
    ("max_states", 32768),
];

/// A single-destination trace whose bidirectional search stopped on a work
/// limit (`search.direction: "bidirectional"`) gets `search.widen`: the same
/// destination as a bounded target search with the largest allowance. It is
/// an optional new selection with its own cost, never a page continuation,
/// and it walks outgoing, source-backed, non-structural links only.
pub(super) fn attach(value: &mut Value, arguments: &Value) {
    let Some(search) = value.get_mut("search").and_then(Value::as_object_mut) else {
        return;
    };
    if search.get("direction").and_then(Value::as_str) != Some("bidirectional") {
        return;
    }
    let Some(to) = arguments.get("to").and_then(Value::as_str) else {
        return;
    };
    let mut widen = arguments.clone();
    widen["to"] = json!([to]);
    widen["search"] = Value::Object(
        WIDEST
            .iter()
            .map(|(key, limit)| ((*key).to_string(), json!(limit)))
            .collect(),
    );
    if let Some(page) = widen.get_mut("page").and_then(Value::as_object_mut) {
        page.remove("cursor");
    }
    search.insert(
        "widen".into(),
        json!({"tool": "kmp_trace", "arguments": widen}),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_partial_single_destination_trace_offers_the_widest_bounded_search() {
        let mut value =
            json!({"search": {"direction": "bidirectional", "stop_reason": "node_budget"}});
        let arguments =
            json!({"about": "p", "from": "a", "to": "b", "page": {"cursor": "old", "entries": 4}});
        attach(&mut value, &arguments);
        let widen = &value["search"]["widen"];
        assert_eq!(widen["tool"], "kmp_trace");
        assert_eq!(widen["arguments"]["to"], json!(["b"]));
        assert_eq!(widen["arguments"]["search"]["max_nodes"], 4096);
        assert_eq!(widen["arguments"]["page"], json!({"entries": 4}));
        assert_eq!(widen["arguments"]["about"], "p");
    }

    #[test]
    fn other_searches_are_left_alone() {
        let mut bounded = json!({"search": {"direction": "outgoing"}});
        attach(&mut bounded, &json!({"to": ["b"]}));
        assert!(bounded["search"].get("widen").is_none());
        let mut plain = json!({"trace": []});
        attach(&mut plain, &json!({"to": "b"}));
        assert!(plain.get("search").is_none());
    }
}
