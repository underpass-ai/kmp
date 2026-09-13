use serde_json::{Value, json};

use crate::contract::schema::primitives::*;
pub(crate) fn view_output_schema() -> Value {
    output_object(json!({
        "view_id": described("string", "The view this answer is about."),
        "view_revision": described("integer", "The view's own revision, which is not the memory's."),
        "viewer_available": described("boolean", "Whether this MCP process mounted a ChronoLoom browser for the semantic view."),
        "state": {
            "type": "object",
            "additionalProperties": true,
            "description": "The semantic view state: about, clock, focus, projection, selection, trace, search, and the provenance of the last change."
        },
        "applied": described("boolean", "Whether this call is the one that moved the view. A replayed idempotency key answers false, and so does an intent none of whose named refs are in this store — `unhonored` then says so first and names every one of them."),
        "opened": described("boolean", "Whether a view was opened or rehydrated."),
        "unhonored": {
            "type": "array",
            "items": {"type": "string"},
            "description": "What this call could not honor, in the caller's words: parts of the intent this build cannot render yet, and the refs it named that this store does not hold. Absence is always named here and never silently drawn, so read it even when the call succeeded."
        },
        "clocks": {"type": "array", "items": {"type": "string"}, "description": "The clocks the axis can read."},
        "semantic_zoom_ladder": {"type": "array", "items": {"type": "string"}, "description": "The rungs, coarse to fine."},
        "reads": described("string", "What this answer is, in the reader's terms."),
        "observability": described("string", "How requested overlay series are queried and aligned on the view's temporal axis.")
    }))
}

pub(crate) fn view_output_with_url_schema() -> Value {
    let mut schema = view_output_schema();
    schema["properties"]["url"] = described(
        "string",
        "Loopback ChronoLoom URL carrying this session's capability. Present when this MCP process mounted the local viewer.",
    );
    schema
}
