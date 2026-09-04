use kmp_proto::v1beta1::{
    InspectInclude, PageRequest, TemporalAxis, TemporalCursor, TemporalInclude, TemporalInterval,
    TemporalLimit, TemporalWindow,
};

pub(super) fn temporal_axis_from_arguments(arguments: &Value) -> Result<i32, String> {
    let arguments = object(arguments, "tool arguments")?;
    let axis = optional_string_field(arguments, "axis", "axis")?;
    Ok(match axis.as_deref() {
        None => TemporalAxis::Unspecified,
        Some("occurred") => TemporalAxis::Occurred,
        Some("observed") => TemporalAxis::Observed,
        Some("ingested") => TemporalAxis::Ingested,
        Some("validity") => TemporalAxis::Validity,
        Some(value) => {
            return Err(format!(
                "temporal axis must be one of `occurred`, `observed`, `ingested`, or `validity`; got `{value}`"
            ));
        }
    } as i32)
}
use serde_json::Value;

use super::common::{
    object, optional_bool_field, optional_object_field, optional_positive_u32_field,
    optional_string_field, optional_timestamp_field, optional_u32_field, required_object_field,
};

pub(super) fn temporal_cursor_from_arguments(
    arguments: &Value,
    cursor_key: &str,
) -> Result<TemporalCursor, String> {
    let arguments = object(arguments, "tool arguments")?;
    let cursor = required_object_field(arguments, cursor_key, cursor_key)?;
    let ref_value = optional_string_field(cursor, "ref", &format!("{cursor_key}.ref"))?;
    let time = optional_timestamp_field(cursor, "time", &format!("{cursor_key}.time"))?;
    let sequence =
        optional_positive_u32_field(cursor, "sequence", &format!("{cursor_key}.sequence"))?;
    let present = [
        ref_value
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty()),
        time.is_some(),
        sequence.is_some(),
    ]
    .into_iter()
    .filter(|value| *value)
    .count();

    if present != 1 {
        return Err(format!(
            "temporal cursor `{cursor_key}` requires exactly one of `ref`, `time`, or `sequence`"
        ));
    }

    Ok(TemporalCursor {
        r#ref: ref_value.unwrap_or_default(),
        time,
        sequence,
    })
}

pub(super) fn temporal_window_from_arguments(
    arguments: &Value,
) -> Result<Option<TemporalWindow>, String> {
    let Some(window) =
        optional_object_field(object(arguments, "tool arguments")?, "window", "window")?
    else {
        return Ok(None);
    };
    if window.contains_key("before_seconds") || window.contains_key("after_seconds") {
        return Err(
            "temporal window seconds are not supported by KernelMemoryService in this cut"
                .to_string(),
        );
    }
    Ok(Some(TemporalWindow {
        before_entries: optional_u32_field(window, "before_entries", "window.before_entries")?
            .unwrap_or_default(),
        after_entries: optional_u32_field(window, "after_entries", "window.after_entries")?
            .unwrap_or_default(),
    }))
}

pub(super) fn temporal_limit_from_arguments(
    arguments: &Value,
) -> Result<Option<TemporalLimit>, String> {
    let Some(limit) =
        optional_object_field(object(arguments, "tool arguments")?, "limit", "limit")?
    else {
        return Ok(None);
    };
    Ok(Some(TemporalLimit {
        entries: optional_positive_u32_field(limit, "entries", "limit.entries")?
            .unwrap_or_default(),
        tokens: optional_positive_u32_field(limit, "tokens", "limit.tokens")?.unwrap_or_default(),
    }))
}

pub(super) fn temporal_include_from_arguments(
    arguments: &Value,
) -> Result<Option<TemporalInclude>, String> {
    let Some(include) =
        optional_object_field(object(arguments, "tool arguments")?, "include", "include")?
    else {
        return Ok(None);
    };
    let raw_refs = optional_bool_field(include, "raw_refs", "include.raw_refs")?.unwrap_or(false);
    Ok(Some(TemporalInclude {
        evidence: optional_bool_field(include, "evidence", "include.evidence")?.unwrap_or(false),
        relations: optional_bool_field(include, "relations", "include.relations")?.unwrap_or(false),
        raw_refs,
    }))
}

pub(super) fn page_from_arguments(arguments: &Value) -> Result<Option<PageRequest>, String> {
    let Some(page) = optional_object_field(object(arguments, "tool arguments")?, "page", "page")?
    else {
        return Ok(None);
    };
    Ok(Some(PageRequest {
        entries: optional_positive_u32_field(page, "entries", "page.entries")?.unwrap_or_default(),
        cursor: optional_string_field(page, "cursor", "page.cursor")?.unwrap_or_default(),
    }))
}

