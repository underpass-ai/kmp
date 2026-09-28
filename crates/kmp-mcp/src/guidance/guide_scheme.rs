use serde_json::{Value, json};

pub(crate) const TOPICS: [(&str, &str); 10] = [
    ("wake", "Resume known work"),
    ("write", "Record separate facts with evidence and labels"),
    ("ask", "Retrieve semantic evidence or stop at UNKNOWN"),
    ("time", "Navigate history with the right clock"),
    ("audit", "Inspect a claim or trace its proof"),
    ("condense", "Write a compact card for one stored body"),
    ("relate", "Compare explicitly selected abouts"),
    ("curate", "Find missing and doubtful relations with Jev"),
    ("view", "Frame memory in ChronoLoom"),
    ("guide", "Resume identity and expand or fold this scheme"),
];

/// The tools each topic teaches. A topic card serves their argument prose
/// (`card.parameters`), which `tools/list` no longer carries (#850). Every
/// model-facing tool belongs to exactly one topic.
pub(crate) const TOPIC_TOOLS: [(&str, &[&str]); 10] = [
    ("wake", &["kmp_wake"]),
    ("write", &["kmp_write_memory", "kmp_ingest", "kmp_relabel"]),
    ("ask", &["kmp_ask"]),
    ("time", &["kmp_time"]),
    ("audit", &["kmp_inspect", "kmp_trace"]),
    ("condense", &["kmp_condense", "kmp_summaries_audit"]),
    ("relate", &["kmp_relate"]),
    ("curate", &["kmp_curate"]),
    (
        "view",
        &[
            "kmp_view_open",
            "kmp_view_apply_intent",
            "kmp_view_get_state",
        ],
    ),
    ("guide", &["kmp_guide"]),
];

/// The tools whose arguments `topic` explains; empty for an unknown topic.
pub(crate) fn topic_tools(topic: &str) -> &'static [&'static str] {
    TOPIC_TOOLS
        .iter()
        .find(|(name, _)| *name == topic)
        .map_or(&[], |(_, tools)| tools)
}

/// The topic that explains `tool`'s arguments, if the tool is model-facing.
pub(crate) fn tool_topic(tool: &str) -> Option<&'static str> {
    TOPIC_TOOLS
        .iter()
        .find(|(_, tools)| tools.contains(&tool))
        .map(|(topic, _)| *topic)
}

pub(crate) fn scheme() -> Value {
    json!(TOPICS.map(|(topic, purpose)| json!({"topic":topic,"purpose":purpose})))
}
