//! The `paths` facet of a view intent, as the agent asked for it.
//!
//! An agent does not hand the loom a path: it names where a path starts and,
//! optionally, where it must arrive. The dispatch then runs the same
//! `kmp_curate` `mode: paths` search the agent could run itself and the view
//! keeps the answer. What this parses is the request alone, so a retried
//! intent digests as the same intent whatever the search found the first
//! time.

use kmp_viewer::PathsDto;
use serde_json::Value;

use crate::serving::ToolError;

const MAX_HOPS: u64 = 12;

/// The requested paths: `None` leaves the facet alone, `Some(None)` clears
/// it, `Some(Some(_))` asks for a search. Every end it names joins `refs`,
/// so the store is asked whether it holds them before anything is searched.
pub(crate) fn requested_paths(
    arguments: &Value,
    refs: &mut Vec<String>,
) -> Result<Option<Option<PathsDto>>, ToolError> {
    let Some(paths) = arguments.get("paths") else {
        return Ok(None);
    };
    if paths.is_null() {
        return Ok(Some(None));
    }
    let Some(from) = paths
        .get("from")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|from| !from.is_empty())
    else {
        return Err(ToolError::invalid_argument(
            "paths needs `from`: the fact every chain starts from",
        ));
    };
    let to = match paths.get("to") {
        None | Some(Value::Null) => None,
        Some(Value::String(to)) if !to.trim().is_empty() => Some(to.trim().to_string()),
        Some(_) => {
            return Err(ToolError::invalid_argument(
                "paths.to is the ref a chain must reach, or omitted",
            ));
        }
    };
    let max_hops = match paths.get("max_hops") {
        None | Some(Value::Null) => None,
        Some(value) => match value.as_u64() {
            Some(hops @ 1..=MAX_HOPS) => Some(hops as u8),
            _ => {
                return Err(ToolError::invalid_argument(
                    "paths.max_hops is a whole number from 1 to 12",
                ));
            }
        },
    };
    refs.push(from.to_string());
    if let Some(to) = to.as_ref() {
        refs.push(to.clone());
    }
    Ok(Some(Some(PathsDto {
        from: from.to_string(),
        to,
        max_hops,
        ..PathsDto::default()
    })))
}

#[cfg(test)]
mod tests {
    use super::requested_paths;
    use serde_json::json;

    #[test]
    fn a_request_names_its_ends_and_nothing_found_yet() {
        let mut refs = Vec::new();
        let asked = requested_paths(
            &json!({"paths": {"from": "a:1", "to": "b:2", "max_hops": 4}}),
            &mut refs,
        )
        .expect("valid")
        .expect("touches paths")
        .expect("asks for a search");
        assert_eq!(refs, ["a:1", "b:2"]);
        assert_eq!(asked.max_hops, Some(4));
        assert!(asked.paths.is_empty() && asked.review_token.is_none());
    }

    #[test]
    fn null_clears_absence_leaves_alone_and_shapes_are_refused() {
        let mut refs = Vec::new();
        assert_eq!(requested_paths(&json!({}), &mut refs).expect("ok"), None);
        assert_eq!(
            requested_paths(&json!({"paths": null}), &mut refs).expect("ok"),
            Some(None)
        );
        for wrong in [
            json!({"paths": {}}),
            json!({"paths": {"from": "a", "to": 3}}),
            json!({"paths": {"from": "a", "max_hops": 40}}),
        ] {
            assert!(requested_paths(&wrong, &mut refs).is_err(), "{wrong}");
        }
        assert!(refs.is_empty());
    }
}
