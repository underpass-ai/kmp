use serde_json::{Value, json};

use crate::application::dto::guide_request_document_dto::GuideRequestDocumentDto;
use crate::domain::release_error::ReleaseError;

/// Render the entry and an exact index; keep extended bodies in guide memory.
pub struct AgentGuideMarkdownMapper;

impl AgentGuideMarkdownMapper {
    pub fn map(requests: &[GuideRequestDocumentDto]) -> Result<String, ReleaseError> {
        let request = requests
            .iter()
            .find(|request| request.about() == "guide:kmp-agent")
            .ok_or_else(|| ReleaseError::invalid("agent guide is missing"))?;
        let entries = request.body["memory"]["entries"]
            .as_array()
            .ok_or_else(|| ReleaseError::invalid("agent guide entries are missing"))?;
        let overview = entries
            .iter()
            .find(|entry| entry["id"] == "guide:kmp-agent:overview")
            .ok_or_else(|| ReleaseError::invalid("agent overview is missing"))?;
        let body = overview["text"]
            .as_str()
            .ok_or_else(|| ReleaseError::invalid("agent overview has no text"))?;
        let mut text = format!(
            "# KMP agent entry\n\nGuide version: `{}`. Asset key: `{}`.\n\n{}\n\n",
            overview["metadata"]["guide_version"]
                .as_str()
                .unwrap_or_default(),
            request.body["idempotency_key"].as_str().unwrap_or_default(),
            body.trim()
        );
        text.push_str("## First use of a verb\n\nRead its row with `kmp_inspect` if that guidance is absent from context. Reuse it afterwards. Exact refs in `guide:kmp-agent`:\n\n| Use / tools | Ref |\n| --- | --- |\n");
        for entry in entries {
            if let Some(title) = entry["metadata"]["guide_title"].as_str() {
                let tool_names = entries
                    .iter()
                    .filter(|tool| tool["metadata"]["guide_ref"] == entry["id"])
                    .filter_map(|tool| tool["metadata"]["tool_name"].as_str())
                    .collect::<Vec<_>>();
                if tool_names.is_empty() {
                    continue;
                }
                let label = format!("{title}: `{}`", tool_names.join("`, `"));
                Self::row(&mut text, &label, entry)?;
            }
        }
        for tool in entries
            .iter()
            .filter(|entry| entry["metadata"]["tool_name"].is_string())
        {
            if !entries.iter().any(|entry| {
                entry["id"] == tool["metadata"]["guide_ref"]
                    && entry["metadata"]["guide_title"].is_string()
            }) {
                return Err(ReleaseError::invalid(
                    "a live tool has no indexed extended guide",
                ));
            }
        }
        text.push_str("\n## Examples on demand\n\nSelect a relevant lesson and its prerequisites from these maps; do not load every lesson.\n\n| Map | Ref |\n| --- | --- |\n");
        for (id, title) in [
            ("guide:kmp-agent:examples:index", "Cases and prerequisites"),
            (
                "guide:kmp-agent:examples:by-capability",
                "By tool, memory kind or relation",
            ),
        ] {
            let entry = entries
                .iter()
                .find(|entry| entry["id"] == id)
                .ok_or_else(|| ReleaseError::invalid("agent example map is missing"))?;
            Self::row(&mut text, title, entry)?;
        }
        let wake = entries
            .iter()
            .find(|entry| entry["metadata"]["tool_name"] == "kmp_wake")
            .ok_or_else(|| ReleaseError::invalid("wake tool reference is missing"))?;
        let arguments = json!({
            "about": request.about(), "ref": wake["metadata"]["guide_ref"],
            "include": {"details": false, "incoming": false, "outgoing": false},
            "budget": {"max_bytes": 10000}
        });
        let arguments = serde_json::to_string_pretty(&arguments).map_err(|error| {
            ReleaseError::invalid(format!("could not encode guide read: {error}"))
        })?;
        text.push_str(&format!("\n## Read a selected node\n\nExample `kmp_inspect` call; change only `ref` for another row:\n\n```json\n{arguments}\n```\n\nThe lesson is in `object.text`; adjacent links stay unexpanded. Execute returned `next_actions` when partial. Missing ref: check store and guide version before syncing matching assets. The human guide is `guide:kmp` in ChronoLoom.\n\nGenerated; edit the editorial source, not this file.\n"));
        Ok(text)
    }

    fn row(text: &mut String, label: &str, entry: &Value) -> Result<(), ReleaseError> {
        let node_ref = entry["id"]
            .as_str()
            .ok_or_else(|| ReleaseError::invalid("indexed guide entry has no ref"))?;
        text.push_str(&format!("| {label} | `{node_ref}` |\n"));
        Ok(())
    }
}
