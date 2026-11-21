//! What `tools/list` stopped publishing is still the law. For every
//! constraint the full contract holds and the catalogue leaves out, a call
//! that breaks only that constraint is built and must be refused, naming the
//! argument. The cross-field rules are tagged and documented, and the
//! catalogue keeps no conditional beyond the `required` of a first call.
#![cfg(test)]

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use super::cross_field_rule::{self, RULE_KEYWORD};
use super::published_law::{PUBLISHED_BOUNDS, UNPUBLISHED_CONSTRAINTS, published};
use super::registry::{advertised_tools_list, tools_list_result_with_apps};
use super::schema_law;
use super::validator::reject_invalid_arguments;

/// One accepted call per tool, the same shapes the boundary fuzz uses.
fn baseline(tool: &str) -> Value {
    match tool {
        "kmp_ingest" => json!({"about": "proj:a", "idempotency_key": "ingest-0",
            "memory": {"dimensions": [{"id": "task:t1", "kind": "task"}],
                "entries": [{"id": "proj:a:m0", "kind": "observation", "text": "Deploy failed.",
                    "coordinates": [{"dimension": "task", "scope_id": "task:t1"}]}]},
            "provenance": {"source_kind": "agent", "source_agent": "audit",
                           "observed_at": "2026-11-20T10:00:00Z"}}),
        "kmp_write_memory" => json!({"about": "proj:w", "actor": "audit",
            "labels": {"task": ["t1"]},
            "memories": [{"id": "m0", "kind": "observation", "summary": "Rollback finished.",
                "evidence": "Operator log line."}]}),
        "kmp_wake" | "kmp_relate" | "kmp_summaries_audit" | "kmp_view_open" => {
            json!({"about": "proj:a"})
        }
        "kmp_ask" => json!({"about": "proj:a", "question": "Why did the deploy fail?"}),
        "kmp_time" => json!({"about": "proj:a", "move": "rewind",
            "from": {"time": "2026-12-01T00:00:00Z"}}),
        "kmp_inspect" => json!({"about": "proj:a", "ref": "proj:a:e0"}),
        "kmp_trace" => json!({"about": "proj:a", "from": "proj:a:e0", "to": "proj:a:e1"}),
        "kmp_relabel" => json!({"about": "proj:a", "ref": "proj:a:e0", "actor": "audit",
            "observed_at": "2026-11-20T12:00:00Z", "why": "audit", "add": {"incident": ["i1"]}}),
        "kmp_condense" => json!({"about": "proj:a", "ref": "proj:a:e0", "actor": "audit",
            "language": "en", "scope": "node_body", "card": "Deploy failed.",
            "source": {"revision": 1, "record_digest": "sha256:00"}, "expect": {"absent": true}}),
        "kmp_curate" => json!({"mode": "review", "about": "proj:a"}),
        "kmp_guide" | "kmp_view_get_state" | "kmp_view_undo" => json!({}),
        "kmp_view_apply_intent" => json!({"idempotency_key": "view-0", "search": "disk"}),
        "kmp_view_read_projection" => json!({"about": "proj:a",
            "from": "2026-01-01T00:00:00Z", "to": "2027-01-01T00:00:00Z"}),
        "kmp_view_read_nodes" => json!({"about": "proj:a", "refs": ["proj:a:e0"]}),
        "kmp_view_take_control" => json!({"expected_revision": 1}),
        other => panic!("no baseline for `{other}`: add one"),
    }
}

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

/// A constraint of the full contract: tool, argument path (`a.b[].c`,
/// `labels.*` for a map value), keyword, and the node that holds it.
#[derive(Debug, Clone)]
struct Constraint {
    tool: String,
    path: String,
    keyword: &'static str,
    node: Value,
}

