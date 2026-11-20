//! The published input schema is the law of every call (#921, E1 of the 1.0
//! review). A value the schema refuses is refused here, before dispatch, so
//! no parser ever reads a mistyped guard as "absent" and goes ahead.
//!
//! This walks the JSON Schema keywords the catalogue uses, nothing more:
//! `KEYWORDS` lists them, and a test fails the day a schema uses one this file
//! does not implement. Messages name the argument's path.

use std::collections::HashMap;

use serde_json::{Map, Value, json};

use super::schema_violation::SchemaViolation;
use super::schema_wording::{
    a_kind, condition_wording, listed, not_wording, received, type_allows,
};

type Checked = Result<(), SchemaViolation>;

#[cfg(test)]
/// Every schema keyword this checker understands. Annotation keywords are
/// read by people and agents, never by the check.
pub(crate) const KEYWORDS: &[&str] = &[
    "type",
    "properties",
    "additionalProperties",
    "required",
    "items",
    "enum",
    "const",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "minLength",
    "maxLength",
    "pattern",
    "minItems",
    "maxItems",
    "uniqueItems",
    "minProperties",
    "maxProperties",
    "propertyNames",
    "oneOf",
    "anyOf",
    "allOf",
    "not",
    "if",
    "then",
    "else",
    // Annotations.
    "default",
    "description",
    "title",
    "examples",
    "deprecated",
    "readOnly",
    "writeOnly",
    "$comment",
];

/// Checks `value` against `schema`; `path` names the value in messages.
pub(crate) fn check(schema: &Value, value: &Value, path: &str) -> Checked {
    let schema = match schema {
        Value::Bool(true) => return Ok(()),
        Value::Bool(false) => {
            return Err(SchemaViolation::at(
                path,
                format!("`{path}` is not accepted here."),
            ));
        }
        Value::Object(schema) => schema,
        _ => return Ok(()),
    };
    check_type(schema, value, path)?;
    check_choice(schema, value, path)?;
    match value {
        Value::String(text) => check_text(schema, text, path)?,
        Value::Number(_) => check_bounds(schema, value, path)?,
        Value::Array(items) => check_items(schema, items, path)?,
        Value::Object(object) => check_object(schema, object, path)?,
        Value::Bool(_) | Value::Null => {}
    }
    check_composition(schema, value, path)
}

/// Whether `value` satisfies `schema`, for `if`, `not` and union branches.
pub(crate) fn satisfies(schema: &Value, value: &Value) -> bool {
    check(schema, value, "").is_ok()
}

fn check_type(schema: &Map<String, Value>, value: &Value, path: &str) -> Checked {
    let declared: Vec<&str> = match schema.get("type") {
        Some(Value::String(kind)) => vec![kind.as_str()],
        Some(Value::Array(kinds)) => kinds.iter().filter_map(Value::as_str).collect(),
        _ => return Ok(()),
    };
    if declared.iter().any(|kind| type_allows(kind, value)) {
        return Ok(());
    }
    if declared == ["boolean"] {
        // The wording #921 settled for switches, kept word for word.
        return Err(SchemaViolation::at(
            path,
            format!(
                "`{path}` must be a JSON boolean, true or false without quotes; received {}. \
                 Nothing was written.",
                received(value)
            ),
        ));
    }
    if declared.contains(&"integer") && value.as_f64().is_some_and(|number| number.fract() == 0.0) {
        return Err(SchemaViolation::at(
            path,
            format!(
                "`{path}` must be an integer written without a decimal point; received {}.",
                received(value)
            ),
        ));
    }
    let wanted = declared.iter().map(|kind| a_kind(kind)).collect::<Vec<_>>();
    Err(SchemaViolation::at(
        path,
        format!(
            "`{path}` must be {}; received {}.",
            wanted.join(" or "),
            received(value)
        ),
    ))
}

