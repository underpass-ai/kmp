use super::*;
use kmp_proto::v1beta1::{MemoryRelation, TraceSearchSelection};

fn candidates() -> TraceResponse {
    TraceResponse {
        summary: "Two candidate routes".into(),
        trace: [("a", "b"), ("b", "t"), ("c", "a"), ("c", "t")]
            .into_iter()
            .map(|(a, b)| MemoryRelation {
                source_ref: a.into(),
                target_ref: b.into(),
                ..Default::default()
            })
            .collect(),
        routes: vec![
            TraceRoute {
                target: "t".into(),
                edge_indexes: vec![0, 1],
            },
            TraceRoute {
                target: "t".into(),
                edge_indexes: vec![2, 3],
            },
        ],
        search: Some(TraceSearchSelection {
            from: "a".into(),
            clock_unknown_edges: vec![1, 2],
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn selection() -> TraceMaterialResult {
    TraceMaterialResult {
        selected_candidates: vec![1],
        material_refs: vec!["a".into(), "c".into(), "t".into()],
        covered_groups: vec![0],
        incomplete_groups: vec![],
        benefit: 1,
        evaluated: 3,
        pruned_by_width: 0,
    }
}

#[test]
fn trace_material_reindexes_selected_arrows_and_unknown_clocks_without_rewriting_proof() {
    let original = candidates();
    let mut response = original.clone();
    project(&mut response, selection());
    assert_eq!(response.trace, original.trace[2..]);
    assert_eq!(response.routes[0].edge_indexes, [0, 1]);
    let search = response.search.as_ref().expect("metadata");
    assert_eq!(search.clock_unknown_edges, [0]);
    assert_eq!(
        search.material.as_ref().expect("material").candidate_count,
        2
    );
    let original_hash = ReadSelectionFingerprint::trace_search(&response);
    let mut changed = original;
    changed.trace[0].target_ref = "changed omitted candidate".into();
    project(&mut changed, selection());
    assert_eq!(changed.trace, response.trace);
    assert_ne!(
        ReadSelectionFingerprint::trace_search(&changed),
        original_hash
    );
}

#[test]
fn trace_material_request_defaults_and_invalid_requirements_cross_the_domain_boundary() {
    let options = TraceMaterialSelectionOptions {
        max_material_nodes: 5,
        ..Default::default()
    };
    let mapped = request(options).expect("defaults");
    assert_eq!(mapped.max_paths, 4);
    let targets = ["t".to_string()].into();
    mapped.validate(&targets).expect("valid");
    assert!(request(Default::default()).is_err());
    let invalid = TraceMaterialSelection {
        groups: vec![TraceProofRequirement {
            weight: 1,
            alternatives: vec![["foreign".into()].into()],
        }],
        ..mapped
    };
    assert!(invalid.validate(&targets).is_err());
}
