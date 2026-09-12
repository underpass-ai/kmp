use super::dimensions::DimensionSelectionMapper;
use super::memory_budget::MemoryBudgetMapper;
use super::temporal::TemporalOptionsMapper;
use crate::contract::validator::{required_string, validate_required_arguments};
use kmp_proto::v1beta1::TemporalNearRequest;
use serde_json::Value;
pub(crate) struct NearRequestMapper;

impl NearRequestMapper {
    pub(crate) fn from_arguments(arguments: &Value) -> Result<TemporalNearRequest, String> {
        validate_required_arguments(arguments, &["about"])?;
        Ok(TemporalNearRequest {
            entry_selection: TemporalOptionsMapper::entry_selection_from_arguments(arguments)?,
            interval: TemporalOptionsMapper::interval_from_arguments(arguments)?,
            about: required_string(arguments, "about")?,
            around: Some(TemporalOptionsMapper::temporal_cursor_from_arguments(
                arguments, "around",
            )?),
            dimensions: DimensionSelectionMapper::from_arguments(arguments)?,
            window: TemporalOptionsMapper::temporal_window_from_arguments(arguments)?,
            limit: TemporalOptionsMapper::temporal_limit_from_arguments(arguments)?,
            include: TemporalOptionsMapper::temporal_include_from_arguments(arguments)?,
            budget: Some(MemoryBudgetMapper::from_arguments(arguments, 2400, 3)?),
            axis: TemporalOptionsMapper::temporal_axis_from_arguments(arguments)?,
        })
    }
}
