//! Every cross-field conditional of the full contract is a tagged rule that
//! a guide card documents, a refusal names it with its fix, and an object
//! schema is closed unless it says otherwise.
#![cfg(test)]

use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::cross_field_rule::{self, RULE_KEYWORD};
use super::registry::tools_list_result_with_apps;
use super::schema_law;
use super::validator::reject_invalid_arguments;

fn input_schemas(result: &Value) -> Vec<(String, Value)> {
    result["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|tool| {
            (
                tool["name"].as_str().unwrap_or_default().to_string(),
                tool["inputSchema"].clone(),
            )
        })
        .collect()
}

/// Every conditional below `schema` outside a tagged rule, with its path.
/// A list whose every branch is a tagged rule only carries rules; a typed
/// union is a type, not a rule; the root split of a continuation tool
/// (`if` continuation, `else` the first call's `required`) is published.
fn untagged_conditionals(schema: &Value, path: &str, root: bool, out: &mut Vec<String>) {
    let Some(object) = schema.as_object() else {
        return;
    };
    if object.contains_key(RULE_KEYWORD) {
        return;
    }
    let tagged = |value: &Value| value.get(RULE_KEYWORD).is_some();
    let split = root && object.get("if") == Some(&json!({"required": ["continuation"]}));
    for keyword in ["if", "then", "else", "not", "propertyNames"] {
        if let Some(child) = object.get(keyword)
            && !tagged(child)
            && !(split && matches!(keyword, "if" | "else"))
        {
            out.push(format!("{path} {keyword}"));
        }
    }
    if let Some(Value::Array(branches)) = object.get("allOf")
        && !branches.iter().all(tagged)
    {
        out.push(format!("{path} allOf"));
    }
    for keyword in ["anyOf", "oneOf"] {
        if let Some(Value::Array(branches)) = object.get(keyword)
            && !branches
                .iter()
                .all(|branch| branch.get("type").is_some() || branch.get("const").is_some())
        {
            out.push(format!("{path} {keyword}"));
        }
    }
    for (key, child) in object {
        match key.as_str() {
            "properties" => {
                for (name, property) in child.as_object().into_iter().flatten() {
                    untagged_conditionals(property, &format!("{path}.{name}"), false, out);
                }
            }
            "items" | "additionalProperties" => {
                untagged_conditionals(child, &format!("{path}/{key}"), false, out)
            }
            "else" if split => {
                let mut otherwise = child.clone();
                if let Some(otherwise) = otherwise.as_object_mut() {
                    otherwise.remove("required");
                }
                untagged_conditionals(&otherwise, &format!("{path}/else"), false, out)
            }
            "anyOf" | "oneOf" | "allOf" => {
                for (index, branch) in child.as_array().into_iter().flatten().enumerate() {
                    untagged_conditionals(branch, &format!("{path}/{key}/{index}"), false, out);
                }
            }
            _ => {}
        }
    }
}

#[test]
fn every_cross_field_conditional_is_a_tagged_documented_rule() {
    let full = tools_list_result_with_apps(true);
    let mut untagged = Vec::new();
    let mut found = BTreeSet::new();
    for (tool, schema) in input_schemas(&full) {
        untagged_conditionals(&schema, &tool, true, &mut untagged);
        let text = schema.to_string();
        for rule in cross_field_rule::all()
            .iter()
            .filter(|rule| rule.tool == tool)
        {
            assert!(
                text.contains(&format!("\"{RULE_KEYWORD}\":\"{}\"", rule.id)),
                "rule `{}` is documented for {tool} and enforced nowhere in its schema",
                rule.id
            );
        }
        let marker = format!("\"{RULE_KEYWORD}\":\"");
        let mut at = 0;
        while let Some(offset) = text[at..].find(&marker) {
            let start = at + offset + marker.len();
            let end = start + text[start..].find('"').expect("closing quote");
            found.insert((tool.clone(), text[start..end].to_string()));
            at = end;
        }
    }
    for (tool, id) in &found {
        assert!(
            cross_field_rule::named(tool, id).is_some(),
            "{tool} enforces rule `{id}`, which no guide card documents"
        );
    }
    assert!(
        untagged.is_empty(),
        "cross-field conditionals without a documented rule: {untagged:?}"
    );
}

#[test]
fn an_object_schema_without_additional_properties_is_closed() {
    let schema = json!({"type":"object","properties":{"a":{"type":"string"},
        "map":{"type":"object","additionalProperties":{"type":"string"}},
        "open":{"type":"object","additionalProperties":true}},
        "allOf":[{"if":{"properties":{"a":{"const":"x"}}},"then":{"required":["map"]}}]});
    let refused = schema_law::check(&schema, &json!({"a":"y","zz":1}), "t").expect_err("closed");
    assert!(
        refused.unknown && refused.message.contains("`zz`"),
        "{refused:?}"
    );
    assert!(
        schema_law::check(
            &schema,
            &json!({"a":"y","map":{"any":"v"},"open":{"n":1}}),
            "t"
        )
        .is_ok()
    );
    // A conditional fragment declares no type and closes nothing.
    assert!(schema_law::check(&schema, &json!({"a":"x","map":{}}), "t").is_ok());
}

#[test]
fn a_rule_refusal_names_the_rule_and_its_fix() {
    let error = reject_invalid_arguments(
        "kmp_write_memory",
        &json!({"about": "a:b", "memories": [{"id": "m", "kind": "observation",
            "summary": "S.", "evidence": "E."}]}),
    )
    .expect_err("a memory without a label is refused");
    assert!(
        error.message.starts_with("Rule `write.labels`: "),
        "{}",
        error.message
    );
    assert!(error.message.contains("Fix: "), "{}", error.message);
    assert_eq!(error.feedback[0]["rule"], "write.labels");
    // Beside a handle the law accepts any declared argument: whether its
    // value is the one the handle binds is the server's comparison with the
    // retained call (`tests/continuation_bound_arguments.rs`).
    reject_invalid_arguments(
        "kmp_time",
        &json!({"continuation": format!("read_{}", "0".repeat(32)), "move": "forward"}),
    )
    .expect("the law leaves a resent argument to the retained call");
}
