//! What `tools/list` publishes of the call contract, and what it leaves to
//! a cheaper statement.
//!
//! Every call is checked against the full contract (`registry::
//! tools_list_result`). The catalogue carries what an agent needs to form a
//! first valid call: types, enums, `required` (with the one `if`/`else` that
//! says a continuation needs nothing else) and the few constraints that
//! prevent a wrong first call ([`PUBLISHED_BOUNDS`]). The rest of the law is
//! stated once instead of on every argument:
//!
//! - unknown keys are refused in every object (`schema_law::check_object`),
//!   said once in the server instructions, so `additionalProperties: false`
//!   is not repeated on each object; an object that is genuinely open keeps
//!   its explicit `additionalProperties`;
//! - the cross-field rules, tagged in the contract and documented, with their
//!   fix, in the guide card of the argument they concern (`cross_field_rule`);
//! - the bounds and patterns that only restate a check the refusal explains
//!   by itself (`schema_law` names the argument and the limit).
//!
//! `published_law_audit` proves each of them is still refused.

use serde_json::{Map, Value};

use super::cross_field_rule::RULE_KEYWORD;

/// The constraint keywords the catalogue does not repeat.
pub(crate) const UNPUBLISHED_CONSTRAINTS: [&str; 13] = [
    "minLength",
    "maxLength",
    "pattern",
    "minItems",
    "maxItems",
    "uniqueItems",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "minProperties",
    "maxProperties",
    "propertyNames",
];

/// Annotations the catalogue does not repeat: `kmp_guide` serves defaults
/// in the argument's line, and a branch title only words a refusal.
const UNPUBLISHED_ANNOTATIONS: [&str; 2] = ["default", "title"];

/// `(tool, argument path, keyword)` kept in the catalogue because an agent
/// composes that value itself and would otherwise get it wrong on the first
/// call: a local memory id is a name the agent invents.
pub(crate) const PUBLISHED_BOUNDS: [(&str, &str, &str); 1] =
    [("kmp_write_memory", "memories[].id", "pattern")];

/// The input schemas as `tools/list` publishes them (see the module docs).
pub(crate) fn published(mut result: Value) -> Value {
    for tool in result["tools"].as_array_mut().into_iter().flatten() {
        let name = tool["name"].as_str().unwrap_or_default().to_owned();
        if let Some(schema) = tool.get_mut("inputSchema") {
            strip(&name, "", schema);
        }
    }
    result
}

fn strip(tool: &str, path: &str, schema: &mut Value) {
    let Some(object) = schema.as_object_mut() else {
        if let Value::Array(items) = schema {
            items.iter_mut().for_each(|item| strip(tool, path, item));
        }
        return;
    };
    for keyword in UNPUBLISHED_CONSTRAINTS {
        let kept = PUBLISHED_BOUNDS
            .iter()
            .any(|(t, p, k)| *t == tool && *p == path && *k == keyword);
        if !kept {
            object.remove(keyword);
        }
    }
    for keyword in UNPUBLISHED_ANNOTATIONS {
        object.remove(keyword);
    }
    if object.get("additionalProperties") == Some(&Value::Bool(false)) {
        object.remove("additionalProperties");
    }
    if let Some(Value::Object(properties)) = object.get_mut("properties") {
        for (name, child) in properties.iter_mut() {
            let child_path = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}.{name}")
            };
            strip(tool, &child_path, child);
        }
    }
    if let Some(items) = object.get_mut("items") {
        strip(tool, &format!("{path}[]"), items);
    }
    if let Some(entries @ Value::Object(_)) = object.get_mut("additionalProperties") {
        strip(tool, &format!("{path}.*"), entries);
    }
    for keyword in ["if", "then", "else"] {
        if let Some(child) = object.get_mut(keyword) {
            strip(tool, path, child);
        }
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(Value::Array(branches)) = object.get_mut(keyword) {
            branches
                .iter_mut()
                .for_each(|branch| strip(tool, path, branch));
        }
    }
    without_rules(object);
    merge_typed_union(object);
}

/// A union whose branches each hold one distinct JSON type is a type: it is
/// published as `type: [..]` with the branches' keywords side by side, which
/// says the same thing, since each keyword applies to one type only (`items`
/// to arrays, `properties` and `required` to objects). A union with a
/// `const` branch, or whose branches share a keyword, stays as it is.
fn merge_typed_union(object: &mut Map<String, Value>) {
    if object.contains_key("type") {
        return;
    }
    for keyword in ["oneOf", "anyOf"] {
        let Some(Value::Array(branches)) = object.get(keyword) else {
            continue;
        };
        let mut kinds = Vec::new();
        let mut merged = Map::new();
        for branch in branches {
            let Some(branch) = branch.as_object() else {
                return;
            };
            let Some(Value::String(kind)) = branch.get("type") else {
                return;
            };
            if kinds.contains(kind) {
                return;
            }
            kinds.push(kind.clone());
            for (key, value) in branch {
                if key != "type" && merged.insert(key.clone(), value.clone()).is_some() {
                    return;
                }
            }
        }
        object.remove(keyword);
        object.insert(
            "type".into(),
            Value::Array(kinds.into_iter().map(Value::String).collect()),
        );
        object.extend(merged);
        return;
    }
}

/// Drops every conditional tagged as a cross-field rule, then the `if` left
/// without a branch and the empty lists.
fn without_rules(object: &mut Map<String, Value>) {
    let tagged = |value: &Value| value.get(RULE_KEYWORD).is_some();
    let empty = |value: &Value| value.as_object().is_some_and(Map::is_empty);
    for keyword in ["then", "else"] {
        if object
            .get(keyword)
            .is_some_and(|value| tagged(value) || empty(value))
        {
            object.remove(keyword);
        }
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(Value::Array(branches)) = object.get_mut(keyword) {
            branches.retain(|branch| !tagged(branch));
            if branches.is_empty() {
                object.remove(keyword);
            }
        }
    }
    if object.contains_key("if") && !object.contains_key("then") && !object.contains_key("else") {
        object.remove("if");
    }
}
