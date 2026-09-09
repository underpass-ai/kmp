use kmp_domain::KnownMemoryRelationType;
use serde_json::Value;

const REQUEST: &str =
    include_str!("../../../api/examples/inference-prompts/kmp-write-memory.request.json");
const KMP_SCHEMA: &str =
    include_str!("../../../api/examples/kernel/v1beta1/kmp/kernel-memory-protocol.schema.json");

#[test]
fn writer_inference_fixture_uses_the_native_argument_schema() {
    let request: Value = serde_json::from_str(REQUEST)
        .expect("kernel write memory request fixture should be valid JSON");
    assert_eq!(request["response_format"]["type"], "json_schema");
    assert_eq!(
        request["response_format"]["json_schema"]["name"],
        "kmp_write_memory_arguments"
    );

    let catalog = kmp_mcp::kmp_mcp_tools_list_result();
    let native = catalog["tools"]
        .as_array()
        .expect("tools/list must return a tools array")
        .iter()
        .find(|tool| tool["name"] == "kmp_write_memory")
        .expect("native catalogue must include the writer");
    // The inference client must constrain the same payload accepted by MCP.
    // Editorial wording is deliberately excluded from this contract check.
    assert_eq!(
        without_descriptions(request["response_format"]["json_schema"]["schema"].clone()),
        without_descriptions(native["inputSchema"].clone())
    );
}

fn without_descriptions(mut value: Value) -> Value {
    match &mut value {
        Value::Object(fields) => {
            fields.remove("description");
            for field in fields.values_mut() {
                *field = without_descriptions(field.take());
            }
        }
        Value::Array(items) => {
            for item in items {
                *item = without_descriptions(item.take());
            }
        }
        _ => {}
    }
    value
}

#[test]
fn kmp_write_memory_schema_uses_core_relation_vocabulary() {
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
