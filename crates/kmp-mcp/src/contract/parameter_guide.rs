//! Argument prose, served by `kmp_guide` instead of `tools/list` (#850).
//!
//! The contract keeps every parameter description: they are written next to
//! the schema they explain, in `contract::tools`, and nowhere else. What
//! changes is where they are advertised. `tools/list` is paid by every host on
//! every session and parameter prose was more than half of it, so the
//! advertised input schemas carry types, enums, bounds and required fields
//! only. `kmp_guide` projects the same descriptions, at serve time and from
//! the same full contract, into the card of the topic that teaches each tool
//! (`card.parameters`). The two can never drift because there is one source.

use serde_json::{Map, Value, json};

use crate::contract::registry::tools_list_result;
use crate::guidance::topic_tools;

/// The schema keywords whose value is itself a schema at the same path.
const SAME_PATH: [&str; 5] = ["if", "then", "else", "not", "contains"];
/// The schema keywords whose value is a list of schemas at the same path.
const SAME_PATH_LISTS: [&str; 3] = ["anyOf", "oneOf", "allOf"];

/// Visits every schema below `schema` with its argument path. Only schema
/// positions are visited: a property literally named `description`, an enum
/// value or a `default` object is never mistaken for an annotation.
fn visit(schema: &mut Value, path: &str, f: &mut impl FnMut(&str, &mut Map<String, Value>)) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    f(path, object);
    if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
        for (name, child) in properties.iter_mut() {
            let child_path = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}.{name}")
            };
            visit(child, &child_path, f);
        }
    }
    for key in ["additionalProperties", "patternProperties"] {
        match object.get_mut(key) {
            Some(Value::Object(inner)) if key == "patternProperties" => {
                for child in inner.values_mut() {
                    visit(child, &format!("{path}.*"), f);
                }
            }
            Some(child @ Value::Object(_)) => visit(child, &format!("{path}.*"), f),
            _ => {}
        }
    }
    match object.get_mut("items") {
        Some(Value::Array(items)) => {
            for (index, child) in items.iter_mut().enumerate() {
                visit(child, &format!("{path}[{index}]"), f);
            }
        }
        Some(child) => visit(child, &format!("{path}[]"), f),
        None => {}
    }
    for key in SAME_PATH {
        if let Some(child) = object.get_mut(key) {
            visit(child, path, f);
        }
    }
    for key in SAME_PATH_LISTS {
        if let Some(Value::Array(children)) = object.get_mut(key) {
            for child in children {
                visit(child, path, f);
            }
        }
    }
    for key in ["$defs", "definitions"] {
        if let Some(Value::Object(defs)) = object.get_mut(key) {
            for (name, child) in defs.iter_mut() {
                visit(child, &format!("{key}.{name}"), f);
            }
        }
    }
}

/// One `path: description` line per documented argument of one input schema,
/// in schema order, without repeating an identical line.
pub(crate) fn parameter_lines(input_schema: &Value) -> Vec<String> {
    let mut schema = input_schema.clone();
    let mut lines: Vec<String> = Vec::new();
    visit(&mut schema, "", &mut |path, object| {
        if let Some(description) = object.get("description").and_then(Value::as_str) {
            let line = if path.is_empty() {
                format!("(arguments): {description}")
            } else {
                format!("{path}: {description}")
            };
            if !lines.contains(&line) {
                lines.push(line);
            }
        }
    });
    lines
}

/// `card.parameters` for one guide topic: every tool the topic teaches, with
/// its argument lines projected from the full contract.
pub(crate) fn topic_parameters(topic: &str) -> Value {
    let contract = tools_list_result();
    let mut parameters = Map::new();
    for name in topic_tools(topic) {
        if let Some(tool) = contract["tools"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|tool| tool["name"] == *name)
        {
            parameters.insert(
                (*name).to_string(),
                json!(parameter_lines(&tool["inputSchema"])),
            );
        }
    }
    Value::Object(parameters)
}

/// Drops every parameter description from the advertised input schemas and
/// points each tool at the guide topic that now serves them. `tools_list_result`
/// keeps them: it is the contract the guide reads.
pub(crate) fn without_parameter_descriptions(mut result: Value) -> Value {
    for tool in result["tools"].as_array_mut().into_iter().flatten() {
        if let Some(schema) = tool.get_mut("inputSchema") {
            visit(schema, "", &mut |_, object| {
                object.remove("description");
            });
        }
        let name = tool["name"].as_str().unwrap_or_default().to_owned();
        if let Some(topic) = crate::guidance::tool_topic(&name)
            && let Some(description) = tool["description"].as_str()
        {
            tool["description"] = json!(format!(
                "{description} Arguments explained: kmp_guide topic {topic}."
            ));
        }
    }
    result
}
