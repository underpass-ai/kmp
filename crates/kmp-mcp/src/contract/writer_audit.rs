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
    /// The tool documentation is generated from the writer spec; this pins
    /// that every cataloged type appears with its quality tier, in both the
    /// writer's and the batch surface, so a model reading `tools/list` learns
    /// the vocabulary the kernel will actually validate.
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

        for relation_type in KnownMemoryRelationType::writer_relation_types() {
            let spec = relation_type
                .writer_spec()
                .expect("writer relation types carry a spec");
            for doc in [&writer_doc, &ingest_doc] {
                assert!(
                    doc.contains(&format!(
                        "{} ({};",
                        spec.relation_type().as_str(),
                        spec.quality().as_str()
                    )),
                    "documentation names `{}` with its quality tier",
                    spec.relation_type().as_str()
                );
            }
        }
        assert!(
            writer_doc.contains("anemic types are an honest fallback"),
            "documentation states the anemic-fallback doctrine"
        );
    }
    #[test]
    fn each_verb_publishes_the_defaults_its_backend_uses() {
        let tools = tools_list_result();
        let tools = tools["tools"].as_array().expect("tools");
        let defaults = [
            ("kmp_wake", 1_600, 2),
            ("kmp_ask", 2_400, 2),
            ("kmp_relate", 2_400, 2),
            ("kmp_goto", 2_400, 3),
            ("kmp_near", 2_400, 3),
            ("kmp_rewind", 2_400, 3),
            ("kmp_forward", 2_400, 3),
            ("kmp_trace", 1_600, 1),
        ];

        for (name, tokens, depth) in defaults {
            let tool = tools
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap_or_else(|| panic!("{name}"));
            let budget = &tool["inputSchema"]["properties"]["budget"]["properties"];
            assert_eq!(budget["tokens"]["default"], tokens, "{name} tokens");
            assert_eq!(budget["depth"]["default"], depth, "{name} depth");
            assert_eq!(budget["max_bytes"]["default"], 10_000);
            assert_eq!(budget["detail"]["default"], "balanced");
        }
    }
}
