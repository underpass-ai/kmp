use serde_json::{Value, json};

/// The largest allowance a trace search accepts, as `search` fields.
fn widest() -> [(&'static str, u32); 4] {
    let widest = kmp_domain::TraceSearchLimits::widest();
    [
        ("max_nodes", widest.nodes),
        ("max_edges", widest.edges),
        ("max_depth", widest.depth),
        ("max_states", widest.states),
    ]
}

/// A single-destination trace whose bidirectional search stopped on a work
/// limit (`search.direction: "bidirectional"`) gets `search.widen`: the same
/// trace of the same destination — bidirectional over every stored relation,
/// without filtering by why or evidence — under the largest allowance. It is
/// an optional new selection with its own cost, never a page continuation.
/// A call that already had the largest allowance is not offered it again.
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
    if is_widest(arguments) {
        return;
    }
    let mut widen = arguments.clone();
    widen["to"] = json!(to);
    let mut allowance =
        serde_json::Map::from_iter([("direction".to_string(), json!("bidirectional"))]);
    allowance.extend(
        widest()
            .into_iter()
            .map(|(key, limit)| (key.to_string(), json!(limit))),
    );
    widen["search"] = Value::Object(allowance);
    if let Some(page) = widen.get_mut("page").and_then(Value::as_object_mut) {
        page.remove("cursor");
    }
    search.insert(
        "widen".into(),
        json!({"tool": "kmp_trace", "arguments": widen}),
    );
}

fn is_widest(arguments: &Value) -> bool {
    widest().into_iter().all(|(key, limit)| {
        arguments
            .pointer(&format!("/search/{key}"))
            .and_then(Value::as_u64)
            == Some(u64::from(limit))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_partial_single_destination_trace_offers_the_same_trace_with_the_widest_allowance() {
        let mut value =
            json!({"search": {"direction": "bidirectional", "stop_reason": "node_budget"}});
        let arguments =
            json!({"about": "p", "from": "a", "to": "b", "page": {"cursor": "old", "entries": 4}});
        attach(&mut value, &arguments);
        let widen = &value["search"]["widen"];
        assert_eq!(widen["tool"], "kmp_trace");
        assert_eq!(widen["arguments"]["to"], json!("b"));
        assert_eq!(
            widen["arguments"]["search"],
            json!({"direction": "bidirectional", "max_nodes": 4096, "max_edges": 32768,
                   "max_depth": 1024, "max_states": 32768})
        );
        assert_eq!(widen["arguments"]["page"], json!({"entries": 4}));
        assert_eq!(widen["arguments"]["about"], "p");
    }

    #[test]
    fn a_widened_trace_that_stops_again_is_not_offered_the_same_allowance() {
        let mut value =
            json!({"search": {"direction": "bidirectional", "stop_reason": "node_budget"}});
        let mut first = value.clone();
        attach(&mut first, &json!({"about": "p", "from": "a", "to": "b"}));
        let widened = first["search"]["widen"]["arguments"].clone();
        attach(&mut value, &widened);
        assert!(value["search"].get("widen").is_none());
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
