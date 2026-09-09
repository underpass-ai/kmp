//! Callable recall continuations and lossless request arguments at the boundary.
use super::*;
use kmp_proto::v1beta1::{RecallCall, TemporalAxis, TemporalInterval};

pub(super) fn call(arguments: &Value, cursor: Option<&str>, max_bytes: Option<usize>) -> Value {
    let mut arguments = arguments.clone();
    if !arguments.is_object() {
        arguments = json!({});
    }
    if let Some(cursor) = cursor {
        arguments["page"]["cursor"] = json!(cursor);
    } else if let Some(page) = arguments.get_mut("page").and_then(Value::as_object_mut) {
        page.remove("cursor");
    }
    if let Some(max_bytes) = max_bytes {
        arguments["budget"]["max_bytes"] = json!(max_bytes);
    }
    json!({"tool": if arguments.get("question").is_some() { "kmp_ask" } else { "kmp_wake" }, "arguments": arguments})
}

pub(super) fn call_from_value(value: &Value) -> RecallCall {
    RecallCall {
        tool: string_at(value, "/tool"),
        arguments_json: value["arguments"].to_string(),
    }
}

pub(super) fn call_value(call: &RecallCall) -> Value {
    json!({"tool":call.tool,"arguments":serde_json::from_str::<Value>(&call.arguments_json).expect("projection emits valid JSON arguments")})
}

pub(super) fn insert_time(
    arguments: &mut Map<String, Value>,
    axis: i32,
    as_of: Option<&TemporalCursor>,
    interval: Option<&TemporalInterval>,
) {
    if axis != TemporalAxis::Unspecified as i32 {
        arguments.insert("axis".into(), json!(temporal_axis_label(axis)));
    }
    if let Some(as_of) = as_of {
        let mut value = Map::new();
        if let Some(time) = as_of.time {
            value.insert("time".into(), json!(time.to_string()));
        }
        insert_non_empty(&mut value, "ref", &as_of.r#ref);
        arguments.insert("as_of".into(), Value::Object(value));
    }
    if let Some(interval) = interval {
        let mut value = Map::new();
        if let Some(start) = interval.start {
            value.insert("start".into(), json!(start.to_string()));
        }
        if let Some(end) = interval.end {
            value.insert("end".into(), json!(end.to_string()));
        }
        arguments.insert("interval".into(), Value::Object(value));
    }
}

// One conservative sizing pass, independent of the requested ceiling. Include
// full core, the largest expansion and the worst metadata/action envelope, so
// accepting the proposed budget can advance without another guessed allowance.
pub(super) fn progress_bytes(
    plan: &ProjectionPlan,
    hash: &str,
    budget: &ProjectionBudget,
) -> usize {
    let mut value = plan.core.clone();
    if let Some(item) = plan
        .items
        .iter()
        .max_by_key(|item| serialized_bytes(&item.value))
    {
        push_array(&mut value, item.section.path(), item.value.clone());
    }
    let planning = ProjectionBudget {
        byte_limit: usize::MAX,
        token_limit: u32::MAX,
        detail: Detail::Balanced,
        page_entries: usize::MAX,
        ..*budget
    };
    attach_metadata(
        &mut value,
        plan,
        &plan.items,
        &[],
        0,
        plan.items.len(),
        hash,
        &planning,
        false,
        true,
    );
    serialized_bytes(&value)
}
