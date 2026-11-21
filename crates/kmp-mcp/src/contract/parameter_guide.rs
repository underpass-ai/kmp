//! Argument prose, served by `kmp_guide` instead of `tools/list` (#850).
//!
//! The contract keeps every parameter description: they are written next to
//! the schema they explain, in `contract::tools`, and nowhere else. What
//! changes is where they are advertised. `tools/list` is paid by every host on
//! every session and parameter prose was more than half of it, so the
//! advertised input schemas carry types, enums, bounds and required fields,
//! plus one short line on the few arguments agents were refused over
//! ([`ADVERTISED_LINES`]). `kmp_guide` projects the full descriptions, at serve
//! time and from the same contract, into the card of the topic that teaches
//! each argument (`parameters`). The two can never drift because there is one
//! source.

use serde_json::{Map, Value, json};

use crate::contract::registry::tools_list_result;
use crate::guide::GuideTopic;

/// The short lines `tools/list` keeps, written once here: each answers a
/// refusal agents met when they saw only the bare schema, on the tools where
/// it was met (`about` on the two first calls of new work, `continuation` on
/// the write that resumes its review). `tools` empty means every tool with
/// that argument.
pub(crate) const ADVERTISED_LINES: [(&[&str], &str, &str); 7] = [
    (
        &["kmp_write_memory", "kmp_wake"],
        "about",
        "Copy exactly from the task or a returned result.",
    ),
    (
        &[
            "kmp_write_memory",
            "kmp_relabel",
            "kmp_condense",
            "kmp_curate",
        ],
        "actor",
        "Writer name. Omitted: the MCP client name.",
    ),
    (
        &["kmp_write_memory"],
        "labels",
        "At least one key with one value, here or per memory: {\"topic\":[\"x\"]}.",
    ),
    (
        &["kmp_write_memory"],
        "memories[].labels",
        "At least one key with one value, here or on the packet.",
    ),
    (
        &["kmp_write_memory"],
        "memories[].evidence",
        "Required: the source text or observation behind this memory.",
    ),
    (
        &["kmp_write_memory"],
        "continuation",
        "Send alone, exactly as returned.",
    ),
    // The cursor each move takes is rule `time.cursor`; a real agent sent
    // `forward` without one once the rule left the catalogue (tokens-2).
    (
        &["kmp_time"],
        "move",
        "rewind/forward take from or interval; goto takes at; near takes around.",
    ),
];

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

/// One `(path, description)` pair per documented argument of one input
/// schema, in schema order, without repeating an identical pair.
fn documented(input_schema: &Value) -> Vec<(String, String)> {
    let mut schema = input_schema.clone();
    let mut pairs: Vec<(String, String)> = Vec::new();
    visit(&mut schema, "", &mut |path, object| {
        // `tools/list` leaves defaults out (`published_law`); the argument's
        // line says it instead.
        let default = object
            .get("default")
            .map(|default| format!("Default: {default}."));
        let description = match (object.get("description").and_then(Value::as_str), default) {
            (Some(text), Some(default)) => format!("{text} {default}"),
            (Some(text), None) => text.to_string(),
            (None, Some(default)) => default,
            (None, None) => return,
        };
        let pair = (path.to_string(), description);
        if !pairs.contains(&pair) {
            pairs.push(pair);
        }
    });
    pairs
}

fn line(path: &str, description: &str) -> String {
    if path.is_empty() {
        format!("(arguments): {description}")
    } else {
        format!("{path}: {description}")
    }
}

/// One `path: description` line per documented argument of one input schema,
/// in schema order, without repeating an identical line.
#[cfg(test)]
pub(crate) fn parameter_lines(input_schema: &Value) -> Vec<String> {
    documented(input_schema)
        .iter()
        .map(|(path, description)| line(path, description))
        .collect()
}

/// `parameters` for one guide topic: every tool the topic teaches, with the
/// argument lines it owns, projected from the full contract. A description
/// the same tool already gave for another argument is not spelled twice:
/// that line reads `path: as <first path>.`
pub(crate) fn topic_parameters(topic: &GuideTopic) -> Value {
    let contract = tools_list_result();
    let mut parameters = Map::new();
    for name in topic.tools() {
        if let Some(tool) = contract["tools"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|tool| tool["name"] == name)
        {
            let mut first: Vec<(String, String)> = Vec::new();
            let mut lines = Vec::new();
            for (path, description) in documented(&tool["inputSchema"]) {
                if !topic.serves_path(name, &path) {
                    continue;
                }
                match first.iter().find(|(_, seen)| *seen == description) {
                    Some((earlier, _)) if !path.is_empty() && !earlier.is_empty() => {
                        lines.push(format!("{path}: as {earlier}."));
                    }
                    _ => {
                        lines.push(line(&path, &description));
                        first.push((path, description));
                    }
                }
            }
            parameters.insert(name.to_string(), json!(lines));
        }
    }
    Value::Object(parameters)
}

/// Drops every parameter description from the advertised input schemas,
/// except the short [`ADVERTISED_LINES`], and points each tool at the guide
/// topic that serves the rest. `tools_list_result` keeps them all: it is the
/// contract the guide reads.
pub(crate) fn without_parameter_descriptions(mut result: Value) -> Value {
    for tool in result["tools"].as_array_mut().into_iter().flatten() {
        let name = tool["name"].as_str().unwrap_or_default().to_owned();
        if let Some(schema) = tool.get_mut("inputSchema") {
            // The property schema is visited before the subschemas at its
            // path, so the line lands once, on the property itself.
            let mut placed: Vec<&str> = Vec::new();
            visit(schema, "", &mut |path, object| {
                object.remove("description");
                if let Some((_, advertised, line)) =
                    ADVERTISED_LINES.iter().find(|(tools, advertised, _)| {
                        *advertised == path && (tools.is_empty() || tools.contains(&name.as_str()))
                    })
                    && !placed.contains(advertised)
                {
                    placed.push(advertised);
                    object.insert("description".into(), json!(line));
                }
            });
        }
        if name == "kmp_guide" {
            continue;
        }
        if let Some(topic) = GuideTopic::of_tool(&name)
            && let Some(description) = tool["description"].as_str()
        {
            // A verb named like its topic needs no pointer: the index and
            // the `topic` enum already name it.
            if format!("kmp_{}", topic.name) != name {
                tool["description"] = json!(format!("{description} Guide topic: {}.", topic.name));
            }
        }
    }
    result
}
