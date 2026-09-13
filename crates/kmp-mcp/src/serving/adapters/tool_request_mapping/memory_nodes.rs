use super::json_fields::JsonFieldReader;
use kmp_proto::v1beta1::ReadNodesRequest;
use serde_json::Value;

pub(crate) struct MemoryNodesRequestMapper;
impl MemoryNodesRequestMapper {
    pub(crate) fn from_arguments(arguments: &Value) -> Result<ReadNodesRequest, String> {
        let object = JsonFieldReader::object(arguments, "tool arguments")?;
        let refs = object
            .get("refs")
            .and_then(Value::as_array)
            .ok_or("refs must be an array")?
            .iter()
            .map(|r| {
                r.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| "refs must contain strings".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ReadNodesRequest {
            expect_snapshot: JsonFieldReader::optional_string_field(
                object,
                "expect_snapshot",
                "expect_snapshot",
            )?
            .unwrap_or_default(),
            about: JsonFieldReader::required_string_field(object, "about", "about")?,
            refs,
            max_edges: JsonFieldReader::optional_positive_u32_field(
                object,
                "max_edges",
                "max_edges",
            )?
            .unwrap_or(2048),
        })
    }
}
