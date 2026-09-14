//! Deterministic card targeting over the already selected snapshot. No body
//! loads, text interpretation, or changes to selection and proof admission.

use crate::{
    NodeCardExpectation, NodeCardStatus, TraceCondenseCandidate, TraceCondenseCandidates,
    TraceProofResult,
};
use std::collections::{BTreeMap, BTreeSet};

/// Presentation defaults, not caller-controlled proof or storage budgets.
pub const MIN_BODY_BYTES: u64 = 1_024;
pub const MAX_CANDIDATES: usize = 8;

pub fn recommend(
    proof: &TraceProofResult,
    memberships: &[BTreeSet<String>],
) -> TraceCondenseCandidates {
    let mut sources = BTreeMap::<&str, BTreeSet<&str>>::new();
    for support in &proof.supports {
        sources
            .entry(&support.target_node_id)
            .or_default()
            .insert(&support.source_node_id);
    }
    let mut sharing = BTreeMap::<&str, u32>::new();
    for members in memberships {
        // A source supporting several entries in one route/group still counts
        // only once. Relations are the selected, scope/time-admitted supports.
        let mut refs: BTreeSet<&str> = members.iter().map(String::as_str).collect();
        for member in members {
            if let Some(supports) = sources.get(member.as_str()) {
                refs.extend(supports);
            }
        }
        for reference in refs {
            *sharing.entry(reference).or_default() += 1;
        }
    }

    let mut result = TraceCondenseCandidates::default();
    for object in &proof.objects {
        let (Some(descriptor), Some(card)) = (&object.descriptor, &object.card) else {
            continue;
        };
        // Classify once, with card usability before the size heuristic.
        let expect = match card.status {
            NodeCardStatus::Valid => {
                result.valid += 1;
                continue;
            }
            NodeCardStatus::AfterCut => {
                result.after_cut += 1;
                continue;
            }
            NodeCardStatus::Absent => NodeCardExpectation::Absent,
            NodeCardStatus::Stale => {
                let Some(stored) = &card.stored else {
                    // Never fabricate a CAS revision from a malformed view.
                    continue;
                };
                NodeCardExpectation::CardRevision(stored.card_revision)
            }
        };
        if descriptor.body_bytes < MIN_BODY_BYTES {
            result.below_floor += 1;
            continue;
        }
        result.items.push(TraceCondenseCandidate {
            descriptor: descriptor.clone(),
            card_status: card.status,
            shared_by: sharing
                .get(descriptor.node_id.as_str())
                .copied()
                .unwrap_or(0),
            expect,
        });
    }
    result.items.sort_by(|left, right| {
        right
            .shared_by
            .cmp(&left.shared_by)
            .then_with(|| right.descriptor.body_bytes.cmp(&left.descriptor.body_bytes))
            .then_with(|| left.descriptor.node_id.cmp(&right.descriptor.node_id))
    });
    result.omitted_count = result.items.len().saturating_sub(MAX_CANDIDATES) as u32;
    result.items.truncate(MAX_CANDIDATES);
    result
}

#[cfg(test)]
#[path = "trace_condense_policy_tests.rs"]
mod tests;
