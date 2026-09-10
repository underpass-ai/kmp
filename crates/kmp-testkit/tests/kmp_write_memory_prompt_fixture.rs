use kmp_domain::KnownMemoryRelationType;
use serde_json::Value;

const KMP_SCHEMA: &str =
    include_str!("../../../api/examples/kernel/v1beta1/kmp/kernel-memory-protocol.schema.json");

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
