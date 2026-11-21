//! The writer's advertised contract, audited: the schema admits what the
//! runtime accepts, the vocabulary documentation matches the writer spec,
//! and every verb publishes the defaults its backend uses.
#![cfg(test)]

#[allow(unused_imports)]
use crate::contract::registry::*;
#[allow(unused_imports)]
use crate::contract::tools::write_memory::{read_context_schema, write_memory_schema};

mod tests {
    #[allow(unused_imports)]
    use super::*;

    use crate::contract::schema::relation_vocabulary::relation_vocabulary_lines;
    use kmp_domain::KnownMemoryRelationType;

    #[test]
    fn writer_schema_has_one_record_shape_for_single_and_batch_writes() {
        let writer = write_memory_schema();
        for retired in ["intent", "scope", "current", "semantic_delta", "connect_to"] {
            assert!(writer["properties"].get(retired).is_none());
        }
        assert_eq!(writer["properties"]["memories"]["minItems"], 1);
        assert_eq!(
            writer["properties"]["memories"]["items"]["required"],
            serde_json::json!(["id", "kind", "summary"])
        );
        assert!(
            writer["properties"]["memories"]["items"]["properties"]["connect_to"]
                .get("minItems")
                .is_none()
        );
    }
    /// The vocabulary is generated from the writer spec and served by
    /// `kmp_guide` topic `relations`; `tools/list` keeps the doctrine and names
    /// where the vocabulary lives, in both the writer's and the batch surface.
    #[test]
    fn relation_vocabulary_documentation_matches_the_writer_spec() {
        let tools = tools_list_result();
        let writer_doc = tools["tools"][1]["inputSchema"]["properties"]["memories"]["items"]["properties"]["connect_to"]["items"]
            ["properties"]["rel"]["description"]
            .as_str()
            .expect("writer rel carries generated documentation")
            .to_string();
        let ingest_doc = tools["tools"][0]["inputSchema"]["properties"]["memory"]["properties"]
            ["relations"]["items"]["properties"]["rel"]["description"]
            .as_str()
            .expect("ingest rel carries generated documentation")
            .to_string();
        for doc in [&writer_doc, &ingest_doc] {
            assert!(
                doc.contains("anemic types are an honest fallback"),
                "documentation states the anemic-fallback doctrine"
            );
            assert!(
                doc.contains("kmp_guide topic relations"),
                "documentation names where the vocabulary lives"
            );
        }

        let lines = relation_vocabulary_lines();
        for relation_type in KnownMemoryRelationType::writer_relation_types() {
            let spec = relation_type
                .writer_spec()
                .expect("writer relation types carry a spec");
            let prefix = format!(
                "{} ({};",
                spec.relation_type().as_str(),
                spec.quality().as_str()
            );
            assert!(
                lines
                    .iter()
                    .any(|line| line.starts_with(&prefix) && line.contains(spec.reason())),
                "the guide names `{}` with its quality tier and reason",
                spec.relation_type().as_str()
            );
        }
    }
    #[test]
    fn each_verb_publishes_only_the_budget_fields_its_read_honours() {
        // E11: one shared budget declared five fields on six tools; trace,
        // relate and curate ignored most of them and `tokens` changed nothing
        // anywhere. Each tool now declares the fields its read applies, with
        // the defaults its backend uses.
        let tools = tools_list_result();
        let tools = tools["tools"].as_array().expect("tools");
        let honoured: [(&str, &[&str], Option<u64>); 6] = [
            ("kmp_wake", &["depth", "detail", "max_bytes", "max_entries"], Some(2)),
            ("kmp_ask", &["depth", "detail", "max_bytes", "max_entries"], Some(2)),
            ("kmp_relate", &["depth", "max_bytes"], Some(2)),
            ("kmp_time", &["depth", "detail", "max_bytes", "max_entries"], Some(3)),
            ("kmp_trace", &["max_bytes"], None),
            ("kmp_curate", &[], None),
        ];
        for (name, fields, depth) in honoured {
            let tool = tools
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap_or_else(|| panic!("{name}"));
            let budget = &tool["inputSchema"]["properties"]["budget"]["properties"];
            let mut declared = budget
                .as_object()
                .map(|fields| fields.keys().map(String::as_str).collect::<Vec<_>>())
                .unwrap_or_default();
            declared.sort_unstable();
            assert_eq!(declared, fields, "{name} budget fields");
            if let Some(depth) = depth {
                assert_eq!(budget["depth"]["default"], depth, "{name} depth");
            }
            if fields.contains(&"max_bytes") {
                assert_eq!(budget["max_bytes"]["default"], 10_000);
            }
            if fields.contains(&"detail") {
                assert_eq!(budget["detail"]["default"], "balanced");
            }
        }
    }
}
