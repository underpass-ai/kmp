use serde_json::{Value, json};

use kmp_proto::v1beta1::IngestResponse;

pub(crate) fn ingest_from_response(response: IngestResponse) -> Value {
    let memory = response.memory.as_ref();
    let accepted = memory.and_then(|memory| memory.accepted.as_ref());

    json!({
        "summary": response.summary,
        "memory": {
            "about": memory.map(|memory| memory.about.as_str()).unwrap_or(""),
            "memory_id": memory.map(|memory| memory.memory_id.as_str()).unwrap_or(""),
            "accepted": {
                "entries": accepted.map(|accepted| accepted.entries).unwrap_or_default(),
                "relations": accepted.map(|accepted| accepted.relations).unwrap_or_default(),
                "evidence": accepted.map(|accepted| accepted.evidence).unwrap_or_default()
            },
            "read_after_write_ready": memory
                .map(|memory| memory.read_after_write_ready)
                .unwrap_or(false),
            "created_dimensions": memory
                .map(|memory| memory.created_dimensions.clone())
                .unwrap_or_default(),
            "resembling_labels": memory
                .map(|memory| {
                    memory
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
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        },
        "warnings": response.warnings
    })
}
