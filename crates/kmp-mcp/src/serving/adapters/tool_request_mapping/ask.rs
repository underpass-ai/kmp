use super::answer_policy::AnswerPolicyMapper;
use super::dimensions::DimensionSelectionMapper;
use super::json_fields::JsonFieldReader;
use super::memory_budget::MemoryBudgetMapper;
use super::temporal::TemporalOptionsMapper;
use crate::contract::validator::{optional_string, required_string, validate_required_arguments};
use kmp_proto::v1beta1::AskRequest;
use serde_json::Value;
pub(crate) struct AskRequestMapper;

impl AskRequestMapper {
    pub(crate) fn from_arguments(arguments: &Value) -> Result<AskRequest, String> {
        validate_required_arguments(arguments, &["about", "question"])?;
        // `prefer` used to be rejected by name here. It is one of the keys the
        // schema already excludes, and the boundary now refuses every unknown key
        // rather than the one somebody remembered — the branch was only reachable
        // because the declared strictness was not applied.
        let arguments_object = JsonFieldReader::object(arguments, "tool arguments")?;
        Ok(AskRequest {
            about: required_string(arguments, "about")?,
            question: required_string(arguments, "question")?,
            asked_as: optional_string(arguments, "asked_as").unwrap_or_default(),
            answer_policy: AnswerPolicyMapper::from_object(arguments_object)?,
            budget: Some(MemoryBudgetMapper::from_arguments(arguments, 2400, 2)?),
            dimensions: DimensionSelectionMapper::from_arguments(arguments)?,
            page: TemporalOptionsMapper::page_from_arguments(arguments)?,
            as_of: TemporalOptionsMapper::as_of_from_arguments(arguments)?,
            interval: TemporalOptionsMapper::interval_from_arguments(arguments)?,
            axis: TemporalOptionsMapper::temporal_axis_from_arguments(arguments)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn ask_rejects_a_byte_budget_below_the_contract_floor() {
        let error = AskRequestMapper::from_arguments(&json!({
            "about": "project:kmp",
            "question": "What is current?",
            "budget": {"max_bytes": 511}
        }))
        .expect_err("sub-floor byte budget");

        assert!(error.contains("budget.max_bytes"));
        assert!(error.contains("at least 512"));
    }
}
