use serde_json::{Value, json};

use kmp_proto::v1beta1::{CondenseResponse, NodeCardView};

pub(crate) fn condense_from_response(response: CondenseResponse) -> Value {
    json!({
        "summary": response.summary,
        "ref": response.r#ref,
        "language": response.language,
        "card": response.card.map(card_json),
        "warnings": response.warnings
    })
}

/// A card on the wire.
///
/// `text` is emitted only for a valid card. Every other state reaches this
/// function with an empty string because the domain never filled it, and this
/// function has no branch that could put prose back into a stale, absent or
/// post-cut card.
pub(crate) fn card_json(card: NodeCardView) -> Value {
    let mut value = json!({
        "status": card.status,
        "source_revision": card.source_revision,
        "source_content_hash": card.source_content_hash,
        "source_record_digest": card.source_record_digest,
        "source_body_bytes": card.source_body_bytes,
        "card_revision": card.card_revision,
        "authored_by": card.authored_by,
        "authored_at": card.authored_at,
        "text_bytes": card.text_bytes
    });
    if !card.text.is_empty() {
        value["text"] = json!(card.text);
    }
    value
}
