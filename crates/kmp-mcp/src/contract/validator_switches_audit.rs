//! Every boolean a write tool declares is refused in any other type before
//! the tool runs, so no switch of a write can be read as its default. The
//! walk reads the schemas themselves: a boolean added later is covered
//! without touching this file.
#![cfg(test)]

use serde_json::{Value, json};

use super::registry::tools_list_result;
use super::validator::{reject_invalid_arguments, reject_invalid_write_arguments};

/// The tools that change memory. `kmp_guide` is checked before dispatch too
/// but changes no memory; reads keep their current tolerance.
const WRITE_TOOLS: [&str; 5] = [
    "kmp_write_memory",
    "kmp_ingest",
    "kmp_relabel",
    "kmp_condense",
    "kmp_curate",
];

/// One `(path, arguments)` per declared boolean below `schema`, the
/// arguments carrying `"true"` there and nothing else. Unions are not
/// entered: which branch an argument meets depends on its value.
fn mistyped_booleans(
    schema: &Value,
    path: &str,
    place: &dyn Fn(Value) -> Value,
) -> Vec<(String, Value)> {
    if schema["type"] == "boolean" {
        return vec![(path.to_owned(), place(json!("true")))];
    }
    let mut found = Vec::new();
    if let Some(properties) = schema["properties"].as_object() {
        for (key, nested) in properties {
            let inner = |value: Value| place(json!({ key.clone(): value }));
            found.extend(mistyped_booleans(nested, &format!("{path}.{key}"), &inner));
        }
    }
    if let Some(items) = schema.get("items").filter(|items| items.is_object()) {
        let inner = |value: Value| place(json!([value]));
        found.extend(mistyped_booleans(items, &format!("{path}[0]"), &inner));
    }
    found
}

fn input_schema(tool: &str) -> Value {
    tools_list_result()["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|definition| definition["name"] == tool)
        .unwrap_or_else(|| panic!("{tool} is declared"))["inputSchema"]
        .clone()
}

/// Every `"type": "boolean"` anywhere in a schema, unions included.
fn declared_booleans(schema: &Value) -> usize {
    match schema {
        Value::Object(object) => {
            usize::from(object.get("type").and_then(Value::as_str) == Some("boolean"))
                + object.values().map(declared_booleans).sum::<usize>()
        }
        Value::Array(items) => items.iter().map(declared_booleans).sum(),
        _ => 0,
    }
}

#[test]
fn the_walk_reaches_every_boolean_a_write_declares() {
    for tool in WRITE_TOOLS {
        let schema = input_schema(tool);
        let walked = mistyped_booleans(&schema, tool, &|value| value).len();
        assert_eq!(
            walked,
            declared_booleans(&schema),
            "{tool}: a boolean the walk misses"
        );
    }
}

#[test]
fn every_boolean_a_write_declares_is_refused_as_text() {
    let mut paths = Vec::new();
    for tool in WRITE_TOOLS {
        let schema = input_schema(tool);
        for (path, arguments) in mistyped_booleans(&schema, tool, &|value| value) {
            let error = reject_invalid_write_arguments(tool, &arguments)
                .expect_err(&format!("{path} accepted \"true\": {arguments}"));
            assert!(
                error.message.contains(&format!("`{path}`")) && error.message.contains("boolean"),
                "{path}: {}",
                error.message
            );
            paths.push(path);
        }
    }
    // The walk must reach the switches QA and the review named, or it
    // proves nothing.
    for expected in [
        "kmp_write_memory.options.dry_run",
        "kmp_write_memory.options.strict",
        "kmp_ingest.dry_run",
        "kmp_relabel.options.dry_run",
        "kmp_relabel.options.strict",
        "kmp_curate.accepted[0].reverse",
        "kmp_curate.accepted[0].confirm_doubted",
        "kmp_condense.expect.absent",
    ] {
        assert!(
            paths.iter().any(|path| path == expected),
            "{expected} not walked: {paths:?}"
        );
    }
}

#[test]
fn every_non_boolean_value_of_a_switch_is_refused() {
    for value in [
        json!("false"),
        json!(1),
        json!(0),
        Value::Null,
        json!([]),
        json!({}),
    ] {
        let arguments = json!({"options": {"dry_run": value.clone()}});
        let error = reject_invalid_write_arguments("kmp_write_memory", &arguments)
            .expect_err("never coerced");
        assert!(
            error.message.contains("kmp_write_memory.options.dry_run"),
            "{value}: {}",
            error.message
        );
        assert!(
            error.message.contains("Nothing was written"),
            "{}",
            error.message
        );
    }
    for value in [true, false] {
        let arguments = json!({"options": {"dry_run": value}});
        reject_invalid_write_arguments("kmp_write_memory", &arguments).expect("a boolean passes");
    }
}

/// Reads are out of scope: their tolerance is unchanged by this check.
#[test]
fn the_read_check_is_unchanged() {
    let arguments = json!({"options": {"dry_run": "true"}});
    reject_invalid_arguments("kmp_write_memory", &arguments)
        .expect("the general check does not type switches");
}
