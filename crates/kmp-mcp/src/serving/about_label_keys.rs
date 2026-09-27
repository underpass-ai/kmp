//! The label keys an about already uses, read for a refusal to quote.
//!
//! One concept: a writer refused for its labels is told the vocabulary the
//! about already has, so it reuses a key instead of guessing one. It is the
//! same catalogue `kmp_wake` lists, read through the projection the viewer
//! uses. The read is best-effort: when it fails the refusal simply does not
//! name keys, and nothing is ever written or defaulted from it.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::serving::kernel_mcp_server::KernelMcpServer;

impl KernelMcpServer {
    /// The distinct label keys `about` holds, sorted, or nothing when the
    /// store could not be read.
    pub(super) async fn about_label_keys(&self, about: &str) -> Option<Vec<String>> {
        let response = self
            .backend
            .call_tool(
                "kmp_view_read_projection",
                &json!({
                    "about": about,
                    "from": "0001-01-01T00:00:00Z",
                    "to": "9999-12-31T23:59:59Z",
                    "lod": "atlas",
                    "dimensions": {"scope": "abouts", "abouts": [about]}
                }),
            )
            .await
            .ok()?;
        Some(label_keys(&response))
    }
}

/// The keys of the projection's label catalogue.
fn label_keys(response: &Value) -> Vec<String> {
    response
        .pointer("/structuredContent/labels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|label| label.get("dimension").and_then(Value::as_str))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_are_distinct_and_sorted() {
        let response = json!({"structuredContent": {"labels": [
            {"dimension": "topic", "value": "write-path"},
            {"dimension": "component", "value": "kmp-mcp"},
            {"dimension": "topic", "value": "lint"},
            {"value": "orphan"}
        ]}});

        assert_eq!(label_keys(&response), ["component", "topic"]);
        assert!(label_keys(&json!({})).is_empty());
    }

    #[tokio::test]
    async fn a_store_without_labels_names_none() {
        let server = KernelMcpServer::fixture();

        assert_eq!(
            server.about_label_keys("project:kmp").await,
            Some(Vec::new())
        );
    }
}
