use super::dimensions::DimensionSelectionMapper;
use super::json_fields::JsonFieldReader;
use super::temporal::TemporalOptionsMapper;
use kmp_proto::v1beta1::{MemoryBudget, PageRequest, ProjectVisualRequest, VisualLevelOfDetail};
use serde_json::Value;

pub(crate) struct VisualProjectionRequestMapper;

impl VisualProjectionRequestMapper {
    pub(crate) fn from_arguments(arguments: &Value) -> Result<ProjectVisualRequest, String> {
        let object = JsonFieldReader::object(arguments, "tool arguments")?;
        let level_of_detail =
            match JsonFieldReader::optional_string_field(object, "lod", "lod")?.as_deref() {
                None | Some("atlas") => VisualLevelOfDetail::Atlas,
                Some("episode") => VisualLevelOfDetail::Episode,
                Some("moment") => VisualLevelOfDetail::Moment,
                Some(value) => return Err(format!("argument `lod` has unknown level `{value}`")),
            };
        Ok(ProjectVisualRequest {
            about: JsonFieldReader::required_string_field(object, "about", "about")?,
            from: Some(JsonFieldReader::required_timestamp_field(
                object, "from", "from",
            )?),
            to: Some(JsonFieldReader::required_timestamp_field(
                object, "to", "to",
            )?),
            axis: TemporalOptionsMapper::temporal_axis_from_arguments(arguments)?,
            dimensions: DimensionSelectionMapper::from_arguments(arguments)?,
            level_of_detail: level_of_detail as i32,
            bin_count: JsonFieldReader::optional_positive_u32_field(object, "bins", "bins")?
                .unwrap_or(64),
            page: Some(PageRequest {
                entries: JsonFieldReader::optional_positive_u32_field(object, "limit", "limit")?
                    .unwrap_or(512),
                cursor: JsonFieldReader::optional_string_field(object, "cursor", "cursor")?
                    .unwrap_or_default(),
            }),
            budget: Some(MemoryBudget {
                depth: JsonFieldReader::optional_positive_u32_field(object, "depth", "depth")?
                    .unwrap_or(3),
                ..Default::default()
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn projection_request_requires_an_explicit_half_open_range() {
        let request = VisualProjectionRequestMapper::from_arguments(&json!({
            "about": "project:kmp",
            "from": "2026-08-01T00:00:00Z",
            "to": "2026-09-01T00:00:00Z",
            "axis": "ingested",
            "lod": "moment"
        }))
        .expect("visual projection request");
        assert_eq!(request.level_of_detail, VisualLevelOfDetail::Moment as i32);
        assert!(request.from.is_some());
        assert!(request.to.is_some());
    }
}
