use super::temporal::TemporalOptionsMapper;
use crate::contract::validator::{required_string, validate_required_arguments};
use kmp_proto::v1beta1::InspectRequest;
use serde_json::Value;
pub(crate) struct InspectRequestMapper;

impl InspectRequestMapper {
    pub(crate) fn from_arguments(arguments: &Value) -> Result<InspectRequest, String> {
        validate_required_arguments(arguments, &["about", "ref"])?;
        Ok(InspectRequest {
            about: required_string(arguments, "about")?,
            r#ref: required_string(arguments, "ref")?,
            include: TemporalOptionsMapper::inspect_include_from_arguments(arguments)?,
            expect_revision: expect_revision_from_arguments(arguments)?,
        })
    }
}

/// The positive body revision declared by a canonical expansion; zero means current.
fn expect_revision_from_arguments(arguments: &Value) -> Result<u64, String> {
    let Some(expect) = arguments.get("expect") else {
        return Ok(0);
    };
    let revision = expect
        .as_object()
        .and_then(|expect| expect.get("revision"))
        .and_then(Value::as_u64)
        .filter(|revision| *revision > 0)
        .ok_or_else(|| {
            "expect.revision must be the positive body revision this expansion declares".to_string()
        })?;
    Ok(revision)
}
