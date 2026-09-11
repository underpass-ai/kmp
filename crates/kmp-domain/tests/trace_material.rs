use kmp_domain::{
    NodeRelationProjection, RelationExplanation, RelationSemanticClass, TraceMaterialSelection,
    TraceProofRequirement, TraceRoute, TraceSearchResult, TraceSearchStop, select_trace_material,
};
use std::collections::BTreeSet;

fn strings(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).into()).collect()
}
fn candidates(paths: &[&[&str]]) -> TraceSearchResult {
    let mut r = TraceSearchResult {
        routing: None,
        material: None,
        from: "s".into(),
        follow: vec![],
        paths_per_target: 2,
        considered_states: 0,
        incomplete_targets: vec![],
        routes: vec![],
        relations: vec![],
        unreached: vec![],
        stop: TraceSearchStop::TargetsReached,
        discovered_nodes: 0,
        scanned_edges: 0,
        expanded_nodes: 0,
        leaves: 0,
        coordinate_rows: 0,
        clock_unknown_edges: vec![],
        temporal_axis: Default::default(),
        resolved_as_of: None,
        temporal_selection_resolved: true,
    };
    for path in paths {
        let mut indexes = vec![];
        for pair in path.windows(2) {
            indexes.push(r.relations.len() as u32);
            r.relations.push(NodeRelationProjection {
                source_node_id: pair[0].into(),
                target_node_id: pair[1].into(),
                relation_type: "depends_on".into(),
                explanation: RelationExplanation::new(RelationSemanticClass::Causal)
                    .with_rationale("An explicit dependency in the fixture.")
                    .with_evidence(format!("{} requires {}.", pair[0], pair[1])),
            });
        }
        r.routes.push(TraceRoute {
            target: path.last().expect("path").to_string(),
            edge_indexes: indexes,
        });
    }
    r
}
fn group(weight: u32, alternatives: &[&[&str]]) -> TraceProofRequirement {
    TraceProofRequirement {
        weight,
        alternatives: alternatives.iter().map(|a| strings(a)).collect(),
    }
}
fn policy(nodes: u32, groups: Vec<TraceProofRequirement>) -> TraceMaterialSelection {
    TraceMaterialSelection {
        max_nodes: nodes,
        max_paths: 4,
        groups,
    }
}

#[test]
fn shared_material_prefers_two_complete_paths_over_a_locally_shortest_choice() {
    let c = candidates(&[
        &["s", "a", "x"],
        &["s", "hub", "x"],
        &["s", "b", "y"],
        &["s", "hub", "y"],
    ]);
    let r =
        select_trace_material(&c, &strings(&["x", "y"]), &policy(4, vec![])).expect("selection");
    assert_eq!(r.selected_candidates, [1, 3]);
    assert_eq!(r.material_refs, ["hub", "s", "x", "y"]);
    assert_eq!(r.benefit, 2);
    assert_eq!(r.covered_groups, [0, 1]);
}

#[test]
fn and_requires_all_members_and_unreachable_requirements_never_become_empty_sets() {
    let c = candidates(&[&["s", "a"], &["s", "b"], &["s", "c"]]);
    let targets = strings(&["a", "b", "c", "missing"]);
    let r = select_trace_material(
        &c,
        &targets,
        &policy(4, vec![group(3, &[&["a", "b", "c"]])]),
    )
    .expect("and");
    assert_eq!(r.selected_candidates, [0, 1, 2]);
    assert_eq!(r.benefit, 3);
    let r = select_trace_material(
        &c,
        &targets,
        &policy(3, vec![group(3, &[&["a", "b", "c"]])]),
    )
    .expect("partial");
    assert!(r.selected_candidates.is_empty());
    assert_eq!(r.incomplete_groups, [0]);
    assert_eq!(r.benefit, 0, "heuristic progress is not actual coverage");
    let r = select_trace_material(&c, &targets, &policy(8, vec![group(4, &[&["missing"]])]))
        .expect("unknown");
    assert_eq!(r.benefit, 0);
    assert!(r.material_refs.is_empty());
}

