use super::json_fields::JsonFieldReader;
use kmp_proto::v1beta1::AnswerPolicy;
use serde_json::{Map, Value};

/// Maps the existing answer policy wire values.
pub(super) struct AnswerPolicyMapper;

impl AnswerPolicyMapper {
    pub(super) fn from_object(arguments: &Map<String, Value>) -> Result<i32, String> {
        Ok(
            match JsonFieldReader::optional_string_field(
                arguments,
                "answer_policy",
                "answer_policy",
            )?
            .as_deref()
            {
                None | Some("evidence_or_unknown") => AnswerPolicy::EvidenceOrUnknown as i32,
                Some("show_conflicts") => AnswerPolicy::ShowConflicts as i32,
                Some("best_effort") => AnswerPolicy::BestEffort as i32,
                Some(other) => return Err(format!("invalid answer_policy `{other}`")),
            },
        )
    }
}
