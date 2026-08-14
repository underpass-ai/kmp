use std::sync::{Arc, Mutex};

use kmp_application::{ApplicationError, RoutingProjectionWriter};
use kmp_domain::{
    DomainError, NodeDetailProjection, NodeProjection, PortError, ProjectionMutation,
    ProjectionWriter,
};

#[test]
fn application_error_formats_wrapped_and_validation_messages() {
    let domain = ApplicationError::from(DomainError::EmptyValue("root_node_id"));
    let ports = ApplicationError::from(PortError::Unavailable("valkey down".to_string()));
    let validation = ApplicationError::Validation("invalid replay request".to_string());

    assert_eq!(domain.to_string(), "root_node_id cannot be empty");
    assert_eq!(ports.to_string(), "valkey down");
    assert_eq!(validation.to_string(), "invalid replay request");
}

#[tokio::test]
async fn routing_projection_writer_sends_graph_and_detail_mutations_to_the_right_writer() {
    let graph_calls = Arc::new(Mutex::new(Vec::new()));
    let detail_calls = Arc::new(Mutex::new(Vec::new()));
    let graph_writer = RecordingWriter::new(graph_calls.clone());
    let detail_writer = RecordingWriter::new(detail_calls.clone());
    let writer = RoutingProjectionWriter::new(graph_writer, detail_writer);

    writer
        .apply_mutations(vec![
            ProjectionMutation::UpsertNode(NodeProjection {
                node_id: "node-123".to_string(),
                node_kind: "capability".to_string(),
                title: "Root node".to_string(),
                summary: "summary".to_string(),
                status: "ACTIVE".to_string(),
                labels: vec!["Capability".to_string()],
                properties: Default::default(),
                provenance: None,
            }),
            ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
                node_id: "node-123".to_string(),
                detail: "expanded detail".to_string(),
                content_hash: "hash-1".to_string(),
                revision: 3,
            }),
        ])
        .await
        .expect("routing should succeed");

    let graph_calls = graph_calls.lock().expect("graph calls should lock");
    let detail_calls = detail_calls.lock().expect("detail calls should lock");

    assert_eq!(graph_calls.len(), 1);
    assert_eq!(detail_calls.len(), 1);
    assert!(matches!(
        graph_calls[0].as_slice(),
        [ProjectionMutation::UpsertNode(node)] if node.node_id == "node-123"
    ));
    assert!(matches!(
        detail_calls[0].as_slice(),
        [ProjectionMutation::UpsertNodeDetail(detail)] if detail.node_id == "node-123"
    ));
}

#[tokio::test]
async fn routing_projection_writer_skips_empty_partitions() {
    let graph_calls = Arc::new(Mutex::new(Vec::new()));
    let detail_calls = Arc::new(Mutex::new(Vec::new()));
    let graph_writer = RoutingProjectionWriter::new(
        RecordingWriter::new(graph_calls.clone()),
        RecordingWriter::new(detail_calls.clone()),
    );

    graph_writer
        .apply_mutations(vec![ProjectionMutation::UpsertNodeDetail(
            NodeDetailProjection {
                node_id: "node-456".to_string(),
                detail: "detail".to_string(),
                content_hash: "hash-2".to_string(),
                revision: 1,
            },
        )])
        .await
        .expect("routing should succeed");

    assert!(
        graph_calls
            .lock()
            .expect("graph calls should lock")
            .is_empty()
    );
    assert_eq!(
        detail_calls.lock().expect("detail calls should lock").len(),
        1
    );
}

#[derive(Debug, Clone)]
struct RecordingWriter {
    calls: Arc<Mutex<Vec<Vec<ProjectionMutation>>>>,
}

impl RecordingWriter {
    fn new(calls: Arc<Mutex<Vec<Vec<ProjectionMutation>>>>) -> Self {
        Self { calls }
    }
}

impl ProjectionWriter for RecordingWriter {
    async fn apply_mutations(&self, mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        self.calls
            .lock()
            .expect("calls should lock")
            .push(mutations);
        Ok(())
    }
}
