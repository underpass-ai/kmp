//! The checker covers the catalogue: every keyword any input schema uses is
//! one `schema_law` implements, and every `pattern` compiles. A schema change
//! that reaches for a new keyword fails here instead of being silently
//! unenforced.
#![cfg(test)]

use serde_json::{Value, json};

use super::registry::{advertised_input_schemas, tools_list_result, tools_list_result_with_apps};
use super::schema_law::{KEYWORDS, check, matches_pattern};

/// Every schema node below `schema`, with its path.
fn schema_nodes<'a>(schema: &'a Value, path: String, out: &mut Vec<(String, &'a Value)>) {
    let Some(object) = schema.as_object() else {
        return;
    };
    out.push((path.clone(), schema));
    for (key, nested) in object {
        match key.as_str() {
            "properties" => {
                for (name, property) in nested.as_object().into_iter().flatten() {
                    schema_nodes(property, format!("{path}.{name}"), out);
                }
            }
            "additionalProperties" | "items" | "propertyNames" | "not" | "if" | "then" | "else" => {
                schema_nodes(nested, format!("{path}/{key}"), out)
            }
            "oneOf" | "anyOf" | "allOf" => {
                for (index, branch) in nested.as_array().into_iter().flatten().enumerate() {
                    schema_nodes(branch, format!("{path}/{key}/{index}"), out);
                }
            }
            _ => {}
        }
    }
}

fn every_input_schema() -> Vec<(String, Value)> {
    let full = tools_list_result_with_apps(true);
    let advertised = advertised_input_schemas(tools_list_result_with_apps(true));
    [full, advertised, tools_list_result()]
        .iter()
        .flat_map(|list| list["tools"].as_array().cloned().unwrap_or_default())
        .map(|tool| {
            (
                tool["name"].as_str().unwrap_or_default().to_string(),
                tool["inputSchema"].clone(),
            )
        })
        .collect()
}

#[test]
fn every_keyword_the_catalogue_uses_is_enforced() {
    let mut unknown = Vec::new();
    for (tool, schema) in every_input_schema() {
        let mut nodes = Vec::new();
        schema_nodes(&schema, tool.clone(), &mut nodes);
        assert!(
            nodes.len() > 1,
            "{tool}: the walk found no schema below the root"
        );
        for (path, node) in nodes {
            for key in node.as_object().into_iter().flatten().map(|(key, _)| key) {
                if !KEYWORDS.contains(&key.as_str()) {
                    unknown.push(format!("{path}: {key}"));
                }
            }
        }
    }
    assert!(
        unknown.is_empty(),
        "schema keywords schema_law does not enforce: {unknown:?}"
    );
}

#[test]
fn every_pattern_compiles() {
    let mut patterns = 0;
    for (tool, schema) in every_input_schema() {
        let mut nodes = Vec::new();
        schema_nodes(&schema, tool.clone(), &mut nodes);
        for (path, node) in nodes {
            if let Some(pattern) = node.get("pattern").and_then(Value::as_str) {
                patterns += 1;
                assert!(
                    regex::Regex::new(pattern).is_ok(),
                    "{path}: pattern `{pattern}` does not compile"
                );
            }
        }
    }
    assert!(
        patterns > 0,
        "the catalogue declares patterns; the walk found none"
    );
    assert!(matches_pattern("^read_[0-9a-f]{2}$", "read_0a"));
    assert!(!matches_pattern("^read_[0-9a-f]{2}$", "read_zz"));
    assert!(
        !matches_pattern("(", "anything"),
        "a broken pattern refuses"
    );
}

#[test]
fn each_keyword_refuses_what_it_should() {
    let schema = json!({
        "type": "object", "additionalProperties": false, "required": ["mode"],
        "properties": {
            "mode": {"type": "string", "enum": ["a", "b"]},
            "count": {"type": "integer", "minimum": 1, "maximum": 3},
            "tags": {"type": "array", "maxItems": 2, "items": {"type": "string", "pattern": "^[a-z]+$"}},
            "page": {"type": "object", "properties": {"cursor": {"type": "string"}}},
            "to": {"oneOf": [{"type": "string"}, {"type": "array", "items": {"type": "string"}}]},
            "continuation": {"type": "string"}
        },
        "if": {"required": ["continuation"]}, "then": {"maxProperties": 2}
    });
    let refused = |value: Value| check(&schema, &value, "t").expect_err("refused").message;
    assert!(refused(json!({"mode": "a", "page": 1})).contains("`t.page` must be an object"));
    assert!(refused(json!({"mode": "z"})).contains("`t.mode` must be one of `a`, `b`"));
    assert!(refused(json!({"mode": "a", "count": 0})).contains("at least 1"));
    assert!(refused(json!({"mode": "a", "count": 4})).contains("at most 3"));
    assert!(refused(json!({"mode": "a", "count": "2"})).contains("must be an integer"));
    assert!(refused(json!({"mode": "a", "count": 2.0})).contains("without a decimal point"));
    assert!(refused(json!({"mode": "a", "tags": ["a", "b", "c"]})).contains("at most 2 items"));
    assert!(refused(json!({"mode": "a", "tags": ["A"]})).contains("expected form"));
    assert!(refused(json!({"mode": "a", "to": 7})).contains("a string or an array"));
    assert!(refused(json!({"mode": "a", "to": [7]})).contains("`t.to[0]` must be a string"));
    assert!(refused(json!({})).contains("missing required argument `mode`"));
    assert!(
        refused(json!({"mode": "a", "continuation": "c", "count": 1}))
            .starts_with("When `continuation` is given, `t` takes at most 2")
    );
    assert!(check(&schema, &json!({"mode": "a", "to": ["x"], "count": 3}), "t").is_ok());
}

#[test]
fn one_of_refuses_a_value_that_fits_two_shapes() {
    let schema = json!({"oneOf": [{"required": ["a"]}, {"required": ["b"]}]});
    assert!(check(&schema, &json!({"a": 1}), "t").is_ok());
    let message = check(&schema, &json!({"a": 1, "b": 2}), "t")
        .expect_err("ambiguous")
        .message;
    assert!(message.contains("more than one"), "{message}");
    let message = check(&schema, &json!({}), "t")
        .expect_err("neither")
        .message;
    assert_eq!(message, "`t` needs `a` or `b`.");
    let shapes = json!({"oneOf": [
        {"type": "object", "required": ["a"], "properties": {"a": {"type": "string"}}},
        {"type": "object", "required": ["b"], "properties": {"b": {"type": "string"}}, "maxProperties": 1}
    ]});
    let message = check(&shapes, &json!({"c": 1, "d": 2}), "t")
        .expect_err("neither")
        .message;
    assert!(message.contains("fits none of the 2 shapes"), "{message}");
}
