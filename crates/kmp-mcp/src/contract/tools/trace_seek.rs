use serde_json::{Value, json};

pub(super) fn roles_schema() -> Value {
    let step = json!({"oneOf":[{"type":"string","minLength":1},{"type":"object","additionalProperties":false,"required":["rel"],"properties":{
        "rel":{"type":"string","minLength":1},"direction":{"type":"string","enum":["outgoing","incoming"],"default":"outgoing"}}}]});
    json!({"type":"array","minItems":1,"maxItems":8,
    "description":"Find all compatible paths for these required evidence roles from from, without to. A relation string uses its name as role and outgoing direction. Each role follows ordered via moves, its main rel to the witness, then ordered after moves. All roles are required; alternative paths are OR. Bind labels and same_ref at the main witness only, allowing unlabelled bridges. Replaces other search policies except work limits. Example: seek:[\"verified_by\",{\"name\":\"permission\",\"rel\":\"authorizes\",\"direction\":\"incoming\"}], same_labels:[\"event\"].",
    "items":{"oneOf":[{"type":"string","minLength":1},{"type":"object","additionalProperties":false,"required":["rel"],"properties":{
        "name":{"type":"string","minLength":1,"description":"Unique role name; defaults to rel. Required to distinguish repeated relations."},
        "rel":{"type":"string","minLength":1},
        "direction":{"type":"string","enum":["outgoing","incoming"],"default":"outgoing"},
        "via":{"type":"array","maxItems":1023,"items":step.clone()},
        "after":{"type":"array","maxItems":1023,"items":step},
        "labels":{"type":"object","maxProperties":16,"additionalProperties":{"type":"array","minItems":1,"maxItems":64,"uniqueItems":true,"items":{"type":"string","minLength":1}},"description":"Allowed witness values per key. Missing membership remains unknown, never an inferred match."}
    }}]}})
}
