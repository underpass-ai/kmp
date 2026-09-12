use serde_json::{Value, json};

use crate::contract::schema::definition::tool_definition_with_output;
#[allow(unused_imports)]
use crate::contract::schema::primitives::*;
#[allow(unused_imports)]
use crate::contract::schema::request_shape::*;
#[allow(unused_imports)]
use crate::contract::schema::response_shape::*;

pub(crate) fn definition() -> Value {
    tool_definition_with_output(
        "kmp_condense",
        "Write your own compact card for one stored body, so later reads of the same paths can show the card instead of the whole body. Entries and the evidence sources of the same about can both be condensed, which matters because a source shared by several paths is usually the largest record on them. The card is your derived view, never canonical memory and never proof: it stands only for the body version you declare, and a later write makes it stale rather than wrong. Declare the revision and record digest the read gave you; a body that moved under you is refused with both versions named.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["about", "ref", "language", "scope", "card", "source", "expect"],
            "properties": {
                "about": string_schema("Memory anchor that owns the ref being condensed."),
                "ref": string_schema("Ref whose body this card stands for: an entry of this about, or one of its stored evidence sources. A ref of another about is refused."),
                "language": string_schema("Language tag the card is written in. Part of its identity: a card in another language neither replaces nor shadows this one."),
                "scope": {
                    "type": "string",
                    "const": "node_body",
                    "description": "The only admitted scope. A card declares one dependency — this ref's body — so it may not state anything a path, a neighborhood or a clock would be needed for. Nothing verifies the prose; the contract only fixes which body version it answers for."
                },
                "card": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 4096,
                    "description": "Nonempty text, at most 4096 UTF-8 bytes and strictly shorter in bytes than the canonical body. maxLength alone counts characters; the byte limit also applies."
                },
                "source": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["revision", "record_digest"],
                    "description": "Copy revision and record_digest from Trace with search.proof:true and a body option such as proof_refs:[]. Read that canonical body before writing; Inspect and legacy Trace do not expose this descriptor. Never construct a digest.",
                    "properties": {
                        "revision": {"type": "integer", "minimum": 1, "description": "Body revision the card was written from."},
                        "content_hash": string_schema("Public token of that body version, kept as provenance. Validity is not decided on it: a write can change the text without changing it."),
                        "record_digest": string_schema("Digest over the exact stored record of that body version. This is what a later read compares to decide whether the card still stands.")
                    }
                },
                "expect": {
                    "type": "object",
                    "additionalProperties": false,
                    "description": "Compare-and-set on the card itself, separate from the body. Exactly one of absent or card_revision.",
                    "oneOf": [
                        {"required": ["absent"]},
                        {"required": ["card_revision"]}
                    ],
                    "properties": {
                        "absent": {"type": "boolean", "const": true, "description": "No card is stored for this ref and language yet. Refused, naming the stored revision, if one is."},
                        "card_revision": {"type": "integer", "minimum": 1, "description": "The card revision you are replacing. Refused if another reader replaced it first."}
                    }
                },
                "actor": string_schema("Writer name recorded as the card's author. Defaults to the persistent agent name when context_id is supplied.")
            }
        }),
        condense_output_schema(),
    )
}

fn condense_output_schema() -> Value {
    output_object(json!({
        "summary": described("string", "Which card revision was stored, for which entry, language and body revision."),
        "ref": described("string", "Ref this card stands for."),
        "language": described("string", "Language of the stored card."),
        "card": output_object(json!({
            "status": described("string", "Always valid for a freshly written card: it describes the body version it was just checked against."),
            "text": described("string", "The stored card text."),
            "source_revision": described("integer", "Body revision this card stands for."),
            "source_content_hash": described("string", "Public token of that body version."),
            "source_record_digest": described("string", "Record digest this card is bound to."),
            "source_body_bytes": described("integer", "Canonical UTF-8 bytes of the body it stands in for."),
            "card_revision": described("integer", "This card's own revision. Declare it in expect to replace this card."),
            "authored_by": described("string", "Writer recorded as author."),
            "authored_at": described("string", "Kernel-stamped instant. A historical read at an earlier cut does not show this card."),
            "text_bytes": described("integer", "Bytes of the stored card text.")
        })),
        "warnings": warnings_output_schema()
    }))
}
