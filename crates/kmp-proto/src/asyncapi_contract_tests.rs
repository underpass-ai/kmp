const ASYNCAPI_CONTRACT: &str =
    include_str!("../../../api/asyncapi/context-projection.v1beta1.yaml");

#[test]
fn asyncapi_declares_expected_metadata() {
    assert!(
        ASYNCAPI_CONTRACT
            .lines()
            .any(|line| line == "asyncapi: 2.6.0")
    );
    assert!(
        ASYNCAPI_CONTRACT
            .lines()
            .any(|line| line == "defaultContentType: application/json")
    );
}

#[test]
fn asyncapi_exposes_generic_kernel_subjects_with_expected_directions() {
    let graph_node_channel = top_level_block("  graph.node.materialized:");
    assert!(graph_node_channel.contains(&"    subscribe:"));
    assert!(
        graph_node_channel.contains(&"        $ref: '#/components/messages/GraphNodeMaterialized'")
    );

    let graph_relation_channel = top_level_block("  graph.relation.materialized:");
    assert!(graph_relation_channel.contains(&"    subscribe:"));
    assert!(
        graph_relation_channel
            .contains(&"        $ref: '#/components/messages/GraphRelationMaterialized'")
    );

    let node_detail_channel = top_level_block("  node.detail.materialized:");
    assert!(node_detail_channel.contains(&"    subscribe:"));
    assert!(
        node_detail_channel
            .contains(&"        $ref: '#/components/messages/NodeDetailMaterialized'")
    );

    let bundle_generated_channel = top_level_block("  context.bundle.generated:");
    assert!(bundle_generated_channel.contains(&"    publish:"));
    assert!(
        bundle_generated_channel
            .contains(&"        $ref: '#/components/messages/ContextBundleGenerated'")
    );
}

#[test]
fn asyncapi_event_envelope_required_fields_are_stable() {
    let event_envelope = top_level_block("    EventEnvelope:");

    assert_eq!(
        nested_sequence_entries(&event_envelope, "      required:"),
        vec![
            "event_id",
            "correlation_id",
            "causation_id",
            "occurred_at",
            "aggregate_id",
            "aggregate_type",
            "schema_version",
        ]
    );
}

#[test]
fn context_bundle_generated_payload_uses_root_node_id_semantics() {
    let payload = top_level_block("    ContextBundleGeneratedData:");

    assert_eq!(
        nested_sequence_entries(&payload, "      required:"),
        vec![
            "root_node_id",
            "roles",
            "revision",
            "content_hash",
            "projection_watermark",
        ]
    );
    assert!(
        nested_mapping_keys(&payload, "      properties:").contains(&"root_node_id".to_string())
    );
    assert!(!nested_mapping_keys(&payload, "      properties:").contains(&"case_id".to_string()));
}

#[test]
fn graph_node_and_detail_payloads_remain_generic() {
    let graph_node = top_level_block("    GraphNodeMaterializedData:");
    let graph_relation = top_level_block("    GraphRelationMaterializedData:");
    let node_detail = top_level_block("    NodeDetailMaterializedData:");

    assert_eq!(
        nested_sequence_entries(&graph_node, "      required:"),
        vec!["node_id", "node_kind", "title"]
    );
    assert_eq!(
        nested_sequence_entries(&graph_relation, "      required:"),
        vec![
            "source_node_id",
            "target_node_id",
            "relation_type",
            "explanation",
        ]
    );
    assert_eq!(
        nested_sequence_entries(&node_detail, "      required:"),
        vec!["node_id", "detail", "content_hash", "revision"]
    );
}

fn top_level_block(anchor: &str) -> Vec<&'static str> {
    let lines = ASYNCAPI_CONTRACT.lines().collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|line| *line == anchor)
        .unwrap_or_else(|| panic!("missing anchor line `{anchor}`"));
    let base_indent = indentation(anchor);
    let mut block = vec![lines[start]];

    for line in lines.iter().skip(start + 1) {
        if !line.trim().is_empty() && indentation(line) <= base_indent {
            break;
        }
        block.push(*line);
    }

    block
}

fn nested_sequence_entries(block: &[&str], anchor: &str) -> Vec<String> {
    let anchor_index = block
        .iter()
        .position(|line| *line == anchor)
        .unwrap_or_else(|| panic!("missing sequence anchor `{anchor}`"));
    let sequence_indent = indentation(anchor);
    let mut entries = Vec::new();

    for line in block.iter().skip(anchor_index + 1) {
        if line.trim().is_empty() {
            continue;
        }

        let indent = indentation(line);
        if indent <= sequence_indent {
            break;
        }

        let trimmed = line.trim();
        if let Some(item) = trimmed.strip_prefix("- ") {
            entries.push(item.to_string());
        }
    }

    entries
}

fn nested_mapping_keys(block: &[&str], anchor: &str) -> Vec<String> {
    let anchor_index = block
        .iter()
        .position(|line| *line == anchor)
        .unwrap_or_else(|| panic!("missing mapping anchor `{anchor}`"));
    let mapping_indent = indentation(anchor);
    let mut keys = Vec::new();

    for line in block.iter().skip(anchor_index + 1) {
        if line.trim().is_empty() {
            continue;
        }

        let indent = indentation(line);
        if indent <= mapping_indent {
            break;
        }

        let trimmed = line.trim();
        if let Some((key, _)) = trimmed.split_once(':') {
            keys.push(key.to_string());
        }
    }

    keys
}

fn indentation(line: &str) -> usize {
    line.chars()
        .take_while(|character| *character == ' ')
        .count()
}
