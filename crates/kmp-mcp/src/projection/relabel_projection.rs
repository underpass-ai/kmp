use serde_json::{Value, json};

use kmp_proto::v1beta1::{EntryLabel, RelabelResponse};

/// The kernel's relabel response as the backend hands it to the server:
/// what was added and removed, every label the memory stands in now, the
/// dimensions created and the labels that resemble one the about holds.
pub(crate) fn relabel_from_response(response: RelabelResponse) -> Value {
    let memory = response.memory.unwrap_or_default();
    json!({
        "summary": response.summary,
        "memory": {
            "about": memory.about,
            "ref": memory.r#ref,
            "added": labels_json(&memory.added),
            "removed": labels_json(&memory.removed),
            "labels": labels_json(&memory.labels),
            "created_dimensions": memory.created_dimensions,
            "resembling_labels": memory
                .resembling_labels
                .iter()
                .map(|label| {
                    json!({
                        "key": label.key,
                        "value": label.value,
                        "existing_key": label.existing_key,
                        "existing_value": label.existing_value,
                        "kind": label.kind,
                        "why": label.why
                    })
                })
                .collect::<Vec<_>>(),
            "read_after_write_ready": memory.read_after_write_ready
        },
        "warnings": response.warnings
    })
}

fn labels_json(labels: &[EntryLabel]) -> Vec<Value> {
    labels
        .iter()
        .map(|label| json!({"key": label.key, "value": label.value}))
        .collect()
}
