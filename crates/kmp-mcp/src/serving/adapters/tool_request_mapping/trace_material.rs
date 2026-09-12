use super::json_fields::JsonFieldReader;
use kmp_proto::v1beta1::{
    TraceMaterialSelectionOptions, TraceProofAlternative, TraceProofRequirement,
};
use serde_json::Value;

/// Maps the existing trace material options at the MCP boundary.
pub(super) struct TraceMaterialOptionsMapper;

impl TraceMaterialOptionsMapper {
    pub(super) fn from_arguments(value: &Value) -> Result<TraceMaterialSelectionOptions, String> {
        let s = JsonFieldReader::object(value, "search.select")?;
        let bounded = |key: &str, default: Option<u32>, max: u32| {
            let v = JsonFieldReader::optional_u32_field(s, key, &format!("search.select.{key}"))?
                .or(default)
                .ok_or_else(|| format!("search.select.{key} is required"))?;
            if v == 0 || v > max {
                Err(format!("search.select.{key} must be 1..{max}"))
            } else {
                Ok(v)
            }
        };
        let groups = match s.get("groups") {
            None => vec![],
            Some(Value::Array(groups)) if groups.len() <= 8 => groups
                .iter()
                .map(|group| {
                    let g = JsonFieldReader::object(group, "search.select.groups[]")?;
                    let weight = JsonFieldReader::optional_u32_field(
                        g,
                        "weight",
                        "search.select.groups[].weight",
                    )?
                    .unwrap_or(1);
                    if !(1..=1000).contains(&weight) {
                        return Err("group weight must be 1..1000".into());
                    }
                    let Some(Value::Array(alternatives)) = g.get("alternatives") else {
                        return Err("group alternatives must be an array of ref arrays".into());
                    };
                    if alternatives.is_empty() || alternatives.len() > 8 {
                        return Err("group requires 1..8 alternatives".into());
                    }
                    let alternatives = alternatives
                        .iter()
                        .map(|alt| {
                            let Value::Array(refs) = alt else {
                                return Err("each alternative must be a ref array".into());
                            };
                            if refs.is_empty() || refs.len() > 8 {
                                return Err("each alternative requires 1..8 refs from to".into());
                            }
                            let refs = refs
                                .iter()
                                .map(|r| {
                                    r.as_str()
                                        .filter(|s| !s.trim().is_empty())
                                        .map(str::to_string)
                                        .ok_or_else(|| {
                                            "alternative requires nonempty string refs".to_string()
                                        })
                                })
                                .collect::<Result<Vec<_>, _>>()?;
                            if refs.iter().collect::<std::collections::BTreeSet<_>>().len()
                                != refs.len()
                            {
                                return Err("each alternative requires distinct refs".into());
                            }
                            Ok(TraceProofAlternative { refs })
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    Ok(TraceProofRequirement {
                        alternatives,
                        weight,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
            _ => return Err("search.select.groups must be an array of at most 8 groups".into()),
        };
        Ok(TraceMaterialSelectionOptions {
            max_material_nodes: bounded("max_material_nodes", None, 4096)?,
            max_paths: bounded("max_paths", Some(4), 8)?,
            groups,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn trace_material_parser_keeps_and_or_requirements_and_rejects_invalid_shapes() {
        let valid = TraceMaterialOptionsMapper::from_arguments(
            &json!({"max_material_nodes":4,"groups":[{"alternatives":[["a","b"],["c"]]}]}),
        )
        .expect("valid");
        assert_eq!(valid.max_paths, 4);
        assert_eq!(valid.groups[0].weight, 1);
        assert_eq!(valid.groups[0].alternatives[0].refs, ["a", "b"]);
        for input in [
            json!({}),
            json!({"max_material_nodes":0}),
            json!({"max_material_nodes":4097}),
            json!({"max_material_nodes":4,"max_paths":9}),
            json!({"max_material_nodes":4,"groups":null}),
            json!({"max_material_nodes":4,"groups":[{"alternatives":[[]]}]}),
            json!({"max_material_nodes":4,"groups":[{"alternatives":[["a","a"]]}]}),
            json!({"max_material_nodes":4,"groups":[{"weight":0,"alternatives":[["a"]]}]}),
        ] {
            assert!(
                TraceMaterialOptionsMapper::from_arguments(&input).is_err(),
                "accepted {input}"
            );
        }
    }
}
