use kmp_proto::v1beta1::{
    DimensionScopeMode, DimensionSelection, DimensionSelectionMode, LabelSelector,
    LabelSelectorOperator,
};
use serde_json::{Map, Value};

use super::common::{
    object, optional_object_field, optional_string_array_field, optional_string_field,
};

pub(super) fn dimension_selection_from_arguments(
    arguments: &Value,
) -> Result<Option<DimensionSelection>, String> {
    optional_object_field(
        object(arguments, "tool arguments")?,
        "dimensions",
        "dimensions",
    )?
    .map(dimension_selection_from_object)
    .transpose()
}

fn dimension_selection_from_object(
    dimensions: &Map<String, Value>,
) -> Result<DimensionSelection, String> {
    let mode = match optional_string_field(dimensions, "mode", "dimensions.mode")?.as_deref() {
        None | Some("all") => DimensionSelectionMode::All as i32,
        Some("only") => DimensionSelectionMode::Only as i32,
        Some("except") => DimensionSelectionMode::Except as i32,
        Some(other) => return Err(format!("invalid dimensions.mode `{other}`")),
    };
    let include = optional_string_array_field(dimensions, "include", "dimensions.include")?;
    let exclude = optional_string_array_field(dimensions, "exclude", "dimensions.exclude")?;
    let scope_ids = optional_string_array_field(dimensions, "scope_ids", "dimensions.scope_ids")?;
    let scope = match optional_string_field(dimensions, "scope", "dimensions.scope")?.as_deref() {
        None | Some("current_about") => DimensionScopeMode::CurrentAbout as i32,
        Some("abouts") => DimensionScopeMode::Abouts as i32,
        Some("all_abouts") => DimensionScopeMode::AllAbouts as i32,
        Some(other) => return Err(format!("invalid dimensions.scope `{other}`")),
    };
    let abouts = optional_string_array_field(dimensions, "abouts", "dimensions.abouts")?;
    let selectors = label_selectors_from_field(dimensions)?;

    validate_dimension_mode(mode, &include, &exclude)?;
    validate_dimension_scope(scope, &abouts)?;

    Ok(DimensionSelection {
        mode,
        include,
        exclude,
        scope,
        abouts,
        scope_ids,
        selectors,
    })
}

/// `dimensions.selectors[]`: `{ key, op, values }` per predicate. The shape
/// is checked here so a caller hears which selector is wrong; the kernel
/// checks the same rules again on its side of the wire.
fn label_selectors_from_field(
    dimensions: &Map<String, Value>,
) -> Result<Vec<LabelSelector>, String> {
    let Some(value) = dimensions.get("selectors") else {
        return Ok(Vec::new());
    };
    let Some(items) = value.as_array() else {
        return Err("dimensions.selectors must be an array of selector objects".to_string());
    };
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let path = format!("dimensions.selectors[{index}]");
            let selector = object(item, &path)?;
            let key = optional_string_field(selector, "key", &format!("{path}.key"))?
                .map(|key| key.trim().to_string())
                .filter(|key| !key.is_empty())
                .ok_or_else(|| format!("{path}.key is required"))?;
            let op = match optional_string_field(selector, "op", &format!("{path}.op"))?.as_deref()
            {
                Some("in") => LabelSelectorOperator::In,
                Some("notin") => LabelSelectorOperator::NotIn,
                Some("exists") => LabelSelectorOperator::Exists,
                Some("notexists") => LabelSelectorOperator::NotExists,
                Some(other) => {
                    return Err(format!(
                        "invalid {path}.op `{other}`; expected in, notin, exists or notexists"
                    ));
                }
                None => {
                    return Err(format!(
                        "{path}.op is required: in, notin, exists or notexists"
                    ));
                }
            };
            let values =
                optional_string_array_field(selector, "values", &format!("{path}.values"))?
                    .into_iter()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .collect::<Vec<_>>();
            match op {
                LabelSelectorOperator::In | LabelSelectorOperator::NotIn if values.is_empty() => {
                    return Err(format!(
                        "{path}: `{key} {}` requires at least one value",
                        op_label(op)
                    ));
                }
                LabelSelectorOperator::Exists | LabelSelectorOperator::NotExists
                    if !values.is_empty() =>
                {
                    return Err(format!("{path}: `{key} {}` takes no values", op_label(op)));
                }
                _ => {}
            }
            Ok(LabelSelector {
                key,
                op: op as i32,
                values,
            })
        })
        .collect()
}

fn op_label(op: LabelSelectorOperator) -> &'static str {
    match op {
        LabelSelectorOperator::In => "in",
        LabelSelectorOperator::NotIn => "notin",
        LabelSelectorOperator::Exists => "exists",
        LabelSelectorOperator::NotExists => "notexists",
        LabelSelectorOperator::Unspecified => "unspecified",
    }
}

