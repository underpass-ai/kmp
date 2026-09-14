use serde_json::{Value, json};

pub(super) fn schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "description": "Compact proof orientation over the whole selection, identical on every page. Items rank by shared_by descending, body_bytes descending, then ref; at most 8 bodies of at least 1024 UTF-8 bytes. The floor is a heuristic, not a savings guarantee. Counts are disjoint: valid and after_cut take precedence over below_floor; objects without a stored body are outside these counts. Read the canonical body before authoring a card; copy source and expect into kmp_condense with this about, ref, compact language and scope node_body. This is not fetched proof or a required next action. A card written now cannot help a past cutoff.",
        "required": ["items", "omitted_count", "below_floor", "valid", "after_cut"],
        "properties": {
            "items": {"type":"array", "maxItems":8, "items":{
                "type":"object", "additionalProperties":false,
                "required":["ref","body_bytes","record_bytes","card_status","shared_by","source","expect"],
                "properties":{
                    "ref":{"type":"string"},
                    "body_bytes":{"type":"integer","minimum":1024},
                    "record_bytes":{"type":"integer","minimum":0},
                    "card_status":{"type":"string","enum":["absent","stale"]},
                    "shared_by":{"type":"integer","minimum":0,"description":"Returned routes, or structurally complete seek groups, whose selected proof uses this ref, once per route/group."},
                    "source":{"type":"object","additionalProperties":false,"required":["revision","record_digest"],"properties":{
                        "revision":{"type":"integer","minimum":1},"record_digest":{"type":"string"}
                    }},
                    "expect":{"oneOf":[
                        {"type":"object","additionalProperties":false,"required":["absent"],"properties":{"absent":{"const":true}}},
                        {"type":"object","additionalProperties":false,"required":["card_revision"],"properties":{"card_revision":{"type":"integer","minimum":1}}}
                    ]}
                }
            }},
            "omitted_count":{"type":"integer","minimum":0},
            "below_floor":{"type":"integer","minimum":0},
            "valid":{"type":"integer","minimum":0},
            "after_cut":{"type":"integer","minimum":0}
        }
    })
}
