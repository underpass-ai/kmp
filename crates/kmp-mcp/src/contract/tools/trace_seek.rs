use serde_json::{Value, json};

pub(super) fn roles_schema() -> Value {
    let step = json!({"oneOf":[{"type":"string","minLength":1},{"type":"object","additionalProperties":false,"required":["rel"],"properties":{
        "rel":{"type":"string","minLength":1},"direction":{"type":"string","enum":["outgoing","incoming"],"default":"outgoing"}}}]});
    json!({"type":"array","minItems":1,"maxItems":8,
    "description":"Find compatible paths for required evidence roles from from, without to. A relation string uses its name as role and outgoing direction. Each role follows via, its main rel from anchor to witness, then after. All roles are required; paths are alternatives. Labels bind the witness; same_ref can equate anchors or witnesses across roles, allowing unlabelled bridges. Replaces other search policies except work limits. Example: seek:[\"verified_by\",{\"name\":\"permission\",\"rel\":\"authorizes\",\"direction\":\"incoming\"}], same_labels:[\"event\"].",
    "items":{"oneOf":[{"type":"string","minLength":1},{"type":"object","additionalProperties":false,"required":["rel"],"properties":{
        "name":{"type":"string","minLength":1,"description":"Unique role name; defaults to rel. Required to distinguish repeated relations."},
        "rel":{"type":"string","minLength":1},
        "direction":{"type":"string","enum":["outgoing","incoming"],"default":"outgoing"},
        "via":{"oneOf":[{"type":"array","maxItems":1023,"items":step.clone()},{"const":"context"}],"description":"context discovers all minimum-hop prefixes to reachable relation origins through justified links in either direction at the same cut. It chooses intermediate sequences; rel/direction still declare the goal. These are leads requiring review, not inferred evidence for the seed. Longer prefixes to the same origin are omitted; ties survive. No label match creates an identity."},
        "after":{"type":"array","maxItems":1023,"items":step},
        "labels":{"type":"object","maxProperties":16,"additionalProperties":{"type":"array","minItems":1,"maxItems":64,"uniqueItems":true,"items":{"type":"string","minLength":1}},"description":"Allowed witness values per key. Missing membership remains unknown, never an inferred match."}
    }}]}})
}

pub(super) fn same_ref_schema() -> Value {
    json!({"type":"array","maxItems":8,"items":{"type":"array","minItems":2,"maxItems":16,"uniqueItems":true,
        "items":{"oneOf":[{"type":"string","minLength":1},
            {"type":"object","additionalProperties":false,"required":["role","at"],"properties":{
                "role":{"type":"string","minLength":1},"at":{"enum":["anchor","witness"]}}}]}},
        "description":"With seek, equate exact refs in each group. A role string means its main witness; {role,at:anchor} names the main relation traversal start after via/context. Incoming keeps stored arrows: its anchor is the stored target. Endpoints belong to one group; a role may join by anchor and witness separately. Only when the question requires the same action, equate those anchors. Verifying a copy and authorizing its publication concern different actions: do not add anchor equality. Do not remove a required equality merely to obtain a group. Same event labels alone do not identify an action. Identity needs its evidenced same_entity_as link."})
}
