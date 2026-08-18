use kmp_domain::KnownMemoryRelationType;
use serde_json::Value;

const PROMPT: &str =
    include_str!("../../../api/examples/inference-prompts/kernel-write-memory.txt");
const REQUEST: &str =
    include_str!("../../../api/examples/inference-prompts/kernel-write-memory.request.json");
const KMP_SCHEMA: &str =
    include_str!("../../../api/examples/kernel/v1beta1/kmp/kernel-memory-protocol.schema.json");

#[test]
fn kernel_write_memory_prompt_fixture_is_schema_constrained() {
    assert!(PROMPT.contains("kernel_write_memory"));
    assert!(PROMPT.contains("read_context"));
    assert!(PROMPT.contains("Do not use vague relations"));
    assert!(PROMPT.contains("`why` explains why the specific semantic"));
    assert!(PROMPT.contains("`evidence` names the concrete observation or source"));
    assert!(PROMPT.contains("does not generate"));
    assert!(PROMPT.contains("a missing rationale or proof"));

    let request: Value = serde_json::from_str(REQUEST)
        .expect("kernel write memory request fixture should be valid JSON");
    let request_prompt = request["messages"][1]["content"]
        .as_str()
        .expect("request fixture should carry the writer prompt");
    assert!(request_prompt.contains("why explains why the specific semantic connection holds"));
    assert!(request_prompt.contains("evidence names the concrete observation or source"));
    assert!(request_prompt.contains("does not generate a missing rationale or proof"));
    assert_eq!(
        request["response_format"]["json_schema"]["name"],
        "kernel_write_memory_arguments"
    );

    let schema = &request["response_format"]["json_schema"]["schema"];
    assert_eq!(schema["type"], "object");
    assert!(
        schema["required"]
            .as_array()
            .expect("schema required should be an array")
            .iter()
            .any(|field| field == "read_context")
    );
    assert!(schema["properties"].get("connect_to").is_some());
    assert!(schema["properties"].get("read_context").is_some());

    let rel_enum = schema["$defs"]["write_link"]["properties"]["rel"]["enum"]
        .as_array()
        .expect("relation enum should be an array");
    assert!(rel_enum.iter().any(|rel| rel == "chosen_because"));
    assert!(!rel_enum.iter().any(|rel| rel == "related_to"));
    assert_eq!(string_enum(rel_enum), core_writer_relation_names());
}

#[test]
fn kernel_write_memory_schema_uses_core_relation_vocabulary() {
    let schema: Value =
        serde_json::from_str(KMP_SCHEMA).expect("KMP schema fixture should be valid JSON");
    let rel_enum = schema["$defs"]["writer_relation_name"]["enum"]
        .as_array()
        .expect("relation enum should be an array");

    assert_eq!(string_enum(rel_enum), core_writer_relation_names());
}

fn core_writer_relation_names() -> Vec<String> {
    KnownMemoryRelationType::writer_relation_types()
        .iter()
        .map(|relation_type| relation_type.as_str().to_string())
        .collect()
}

fn string_enum(values: &[Value]) -> Vec<String> {
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("enum values should be strings")
                .to_string()
        })
        .collect()
}
