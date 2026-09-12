use super::memory_budget::MemoryBudgetMapper;
use super::temporal::TemporalOptionsMapper;
use super::trace_search::TraceSearchOptionsMapper;
use crate::contract::validator::{optional_string, required_string, validate_required_arguments};
use crate::projection::relation_cursor::RelationCursor;
use kmp_proto::v1beta1::TraceRequest;
use serde_json::Value;
pub(crate) struct TraceRequestMapper;

impl TraceRequestMapper {
    pub(crate) fn from_arguments(arguments: &Value) -> Result<TraceRequest, String> {
        validate_required_arguments(arguments, &["about", "from"])?;
        let (to, targets, search) = TraceSearchOptionsMapper::from_arguments(arguments)?;
        Ok(TraceRequest {
            targets,
            search,
            as_of: TemporalOptionsMapper::as_of_from_arguments(arguments)?,
            interval: TemporalOptionsMapper::interval_from_arguments(arguments)?,
            axis: TemporalOptionsMapper::temporal_axis_from_arguments(arguments)?,
            about: required_string(arguments, "about")?,
            from: required_string(arguments, "from")?,
            to,
            goal: optional_string(arguments, "goal")
                .or_else(|| optional_string(arguments, "role"))
                .unwrap_or_default(),
            budget: Some(MemoryBudgetMapper::from_arguments(arguments, 1600, 1)?),
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
