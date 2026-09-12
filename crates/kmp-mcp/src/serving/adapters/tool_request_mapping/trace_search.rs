use super::dimensions::DimensionSelectionMapper;
use super::json_fields::JsonFieldReader;
use super::trace_material::TraceMaterialOptionsMapper;
use super::trace_seek::TraceSeekOptionsMapper;
use kmp_proto::v1beta1::TraceSearchOptions;
use serde_json::Value;

/// Maps the existing trace search options at the MCP boundary.
pub(super) struct TraceSearchOptionsMapper;

impl TraceSearchOptionsMapper {
    pub(super) fn from_arguments(
        value: &Value,
    ) -> Result<(String, Vec<String>, Option<TraceSearchOptions>), String> {
        let root = JsonFieldReader::object(value, "trace")?;
        let seeking = value.get("search").and_then(|s| s.get("seek")).is_some();
        if let Some(search) = value.get("search") {
            TraceSeekOptionsMapper::validate_mode(
                JsonFieldReader::object(search, "search")?,
                root.contains_key("to"),
            )?;
        }
        let (to, targets) = match value.get("to") {
            None if seeking => (String::new(), vec![]),
            Some(Value::String(s)) if !s.trim().is_empty() => (s.trim().to_string(), vec![]),
            Some(Value::Array(_)) => {
                let targets = JsonFieldReader::optional_string_array_field(root, "to", "to")?;
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
                let s = JsonFieldReader::object(s, "search")?;
                let limit = |key: &str, default, max| -> Result<u32, String> {
                    let value =
                        JsonFieldReader::optional_u32_field(s, key, &format!("search.{key}"))?
                            .unwrap_or(default);
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
                            let step = JsonFieldReader::object(step, "search.follow[]")?;
                            let required = |key| {
                                JsonFieldReader::optional_string_field(
                                    step,
                                    key,
                                    "search.follow[]",
                                )?
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
                if !follow.is_empty()
                    && (s.contains_key("direction") || s.contains_key("relations"))
                {
                    return Err("search.follow replaces direction and relations".into());
                }
                let dimensions = |key: &str| {
                    s.get(key)
                        .map(|v| {
                            JsonFieldReader::object(v, &format!("search.{key}"))
                                .and_then(DimensionSelectionMapper::from_object)
                                .map_err(|e| format!("search.{key}: {e}"))
                        })
                        .transpose()
                };
                Ok::<_, String>(TraceSearchOptions {
                    proof: JsonFieldReader::optional_bool_field(s, "proof", "search.proof")?
                        .unwrap_or(false),
                    seek: TraceSeekOptionsMapper::from_arguments(s)?,
                    dimensions: dimensions("dimensions")?,
                    prefer_dimensions: dimensions("prefer_dimensions")?,
                    select: s
                        .get("select")
                        .map(TraceMaterialOptionsMapper::from_arguments)
                        .transpose()?,
                    paths_per_target: if seeking {
                        0
                    } else {
                        limit("paths_per_target", 1, 8)?
                    },
                    max_states: limit("max_states", 4096, 32768)?,
                    max_nodes: limit("max_nodes", 256, 4096)?,
                    max_edges: limit("max_edges", 2048, 32768)?,
                    max_depth: limit("max_depth", 128, 1024)?,
                    direction: JsonFieldReader::optional_string_field(
                        s,
                        "direction",
                        "search.direction",
                    )?
                    .unwrap_or_default(),
                    follow,
                    relations: JsonFieldReader::optional_string_array_field(
                        s,
                        "relations",
                        "search.relations",
                    )?,
                    max_body_record_bytes: s
                        .get("max_body_record_bytes")
                        .map(|value| {
                            value.as_u64().filter(|bytes| *bytes > 0).ok_or_else(|| {
                                "search.max_body_record_bytes is a positive byte ceiling; omit it \
                                 for the unbounded read"
                                    .to_string()
                            })
                        })
                        .transpose()?
                        .unwrap_or(0),
                    // Absent and empty are different requests: absent delivers
                    // every selected body, empty delivers descriptors only.
                    proof_refs: s
                        .get("proof_refs")
                        .map(|value| {
                            value
                                .as_array()
                                .ok_or_else(|| "search.proof_refs is an array of refs".to_string())
                                .and_then(|refs| {
                                    refs.iter()
                                        .map(|reference| {
                                            reference
                                                .as_str()
                                                .filter(|reference| !reference.trim().is_empty())
                                                .map(str::to_string)
                                                .ok_or_else(|| {
                                                    "search.proof_refs takes nonempty refs"
                                                        .to_string()
                                                })
                                        })
                                        .collect::<Result<Vec<_>, String>>()
                                })
                        })
                        .transpose()?
                        .map(|refs| kmp_proto::v1beta1::TraceBodyRefs { refs }),
                    expect_selection: JsonFieldReader::optional_string_field(
                        s,
                        "expect_selection",
                        "search.expect_selection",
                    )?
                    .unwrap_or_default(),
                    compact_language: s
                        .get("compact")
                        .map(|value| {
                            JsonFieldReader::object(value, "search.compact").and_then(|compact| {
                                JsonFieldReader::optional_string_field(
                                    compact,
                                    "language",
                                    "search.compact",
                                )?
                                .filter(|language| !language.trim().is_empty())
                                .ok_or_else(|| {
                                    "search.compact requires the language its cards were \
                                         written in"
                                        .to_string()
                                })
                            })
                        })
                        .transpose()?
                        .unwrap_or_default(),
                })
            })
            .transpose()?;
        Ok((to, targets, search))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn dimensions_use_the_common_parser_and_name_their_search_field_in_errors() {
        let request = json!({"to":["t"],"search":{"dimensions":{"mode":"only","include":["env"]},"prefer_dimensions":{"selectors":[{"key":"env","op":"in","values":["prod"]}]}}});
        let (_, _, options) = TraceSearchOptionsMapper::from_arguments(&request).expect("parsed");
        let options = options.expect("search");
        assert_eq!(options.dimensions.expect("hard").include, ["env"]);
        assert_eq!(
            options.prefer_dimensions.expect("soft").selectors[0].values,
            ["prod"]
        );
        let bad = json!({"to":["t"],"search":{"prefer_dimensions":{"selectors":[{"key":"env","op":"in"}]}}});
        assert!(
            TraceSearchOptionsMapper::from_arguments(&bad)
                .expect_err("missing values")
                .starts_with("search.prefer_dimensions:")
        );
    }
}

#[cfg(test)]
#[path = "trace_search_mode_tests.rs"]
mod mode_tests;
