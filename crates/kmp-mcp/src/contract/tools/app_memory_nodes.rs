use crate::contract::handshake::CHRONOLOOM_APP_URI;
use serde_json::{Value, json};

pub(crate) fn definition() -> Value {
    json!({
        "name":"kmp_view_read_nodes",
        "description":"Read bounded node headers and coordinates from one snapshot for ChronoLoom framing. No canonical bodies or evidence are loaded. Available only to the MCP App.",
        "inputSchema": {
            "type":"object", "additionalProperties":false, "required":["about","refs"],
            "properties":{
                "about":{"type":"string","minLength":1},
                "refs":{"type":"array","minItems":1,"maxItems":64,"items":{"type":"string","minLength":1}},
                "expect_snapshot":{"type":"string","minLength":1},
                "max_edges":{"type":"integer","minimum":1,"maximum":32768,"default":2048}
            }
        },
        "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false},
        "_meta":{"ui":{"resourceUri":CHRONOLOOM_APP_URI,"visibility":["app"]}}
    })
}