/// Every constraint below `schema` outside a tagged rule, visiting the
/// argument structure the way `published_law` does.
fn constraints(tool: &str, path: &str, schema: &Value, out: &mut Vec<Constraint>) {
    let Some(object) = schema.as_object() else {
        return;
    };
    if object.contains_key(RULE_KEYWORD) {
        return;
    }
    for keyword in UNPUBLISHED_CONSTRAINTS {
        if object.contains_key(keyword) {
            out.push(Constraint {
                tool: tool.to_string(),
                path: path.to_string(),
                keyword,
                node: schema.clone(),
            });
        }
    }
    if let Some(Value::Object(properties)) = object.get("properties") {
        for (name, child) in properties {
            let child_path = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}.{name}")
            };
            constraints(tool, &child_path, child, out);
        }
    }
    if let Some(items) = object.get("items") {
        constraints(tool, &format!("{path}[]"), items, out);
    }
    if let Some(entries @ Value::Object(_)) = object.get("additionalProperties") {
        constraints(tool, &format!("{path}.*"), entries, out);
    }
    for keyword in ["anyOf", "oneOf"] {
        // A typed union: each branch holds the constraints of one type.
        for branch in object
            .get(keyword)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            constraints(tool, path, branch, out);
        }
    }
}

fn kinds(schema: &Value) -> Vec<String> {
    match &schema["type"] {
        Value::String(kind) => vec![kind.clone()],
        Value::Array(kinds) => kinds
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect(),
        _ => Vec::new(),
    }
}

/// A small valid value of `schema`; `salt` keeps array items distinct.
fn minimal(schema: &Value, salt: usize) -> Value {
    if let Some(constant) = schema.get("const") {
        return constant.clone();
    }
    if let Some(Value::Array(choices)) = schema.get("enum") {
        return choices[salt % choices.len()].clone();
    }
    for union in ["oneOf", "anyOf"] {
        if let Some(Value::Array(branches)) = schema.get(union)
            && schema.get("type").is_none()
            && let Some(first) = branches.first()
        {
            return minimal(first, salt);
        }
    }
    let kind = kinds(schema)
        .into_iter()
        .find(|kind| kind != "null")
        .unwrap_or_else(|| "object".to_string());
    match kind.as_str() {
        "string" => {
            let length = schema["minLength"].as_u64().unwrap_or(1) as usize;
            let letter = (b'a' + (salt % 26) as u8) as char;
            let mut candidates = vec![
                letter.to_string().repeat(length.max(1)),
                format!("read_{}", "0".repeat(32)),
                "0".repeat(64),
            ];
            if let Some(pattern) = schema["pattern"].as_str() {
                candidates.retain(|text| schema_law::matches_pattern(pattern, text));
            }
            Value::String(candidates.remove(0))
        }
        "integer" | "number" => json!(schema["minimum"].as_i64().unwrap_or(1).max(1)),
        "boolean" => json!(true),
        "array" => {
            let count = schema["minItems"].as_u64().unwrap_or(0) as usize;
            let items = schema.get("items").cloned().unwrap_or(json!({}));
            Value::Array((0..count).map(|n| minimal(&items, salt + n)).collect())
        }
        _ => {
            let mut object = Map::new();
            for key in schema["required"].as_array().into_iter().flatten() {
                let key = key.as_str().unwrap_or_default();
                object.insert(key.to_string(), minimal(&schema["properties"][key], salt));
            }
            if schema["minProperties"].as_u64().unwrap_or(0) > 0
                && let Some(entries @ Value::Object(_)) = schema.get("additionalProperties")
            {
                object.insert("k".to_string(), minimal(entries, salt));
            }
            Value::Object(object)
        }
    }
}

