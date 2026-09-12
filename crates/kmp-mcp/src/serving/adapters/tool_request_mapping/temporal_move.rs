use super::dimensions::DimensionSelectionMapper;
use super::memory_budget::MemoryBudgetMapper;
use super::temporal::TemporalOptionsMapper;
use crate::contract::validator::{required_string, validate_required_arguments};
use kmp_proto::v1beta1::TemporalMoveRequest;
use serde_json::Value;
pub(crate) struct TemporalMoveRequestMapper;

impl TemporalMoveRequestMapper {
    pub(crate) fn from_arguments(
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
            entry_selection: TemporalOptionsMapper::entry_selection_from_arguments(arguments)?,
            interval: TemporalOptionsMapper::interval_from_arguments(arguments)?,
            about: required_string(arguments, "about")?,
            cursor: arguments
                .get(cursor_key)
                .map(|_| {
                    TemporalOptionsMapper::temporal_cursor_from_arguments(arguments, cursor_key)
                })
                .transpose()?,
            dimensions: DimensionSelectionMapper::from_arguments(arguments)?,
            window: TemporalOptionsMapper::temporal_window_from_arguments(arguments)?,
            limit: TemporalOptionsMapper::temporal_limit_from_arguments(arguments)?,
            include: TemporalOptionsMapper::temporal_include_from_arguments(arguments)?,
            budget: Some(MemoryBudgetMapper::from_arguments(arguments, 2400, 3)?),
            axis: TemporalOptionsMapper::temporal_axis_from_arguments(arguments)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn temporal_request_carries_the_explicit_clock_axis() {
        let request = TemporalMoveRequestMapper::from_arguments(
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
