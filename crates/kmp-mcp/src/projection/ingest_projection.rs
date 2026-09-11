use serde_json::{Value, json};

use kmp_proto::v1beta1::IngestResponse;

pub(crate) fn ingest_from_response(response: IngestResponse) -> Value {
    let memory = response.memory.as_ref();
    let accepted = memory.and_then(|memory| memory.accepted.as_ref());

    json!({
        "neighborhood": response.neighborhood.as_ref().map(|view| json!({
            "token": view.token, "eligible": view.eligible, "omitted": view.omitted,
            "omitted_conflicts": view.omitted_conflicts, "abouts": view.abouts, "partial": view.partial,
            "links": view.links.iter().map(|link| json!({"from":link.from,"rel":link.rel,"to":link.to})).collect::<Vec<_>>(),
            "items": view.items.iter().map(|item| json!({
                "about": item.about, "ref": item.r#ref, "state": item.state,
                "kind": item.kind, "reason": item.reason, "text": item.text,
                "text_omitted": item.text_omitted,
                "clocks": item.clocks.iter().map(|clock| (clock.axis.clone(), json!(clock.values))).collect::<serde_json::Map<String, Value>>(),
            })).collect::<Vec<_>>()
        })),
        "summary": response.summary,
        "memory": {
            "about": memory.map(|memory| memory.about.as_str()).unwrap_or(""),
            "memory_id": memory.map(|memory| memory.memory_id.as_str()).unwrap_or(""),
            "receipt_ref": memory.and_then(|memory| memory.receipt_ref.as_deref()),
            "replayed": memory.map(|memory| memory.replayed),
            "clocks": memory.and_then(|memory| memory.clocks.as_ref()).map(write_clocks_json),
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

fn write_clocks_json(value: &kmp_proto::v1beta1::WriteClocks) -> Value {
    fn clock(value: Option<&kmp_proto::v1beta1::WriteClockCoverage>) -> Value {
        value
            .map(|v| {
                json!({
                    "entries": v.entries, "distinct_values": v.distinct_values,
                    "single_value": v.single_value.map(|time| time.to_string())
                })
            })
            .unwrap_or(Value::Null)
    }
    json!({
        "scope": "accepted_command", "entries": value.entries,
        "relations": value.relations.as_ref().map(|r| json!({
            "relations": r.relations, "occurred": r.occurred, "observed": r.observed,
            "ingested": r.ingested, "valid_from": r.valid_from, "valid_until": r.valid_until
        })),
        "occurred": clock(value.occurred.as_ref()),
        "observed": clock(value.observed.as_ref()),
        "ingested": clock(value.ingested.as_ref()),
        "valid_from": clock(value.valid_from.as_ref()),
        "valid_until": clock(value.valid_until.as_ref())
    })
}