fn check_choice(schema: &Map<String, Value>, value: &Value, path: &str) -> Checked {
    if let Some(Value::Array(allowed)) = schema.get("enum")
        && !allowed.contains(value)
    {
        return Err(SchemaViolation::at(
            path,
            format!(
                "`{path}` must be one of {}; received {}.",
                listed(allowed),
                received(value)
            ),
        )
        .allowing(allowed));
    }
    if let Some(constant) = schema.get("const")
        && constant != value
    {
        return Err(SchemaViolation::at(
            path,
            format!(
                "`{path}` must be {}; received {}.",
                listed(std::slice::from_ref(constant)),
                received(value)
            ),
        ));
    }
    Ok(())
}

fn check_text(schema: &Map<String, Value>, text: &str, path: &str) -> Checked {
    let length = text.chars().count() as u64;
    if let Some(minimum) = schema.get("minLength").and_then(Value::as_u64)
        && length < minimum
    {
        return Err(SchemaViolation::at(
            path,
            if minimum == 1 {
                format!("`{path}` must not be empty: give it a value or leave the argument out.")
            } else {
                format!("`{path}` must be at least {minimum} characters; received {length}.")
            },
        ));
    }
    if let Some(maximum) = schema.get("maxLength").and_then(Value::as_u64)
        && length > maximum
    {
        return Err(SchemaViolation::at(
            path,
            format!("`{path}` must be at most {maximum} characters; received {length}."),
        ));
    }
    if let Some(pattern) = schema.get("pattern").and_then(Value::as_str)
        && !matches_pattern(pattern, text)
    {
        return Err(SchemaViolation::at(
            path,
            format!(
                "`{path}` does not have the expected form `{pattern}`; received {}.",
                received(&Value::String(text.to_string()))
            ),
        ));
    }
    Ok(())
}

/// A catalogue pattern, compiled once. A pattern that does not compile
/// refuses every value: a test compiles each one, so this cannot ship.
pub(crate) fn matches_pattern(pattern: &str, text: &str) -> bool {
    static COMPILED: std::sync::OnceLock<std::sync::Mutex<HashMap<String, Option<regex::Regex>>>> =
        std::sync::OnceLock::new();
    let mut compiled = COMPILED
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    compiled
        .entry(pattern.to_string())
        .or_insert_with(|| regex::Regex::new(pattern).ok())
        .as_ref()
        .is_some_and(|regex| regex.is_match(text))
}

fn check_bounds(schema: &Map<String, Value>, value: &Value, path: &str) -> Checked {
    let Some(number) = value.as_f64() else {
        return Ok(());
    };
    let bound = |key: &str| schema.get(key).and_then(Value::as_f64);
    let refused = |relation: &str, limit: &Value| {
        Err(SchemaViolation::at(
            path,
            format!("`{path}` must be {relation} {limit}; received {value}."),
        ))
    };
    if let Some(minimum) = bound("minimum")
        && number < minimum
    {
        return refused("at least", &schema["minimum"]);
    }
    if let Some(maximum) = bound("maximum")
        && number > maximum
    {
        return refused("at most", &schema["maximum"]);
    }
    if let Some(minimum) = bound("exclusiveMinimum")
        && number <= minimum
    {
        return refused("greater than", &schema["exclusiveMinimum"]);
    }
    if let Some(maximum) = bound("exclusiveMaximum")
        && number >= maximum
    {
        return refused("less than", &schema["exclusiveMaximum"]);
    }
    Ok(())
}

fn check_items(schema: &Map<String, Value>, items: &[Value], path: &str) -> Checked {
    let count = items.len() as u64;
    if let Some(minimum) = schema.get("minItems").and_then(Value::as_u64)
        && count < minimum
    {
        return Err(SchemaViolation::at(
            path,
            if count == 0 {
                format!(
                    "`{path}` must not be empty: give at least {minimum} item(s) or leave the \
                     argument out."
                )
            } else {
                format!("`{path}` needs at least {minimum} items; received {count}.")
            },
        ));
    }
    if let Some(maximum) = schema.get("maxItems").and_then(Value::as_u64)
        && count > maximum
    {
        return Err(SchemaViolation::at(
            path,
            format!("`{path}` takes at most {maximum} items; received {count}."),
        ));
    }
    if schema.get("uniqueItems") == Some(&Value::Bool(true)) {
        let mut seen = HashMap::with_capacity(items.len());
        for (index, entry) in items.iter().enumerate() {
            if let Some(first) = seen.insert(entry.to_string(), index) {
                return Err(SchemaViolation::at(
                    path,
                    format!(
                        "`{path}` lists the same value twice (items {first} and {index}): \
                         {entry}. Give each value once."
                    ),
                ));
            }
        }
    }
    if let Some(each) = schema.get("items") {
        for (index, entry) in items.iter().enumerate() {
            check(each, entry, &format!("{path}[{index}]"))?;
        }
    }
    Ok(())
}

