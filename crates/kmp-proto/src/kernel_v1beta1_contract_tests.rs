use crate::v1beta1;
use prost::Message;
use prost_types::{
    DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet,
    ServiceDescriptorProto,
};

const KERNEL_PACKAGE: &str = "underpass.rehydration.kernel.v1beta1";

#[test]
fn v1beta1_query_service_surface_is_stable() {
    let descriptor_set = decode_kernel_descriptor_set();
    let query_file = kernel_file(&descriptor_set, "query.proto");

    assert_eq!(
        service_method_names(query_file, "ContextQueryService"),
        vec![
            "GetContext",
            "GetContextPath",
            "GetNodeDetail",
            "RehydrateSession",
            "ValidateScope"
        ]
    );
}

#[test]
fn v1beta1_command_service_surface_is_stable() {
    let descriptor_set = decode_kernel_descriptor_set();
    let command_file = kernel_file(&descriptor_set, "command.proto");

    assert_eq!(
        service_method_names(command_file, "ContextCommandService"),
        vec!["UpdateContext"]
    );
}

#[test]
fn v1beta1_kernel_memory_service_surface_is_stable() {
    let descriptor_set = decode_kernel_descriptor_set();
    let memory_file = kernel_file(&descriptor_set, "memory.proto");

    assert_eq!(
        service_method_names(memory_file, "KernelMemoryService"),
        vec![
            "Ingest", "Wake", "Ask", "Goto", "Near", "Rewind", "Forward", "Trace", "Inspect"
        ]
    );
    assert_eq!(
        service_method_io_types(memory_file, "KernelMemoryService"),
        method_io_types(&[
            ("Ingest", "IngestRequest", "IngestResponse"),
            ("Wake", "WakeRequest", "WakeResponse"),
            ("Ask", "AskRequest", "AskResponse"),
            ("Goto", "GotoRequest", "GotoResponse"),
            ("Near", "NearRequest", "NearResponse"),
            ("Rewind", "RewindRequest", "RewindResponse"),
            ("Forward", "ForwardRequest", "ForwardResponse"),
            ("Trace", "TraceRequest", "TraceResponse"),
            ("Inspect", "InspectRequest", "InspectResponse"),
        ])
    );
}

#[test]
fn v1beta1_kernel_memory_core_fields_are_stable() {
    let descriptor_set = decode_kernel_descriptor_set();
    let memory_file = kernel_file(&descriptor_set, "memory.proto");

    assert_eq!(
        message_field_names(memory_file, "IngestRequest"),
        vec![
            "about",
            "memory",
            "provenance",
            "idempotency_key",
            "dry_run"
        ]
    );
    assert_eq!(
        message_field_names(memory_file, "TemporalCoordinate"),
        vec![
            "dimension",
            "scope_id",
            "occurred_at",
            "observed_at",
            "ingested_at",
            "valid_from",
            "valid_until",
            "sequence",
            "rank",
            "metadata",
        ]
    );
    assert_eq!(
        message_field_names(memory_file, "TemporalMoveRequest"),
        vec![
            "about",
            "cursor",
            "dimensions",
            "window",
            "limit",
            "include",
            "budget",
        ]
    );
    assert_eq!(
        message_field_names(memory_file, "GotoRequest"),
        vec![
            "about",
            "cursor",
            "dimensions",
            "window",
            "limit",
            "include",
            "budget",
        ]
    );
    assert_eq!(
        message_field_names(memory_file, "RewindRequest"),
        vec![
            "about",
            "cursor",
            "dimensions",
            "window",
            "limit",
            "include",
            "budget",
        ]
    );
    assert_eq!(
        message_field_names(memory_file, "ForwardRequest"),
        vec![
            "about",
            "cursor",
            "dimensions",
            "window",
            "limit",
            "include",
            "budget",
        ]
    );
    assert_eq!(
        message_field_names(memory_file, "TemporalNearRequest"),
        vec![
            "about",
            "around",
            "dimensions",
            "window",
            "limit",
            "include",
            "budget",
        ]
    );
    assert_eq!(
        message_field_names(memory_file, "NearRequest"),
        vec![
            "about",
            "around",
            "dimensions",
            "window",
            "limit",
            "include",
            "budget",
        ]
    );
    // The four temporal-move responses share one field set (including the
    // appended `quality`); assert them in a loop so the contract is stated
    // once rather than duplicated per response.
    for response in [
        "GotoResponse",
        "NearResponse",
        "RewindResponse",
        "ForwardResponse",
    ] {
        assert_eq!(
            message_field_names(memory_file, response),
            vec![
                "summary", "temporal", "coverage", "entries", "proof", "warnings", "raw_refs",
                "page", "quality",
            ],
            "temporal response {response} field set drifted",
        );
    }
    assert_eq!(
        message_field_names(memory_file, "InspectResponse"),
        vec![
            "summary", "object", "links", "evidence", "warnings", "raw", "quality"
        ]
    );
    assert_eq!(
        message_field_names(memory_file, "RawMemoryRef"),
        vec![
            "ref",
            "kind",
            "text",
            "coordinates",
            "detail",
            "content_hash",
            "revision"
        ]
    );
    assert_eq!(
        message_field_names(memory_file, "DimensionSelection"),
        vec!["mode", "include", "exclude", "scope", "abouts", "scope_ids"]
    );
}