fn validate_dimension_mode(
    mode: i32,
    include: &[String],
    exclude: &[String],
) -> Result<(), String> {
    match mode {
        value
            if value == DimensionSelectionMode::All as i32
                && (!include.is_empty() || !exclude.is_empty()) =>
        {
            Err("dimension selection mode ALL must not set include or exclude values".to_string())
        }
        value if value == DimensionSelectionMode::Only as i32 && include.is_empty() => {
            Err("dimension selection mode ONLY requires include values".to_string())
        }
        value if value == DimensionSelectionMode::Only as i32 && !exclude.is_empty() => {
            Err("dimension selection mode ONLY must not set exclude values".to_string())
        }
        value if value == DimensionSelectionMode::Except as i32 && exclude.is_empty() => {
            Err("dimension selection mode EXCEPT requires exclude values".to_string())
        }
        value if value == DimensionSelectionMode::Except as i32 && !include.is_empty() => {
            Err("dimension selection mode EXCEPT must not set include values".to_string())
        }
        _ => Ok(()),
    }
}

fn validate_dimension_scope(scope: i32, abouts: &[String]) -> Result<(), String> {
    match scope {
        value if value == DimensionScopeMode::CurrentAbout as i32 && !abouts.is_empty() => {
            Err("dimension scope CURRENT_ABOUT must not set abouts".to_string())
        }
        value if value == DimensionScopeMode::Abouts as i32 && abouts.is_empty() => {
            Err("dimension scope ABOUTS requires at least one about".to_string())
        }
        value if value == DimensionScopeMode::AllAbouts as i32 && !abouts.is_empty() => {
            Err("dimension scope ALL_ABOUTS must not set abouts".to_string())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn dimension_selection_requires_explicit_valid_scope_shape() {
        let empty_abouts = dimension_selection_from_object(
            object(
                &json!({
                    "mode": "only",
                    "include": ["conversation"],
                    "scope": "abouts"
                }),
                "dimensions",
            )
            .expect("dimensions should be object"),
        )
        .expect_err("ABOUTS without abouts should fail fast");
        assert_eq!(
            empty_abouts,
            "dimension scope ABOUTS requires at least one about"
        );

        let all_abouts = dimension_selection_from_object(
            object(
                &json!({
                    "mode": "only",
                    "include": ["conversation"],
                    "scope": "all_abouts"
                }),
                "dimensions",
            )
            .expect("dimensions should be object"),
        )
        .expect("ALL_ABOUTS should be accepted only when explicit");
        assert_eq!(all_abouts.scope, DimensionScopeMode::AllAbouts as i32);

        let exact_scope = dimension_selection_from_object(
            object(
                &json!({
                    "mode": "only",
                    "include": ["conversation"],
                    "scope_ids": ["conversation:rachel"]
                }),
                "dimensions",
            )
            .expect("dimensions should be object"),
        )
        .expect("scope_ids should be accepted as a narrowing filter");
        assert_eq!(exact_scope.scope_ids, vec!["conversation:rachel"]);
    }

    #[test]
    fn selectors_are_read_as_key_op_values_and_refused_when_shapeless() {
        let selectors = dimension_selection_from_object(
            object(
                &json!({
                    "selectors": [
                        { "key": "env", "op": "in", "values": ["prod"] },
                        { "key": "task", "op": "notexists" }
                    ]
                }),
                "dimensions",
            )
            .expect("dimensions should be object"),
        )
        .expect("well-formed selectors are accepted");
        assert_eq!(selectors.selectors.len(), 2);
        assert_eq!(selectors.selectors[0].key, "env");
        assert_eq!(selectors.selectors[0].op, LabelSelectorOperator::In as i32);
        assert_eq!(selectors.selectors[0].values, vec!["prod"]);
        assert_eq!(
            selectors.selectors[1].op,
            LabelSelectorOperator::NotExists as i32
        );

        let refusals = [
            (
                json!({ "selectors": [{ "op": "exists" }] }),
                "dimensions.selectors[0].key is required",
            ),
            (
                json!({ "selectors": [{ "key": "env", "op": "in" }] }),
                "dimensions.selectors[0]: `env in` requires at least one value",
            ),
            (
                json!({ "selectors": [{ "key": "env", "op": "exists", "values": ["prod"] }] }),
                "dimensions.selectors[0]: `env exists` takes no values",
            ),
            (
                json!({ "selectors": [{ "key": "env", "op": "like", "values": ["prod"] }] }),
                "invalid dimensions.selectors[0].op `like`; expected in, notin, exists or notexists",
            ),
        ];
        for (arguments, expected) in refusals {
            let error = dimension_selection_from_object(
                object(&arguments, "dimensions").expect("dimensions should be object"),
            )
            .expect_err("a shapeless selector is refused");
            assert_eq!(error, expected);
        }
    }
}
