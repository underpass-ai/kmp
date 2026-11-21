use std::collections::BTreeMap;

use kmp_domain::{NodeProjection, ProjectionMutation, ProjectionWriter};

use super::read;
use crate::EmbeddedKernelStore;

fn node(id: &str, kind: &str, properties: &[(&str, &str)]) -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: id.into(),
        node_kind: kind.into(),
        title: format!("{id} title"),
        summary: format!("{id} summary"),
        status: "ACTIVE".into(),
        labels: vec!["memory".into(), "entry".into()],
        properties: properties
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect(),
        provenance: None,
    })
}

/// A header is the node's identity, kind, status, labels, the placeholder
/// flag and, except for evidence, its title and summary: every other
/// property and the provenance stay out, however large they are.
#[tokio::test]
async fn a_header_keeps_only_what_admission_reads() {
    let dir = tempfile::tempdir().expect("temporary store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("open fixture store");
    let payload = "x".repeat(64 * 1024);
    store
        .apply_mutations(vec![
            node(
                "entry",
                "observation",
                &[("placeholder", "true"), ("memory_payload_json", &payload)],
            ),
            node("source", "memory_evidence", &[("canonical", &payload)]),
            node("plain", "decision", &[]),
        ])
        .await
        .expect("fixture write");
    let tx = store.begin_read().expect("read snapshot");
    let entry = read(tx.as_ref(), "entry")
        .expect("header reads")
        .expect("entry is stored");
    assert_eq!(entry.node_id, "entry");
    assert_eq!(entry.node_kind, "observation");
    assert_eq!(entry.status, "ACTIVE");
    assert_eq!(
        entry.labels,
        vec!["memory".to_string(), "entry".to_string()]
    );
    assert_eq!(entry.title, "entry title");
    assert_eq!(entry.summary, "entry summary");
    assert_eq!(
        entry.properties,
        BTreeMap::from([("placeholder".to_string(), "true".to_string())])
    );
    assert!(entry.provenance.is_none());
    let source = read(tx.as_ref(), "source")
        .expect("header reads")
        .expect("source is stored");
    assert_eq!((source.title.as_str(), source.summary.as_str()), ("", ""));
    assert!(source.properties.is_empty());
    let plain = read(tx.as_ref(), "plain")
        .expect("header reads")
        .expect("plain is stored");
    assert!(plain.properties.is_empty());
    assert!(read(tx.as_ref(), "absent").expect("header reads").is_none());
}
