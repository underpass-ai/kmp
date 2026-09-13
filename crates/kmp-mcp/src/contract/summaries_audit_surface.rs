//! What `kmp_summaries_audit` advertises, audited on its own.
//!
//! One concept: this verb's advertised arguments. It is kept beside the
//! assembled-surface audit rather than inside it because the two fail for
//! different reasons — that one when the surface as a whole moves, this one
//! when this verb grows a field it does not honour.
#![cfg(test)]

#[allow(unused_imports)]
use crate::contract::registry::*;

mod tests {
    #[allow(unused_imports)]
    use super::*;
    use serde_json::{Value, json};
    use std::collections::BTreeSet;

    fn schema() -> Value {
        tools_list_result()["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .find(|tool| tool["name"] == "kmp_summaries_audit")
            .expect("the audit is advertised")["inputSchema"]
            .clone()
    }

    fn keys(value: &Value) -> BTreeSet<String> {
        value["properties"]
            .as_object()
            .expect("schema properties")
            .keys()
            .cloned()
            .collect()
    }

    fn expected(values: &[&str]) -> BTreeSet<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    /// The audit reaches no gRPC request: it is read off the store's own
    /// event log. Every field it takes is either the scope vocabulary the
    /// reads already share or this response's own projection.
    #[test]
    fn every_advertised_argument_is_scope_vocabulary_or_response_projection() {
        let mut fields = keys(&schema());
        // MCP interaction metadata, removed before dispatch like every
        // other tool's.
        assert!(fields.remove("context_id"));
        assert!(fields.remove("purpose"));

        assert_eq!(
            fields,
            expected(&["about", "budget", "dimensions", "page", "states"])
        );
    }

    /// Borrowed, not invented: the same three words `kmp_ask` reads several
    /// abouts together with.
    #[test]
    fn the_scope_vocabulary_is_the_one_the_reads_already_use() {
        let dimensions = &schema()["properties"]["dimensions"];

        assert_eq!(
            dimensions["properties"]["scope"]["enum"],
            json!(["current_about", "abouts", "all_abouts"])
        );
        // And only what this reading honours. A label predicate would be a
        // filter the event log cannot apply, advertised and then ignored.
        assert_eq!(keys(dimensions), expected(&["abouts", "scope"]));
    }

    #[test]
    fn the_page_and_the_byte_ceiling_follow_the_conventions_already_published() {
        let schema = schema();

        assert_eq!(
            keys(&schema["properties"]["page"]),
            expected(&["cursor", "entries"])
        );
        assert_eq!(
            keys(&schema["properties"]["budget"]),
            expected(&["max_bytes"])
        );
    }

    /// The states are the reading's own closed vocabulary, and a caller may
    /// keep any subset of them.
    #[test]
    fn the_states_filter_names_the_whole_vocabulary() {
        assert_eq!(
            schema()["properties"]["states"]["items"]["enum"],
            json!(["missing", "refused", "stands", "not_required"])
        );
    }
}