/// A value of `node` that breaks only `keyword`.
fn breaking(node: &Value, keyword: &str) -> Value {
    let number = |key: &str| node[key].as_i64().expect("numeric bound");
    let items = node.get("items").cloned().unwrap_or(json!({}));
    match keyword {
        "minLength" => json!("x".repeat(number("minLength") as usize - 1)),
        "maxLength" => json!("x".repeat(number("maxLength") as usize + 1)),
        "pattern" => json!("!! not this form !!"),
        "minItems" => Value::Array(
            (0..number("minItems") - 1)
                .map(|n| minimal(&items, n as usize))
                .collect(),
        ),
        "maxItems" => Value::Array(
            (0..=number("maxItems"))
                .map(|n| minimal(&items, n as usize))
                .collect(),
        ),
        "uniqueItems" => {
            let mut value = minimal(node, 0);
            let first = minimal(&items, 0);
            let array = value.as_array_mut().expect("an array");
            array.clear();
            array.push(first.clone());
            array.push(first);
            value
        }
        "minimum" => json!(number("minimum") - 1),
        "maximum" => json!(number("maximum") + 1),
        "exclusiveMinimum" => json!(number("exclusiveMinimum")),
        "exclusiveMaximum" => json!(number("exclusiveMaximum")),
        "minProperties" => json!({}),
        "maxProperties" => {
            let entries = node
                .get("additionalProperties")
                .cloned()
                .unwrap_or(json!({}));
            Value::Object(
                (0..=number("maxProperties"))
                    .map(|n| (format!("k{n}"), minimal(&entries, n as usize)))
                    .collect(),
            )
        }
        other => panic!("no breaking value for `{other}`"),
    }
}

/// The schema at `path` of `root`, following the same steps as the walk.
fn schema_at<'a>(root: &'a Value, steps: &[String]) -> &'a Value {
    let mut node = root;
    for step in steps {
        let next = match step.as_str() {
            "[]" => node.get("items"),
            "*" => node.get("additionalProperties"),
            key => node["properties"].get(key),
        };
        node = match next {
            Some(next) => next,
            None => {
                // The step sits in a typed union branch.
                let branch = ["oneOf", "anyOf"]
                    .iter()
                    .filter_map(|union| node.get(*union).and_then(Value::as_array))
                    .flatten()
                    .find(|branch| match step.as_str() {
                        "[]" => branch.get("items").is_some(),
                        "*" => branch.get("additionalProperties").is_some(),
                        key => branch["properties"].get(key).is_some(),
                    })
                    .unwrap_or_else(|| panic!("no schema at {steps:?}"));
                return schema_at(branch, std::slice::from_ref(step));
            }
        };
    }
    node
}

fn steps(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in path.split('.').filter(|part| !part.is_empty()) {
        let key = part.trim_end_matches("[]");
        out.push(key.to_string());
        for _ in 0..(part.len() - key.len()) / 2 {
            out.push("[]".to_string());
        }
    }
    out
}

/// The branch of a typed union that can take `step`: an array for `[]`, an
/// object for a key. A node that is no union is its own branch.
fn branch_for<'a>(node: &'a Value, step: &str) -> &'a Value {
    let wanted = if step == "[]" { "array" } else { "object" };
    if kinds(node).iter().any(|kind| kind == wanted) {
        return node;
    }
    ["oneOf", "anyOf"]
        .iter()
        .filter_map(|union| node.get(*union).and_then(Value::as_array))
        .flatten()
        .find(|branch| kinds(branch).iter().any(|kind| kind == wanted))
        .unwrap_or(node)
}

/// `call` with `value` placed at `steps`, creating or replacing each parent
/// that cannot hold the next step with a small valid value of its schema.
fn placed(schema: &Value, call: &mut Value, steps: &[String], value: Value) -> String {
    let mut slot = call;
    let mut node = schema;
    let mut shown = String::new();
    for (index, step) in steps.iter().enumerate() {
        let parent = branch_for(node, step);
        let fits = if step == "[]" {
            slot.is_array()
        } else {
            slot.is_object()
        };
        if !fits {
            *slot = minimal(parent, 0);
        }
        node = schema_at(parent, std::slice::from_ref(step));
        let last = index + 1 == steps.len();
        if step == "[]" {
            shown.push_str("[0]");
            let array = slot.as_array_mut().expect("an array parent");
            if last {
                if array.is_empty() {
                    array.push(value);
                } else {
                    array[0] = value;
                }
                return shown;
            }
            if array.is_empty() {
                array.push(minimal(node, 0));
            }
            slot = &mut array[0];
        } else {
            let key = if step == "*" { "k" } else { step.as_str() };
            shown.push('.');
            shown.push_str(key);
            let object = slot.as_object_mut().expect("an object parent");
            if last {
                object.insert(key.to_string(), value);
                return shown;
            }
            slot = object
                .entry(key.to_string())
                .or_insert_with(|| minimal(node, 0));
        }
    }
    shown
}