#[test]
fn or_weights_and_ties_use_exact_complete_coverage_and_material_once() {
    let c = candidates(&[&["s", "a"], &["s", "b"], &["s", "c"], &["s", "x", "a"]]);
    let targets = strings(&["a", "b", "c"]);
    let p = policy(
        4,
        vec![group(2, &[&["a", "b"], &["c"]]), group(3, &[&["a"]])],
    );
    let r = select_trace_material(&c, &targets, &p).expect("or");
    assert_eq!(r.benefit, 5);
    assert_eq!(
        r.selected_candidates,
        [0, 1],
        "equal benefit/cost prefers lexical indexes"
    );
    assert!(r.pruned_by_width > 0);
    let mut tiny = p.clone();
    tiny.max_nodes = 1;
    assert!(
        select_trace_material(&c, &targets, &tiny)
            .expect("tiny")
            .selected_candidates
            .is_empty()
    );
}

#[test]
fn reverse_edges_keep_the_same_material_and_invalid_routes_or_groups_are_rejected() {
    let mut c = candidates(&[&["s", "a", "b"]]);
    c.relations[0].source_node_id = "a".into();
    c.relations[0].target_node_id = "s".into();
    let targets = strings(&["b"]);
    let r = select_trace_material(&c, &targets, &policy(3, vec![])).expect("mixed");
    assert_eq!(r.material_refs, ["a", "b", "s"]);
    assert!(
        select_trace_material(&c, &targets, &policy(3, vec![group(1, &[&["foreign"]])])).is_err()
    );
    assert!(select_trace_material(&c, &targets, &policy(3, vec![group(0, &[&["b"]])])).is_err());
    assert!(select_trace_material(&c, &targets, &policy(3, vec![group(1, &[&[]])])).is_err());
    c.routes[0].edge_indexes[0] = 500;
    assert!(select_trace_material(&c, &targets, &policy(3, vec![])).is_err());
}

#[test]
fn exhaustive_small_catalogue_oracle_bounds_benefit_and_checks_every_selected_union() {
    let raw: &[&[&str]] = &[
        &["s", "x", "a"],
        &["s", "z", "a"],
        &["s", "y", "b"],
        &["s", "z", "b"],
        &["s", "c"],
        &["s", "x", "c"],
    ];
    let c = candidates(raw);
    let targets = strings(&["a", "b", "c"]);
    for budget in 1..=7 {
        for weight in 1..=5 {
            let p = policy(
                budget,
                vec![
                    group(weight, &[&["a", "b"]]),
                    group(2, &[&["a", "c"], &["b", "c"]]),
                ],
            );
            let result = select_trace_material(&c, &targets, &p).expect("beam");
            let coverage = |nodes: &BTreeSet<String>| {
                p.groups
                    .iter()
                    .filter(|g| g.alternatives.iter().any(|a| a.is_subset(nodes)))
                    .map(|g| g.weight)
                    .sum::<u32>()
            };
            let mut optimum = 0;
            for mask in 0u32..(1 << raw.len()) {
                if mask.count_ones() > p.max_paths {
                    continue;
                }
                let nodes = raw
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| mask & (1 << i) != 0)
                    .flat_map(|(_, path)| path.iter().map(|s| s.to_string()))
                    .collect::<BTreeSet<_>>();
                if nodes.len() <= budget as usize {
                    optimum = optimum.max(coverage(&nodes));
                }
            }
            let selected = result
                .selected_candidates
                .iter()
                .flat_map(|&i| raw[i as usize].iter().map(|s| s.to_string()))
                .collect::<BTreeSet<_>>();
            assert_eq!(selected, result.material_refs.iter().cloned().collect());
            assert!(selected.len() <= budget as usize);
            assert_eq!(result.benefit, coverage(&selected));
            assert!(result.benefit <= optimum);
            assert!(result.evaluated <= 64 * 4 * 8);
        }
    }
}

#[test]
fn hundred_hop_alternatives_select_one_branch_and_the_complete_and_group() {
    let branch = |prefix: &str| {
        let mut p = vec!["s".to_string()];
        p.extend((1..100).map(|i| format!("{prefix}{i:03}")));
        p.push("t".to_string());
        p
    };
    let a = branch("a");
    let b = branch("b");
    let aa = a.iter().map(String::as_str).collect::<Vec<_>>();
    let bb = b.iter().map(String::as_str).collect::<Vec<_>>();
    let c = candidates(&[&aa, &bb, &["s", "check"], &["s", "rule"]]);
    let r = select_trace_material(
        &c,
        &strings(&["t", "check", "rule"]),
        &policy(103, vec![group(1, &[&["t", "check", "rule"]])]),
    )
    .expect("deep selection");
    assert_eq!(r.benefit, 1);
    assert_eq!(r.selected_candidates, [0, 2, 3]);
    assert_eq!(r.material_refs.len(), 103);
    assert!(r.material_refs.iter().all(|n| !n.starts_with('b')));
}
