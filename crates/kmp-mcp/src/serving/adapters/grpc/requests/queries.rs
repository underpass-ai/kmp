use kmp_proto::v1beta1::{
    AskRequest, InspectRequest, TemporalMoveRequest, TemporalNearRequest, TraceRequest, WakeRequest,
};
use serde_json::Value;

use crate::contract::validator::{optional_string, required_string, validate_required_arguments};

use super::common::{answer_policy_from_object, memory_budget_from_arguments, object};
use super::dimensions::dimension_selection_from_arguments;
use super::temporal::{
    inspect_include_from_arguments, page_from_arguments, temporal_axis_from_arguments,
    temporal_cursor_from_arguments, temporal_include_from_arguments, temporal_limit_from_arguments,
    temporal_window_from_arguments,
};

pub(crate) fn wake_request_from_arguments(arguments: &Value) -> Result<WakeRequest, String> {
    validate_required_arguments(arguments, &["about"])?;
    Ok(WakeRequest {
        about: required_string(arguments, "about")?,
        role: optional_string(arguments, "role").unwrap_or_default(),
        intent: optional_string(arguments, "intent").unwrap_or_default(),
        budget: Some(memory_budget_from_arguments(arguments, 1600, 2)?),
        dimensions: dimension_selection_from_arguments(arguments)?,
        page: page_from_arguments(arguments)?,
    })
}

pub(crate) fn ask_request_from_arguments(arguments: &Value) -> Result<AskRequest, String> {
    validate_required_arguments(arguments, &["about", "question"])?;
    // `prefer` used to be rejected by name here. It is one of the keys the
    // schema already excludes, and the boundary now refuses every unknown key
    // rather than the one somebody remembered — the branch was only reachable
    // because the declared strictness was not applied.
    let arguments_object = object(arguments, "tool arguments")?;
    Ok(AskRequest {
        about: required_string(arguments, "about")?,
        question: required_string(arguments, "question")?,
        answer_policy: answer_policy_from_object(arguments_object)?,
        budget: Some(memory_budget_from_arguments(arguments, 2400, 2)?),
        dimensions: dimension_selection_from_arguments(arguments)?,
        page: page_from_arguments(arguments)?,
    })
}

pub(crate) fn temporal_move_request_from_arguments(
    arguments: &Value,
    direction: &str,
) -> Result<TemporalMoveRequest, String> {
    validate_required_arguments(arguments, &["about"])?;
    let cursor_key = match direction {
        "goto" => "at",
        "rewind" | "forward" => "from",
        _ => return Err(format!("unknown temporal direction `{direction}`")),
    };

    Ok(TemporalMoveRequest {
        about: required_string(arguments, "about")?,
        cursor: Some(temporal_cursor_from_arguments(arguments, cursor_key)?),
        dimensions: dimension_selection_from_arguments(arguments)?,
        window: temporal_window_from_arguments(arguments)?,
        limit: temporal_limit_from_arguments(arguments)?,
        include: temporal_include_from_arguments(arguments)?,
        budget: Some(memory_budget_from_arguments(arguments, 2400, 3)?),
        axis: temporal_axis_from_arguments(arguments)?,
    })
}

pub(crate) fn temporal_near_request_from_arguments(
    arguments: &Value,
) -> Result<TemporalNearRequest, String> {
    validate_required_arguments(arguments, &["about"])?;
    Ok(TemporalNearRequest {
        about: required_string(arguments, "about")?,
        around: Some(temporal_cursor_from_arguments(arguments, "around")?),
        dimensions: dimension_selection_from_arguments(arguments)?,
        window: temporal_window_from_arguments(arguments)?,
        limit: temporal_limit_from_arguments(arguments)?,
        include: temporal_include_from_arguments(arguments)?,
        budget: Some(memory_budget_from_arguments(arguments, 2400, 3)?),
        axis: temporal_axis_from_arguments(arguments)?,
    })
}

pub(crate) fn trace_request_from_arguments(arguments: &Value) -> Result<TraceRequest, String> {
    validate_required_arguments(arguments, &["about", "from", "to"])?;
    Ok(TraceRequest {
        about: required_string(arguments, "about")?,
        from: required_string(arguments, "from")?,
        to: required_string(arguments, "to")?,
        goal: optional_string(arguments, "goal")
            .or_else(|| optional_string(arguments, "role"))
            .unwrap_or_default(),
        budget: Some(memory_budget_from_arguments(arguments, 1600, 1)?),
        page: page_from_arguments(arguments)?,
    })
}

pub(crate) fn inspect_request_from_arguments(arguments: &Value) -> Result<InspectRequest, String> {
    validate_required_arguments(arguments, &["about", "ref"])?;
    Ok(InspectRequest {
        about: required_string(arguments, "about")?,
        r#ref: required_string(arguments, "ref")?,
        include: inspect_include_from_arguments(arguments)?,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn wake_and_ask_carry_byte_budget_and_recall_page_into_grpc() {
        let wake = wake_request_from_arguments(&json!({
            "about": "project:kmp",
            "budget": {"max_bytes": 8192},
            "page": {"entries": 7, "cursor": "kmp1:7:selection"}
        }))
        .expect("wake request");
        assert_eq!(wake.budget.expect("wake budget").max_bytes, 8192);
        assert_eq!(wake.page.expect("wake page").entries, 7);

        let ask = ask_request_from_arguments(&json!({
            "about": "project:kmp",
            "question": "What is current?",
            "budget": {"max_bytes": 4096},
            "page": {"entries": 3, "cursor": "kmp1:3:selection"}
        }))
        .expect("ask request");
        assert_eq!(ask.budget.expect("ask budget").max_bytes, 4096);
        assert_eq!(ask.page.expect("ask page").cursor, "kmp1:3:selection");
    }

    #[test]
    fn grpc_recall_rejects_a_byte_budget_below_the_contract_floor() {
        let error = ask_request_from_arguments(&json!({
            "about": "project:kmp",
            "question": "What is current?",
            "budget": {"max_bytes": 511}
        }))
        .expect_err("sub-floor byte budget");

        assert!(error.contains("budget.max_bytes"));
        assert!(error.contains("at least 512"));
    }

    #[test]
    fn temporal_request_carries_the_explicit_clock_axis() {
        let request = temporal_move_request_from_arguments(
            &json!({
                "about": "project:kmp",
                "axis": "validity",
                "at": {"time": "2026-08-27T12:00:00Z"}
            }),
            "goto",
        )
        .expect("valid temporal request");

        assert_eq!(
            request.axis,
            kmp_proto::v1beta1::TemporalAxis::Validity as i32
        );
    }
}
