use serde_json::{Value, json};

/// Optional new selection, separate from mandatory page continuations.
pub(super) fn attach(value: &mut Value, arguments: &Value) {
    let Some(material) = value
        .pointer_mut("/search/material")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let count = material
        .get("candidate_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let selected = material
        .get("selected_candidates")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if count <= selected as u64 {
        return;
    }
    let mut expand = arguments.clone();
    if let Some(search) = expand.get_mut("search").and_then(Value::as_object_mut) {
        search.remove("select");
    }
    if let Some(page) = expand.get_mut("page").and_then(Value::as_object_mut) {
        page.remove("cursor");
    }
    material.insert(
        "expand_candidates".into(),
        json!({"tool":"kmp_trace","arguments":expand}),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trace_material_expansion_preserves_selection_except_material_and_old_cursor() {
        let mut value = json!({"search":{"material":{"candidate_count":2,"selected_candidates":[]}},"next_actions":[]});
        let args = json!({"about":"p","from":"s","to":["t"],"axis":"observed","as_of":{"time":"2026-09-01T00:00:00Z"},
            "search":{"paths_per_target":2,"max_nodes":20,"select":{"max_material_nodes":1}},
            "page":{"entries":1,"cursor":"old"},"budget":{"max_bytes":2000}});
        attach(&mut value, &args);
        assert_eq!(value["next_actions"], json!([]));
        let action = &value["search"]["material"]["expand_candidates"];
        assert_eq!(action["tool"], "kmp_trace");
        let mut expected = args;
        expected["search"]
            .as_object_mut()
            .expect("search object")
            .remove("select");
        expected["page"]
            .as_object_mut()
            .expect("page object")
            .remove("cursor");
        assert_eq!(action["arguments"], expected);
    }
}