fn check_object(schema: &Map<String, Value>, object: &Map<String, Value>, path: &str) -> Checked {
    let properties = schema.get("properties").and_then(Value::as_object);
    let additional = schema.get("additionalProperties");
    if additional == Some(&Value::Bool(false)) {
        for key in object.keys() {
            if properties.is_some_and(|properties| properties.contains_key(key)) {
                continue;
            }
            let known = properties
                .map(|properties| properties.keys().cloned().collect::<Vec<_>>().join(", "))
                .unwrap_or_default();
            return Err(SchemaViolation::unknown_argument(
                path,
                key,
                format!(
                    "`{path}` has no argument `{key}`. This call would otherwise have been \
                     answered with that argument silently dropped. Accepted here: {known}."
                ),
            ));
        }
    }
    if let Some(names) = schema.get("propertyNames") {
        for key in object.keys() {
            if satisfies(names, &Value::String(key.clone())) {
                continue;
            }
            let accepted = names.get("enum").and_then(Value::as_array).map(|names| {
                names
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            });
            return Err(SchemaViolation::unknown_argument(
                path,
                key,
                match accepted {
                    Some(accepted) => format!(
                        "`{path}` has no argument `{key}` in this form. Accepted here: \
                         {accepted}."
                    ),
                    None => format!("`{path}` cannot take an argument named `{key}`."),
                },
            ));
        }
    }
    for (key, nested) in object {
        let declared = properties.and_then(|properties| properties.get(key));
        let schema = declared.or(additional.filter(|additional| additional.is_object()));
        if let Some(schema) = schema {
            check(schema, nested, &format!("{path}.{key}"))?;
        }
    }
    for key in schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if !object.contains_key(key) {
            return Err(SchemaViolation::at(
                &format!("{path}.{key}"),
                format!("`{path}`: missing required argument `{key}`."),
            ));
        }
    }
    let count = object.len() as u64;
    if let Some(minimum) = schema.get("minProperties").and_then(Value::as_u64)
        && count < minimum
    {
        return Err(SchemaViolation::at(
            path,
            format!("`{path}` must not be empty: give at least {minimum} entry."),
        ));
    }
    if let Some(maximum) = schema.get("maxProperties").and_then(Value::as_u64)
        && count > maximum
    {
        let keys = object.keys().cloned().collect::<Vec<_>>().join(", ");
        return Err(SchemaViolation::at(
            path,
            format!("`{path}` takes at most {maximum} argument(s) here; received {count}: {keys}."),
        ));
    }
    Ok(())
}

fn check_composition(schema: &Map<String, Value>, value: &Value, path: &str) -> Checked {
    for part in schema
        .get("allOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        check(part, value, path)?;
    }
    if let Some(Value::Array(branches)) = schema.get("anyOf") {
        check_union(branches, value, path, false)?;
    }
    if let Some(Value::Array(branches)) = schema.get("oneOf") {
        check_union(branches, value, path, true)?;
    }
    if let Some(forbidden) = schema.get("not")
        && satisfies(forbidden, value)
    {
        return Err(SchemaViolation::at(
            path,
            not_wording(forbidden, value, path),
        ));
    }
    if let Some(condition) = schema.get("if") {
        let (branch, holds) = if satisfies(condition, value) {
            (schema.get("then"), true)
        } else {
            (schema.get("else"), false)
        };
        if let Some(branch) = branch
            && let Err(mut violation) = check(branch, value, path)
        {
            // Only a condition that holds explains the rule ("when `memories`
            // is given"); the `else` of one that does not is the ordinary
            // case, and an inner condition already said its own.
            if holds
                && !violation.message.starts_with("When ")
                && let Some(when) = condition_wording(condition, true)
            {
                violation.message = format!("{when}{}", violation.message);
            }
            return Err(violation);
        }
    }
    Ok(())
}

