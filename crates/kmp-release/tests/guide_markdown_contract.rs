use std::path::Path;

use kmp_release::application::dto::guide_request_document_dto::GuideRequestDocumentDto;
use kmp_release::application::mappers::agent_guide_markdown_mapper::AgentGuideMarkdownMapper;
use serde_json::Value;

#[test]
fn shipped_entry_indexes_every_live_tool_without_loading_extended_bodies() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/kmp/guide");
    let bodies: Vec<Value> = serde_json::from_str(
        &std::fs::read_to_string(root.join("guide.requests.json")).expect("requests"),
    )
    .expect("valid requests");
    let requests = bodies
        .into_iter()
        .map(|body| GuideRequestDocumentDto { body })
        .collect::<Vec<_>>();
    let markdown = std::fs::read_to_string(root.join("AGENT.md")).expect("shipped Markdown");
    assert_eq!(
        AgentGuideMarkdownMapper::map(&requests).expect("render"),
        markdown
    );
    assert!(
        markdown.len() < 10000,
        "the entry is an index, not another manual"
    );
    let guide = requests
        .iter()
        .find(|r| r.about() == "guide:kmp-agent")
        .expect("agent");
    let entries = guide.body["memory"]["entries"].as_array().expect("entries");
    let rows = markdown
        .lines()
        .filter_map(|line| {
            line.strip_prefix("| ")
                .and_then(|line| line.rsplit_once(" | `"))
        })
        .map(|(_, tail)| tail.strip_suffix("` |").expect("exact ref column"))
        .collect::<Vec<_>>();
    for entry in entries {
        let node_ref = entry["id"].as_str().expect("ref");
        let indexed = entry["metadata"]["guide_title"].is_string()
            || entry["metadata"]["example_title"].is_string();
        assert_eq!(
            rows.iter().filter(|r| **r == node_ref).count(),
            usize::from(indexed)
        );
        let body = entry["text"].as_str().expect("body").trim();
        assert_eq!(
            markdown.contains(body),
            node_ref == "guide:kmp-agent:overview",
            "body in entry: {node_ref}"
        );
    }
    assert!(
        rows.iter().all(|r| entries.iter().any(|e| e["id"] == *r)),
        "no invented guide ref"
    );
    let tools = entries
        .iter()
        .filter(|e| e["metadata"]["tool_name"].is_string())
        .collect::<Vec<_>>();
    assert_eq!(tools.len(), 15);
    for tool in tools {
        let reference = tool["metadata"]["guide_ref"]
            .as_str()
            .expect("extended guide mapping");
        assert!(rows.contains(&reference));
        assert!(markdown.contains(tool["metadata"]["tool_name"].as_str().expect("tool name")));
    }
    let json = markdown
        .split("```json\n")
        .nth(1)
        .expect("native read")
        .split("\n```")
        .next()
        .expect("end");
    let call: Value = serde_json::from_str(json).expect("native arguments");
    assert_eq!(call["about"], guide.about());
    assert!(rows.contains(&call["ref"].as_str().expect("real ref")));
    for key in ["details", "incoming", "outgoing"] {
        assert_eq!(call["include"][key], false);
    }
    let skill =
        std::fs::read_to_string(root.join("../skills/kmp-memory/SKILL.md")).expect("router");
    assert!(skill.contains("../../guide/AGENT.md"));
    assert!(
        skill.len() < 2500,
        "the skill must not grow a second manual"
    );
}