pub(super) fn inspect_include_from_arguments(
    arguments: &Value,
) -> Result<Option<InspectInclude>, String> {
    let Some(include) =
        optional_object_field(object(arguments, "tool arguments")?, "include", "include")?
    else {
        return Ok(None);
    };
    let raw = optional_bool_field(include, "raw", "include.raw")?.unwrap_or(false);
    Ok(Some(InspectInclude {
        incoming: optional_bool_field(include, "incoming", "include.incoming")?.unwrap_or(true),
        outgoing: optional_bool_field(include, "outgoing", "include.outgoing")?.unwrap_or(true),
        details: optional_bool_field(include, "details", "include.details")?.unwrap_or(true),
        raw,
    }))
}

/// The instant a recall stands at, when the caller names one: `as_of` with
/// a `ref` or a `time`. A sequence is relative to one dimension and names
/// no instant, so it is refused here with the reason.
pub(super) fn as_of_from_arguments(arguments: &Value) -> Result<Option<TemporalCursor>, String> {
    let arguments = object(arguments, "tool arguments")?;
    let Some(cursor) = optional_object_field(arguments, "as_of", "as_of")? else {
        return Ok(None);
    };
    let ref_value =
        optional_string_field(cursor, "ref", "as_of.ref")?.filter(|value| !value.trim().is_empty());
    let time = optional_timestamp_field(cursor, "time", "as_of.time")?;
    if cursor.contains_key("sequence") {
        return Err(
            "as_of takes `ref` or `time`; a sequence is relative to one dimension and names no \
             instant"
                .to_string(),
        );
    }
    match (ref_value, time) {
        (Some(ref_value), None) => Ok(Some(TemporalCursor {
            r#ref: ref_value,
            time: None,
            sequence: None,
        })),
        (None, Some(time)) => Ok(Some(TemporalCursor {
            r#ref: String::new(),
            time: Some(time),
            sequence: None,
        })),
        _ => Err("as_of requires exactly one of `ref` or `time`".to_string()),
    }
}

/// The half-open span a recall stands within, when the caller names one:
/// `interval` with `start` (inclusive), `end` (exclusive), or both.
pub(super) fn interval_from_arguments(
    arguments: &Value,
) -> Result<Option<TemporalInterval>, String> {
    let arguments = object(arguments, "tool arguments")?;
    let Some(interval) = optional_object_field(arguments, "interval", "interval")? else {
        return Ok(None);
    };
    let start = optional_timestamp_field(interval, "start", "interval.start")?;
    let end = optional_timestamp_field(interval, "end", "interval.end")?;
    if start.is_none() && end.is_none() {
        return Err("interval needs a `start`, an `end`, or both".to_string());
    }
    Ok(Some(TemporalInterval { start, end }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn temporal_cursor_requires_exactly_one_position() {
        let error = temporal_cursor_from_arguments(
            &json!({
                "from": {
                    "ref": "claim:1",
                    "sequence": 1
                }
            }),
            "from",
        )
        .expect_err("ambiguous cursor should fail");

        assert_eq!(
            error,
            "temporal cursor `from` requires exactly one of `ref`, `time`, or `sequence`"
        );
    }

    #[test]
    fn temporal_window_rejects_unsupported_seconds_bounds() {
        let error = temporal_window_from_arguments(&json!({
            "window": {
                "before_seconds": 60,
                "after_entries": 2
            }
        }))
        .expect_err("seconds window bounds are not in the typed gRPC contract");

        assert_eq!(
            error,
            "temporal window seconds are not supported by KernelMemoryService in this cut"
        );
    }

    #[test]
    fn temporal_axis_is_explicit_and_closed() {
        assert_eq!(
            temporal_axis_from_arguments(&json!({"axis": "ingested"})).expect("known axis"),
            TemporalAxis::Ingested as i32
        );
        assert!(temporal_axis_from_arguments(&json!({"axis": "effective"})).is_err());
    }

    #[test]
    fn inspect_include_only_narrows_fields_named_by_the_caller() {
        assert!(
            inspect_include_from_arguments(&json!({}))
                .expect("an absent include is valid")
                .is_none()
        );

        let include = inspect_include_from_arguments(&json!({
            "include": {"details": false}
        }))
        .expect("valid inspect include")
        .expect("include was supplied");

        assert!(include.incoming);
        assert!(include.outgoing);
        assert!(!include.details);
        assert!(!include.raw);
    }
}