#[test]
fn v1beta1_kernel_memory_temporal_presence_fields_are_stable() {
    let descriptor_set = decode_kernel_descriptor_set();
    let memory_file = kernel_file(&descriptor_set, "memory.proto");

    assert_proto3_optional(memory_file, "TemporalCoordinate", "sequence");
    assert_proto3_optional(memory_file, "TemporalCoordinate", "rank");
    assert_proto3_optional(memory_file, "MemoryRelation", "sequence");
    assert_proto3_optional(memory_file, "TemporalCursor", "sequence");
}

#[test]
fn v1beta1_graph_relationship_fields_are_stable() {
    let descriptor_set = decode_kernel_descriptor_set();
    let common_file = kernel_file(&descriptor_set, "common.proto");

    assert_eq!(
        message_field_names(common_file, "GraphRelationship"),
        vec![
            "source_node_id",
            "target_node_id",
            "relationship_type",
            "explanation",
            "provenance",
        ]
    );
    assert_eq!(
        message_field_names(common_file, "GraphRelationshipExplanation"),
        vec![
            "semantic_class",
            "rationale",
            "motivation",
            "method",
            "decision_id",
            "caused_by_node_id",
            "evidence",
            "confidence",
            "sequence",
            "dimension",
            "scope_id",
            "occurred_at",
            "observed_at",
            "ingested_at",
            "valid_from",
            "valid_until",
            "rank",
        ]
    );
}

fn decode_kernel_descriptor_set() -> FileDescriptorSet {
    FileDescriptorSet::decode(v1beta1::FILE_DESCRIPTOR_SET)
        .expect("kernel v1beta1 descriptor set should decode")
}

fn kernel_file<'a>(
    descriptor_set: &'a FileDescriptorSet,
    file_name: &str,
) -> &'a FileDescriptorProto {
    let full_name = format!("underpass/rehydration/kernel/v1beta1/{file_name}");

    descriptor_set
        .file
        .iter()
        .find(|file| {
            file.package.as_deref() == Some(KERNEL_PACKAGE)
                && file.name.as_deref() == Some(full_name.as_str())
        })
        .unwrap_or_else(|| panic!("missing descriptor file `{full_name}`"))
}

fn service_method_names(file: &FileDescriptorProto, service_name: &str) -> Vec<String> {
    service(file, service_name)
        .method
        .iter()
        .map(|method| method.name.clone().expect("method name should be present"))
        .collect()
}

fn service_method_io_types(
    file: &FileDescriptorProto,
    service_name: &str,
) -> Vec<(String, String, String)> {
    service(file, service_name)
        .method
        .iter()
        .map(|method| {
            (
                method.name.clone().expect("method name should be present"),
                stable_type_name(&method.input_type),
                stable_type_name(&method.output_type),
            )
        })
        .collect()
}

fn method_io_types(items: &[(&str, &str, &str)]) -> Vec<(String, String, String)> {
    items
        .iter()
        .map(|(name, input, output)| (name.to_string(), input.to_string(), output.to_string()))
        .collect()
}

fn service<'a>(file: &'a FileDescriptorProto, service_name: &str) -> &'a ServiceDescriptorProto {
    file.service
        .iter()
        .find(|service| service.name.as_deref() == Some(service_name))
        .unwrap_or_else(|| panic!("missing service `{service_name}`"))
}

fn stable_type_name(type_name: &Option<String>) -> String {
    type_name
        .as_deref()
        .and_then(|name| name.rsplit('.').next())
        .unwrap_or_default()
        .to_string()
}

fn message_field_names(file: &FileDescriptorProto, message_name: &str) -> Vec<String> {
    file.message_type
        .iter()
        .find(|message| message.name.as_deref() == Some(message_name))
        .unwrap_or_else(|| panic!("missing message `{message_name}`"))
        .field
        .iter()
        .map(|field| field.name.clone().expect("field name should be present"))
        .collect()
}

fn assert_proto3_optional(file: &FileDescriptorProto, message_name: &str, field_name: &str) {
    assert!(
        message_field(file, message_name, field_name).proto3_optional(),
        "`{message_name}.{field_name}` must preserve proto3 presence"
    );
}

fn message_field<'a>(
    file: &'a FileDescriptorProto,
    message_name: &str,
    field_name: &str,
) -> &'a FieldDescriptorProto {
    file.message_type
        .iter()
        .find(|message| message.name.as_deref() == Some(message_name))
        .unwrap_or_else(|| panic!("missing message `{message_name}`"))
        .field
        .iter()
        .find(|field| field.name.as_deref() == Some(field_name))
        .unwrap_or_else(|| panic!("missing field `{message_name}.{field_name}`"))
}

#[allow(dead_code)]
fn _service_names(file: &FileDescriptorProto) -> Vec<String> {
    file.service.iter().map(service_name).collect()
}

#[allow(dead_code)]
fn _message_names(file: &FileDescriptorProto) -> Vec<String> {
    file.message_type.iter().map(message_name).collect()
}

fn service_name(service: &ServiceDescriptorProto) -> String {
    service
        .name
        .clone()
        .expect("service name should be present")
}

fn message_name(message: &DescriptorProto) -> String {
    message
        .name
        .clone()
        .expect("message name should be present")
}
