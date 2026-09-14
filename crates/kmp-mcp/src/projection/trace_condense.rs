use kmp_proto::v1beta1::{TraceCondenseCandidates, trace_condense_candidate::Expect};
use serde_json::{Value, json};

pub(super) fn project(candidates: &TraceCondenseCandidates) -> Value {
    json!({
        "items": candidates.items.iter().map(|candidate| json!({
            "ref": candidate.r#ref,
            "body_bytes": candidate.body_bytes,
            "record_bytes": candidate.record_bytes,
            "card_status": candidate.card_status,
            "shared_by": candidate.shared_by,
            "source": {
                "revision": candidate.source_revision,
                "record_digest": candidate.source_record_digest
            },
            "expect": match candidate.expect {
                Some(Expect::Absent(true)) => json!({"absent": true}),
                Some(Expect::CardRevision(revision)) => json!({"card_revision": revision}),
                _ => Value::Null,
            }
        })).collect::<Vec<_>>(),
        "omitted_count": candidates.omitted_count,
        "below_floor": candidates.below_floor,
        "valid": candidates.valid,
        "after_cut": candidates.after_cut
    })
}
