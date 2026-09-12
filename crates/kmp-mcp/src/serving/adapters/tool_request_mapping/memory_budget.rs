use super::json_fields::JsonFieldReader;
use kmp_proto::v1beta1::{MemoryBudget, MemoryDetailLevel};
use serde_json::{Map, Value};

/// Maps the existing memory budget wire values.
pub(super) struct MemoryBudgetMapper;

impl MemoryBudgetMapper {
    pub(super) fn from_arguments(
        arguments: &Value,
        default_tokens: u32,
        default_depth: u32,
    ) -> Result<MemoryBudget, String> {
        let arguments = JsonFieldReader::object(arguments, "tool arguments")?;
        let budget = JsonFieldReader::optional_object_field(arguments, "budget", "budget")?;
        let tokens = budget
            .map(|budget| {
                JsonFieldReader::optional_positive_u32_field(budget, "tokens", "budget.tokens")
            })
            .transpose()?
            .flatten()
            .unwrap_or(default_tokens);
        let detail = budget
            .map(Self::detail_level_from_object)
            .transpose()?
            .unwrap_or(MemoryDetailLevel::Unspecified as i32);
        let depth = match JsonFieldReader::optional_positive_u32_field(arguments, "depth", "depth")?
        {
            Some(depth) => depth,
            None => budget
                .map(|budget| {
                    JsonFieldReader::optional_positive_u32_field(budget, "depth", "budget.depth")
                })
                .transpose()?
                .flatten()
                .unwrap_or(default_depth),
        };
        let max_entries = budget
            .map(|budget| {
                JsonFieldReader::optional_positive_u32_field(
                    budget,
                    "max_entries",
                    "budget.max_entries",
                )
            })
            .transpose()?
            .flatten()
            .unwrap_or(0);
        let max_bytes = budget
            .map(|budget| {
                JsonFieldReader::optional_u64_field(budget, "max_bytes", "budget.max_bytes")
            })
            .transpose()?
            .flatten()
            .unwrap_or(0);
        if max_bytes != 0 && max_bytes < 512 {
            return Err("argument `budget.max_bytes` must be at least 512".to_string());
        }

        Ok(MemoryBudget {
            tokens,
            detail,
            depth,
            max_entries,
            max_bytes,
        })
    }

    fn detail_level_from_object(value: &Map<String, Value>) -> Result<i32, String> {
        Ok(
            match JsonFieldReader::optional_string_field(value, "detail", "budget.detail")?
                .as_deref()
            {
                None => MemoryDetailLevel::Unspecified as i32,
                Some("compact") => MemoryDetailLevel::Compact as i32,
                Some("balanced") => MemoryDetailLevel::Balanced as i32,
                Some("full") => MemoryDetailLevel::Full as i32,
                Some(other) => return Err(format!("invalid budget.detail `{other}`")),
            },
        )
    }
}
