use super::common::{
    object, optional_string_array_field, optional_string_field, optional_u32_field,
};
use kmp_proto::v1beta1::TraceSearchOptions;
use serde_json::Value;

pub(super) fn arguments(
    value: &Value,
) -> Result<(String, Vec<String>, Option<TraceSearchOptions>), String> {
    let root = object(value, "trace")?;
    let (to, targets) = match value.get("to") {
        Some(Value::String(s)) if !s.trim().is_empty() => (s.trim().to_string(), vec![]),
        Some(Value::Array(_)) => {
            let targets = optional_string_array_field(root, "to", "to")?;
            if targets.is_empty()
                || targets.len() > 8
                || targets
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != targets.len()
            {
                return Err("to must name 1..8 distinct entry refs".into());
            }
            (targets[0].clone(), targets)
        }
        _ => return Err("to must be a non-empty ref or 1..8 distinct entry refs".into()),
    };
    let search = value
        .get("search")
        .map(|s| {
            let s = object(s, "search")?;
            let limit = |key: &str, default, max| -> Result<u32, String> {
                let value =
                    optional_u32_field(s, key, &format!("search.{key}"))?.unwrap_or(default);
                if value == 0 || value > max {
                    return Err(format!("search.{key} must be 1..{max}"));
                }
                Ok(value)
            };
            let follow = match s.get("follow") {
                None => vec![],
                Some(Value::Array(steps)) if !steps.is_empty() && steps.len() <= 16 => steps
                    .iter()
                    .map(|step| {
                        let step = object(step, "search.follow[]")?;
                        let required = |key| {
                            optional_string_field(step, key, "search.follow[]")?
                                .filter(|s| !s.trim().is_empty())
                                .ok_or_else(|| format!("search.follow[] requires {key}"))
                        };
                        Ok(kmp_proto::v1beta1::TraceRelationStep {
                            rel: required("rel")?,
                            direction: required("direction")?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?,
                _ => return Err("search.follow requires 1..16 moves".into()),
            };
            if !follow.is_empty() && (s.contains_key("direction") || s.contains_key("relations")) {
                return Err("search.follow replaces direction and relations".into());
            }
            let dimensions = |key: &str| {
                s.get(key)
                    .map(|v| {
                        object(v, &format!("search.{key}"))
                            .and_then(super::dimensions::dimension_selection_from_object)
                            .map_err(|e| format!("search.{key}: {e}"))
                    })
                    .transpose()
            };
            Ok::<_, String>(TraceSearchOptions {
                dimensions: dimensions("dimensions")?,
                prefer_dimensions: dimensions("prefer_dimensions")?,
                select: s
                    .get("select")
                    .map(super::trace_material::arguments)
                    .transpose()?,
                paths_per_target: limit("paths_per_target", 1, 8)?,
                max_states: limit("max_states", 4096, 32768)?,
                max_nodes: limit("max_nodes", 256, 4096)?,
                max_edges: limit("max_edges", 2048, 32768)?,
                max_depth: limit("max_depth", 128, 1024)?,
                direction: optional_string_field(s, "direction", "search.direction")?
                    .unwrap_or_default(),
                follow,
                relations: optional_string_array_field(s, "relations", "search.relations")?,
            })
        })
        .transpose()?;
    Ok((to, targets, search))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn dimensions_use_the_common_parser_and_name_their_search_field_in_errors() {
        let request = json!({"to":["t"],"search":{"dimensions":{"mode":"only","include":["env"]},"prefer_dimensions":{"selectors":[{"key":"env","op":"in","values":["prod"]}]}}});
        let (_, _, options) = arguments(&request).expect("parsed");
        let options = options.expect("search");
        assert_eq!(options.dimensions.expect("hard").include, ["env"]);
        assert_eq!(
            options.prefer_dimensions.expect("soft").selectors[0].values,
            ["prod"]
        );
        let bad = json!({"to":["t"],"search":{"prefer_dimensions":{"selectors":[{"key":"env","op":"in"}]}}});
        assert!(
            arguments(&bad)
                .expect_err("missing values")
                .starts_with("search.prefer_dimensions:")
        );
    }
}
