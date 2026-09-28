//! The parameter prose moved from `tools/list` to `kmp_guide` (#850),
//! audited: none is advertised, none is lost, and each tool names the topic
//! whose card now serves it.
#![cfg(test)]

use serde_json::{Value, json};

use crate::contract::parameter_guide::{
    parameter_lines, topic_parameters, without_parameter_descriptions,
};
use crate::contract::registry::{advertised_tools_list, tools_list_result};
use crate::guidance::{tool_topic, topic_tools};

fn has_schema_description(schema: &Value) -> bool {
    // A conservative scan: any `description` key anywhere in the schema.
    match schema {
        Value::Object(object) => {
            object.contains_key("description") || object.values().any(has_schema_description)
        }
        Value::Array(items) => items.iter().any(has_schema_description),
        _ => false,
    }
}

#[test]
fn no_advertised_input_schema_carries_parameter_prose() {
    for apps in [false, true] {
        for output_schemas in [false, true] {
            let advertised = advertised_tools_list(apps, output_schemas);
            for tool in advertised["tools"].as_array().expect("tools") {
                assert!(
                    !has_schema_description(&tool["inputSchema"]),
                    "{} still advertises parameter prose (apps={apps})",
                    tool["name"]
                );
                assert!(
                    tool["description"]
                        .as_str()
                        .is_some_and(|d| !d.trim().is_empty()),
                    "{} keeps its one-line description",
                    tool["name"]
                );
            }
        }
    }
}

#[test]
fn every_model_facing_tool_is_taught_by_exactly_one_topic() {
    let scheme = crate::guidance::scheme();
    let topics: Vec<&str> = scheme
        .as_array()
        .expect("scheme")
        .iter()
        .map(|entry| entry["topic"].as_str().expect("topic"))
        .collect();
    let contract = tools_list_result();
    for tool in contract["tools"].as_array().expect("tools") {
        let name = tool["name"].as_str().expect("name");
        let owners: Vec<&&str> = topics
            .iter()
            .filter(|topic| topic_tools(topic).contains(&name))
            .collect();
        assert_eq!(owners.len(), 1, "{name} is taught by {owners:?}");
        assert_eq!(tool_topic(name), Some(*owners[0]));
    }
}

#[test]
fn every_parameter_description_is_served_by_its_topic_card() {
    let contract = tools_list_result();
    let advertised = advertised_tools_list(false, false);
    for (tool, served) in contract["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .zip(advertised["tools"].as_array().expect("tools"))
    {
        let name = tool["name"].as_str().expect("name");
        let topic = tool_topic(name).expect("model-facing tool has a topic");
        let card = topic_parameters(topic);
        let lines = card[name].as_array().expect("tool listed in its card");
        let expected = parameter_lines(&tool["inputSchema"]);
        assert!(!expected.is_empty(), "{name} documents its arguments");
        assert_eq!(lines.len(), expected.len(), "{name}");
        for line in &expected {
            assert!(lines.contains(&json!(line)), "{name}: {line}");
        }
        let pointer = format!("kmp_guide topic {topic}");
        assert!(
            served["description"]
                .as_str()
                .is_some_and(|d| d.contains(&pointer)),
            "{name} points at {pointer}"
        );
    }
}

#[test]
fn the_write_card_explains_relation_choice_and_the_actor_default() {
    let write = topic_parameters("write");
    let writer = write["kmp_write_memory"].as_array().expect("writer lines");
    assert!(writer.iter().any(|line| {
        line.as_str().is_some_and(|l| {
            l.starts_with("memories[].connect_to[].rel: ") && l.contains("kmp_guide topic write")
        })
    }));
    assert!(
        writer
            .iter()
            .any(|line| line.as_str().is_some_and(|l| l.starts_with("actor: ")))
    );
    assert!(write["kmp_ingest"].is_array() && write["kmp_relabel"].is_array());
    assert!(topic_parameters("time").get("kmp_write_memory").is_none());
}

#[test]
fn only_schema_annotations_are_stripped_never_data_named_description() {
    let result = json!({"tools":[{"name":"kmp_wake","description":"Wake.","inputSchema":{
        "type":"object","description":"root",
        "properties":{
            "description":{"type":"string","description":"a field called description"},
            "items":{"type":"array","items":{"type":"object","description":"row",
                "properties":{"x":{"type":"string","description":"x"}}}},
            "choice":{"anyOf":[{"type":"string","description":"text"},{"type":"null"}]},
            "set":{"type":"object","default":{"description":"kept"}}
        }
    }}]});
    let schema = result["tools"][0]["inputSchema"].clone();
    assert_eq!(
        parameter_lines(&schema),
        vec![
            "(arguments): root",
            "choice: text",
            "description: a field called description",
            "items[]: row",
            "items[].x: x",
        ]
    );
    let stripped = without_parameter_descriptions(result);
    let schema = &stripped["tools"][0]["inputSchema"];
    assert!(schema["properties"]["description"].is_object());
    assert_eq!(
        schema["properties"]["set"]["default"]["description"],
        "kept"
    );
    assert!(schema.get("description").is_none());
    assert!(
        schema["properties"]["items"]["items"]["properties"]["x"]
            .get("description")
            .is_none()
    );
    assert_eq!(
        stripped["tools"][0]["description"],
        "Wake. Arguments explained: kmp_guide topic wake."
    );
}
