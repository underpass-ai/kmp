use super::{evidence_seek_request_from_proto, evidence_seek_response_from_result};
use kmp_domain::{EvidencePathBinding, EvidencePathResult, EvidencePathStatus, TraceSearchStop};
use kmp_proto::v1beta1::{
    TraceReferenceEndpoint, TraceRelationStep, TraceRequest, TraceSearchOptions, TraceSeekOptions,
    TraceSeekRole, TraceWitnessGroup, TraceWitnessLabel,
};

fn request() -> TraceRequest {
    TraceRequest {
        about: "project:test".into(),
        from: "seed".into(),
        search: Some(TraceSearchOptions {
            seek: Some(TraceSeekOptions {
                roles: ["verification", "permission"]
                    .into_iter()
                    .map(|name| TraceSeekRole {
                        name: name.into(),
                        relation: Some(TraceRelationStep {
                            rel: if name == "verification" {
                                "verified_by"
                            } else {
                                "authorizes"
                            }
                            .into(),
                            direction: if name == "verification" {
                                "outgoing"
                            } else {
                                "incoming"
                            }
                            .into(),
                        }),
                        ..Default::default()
                    })
                    .collect(),
                same_labels: vec!["event".into()],
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[test]
fn compiler_binds_only_main_witnesses_and_intersects_shared_constants() {
    let mut request = request();
    let seek = request
        .search
        .as_mut()
        .expect("search")
        .seek
        .as_mut()
        .expect("seek");
    seek.roles[0].via.push(TraceRelationStep {
        rel: "uses_background".into(),
        ..Default::default()
    });
    seek.roles[0].after.push(TraceRelationStep {
        rel: "depends_on".into(),
        ..Default::default()
    });
    seek.roles[0].labels.push(TraceWitnessLabel {
        key: "event".into(),
        values: vec!["A".into(), "B".into()],
    });
    seek.roles[1].labels.push(TraceWitnessLabel {
        key: "event".into(),
        values: vec!["B".into(), "C".into()],
    });
    let native = evidence_seek_request_from_proto(&request)
        .expect("valid")
        .expect("native");
    assert_eq!(native.roles[0].steps.len(), 3);
    assert_eq!(native.roles[0].bindings[0].at(), 2);
    assert_eq!(native.roles[1].bindings[0].at(), 1);
    assert_eq!(
        native
            .constants
            .values()
            .next()
            .expect("constant")
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["B"]
    );
    assert_eq!(
        native.roles[1].steps[0].direction,
        kmp_domain::RelationDirection::Incoming
    );
}

#[test]
fn compiler_rejects_contradictory_constraints_unknown_roles_and_mixed_modes() {
    let mut bad = request();
    bad.to = "target".into();
    assert!(evidence_seek_request_from_proto(&bad).is_err());
    let mut bad = request();
    let seek = bad
        .search
        .as_mut()
        .expect("search")
        .seek
        .as_mut()
        .expect("seek");
    seek.same_ref.push(TraceWitnessGroup {
        endpoints: ["verification", "missing"]
            .into_iter()
            .map(|role| TraceReferenceEndpoint {
                role: role.into(),
                anchor: false,
            })
            .collect(),
    });
    assert!(evidence_seek_request_from_proto(&bad).is_err());
    let mut bad = request();
    for (role, value) in bad
        .search
        .as_mut()
        .expect("search")
        .seek
        .as_mut()
        .expect("seek")
        .roles
        .iter_mut()
        .zip(["A", "B"])
    {
        role.labels.push(TraceWitnessLabel {
            key: "event".into(),
            values: vec![value.into()],
        });
    }
    assert!(evidence_seek_request_from_proto(&bad).is_err());
    let mut identity = request();
    identity
        .search
        .as_mut()
        .expect("search")
        .seek
        .as_mut()
        .expect("seek")
        .same_ref
        .push(TraceWitnessGroup {
            endpoints: ["verification", "permission"]
                .into_iter()
                .map(|role| TraceReferenceEndpoint {
                    role: role.into(),
                    anchor: false,
                })
                .collect(),
        });
    let native = evidence_seek_request_from_proto(&identity)
        .expect("valid")
        .expect("native");
    assert!(matches!(
        native.roles[0].bindings[1],
        EvidencePathBinding::Reference { .. }
    ));
    assert_eq!(
        native.roles[0].bindings[1].name(),
        native.roles[1].bindings[1].name()
    );
}

#[test]
fn compiler_binds_context_and_explicit_anchors_and_keeps_endpoint_groups_disjoint() {
    let mut request = request();
    let seek = request
        .search
        .as_mut()
        .expect("valid test fixture")
        .seek
        .as_mut()
        .expect("valid test fixture");
    seek.roles[0].context = true;
    seek.roles[1].via.push(TraceRelationStep {
        rel: "uses_background".into(),
        ..Default::default()
    });
    seek.same_ref.push(TraceWitnessGroup {
        endpoints: ["verification", "permission"]
            .into_iter()
            .map(|role| TraceReferenceEndpoint {
                role: role.into(),
                anchor: true,
            })
            .collect(),
    });
    seek.same_ref.push(TraceWitnessGroup {
        endpoints: ["verification", "permission"]
            .into_iter()
            .map(|role| TraceReferenceEndpoint {
                role: role.into(),
                anchor: false,
            })
            .collect(),
    });
    let native = evidence_seek_request_from_proto(&request)
        .expect("valid test fixture")
        .expect("valid test fixture");
    assert_eq!(
        native.roles[0]
            .bindings
            .iter()
            .map(|b| b.at())
            .collect::<Vec<_>>(),
        [1, 0, 1]
    );
    assert_eq!(
        native.roles[1]
            .bindings
            .iter()
            .map(|b| b.at())
            .collect::<Vec<_>>(),
        [2, 1, 2]
    );
    let seek = request
        .search
        .as_mut()
        .expect("valid test fixture")
        .seek
        .as_mut()
        .expect("valid test fixture");
    seek.same_ref[1].endpoints[0].anchor = true;
    assert!(evidence_seek_request_from_proto(&request).is_err());
}

#[test]
fn pages_preserve_candidate_group_indexes_and_bind_the_unreturned_domains() {
    use kmp_domain::{EvidencePathBindings, EvidencePathCandidate, EvidencePathGroup};
    let request = request();
    let native = evidence_seek_request_from_proto(&request)
        .expect("valid")
        .expect("native");
    let seek = request
        .search
        .as_ref()
        .expect("search")
        .seek
        .as_ref()
        .expect("seek");
    let bindings = EvidencePathBindings {
        domains: [(
            native.roles[0].bindings[0].name().into(),
            ["A".into()].into(),
        )]
        .into(),
        ..Default::default()
    };
    let result = EvidencePathResult {
        from: "seed".into(),
        context_discovery: false,
        relations: vec![],
        candidates: vec![EvidencePathCandidate {
            role: 0,
            context_hops: 0,
            nodes: vec!["seed".into(), "v".into()],
            edge_indexes: vec![],
            bindings: bindings.clone(),
            clock_unknown: false,
        }],
        groups: vec![EvidencePathGroup {
            candidate_indexes: vec![0],
            bindings,
            clock_unknown: false,
        }],
        missing_roles: vec![],
        status: EvidencePathStatus::Compatible,
        stop: TraceSearchStop::FrontierExhausted,
        discovered_nodes: 2,
        scanned_edges: 1,
        work_states: 2,
        shared_states: 0,
        incompatible_states: 0,
        adjacency_pages: 1,
        coordinate_pages: 1,
        resolved_as_of: None,
        temporal_selection_resolved: true,
        clock_unknown_edges: vec![],
    };
    let page = |cursor| kmp_application::TracePageRequest {
        entries: Some(1),
        cursor: Some(cursor),
    };
    let first = evidence_seek_response_from_result(result.clone(), &native, seek, page(0));
    let second = evidence_seek_response_from_result(result.clone(), &native, seek, page(1));
    assert_eq!(first.candidates[0].witness, "v");
    assert!(first.groups.is_empty());
    assert!(second.candidates.is_empty());
    assert_eq!(second.groups[0].candidate_indexes, [0]);
    assert_eq!(first.selection_fingerprint, second.selection_fingerprint);
    let mut changed = result;
    changed.groups[0]
        .bindings
        .domains
        .values_mut()
        .next()
        .expect("domain")
        .insert("B".into());
    let changed = evidence_seek_response_from_result(changed, &native, seek, page(0));
    assert_eq!(first.candidates, changed.candidates);
    assert_ne!(first.selection_fingerprint, changed.selection_fingerprint);
}
