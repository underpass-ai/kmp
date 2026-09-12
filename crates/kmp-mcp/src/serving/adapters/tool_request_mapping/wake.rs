use super::dimensions::DimensionSelectionMapper;
use super::memory_budget::MemoryBudgetMapper;
use super::temporal::TemporalOptionsMapper;
use crate::contract::validator::{optional_string, required_string, validate_required_arguments};
use kmp_proto::v1beta1::WakeRequest;
use serde_json::Value;
pub(crate) struct WakeRequestMapper;

impl WakeRequestMapper {
    pub(crate) fn from_arguments(arguments: &Value) -> Result<WakeRequest, String> {
        validate_required_arguments(arguments, &["about"])?;
        Ok(WakeRequest {
            about: required_string(arguments, "about")?,
            role: optional_string(arguments, "role").unwrap_or_default(),
            intent: optional_string(arguments, "intent").unwrap_or_default(),
            budget: Some(MemoryBudgetMapper::from_arguments(arguments, 1600, 2)?),
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
    use super::super::AskRequestMapper;
    use super::*;
    use serde_json::json;
    #[test]
    fn wake_and_ask_carry_byte_budget_and_recall_page_into_requests() {
        let wake = WakeRequestMapper::from_arguments(&json!({
            "about": "project:kmp",
            "budget": {"max_bytes": 8192},
            "page": {"entries": 7, "cursor": "kmp1:7:selection"}
        }))
        .expect("wake request");
        assert_eq!(wake.budget.expect("wake budget").max_bytes, 8192);
        assert_eq!(wake.page.expect("wake page").entries, 7);

        let ask = AskRequestMapper::from_arguments(&json!({
            "about": "project:kmp",
            "question": "What is current?",
            "budget": {"max_bytes": 4096},
            "page": {"entries": 3, "cursor": "kmp1:3:selection"}
        }))
        .expect("ask request");
        assert_eq!(ask.budget.expect("ask budget").max_bytes, 4096);
        assert_eq!(ask.page.expect("ask page").cursor, "kmp1:3:selection");
    }
}
