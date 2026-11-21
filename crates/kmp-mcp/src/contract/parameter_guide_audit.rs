//! The parameter prose moved from `tools/list` to `kmp_guide` (#850),
//! audited: only the short lines are advertised, none is lost, every argument
//! is served by exactly one topic, and each tool names its topic.
#![cfg(test)]

use serde_json::{Value, json};

use crate::contract::parameter_guide::{
    ADVERTISED_LINES, parameter_lines, topic_parameters, without_parameter_descriptions,
};
use crate::contract::registry::{advertised_tools_list, tools_list_result};
use crate::guide::GuideTopic;

/// Every `description` string anywhere in a schema.
fn schema_descriptions(schema: &Value, found: &mut Vec<String>) {
    match schema {
        Value::Object(object) => {
            if let Some(Value::String(text)) = object.get("description") {
                found.push(text.clone());
            }
            object
                .values()
                .for_each(|value| schema_descriptions(value, found));
        }
        Value::Array(items) => items
            .iter()
            .for_each(|item| schema_descriptions(item, found)),
        _ => {}
    }
}

#[test]
fn advertised_input_schemas_carry_only_the_short_lines() {
    let allowed: Vec<&str> = ADVERTISED_LINES.iter().map(|(_, _, line)| *line).collect();
    for apps in [false, true] {
        for output_schemas in [false, true] {
            let advertised = advertised_tools_list(apps, output_schemas);
            for tool in advertised["tools"].as_array().expect("tools") {
                let mut found = Vec::new();
                schema_descriptions(&tool["inputSchema"], &mut found);
                for text in &found {
                    assert!(
                        allowed.contains(&text.as_str()),
                        "{} advertises parameter prose: {text}",
                        tool["name"]
                    );
                }
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
    let advertised = advertised_tools_list(false, false);
    let writer = advertised["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "kmp_write_memory")
        .expect("writer");
    let properties = &writer["inputSchema"]["properties"];
    for path in ["about", "actor", "labels", "continuation"] {
        assert!(properties[path]["description"].is_string(), "{path}");
    }
    let record = &properties["memories"]["items"]["properties"];
    assert!(record["evidence"]["description"].is_string());
    assert!(record["labels"]["description"].is_string());
    assert!(record["summary"].get("description").is_none());
}

#[test]
fn every_model_facing_tool_has_a_topic_and_every_topic_names_live_tools() {
    let contract = tools_list_result();
    let names: Vec<&str> = contract["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("name"))
        .collect();
    for name in &names {
        assert!(GuideTopic::of_tool(name).is_some(), "{name} has no topic");
    }
    for topic in GuideTopic::all() {
        for tool in topic.tools() {
            assert!(names.contains(&tool), "{} names {tool}", topic.name);
        }
    }
}

#[test]
fn every_parameter_description_is_served_by_exactly_one_topic_card() {
    let contract = tools_list_result();
    let advertised = advertised_tools_list(false, false);
    let cards: Vec<(&str, Value)> = GuideTopic::all()
        .iter()
        .map(|topic| (topic.name, topic_parameters(topic)))
        .collect();
    for (tool, served) in contract["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .zip(advertised["tools"].as_array().expect("tools"))
    {
        let name = tool["name"].as_str().expect("name");
        let expected = parameter_lines(&tool["inputSchema"]);
        assert!(!expected.is_empty(), "{name} documents its arguments");
        for line in &expected {
            // A repeated description is served as `path: as <first path>.`
            let path = line.split(": ").next().expect("path");
            let owners: Vec<&str> = cards
                .iter()
                .filter(|(_, card)| {
                    card[name].as_array().is_some_and(|lines| {
                        lines.iter().any(|served| {
                            let served = served.as_str().expect("line");
                            served == line
                                || served.strip_prefix(path).is_some_and(|rest| {
                                    rest.starts_with(": as ") && rest.ends_with('.')
                                })
                        })
                    })
                })
                .map(|(topic, _)| *topic)
                .collect();
            assert_eq!(owners.len(), 1, "{name}: {line} served by {owners:?}");
        }
        let topic = GuideTopic::of_tool(name).expect("topic").name;
        let pointer = format!("Guide topic: {topic}.");
        let points = served["description"]
            .as_str()
            .is_some_and(|d| d.ends_with(&pointer));
        // A verb named like its topic is found by name; any other points.
        assert_eq!(
            points,
            name != format!("kmp_{topic}"),
            "{name} and {pointer}"
        );
    }
}

#[test]
fn the_write_and_relations_cards_split_the_writer() {
    let topic = |name| topic_parameters(GuideTopic::named(name).expect("topic"));
    let lines = |card: &Value, tool: &str| -> Vec<String> {
        card[tool]
            .as_array()
            .expect("tool listed")
            .iter()
            .map(|line| line.as_str().expect("line").to_string())
            .collect()
    };
    let write = lines(&topic("write"), "kmp_write_memory");
    assert!(write.iter().any(|l| l.starts_with("actor: ")));
    assert!(write.iter().any(|l| l.starts_with("memories[].evidence: ")));
    assert!(!write.iter().any(|l| l.starts_with("relations")));
    let relations = lines(&topic("relations"), "kmp_write_memory");
    assert!(relations.iter().any(|l| {
        l.starts_with("memories[].connect_to[].rel: ") && l.contains("kmp_guide topic relations")
    }));
    assert!(
        !relations
            .iter()
            .any(|l| l.starts_with("memories[].evidence"))
    );
    assert!(topic("ingest")["kmp_ingest"].is_array());
    assert!(topic("relabel")["kmp_relabel"].is_array());
    assert!(topic("time").get("kmp_write_memory").is_none());
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
            "set: Default: {\"description\":\"kept\"}.",
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
    // kmp_wake is named like its topic: no pointer.
    assert_eq!(stripped["tools"][0]["description"], "Wake.");
}
