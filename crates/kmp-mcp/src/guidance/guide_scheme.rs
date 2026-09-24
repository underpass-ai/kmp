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

pub(crate) fn scheme() -> Value {
    json!(TOPICS.map(|(topic, purpose)| json!({"topic":topic,"purpose":purpose})))
}
