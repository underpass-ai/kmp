use serde_json::{Value, json};

use crate::contract::schema::definition::tool_definition_with_output;
#[allow(unused_imports)]
use crate::contract::schema::primitives::*;
#[allow(unused_imports)]
use crate::contract::schema::relation_vocabulary::*;
#[allow(unused_imports)]
use crate::contract::schema::request_shape::*;
#[allow(unused_imports)]
use crate::contract::schema::response_shape::*;
#[allow(clippy::unused_unit)]
pub(crate) fn definition() -> Value {
    tool_definition_with_output(
        "kmp_relabel",
        "Change the labels one memory stands in without rewriting its text: labels to add, labels to take off, and why. The kernel reads the memory's coordinates and the about's catalogue itself, so name pairs, never coordinates. A label added late inherits the memory's clocks — its time does not move — and its own instant lives on the edge it added, as `method: kmp_relabel` with your `why`; the event log keeps who did it when. Refused, naming what the memory stands in: a label it already stands in, one it does not, a value already used under another key in the about, and taking its last label off. A new label that resembles one the catalogue holds is refused under strict and written with a warning otherwise, unless its key is in `options.labels_new`. Normal calls commit; `options.dry_run` validates against the store and writes nothing.",
        relabel_schema(),
        relabel_output_schema(),
    )
}

pub(crate) fn relabel_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["about", "ref", "actor", "observed_at", "why"],
        "properties": {
            "about": string_schema("The about the memory belongs to."),
            "ref": string_schema("The memory to relabel: an entry ref inside this about, as a read returned it, byte for byte. Never the about anchor, an evidence id or a dimension id."),
            "actor": string_schema("Human, agent, or component making the change."),
            "observed_at": string_schema("RFC3339 timestamp in UTC for the change's provenance. It is kept on the edge each added label makes and in the event log; it never becomes the memory's time. A stamp more than five minutes ahead of the kernel's clock is refused."),
            "source_kind": {
                "type": "string",
                "enum": ["human", "agent", "projection", "derived"]
            },
            "add": {
                "type": "object",
                "additionalProperties": string_schema("The scope id the memory now stands in under that key."),
                "description": "Labels to add, `key: value`. A key is lowercase letters, digits, `_`, `.` or `-`, starting with a letter; a value is a scope id. Read the `labels` catalogue `kmp_wake` returned and reuse what exists: a value already used under another key in this about is refused, naming both. A label the memory already stands in is refused too."
            },
            "remove": {
                "type": "object",
                "additionalProperties": string_schema("The scope id to take the memory out of under that key."),
                "description": "Labels to take off, `key: value`, as `kmp_inspect` with `include.raw` lists them on the memory (the `dimension` is the key, the bare `scope_id` the value). A label the memory does not stand in is refused, naming the ones it does; so is taking off its last label, since a memory stands in at least one."
            },
            "why": string_schema("Why the labels change, in one sentence a later reader can check. Kept with the change and on every edge it adds."),
            "idempotency_key": string_schema("Optional stable idempotency key. Omit to generate one from the arguments."),
            "options": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "dry_run": {
                        "type": "boolean",
                        "description": "When true, validate the relabel against the store — the memory, its labels, the catalogue — and write nothing. Defaults to false: the call commits."
                    },
                    "labels_new": {
                        "type": "array",
                        "items": string_schema("A label key of this relabel."),
                        "description": "Label keys you insist are new even where the catalogue holds one that resembles them: you read the catalogue and mean something else. Every other new label that resembles an existing one is refused under strict and written with a warning otherwise."
                    },
                    "strict": {
                        "type": "boolean",
                        "description": "Defaults to true: a new label that resembles one the about holds is refused, naming both. Set false to write it and be told in `labels.resembling`."
                    }
                }
            }
        }
    })
}

fn relabel_output_schema() -> Value {
    output_object(json!({
        "accepted": described("boolean", "True when the kernel committed the change; false on a dry run."),
        "dry_run": described("boolean", "Whether this call only validated."),
        "summary": described("string", "What changed and how many labels the memory stands in now."),
        "ref": described("string", "The memory relabelled."),
        "labels": {
            "type": "object",
            "additionalProperties": false,
            "description": "The labels as pairs `{ key, value }`: `added` and `removed` by this call, `now` every label the memory stands in afterwards, `created` the added labels the about had never held — vocabulary growing, so make sure it was meant — and `resembling` the added labels that resemble one the about already holds, each with why.",
            "properties": {
                "added": described("array", "Labels added, `{ key, value }`."),
                "removed": described("array", "Labels taken off, `{ key, value }`."),
                "now": described("array", "Every label the memory stands in after the change, by key then value."),
                "created": described("array", "Added labels the about had never held before."),
                "resembling": described("array", "Added labels that resemble one the about holds: `key`, `value`, `existing_key`, `existing_value`, `kind` and `why`.")
            }
        },
        "warnings": warnings_output_schema(),
        "next_suggested_reads": described("array", "The reads that show the change: `kmp_inspect` on the memory with `include.raw`.")
    }))
}
