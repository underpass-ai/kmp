use crate::contract::schema::definition::tool_definition_with_output;
use serde_json::{Value, json};

pub(crate) fn definition() -> Value {
    tool_definition_with_output(
        "kmp_guide",
        "Open the brief capability map and persistent agent context. Start once with a unique registration_key; preserve the returned agent_id and context_id. Resume or expand a topic using context_id. After compaction use agent_id plus a new context_key, retaining the same agent. fold hides a topic without erasing its delivery record. This records served guidance, not understanding or authorization. Missing installed assets return explicit guide-sync repair instructions; preserve the selected store when applying them.",
        json!({"type":"object","additionalProperties":false,"properties":{
            "registration_key":{"type":"string","minLength":1,"description":"Unique logical agent registration. Reuse only to retry that registration; a different agent needs another key."},
            "context_id":{"type":"string","minLength":1,"description":"Exact active context returned by KMP; enough to resume that agent."},
            "agent_id":{"type":"string","minLength":1,"description":"Stable identity, used with context_key to start a fresh context after compaction or a task change."},
            "context_key":{"type":"string","minLength":1,"description":"Unique logical context reset for this agent; reuse on retry."},
            "topic":{"type":"string","enum":["wake","write","ask","time","audit","relate","view","guide"],"description":"Expand one small worked card; extended guidance remains linked."},
            "fold":{"type":"boolean","description":"Hide topic from the expanded scheme. Prior deliveries remain recorded."}
        }}),
        json!({"type":"object","additionalProperties":false,"properties":{
            "summary":{"type":"string","description":"Identity and next guide operation."},
            "agent":{"type":"object","additionalProperties":false,"required":["id","name"],"properties":{
                "id":{"type":"string","description":"Stable opaque identity; preserve across contexts."},
                "name":{"type":"string","description":"Random persistent display name; not an authorization credential."}
            }},
            "context_id":{"type":"string","description":"Exact token to resume this agent context."},
            "guide_revision":{"type":"string","description":"Content revision used for this scheme and its delivery records."},
            "guide_changed":{"type":"boolean","description":"This context previously used another guide revision; rediscover required topics."},
            "durable":{"type":"boolean","description":"Whether agent metadata survives a server restart."},
            "scheme":{"type":"array","description":"Available topics; expanded lists the currently opened ones.","items":{"type":"object","properties":{"topic":{"type":"string"},"purpose":{"type":"string"}}}},
            "expanded":{"type":"array","items":{"type":"string"},"description":"Topics opened in this context and revision, excluding folded ones."},
            "served":{"type":"array","items":{"type":"string"},"description":"Topic cards and exact guide/example refs delivered in this context and revision; not learned or retained."},
            "used":{"type":"array","description":"Observed work calls in this context and revision, never a learning score."},
            "card":{"type":["object","null"],"description":"One requested canonical worked card; null for the map or a fold.","properties":{"ref":{"type":"string"},"text":{"type":"string"}}},
            "next_actions":{"type":"array","description":"Complete optional calls for the extended verb."}
        }}),
    )
}
