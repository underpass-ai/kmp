use super::{
    trace_material_catalog::TraceMaterialCatalog, trace_material_set::TraceMaterialSet,
    trace_material_state::TraceMaterialState,
};
use crate::{DomainError, TraceMaterialResult, TraceMaterialSelection, TraceSearchResult};
use std::collections::BTreeSet;

/// Frozen M2 beam: width 4, alpha 1/2, lambda 0. No global optimality guarantee.
pub fn select_trace_material(
    candidates: &TraceSearchResult,
    targets: &BTreeSet<String>,
    policy: &TraceMaterialSelection,
) -> Result<TraceMaterialResult, DomainError> {
    policy.validate(targets)?;
    if candidates.routes.len() > 64 {
        return Err(DomainError::InvalidState(
            "trace material admits at most 64 candidate paths".into(),
        ));
    }
    let catalogue = TraceMaterialCatalog::new(candidates, &policy.requirements(targets))?;
    let mut incumbent = TraceMaterialState {
        indexes: vec![],
        material: TraceMaterialSet::empty(catalogue.names.len()),
        benefit: 0,
        priority: 0,
    };
    let mut frontier = vec![vec![]];
    let (mut evaluated, mut pruned) = (0, 0);
    for _ in 0..policy.max_paths {
        let mut generated = BTreeSet::new();
        for current in frontier {
            for index in 0..catalogue.paths.len() {
                if current.contains(&index) {
                    continue;
                }
                let mut next = current.clone();
                next.push(index);
                next.sort_unstable();
                generated.insert(next);
            }
        }
        let mut feasible = Vec::new();
        for indexes in generated {
            evaluated += 1;
            let mut material = TraceMaterialSet::empty(catalogue.names.len());
            for &index in &indexes {
                material.union(&catalogue.paths[index]);
            }
            if material.count() > policy.max_nodes {
                continue;
            }
            let (benefit, priority, _) = catalogue.score(&material);
            let state = TraceMaterialState {
                indexes,
                material,
                benefit,
                priority,
            };
            if state.actual_order(&incumbent).is_lt() {
                incumbent = TraceMaterialState {
                    indexes: state.indexes.clone(),
                    material: state.material.clone(),
                    benefit,
                    priority,
                };
            }
            feasible.push(state);
        }
        feasible.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.actual_order(b)));
        pruned += feasible.len().saturating_sub(4) as u32;
        frontier = feasible.into_iter().take(4).map(|s| s.indexes).collect();
        if frontier.is_empty() {
            break;
        }
    }
    let (_, _, covered_groups) = catalogue.score(&incumbent.material);
    let incomplete_groups = (0..catalogue.groups.len() as u32)
        .filter(|g| !covered_groups.contains(g))
        .collect();
    Ok(TraceMaterialResult {
        selected_candidates: incumbent.indexes.iter().map(|i| *i as u32).collect(),
        material_refs: catalogue
            .names
            .into_iter()
            .enumerate()
            .filter(|(i, _)| incumbent.material.contains(*i))
            .map(|(_, n)| n)
            .collect(),
        covered_groups,
        incomplete_groups,
        benefit: incumbent.benefit,
        evaluated,
        pruned_by_width: pruned,
    })
}
