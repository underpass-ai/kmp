use super::{EvidencePathBindings, EvidencePathCandidate, EvidencePathGroup, EvidencePathStatus};
use std::collections::BTreeSet;

/// Incremental all-role compatibility. An empty intersection is irreversible;
/// unknowns are retained. Every surviving proof alternative remains separate.
pub(super) fn join_candidates(
    candidates: &[EvidencePathCandidate],
    roles: usize,
    work: &mut u32,
    max_work: u32,
) -> (Vec<EvidencePathGroup>, bool) {
    let mut groups = vec![EvidencePathGroup {
        candidate_indexes: vec![],
        bindings: EvidencePathBindings::default(),
        clock_unknown: false,
    }];
    for role in 0..roles {
        let mut next = Vec::new();
        let choices: Vec<_> = candidates
            .iter()
            .enumerate()
            .filter(|(_, c)| c.role == role)
            .collect();
        for group in groups {
            for &(index, candidate) in &choices {
                if *work == max_work {
                    return (if role + 1 == roles { next } else { vec![] }, false);
                }
                *work += 1;
                if let Some(bindings) = group.bindings.joined(&candidate.bindings) {
                    let mut candidate_indexes = group.candidate_indexes.clone();
                    candidate_indexes.push(index as u32);
                    next.push(EvidencePathGroup {
                        candidate_indexes,
                        bindings,
                        clock_unknown: group.clock_unknown || candidate.clock_unknown,
                    });
                }
            }
        }
        groups = next;
    }
    (groups, true)
}

pub(super) fn status(groups: &[EvidencePathGroup], missing: bool) -> EvidencePathStatus {
    if missing {
        return EvidencePathStatus::MissingObligation;
    }
    if groups
        .iter()
        .any(|g| !g.bindings.missing.is_empty() || g.clock_unknown)
    {
        return EvidencePathStatus::ReviewRequired;
    }
    if groups.is_empty() {
        return EvidencePathStatus::IncompatibleObligations;
    }
    let domains: BTreeSet<_> = groups.iter().map(|g| &g.bindings.domains).collect();
    if domains.len() > 1 || domains.iter().any(|d| d.values().any(|v| v.len() > 1)) {
        EvidencePathStatus::Ambiguous
    } else {
        EvidencePathStatus::Compatible
    }
}
