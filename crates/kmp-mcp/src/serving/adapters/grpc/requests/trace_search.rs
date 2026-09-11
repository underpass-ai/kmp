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
            Ok::<_, String>(TraceSearchOptions {
                max_nodes: limit("max_nodes", 256, 4096)?,
                max_edges: limit("max_edges", 2048, 32768)?,
                max_depth: limit("max_depth", 128, 1024)?,
                direction: optional_string_field(s, "direction", "search.direction")?
                    .unwrap_or_else(|| "outgoing".into()),
                relations: optional_string_array_field(s, "relations", "search.relations")?,
            })
        })
        .transpose()?;
    Ok((to, targets, search))
}
