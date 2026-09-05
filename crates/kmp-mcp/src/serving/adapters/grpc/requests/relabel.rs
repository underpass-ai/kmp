use kmp_proto::v1beta1::{EntryLabel, LabelPolicy, RelabelRequest};
use serde_json::Value;

use super::common::{
    object, optional_array_field, optional_bool_field, optional_object_field,
    optional_string_array_field, required_string_field,
};
use super::ingest::provenance_from_object;

/// The kernel's relabel request out of the backend arguments the planner
/// compiled: pairs, why, provenance and policy, never coordinates.
pub(crate) fn relabel_request_from_arguments(arguments: &Value) -> Result<RelabelRequest, String> {
    let arguments = object(arguments, "tool arguments")?;
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
        about: required_string_field(arguments, "about", "about")?,
        r#ref: required_string_field(arguments, "ref", "ref")?,
        add: labels_from_field(optional_array_field(arguments, "add", "add")?, "add")?,
        remove: labels_from_field(
            optional_array_field(arguments, "remove", "remove")?,
            "remove",
        )?,
        why: required_string_field(arguments, "why", "why")?,
        provenance: optional_object_field(arguments, "provenance", "provenance")?
            .map(provenance_from_object)
            .transpose()?,
        idempotency_key: required_string_field(arguments, "idempotency_key", "idempotency_key")?,
        dry_run: optional_bool_field(arguments, "dry_run", "dry_run")?.unwrap_or(false),
        label_policy: label_policy as i32,
        intended_new: optional_string_array_field(arguments, "intended_new", "intended_new")?,
    })
}

fn labels_from_field(values: &[Value], path: &str) -> Result<Vec<EntryLabel>, String> {
    values
        .iter()
        .map(|value| {
            let label = object(value, &format!("{path}[]"))?;
            Ok(EntryLabel {
                key: required_string_field(label, "key", &format!("{path}[].key"))?,
                value: required_string_field(label, "value", &format!("{path}[].value"))?,
            })
        })
        .collect()
}
