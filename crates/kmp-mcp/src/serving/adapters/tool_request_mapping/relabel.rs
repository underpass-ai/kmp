use super::json_fields::JsonFieldReader;
use kmp_proto::v1beta1::{EntryLabel, LabelPolicy, RelabelRequest};
use serde_json::Value;

use super::ingest::provenance_from_object;

/// The kernel's relabel request out of the backend arguments the planner
/// compiled: pairs, why, provenance and policy, never coordinates.
pub(crate) struct RelabelRequestMapper;

impl RelabelRequestMapper {
    pub(crate) fn from_arguments(arguments: &Value) -> Result<RelabelRequest, String> {
        let arguments = JsonFieldReader::object(arguments, "tool arguments")?;
        let label_policy = match arguments.get("label_policy").and_then(Value::as_str) {
            None | Some("warn") => LabelPolicy::Warn,
            Some("refuse") => LabelPolicy::Refuse,
            Some(other) => {
                return Err(format!(
                    "label_policy must be `warn` or `refuse`, not `{other}`"
                ));
            }
        };
        Ok(RelabelRequest {
            about: JsonFieldReader::required_string_field(arguments, "about", "about")?,
            r#ref: JsonFieldReader::required_string_field(arguments, "ref", "ref")?,
            add: labels_from_field(
                JsonFieldReader::optional_array_field(arguments, "add", "add")?,
                "add",
            )?,
            remove: labels_from_field(
                JsonFieldReader::optional_array_field(arguments, "remove", "remove")?,
                "remove",
            )?,
            why: JsonFieldReader::required_string_field(arguments, "why", "why")?,
            provenance: JsonFieldReader::optional_object_field(
                arguments,
                "provenance",
                "provenance",
            )?
            .map(provenance_from_object)
            .transpose()?,
            idempotency_key: JsonFieldReader::required_string_field(
                arguments,
                "idempotency_key",
                "idempotency_key",
            )?,
            dry_run: JsonFieldReader::optional_bool_field(arguments, "dry_run", "dry_run")?
                .unwrap_or(false),
            label_policy: label_policy as i32,
            intended_new: JsonFieldReader::optional_string_array_field(
                arguments,
                "intended_new",
                "intended_new",
            )?,
        })
    }
}

fn labels_from_field(values: &[Value], path: &str) -> Result<Vec<EntryLabel>, String> {
    values
        .iter()
        .map(|value| {
            let label = JsonFieldReader::object(value, &format!("{path}[]"))?;
            Ok(EntryLabel {
                key: JsonFieldReader::required_string_field(
                    label,
                    "key",
                    &format!("{path}[].key"),
                )?,
                value: JsonFieldReader::required_string_field(
                    label,
                    "value",
                    &format!("{path}[].value"),
                )?,
            })
        })
        .collect()
}
