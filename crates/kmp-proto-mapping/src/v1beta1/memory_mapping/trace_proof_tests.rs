use super::*;
use crate::v1beta1::memory_mapping::read_selection_fingerprint::ReadSelectionFingerprint;
use kmp_domain::{NodeDetailProjection, NodeProjection, TraceProofObject as DomainObject};

fn response() -> TraceResponse {
    let mut response = TraceResponse::default();
    project(
        &mut response,
        TraceProofResult {
            objects: vec![DomainObject {
                descriptor: None,
                body_state: kmp_domain::TraceBodyState::Loaded,
                card: None,
                node: NodeProjection {
                    node_id: "source".into(),
                    node_kind: "memory_evidence".into(),
                    title: "Source".into(),
                    summary: "Summary only".into(),
                    status: "ACTIVE".into(),
                    labels: vec![],
                    properties: [
                        ("payload_metadata".into(), r#"{"a":"one","b":"two"}"#.into()),
                        ("payload_source".into(), "signed:source".into()),
                        ("payload_time".into(), "2026-09-01T10:00:00Z".into()),
                    ]
                    .into(),
                    provenance: None,
                },
                body: Some(NodeDetailProjection {
                    node_id: "source".into(),
                    detail: "Exact π proof".into(),
                    content_hash: "hash".into(),
                    revision: 3,
                }),
                coordinates: vec![],
            }],
            complete_groups: vec![0],
            body_bytes: 14,
            ..Default::default()
        },
    );
    response
}

#[test]
fn body_source_and_metadata_changes_invalidate_selection_but_map_order_does_not() {
    let original = response();
    assert_eq!(
        original.objects[0]
            .object
            .as_ref()
            .expect("proof object")
            .metadata
            .len(),
        2
    );
    assert!(original.objects[0].has_body);
    assert_eq!(original.objects[0].status, "ACTIVE");
    assert_eq!(
        original.objects[0]
            .object
            .as_ref()
            .expect("proof object")
            .text,
        "Exact π proof"
    );
    let hash = ReadSelectionFingerprint::trace_search(&original);
    let mut reordered = original.clone();
    let metadata = &mut reordered.objects[0]
        .object
        .as_mut()
        .expect("proof object")
        .metadata;
    let values = std::mem::take(metadata);
    for (k, v) in values.into_iter().collect::<Vec<_>>().into_iter().rev() {
        metadata.insert(k, v);
    }
    assert_eq!(hash, ReadSelectionFingerprint::trace_search(&reordered));
    let mut changed = original.clone();
    changed.objects[0]
        .object
        .as_mut()
        .expect("proof object")
        .text = "Changed body".into();
    assert_ne!(hash, ReadSelectionFingerprint::trace_search(&changed));
    let mut changed = original.clone();
    changed.objects[0]
        .object
        .as_mut()
        .expect("proof object")
        .metadata
        .insert("new".into(), "value".into());
    assert_ne!(hash, ReadSelectionFingerprint::trace_search(&changed));
    let mut changed = original;
    changed.objects[0].content_hash = "changed-hash".into();
    assert_ne!(hash, ReadSelectionFingerprint::trace_search(&changed));
}

#[test]
fn combined_page_positions_preserve_whole_objects_and_support_rows() {
    let mut trace = vec![1, 2];
    let mut objects = vec![3, 4];
    let mut supports = vec![5, 6];
    let mut skip = 3;
    let mut remaining = 2;
    page(&mut trace, &mut skip, &mut remaining);
    page(&mut objects, &mut skip, &mut remaining);
    page(&mut supports, &mut skip, &mut remaining);
    assert!(trace.is_empty());
    assert_eq!(objects, [4]);
    assert_eq!(supports, [5]);
    assert_eq!((skip, remaining), (0, 0));
}

#[test]
fn hidden_proof_revision_gaps_and_group_state_change_the_fingerprint() {
    let original = response();
    let fingerprint = ReadSelectionFingerprint::trace_search(&original);
    let mut status = original.clone();
    status.objects[0].status = "SUPERSEDED".into();
    assert_ne!(fingerprint, ReadSelectionFingerprint::trace_search(&status));
    let mut revised = original.clone();
    revised.objects[0].revision += 1;
    assert_ne!(
        fingerprint,
        ReadSelectionFingerprint::trace_search(&revised)
    );
    let mut missing = original.clone();
    missing.gaps.push(kmp_proto::v1beta1::TraceProofGap {
        r#ref: "other-entry".into(),
        reasons: vec!["missing_body".into()],
    });
    assert_ne!(
        fingerprint,
        ReadSelectionFingerprint::trace_search(&missing)
    );
    let mut incomplete = original.clone();
    incomplete
        .proof
        .as_mut()
        .expect("proof")
        .complete_groups
        .clear();
    incomplete
        .proof
        .as_mut()
        .expect("proof")
        .incomplete_groups
        .push(0);
    assert_ne!(
        fingerprint,
        ReadSelectionFingerprint::trace_search(&incomplete)
    );
    let mut source = original.clone();
    source.objects[0].object.as_mut().expect("object").source = "changed source".into();
    assert_ne!(fingerprint, ReadSelectionFingerprint::trace_search(&source));
}
