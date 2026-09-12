use super::dimensions::DimensionSelectionMapper;
use super::memory_budget::MemoryBudgetMapper;
use super::temporal::TemporalOptionsMapper;
use crate::contract::validator::{required_string, validate_required_arguments};
use crate::projection::relation_cursor::RelationCursor;
use kmp_proto::v1beta1::RelateRequest;
use serde_json::Value;
pub(crate) struct RelateRequestMapper;

impl RelateRequestMapper {
    pub(crate) fn from_arguments(arguments: &Value) -> Result<RelateRequest, String> {
        validate_required_arguments(arguments, &["about"])?;
        Ok(RelateRequest {
            about: required_string(arguments, "about")?,
            dimensions: DimensionSelectionMapper::from_arguments(arguments)?,
            interval: TemporalOptionsMapper::interval_from_arguments(arguments)?,
            axis: TemporalOptionsMapper::temporal_axis_from_arguments(arguments)?,
            budget: Some(MemoryBudgetMapper::from_arguments(arguments, 2400, 2)?),
            page: {
                let mut page = TemporalOptionsMapper::page_from_arguments(arguments)?;
                if let Some(page) = &mut page
                    && !page.cursor.is_empty()
                {
                    page.cursor = RelationCursor::kernel_position(&page.cursor)?;
                }
                page
            },
        })
    }
}