#[test]
fn every_constraint_the_catalogue_does_not_publish_is_still_refused() {
    let full = tools_list_result_with_apps(true);
    let catalogue = published(full.clone());
    let mut removed = Vec::new();
    for ((tool, contract), (_, advertised)) in input_schemas(&full)
        .into_iter()
        .zip(input_schemas(&catalogue))
    {
        let mut kept = Vec::new();
        constraints(&tool, "", &advertised, &mut kept);
        let mut all = Vec::new();
        constraints(&tool, "", &contract, &mut all);
        for constraint in all {
            if !kept
                .iter()
                .any(|k| k.path == constraint.path && k.keyword == constraint.keyword)
            {
                removed.push(constraint);
            }
        }
    }
    assert!(
        removed.len() > 100,
        "the walk found too few unpublished constraints ({})",
        removed.len()
    );

    let mut accepted = Vec::new();
    let mut table = BTreeSet::new();
    for constraint in &removed {
        let schema = full["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == constraint.tool.as_str())
            .map(|tool| tool["inputSchema"].clone())
            .unwrap();
        let mut call = baseline(&constraint.tool);
        assert!(
            reject_invalid_arguments(&constraint.tool, &call).is_ok(),
            "baseline of {} is refused",
            constraint.tool
        );
        let value = breaking(&constraint.node, constraint.keyword);
        let shown = placed(&schema, &mut call, &steps(&constraint.path), value);
        let argument = format!("{}{shown}", constraint.tool);
        match reject_invalid_arguments(&constraint.tool, &call) {
            Err(error) if error.message.contains(&format!("`{argument}`")) => {}
            outcome => accepted.push(format!(
                "{} {} <- {}: {:?}",
                constraint.keyword,
                argument,
                call,
                outcome.err().map(|error| error.message)
            )),
        }
        table.insert(format!(
            "{} {} {}",
            constraint.tool, constraint.keyword, constraint.path
        ));
    }
    eprintln!(
        "unpublished constraints still enforced ({}):\n{}",
        table.len(),
        table.into_iter().collect::<Vec<_>>().join("\n")
    );
    assert!(
        accepted.is_empty(),
        "{} unpublished constraints are no longer refused:\n{}",
        accepted.len(),
        accepted.join("\n")
    );
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
fn the_catalogue_publishes_no_closed_marker_and_no_rule() {
    let listed = advertised_tools_list(true, false).to_string();
    assert!(!listed.contains("\"additionalProperties\":false"));
    assert!(!listed.contains(RULE_KEYWORD));
    for keyword in [
        "\"allOf\"",
        "\"not\"",
        "\"then\"",
        "\"propertyNames\"",
        "\"default\"",
    ] {
        assert!(
            !listed.contains(keyword),
            "tools/list still publishes {keyword}"
        );
    }
    for (tool, path, keyword) in PUBLISHED_BOUNDS {
        let tools = advertised_tools_list(true, false);
        let schema = tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["name"] == tool)
            .map(|entry| entry["inputSchema"].clone())
            .unwrap();
        assert!(
            schema_at(&schema, &steps(path)).get(keyword).is_some(),
            "{tool}.{path} lost its published {keyword}"
        );
    }
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
    let error = reject_invalid_arguments(
        "kmp_wake",
        &json!({"continuation": format!("read_{}", "0".repeat(32)), "budget": {}}),
    )
    .expect_err("a continuation is sent alone");
    assert!(
        error.message.starts_with("Rule `continuation-alone`: "),
        "{}",
        error.message
    );
}