/// `anyOf` (`exactly_one == false`) or `oneOf`. When every branch declares a
/// type, the branch that can hold this value's JSON type speaks for the
/// union, so the caller hears the precise problem rather than "none matched".
fn check_union(branches: &[Value], value: &Value, path: &str, exactly_one: bool) -> Checked {
    if branches.iter().all(|branch| branch.get("type").is_some()) {
        let holding = branches
            .iter()
            .filter(|branch| {
                let declared: Vec<&str> = match &branch["type"] {
                    Value::String(kind) => vec![kind.as_str()],
                    Value::Array(kinds) => kinds.iter().filter_map(Value::as_str).collect(),
                    _ => Vec::new(),
                };
                declared.iter().any(|kind| type_allows(kind, value))
            })
            .collect::<Vec<_>>();
        match holding.as_slice() {
            [] => {
                let mut wanted = branches
                    .iter()
                    .filter_map(|branch| branch["type"].as_str())
                    .map(a_kind)
                    .collect::<Vec<_>>();
                wanted.dedup();
                return Err(SchemaViolation::at(
                    path,
                    format!(
                        "`{path}` must be {}; received {}.",
                        wanted.join(" or "),
                        received(value)
                    ),
                ));
            }
            [only] => return check(only, value, path),
            _ => {}
        }
    }
    if let Some(names) = required_alternatives(branches)
        && value.is_object()
        && !names.iter().any(|name| value.get(name).is_some())
    {
        let wording = names
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(" or ");
        return Err(SchemaViolation::at(
            &format!("{path}.{}", names[0]),
            format!("`{path}` needs {wording}."),
        ));
    }
    let outcomes = branches
        .iter()
        .map(|branch| check(branch, value, path))
        .collect::<Vec<_>>();
    let passing = outcomes.iter().filter(|outcome| outcome.is_ok()).count();
    if passing == 1 || (!exactly_one && passing > 1) {
        return Ok(());
    }
    if passing > 1 {
        return Err(SchemaViolation::at(
            path,
            format!(
                "`{path}` fits more than one of the shapes this argument accepts, so it is \
                 ambiguous; give the arguments of exactly one."
            ),
        ));
    }
    // Branches told apart by a property's `const` or `enum` (`move`): the
    // one this value selects speaks for the union.
    let selected = branches
        .iter()
        .zip(&outcomes)
        .filter(|(branch, _)| {
            branch
                .get("properties")
                .is_some_and(|properties| satisfies(&json!({"properties": properties}), value))
        })
        .collect::<Vec<_>>();
    if selected.len() == 1
        && branches
            .iter()
            .all(|branch| branch.get("properties").is_some())
        && let (_, Err(violation)) = selected[0]
    {
        return Err(violation.clone());
    }
    let reasons = branches
        .iter()
        .zip(outcomes)
        .filter_map(|(branch, outcome)| {
            let message = outcome.err()?.message;
            Some(match branch.get("title").and_then(Value::as_str) {
                Some(title) => format!("({title}) {message}"),
                None => message,
            })
        })
        .collect::<Vec<_>>();
    Err(SchemaViolation::at(
        path,
        match reasons.as_slice() {
            [only] => only.clone(),
            _ => format!(
                "`{path}` fits none of the {} shapes it accepts. {}",
                branches.len(),
                reasons.join(" Or: ")
            ),
        },
    ))
}

/// The names, when every branch only requires one argument: "give `from`
/// or `interval`".
fn required_alternatives(branches: &[Value]) -> Option<Vec<String>> {
    branches
        .iter()
        .map(|branch| {
            let object = branch.as_object()?;
            let names = object.get("required")?.as_array()?;
            (object.len() == 1 && names.len() == 1)
                .then(|| names[0].as_str().map(str::to_string))?
        })
        .collect()
}
