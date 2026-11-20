use crate::contract::schema::definition::tool_definition_with_output;
use crate::guide::GuideTopic;
use serde_json::{Value, json};

pub(crate) fn definition() -> Value {
    let topics: Vec<&str> = GuideTopic::all().iter().map(|topic| topic.name).collect();
    tool_definition_with_output(
        "kmp_guide",
        false,
        "Explain one topic before its first use: a short worked card and the arguments of the tools it teaches. Without a topic, list the topics. A plain lookup that reads no store and records nothing.",
        json!({"type":"object","additionalProperties":false,"properties":{
            "topic":{"type":"string","enum":topics,"description":"The topic to explain. Omit to list the topics."}
        }}),
        json!({"type":"object","additionalProperties":false,"properties":{
            "summary":{"type":"string","description":"What was returned and how to read it."},
            "topics":{"type":"array","description":"Without a topic: every topic and what it teaches.","items":{"type":"object","properties":{"topic":{"type":"string"},"purpose":{"type":"string"}}}},
            "topic":{"type":"string","description":"The topic explained."},
            "ref":{"type":"string","description":"The same card's node in the stored agent guide, guide:kmp-agent."},
            "text":{"type":"string","description":"The worked card."},
            "parameters":{"type":"object","additionalProperties":{"type":"array","items":{"type":"string"}},"description":"Argument guide for each tool this topic teaches, one `path: description` line per argument it owns. tools/list advertises types and constraints only."},
            "relation_vocabulary":{"type":"array","items":{"type":"string"},"description":"Topic relations only: every writer relation as `name (quality; classes; when to use)`."}
        }}),
    )
}
