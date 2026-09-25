use kmp_proto::v1beta1::{FactState, RelateResponse};

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::domain::candidate_pair::CandidatePair;
use crate::curate::domain::curate_fact::CurateFact;
use crate::curate::domain::declared_link::DeclaredLink;
use crate::curate::domain::fact_date::fact_date;
use crate::curate::domain::pair_origin::PairOrigin;

/// What curation reads from a relate reading: the current facts, what is
/// declared between them, and the kernel's proposed pairs that nothing
/// declares yet. Superseded and expired facts are history, not candidates.
pub(crate) fn relate_material(response: &RelateResponse) -> CurateMaterial {
    let facts = response
        .facts
        .iter()
        .filter(|fact| fact.state == FactState::Current as i32)
        .map(|fact| CurateFact {
            reference: fact.r#ref.clone(),
            about: fact.about.clone(),
            text: fact.text.clone(),
            occurred: fact
                .coordinates
                .iter()
                .filter_map(|coordinate| coordinate.occurred_at.as_ref())
                .map(|timestamp| timestamp.seconds)
                .min()
                .map(fact_date),
        })
        .collect::<Vec<_>>();
    let about_of = |reference: &str| {
        facts
            .iter()
            .find(|fact| fact.reference == reference)
            .map(|fact| fact.about.clone())
    };
    let declared = response
        .declared
        .iter()
        .map(|relation| DeclaredLink {
            from: relation.source_ref.clone(),
            to: relation.target_ref.clone(),
            rel: relation.rel.clone(),
            why: relation.why.clone(),
            evidence: relation.evidence.clone(),
        })
        .collect::<Vec<_>>();
    let joined = |left: &str, right: &str| {
        declared.iter().any(|link| {
            (link.from == left && link.to == right) || (link.from == right && link.to == left)
        })
    };
    let pairs = response
        .proposed
        .iter()
        .filter_map(|proposal| {
            let from_about = about_of(&proposal.from)?;
            let to_about = about_of(&proposal.to)?;
            (!joined(&proposal.from, &proposal.to)).then(|| CandidatePair {
                from: proposal.from.clone(),
                to: proposal.to.clone(),
                origin: PairOrigin::Kernel {
                    signals: proposal.proposed_by.clone(),
                    why: proposal.why.clone(),
                },
                crosses_abouts: from_about != to_about,
            })
        })
        .collect();
    CurateMaterial {
        facts,
        declared,
        pairs,
        selection: response.selection_fingerprint.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kmp_proto::v1beta1::{MemoryRelation, ProposedLink, RelatedFact};

    fn fact(reference: &str, about: &str, state: FactState) -> RelatedFact {
        RelatedFact {
            r#ref: reference.into(),
            about: about.into(),
            text: format!("text of {reference}"),
            state: state as i32,
            ..RelatedFact::default()
        }
    }

    fn proposal(from: &str, to: &str) -> ProposedLink {
        ProposedLink {
            from: from.into(),
            to: to.into(),
            proposed_by: vec!["identifier".into()],
            why: "both cite #4711".into(),
            ..ProposedLink::default()
        }
    }

    #[test]
    fn only_current_undeclared_pairs_become_candidates() {
        let response = RelateResponse {
            facts: vec![
                fact("a1", "a", FactState::Current),
                fact("a2", "a", FactState::Current),
                fact("a3", "a", FactState::Superseded),
                fact("b1", "b", FactState::Current),
            ],
            declared: vec![MemoryRelation {
                source_ref: "a2".into(),
                target_ref: "a1".into(),
                rel: "supports".into(),
                why: "w".into(),
                evidence: "e".into(),
                ..MemoryRelation::default()
            }],
            proposed: vec![
                proposal("a1", "a2"),
                proposal("a1", "a3"),
                proposal("a1", "b1"),
            ],
            selection_fingerprint: "fp".into(),
            ..RelateResponse::default()
        };
        let material = relate_material(&response);
        assert_eq!(material.selection, "fp");
        assert_eq!(material.facts.len(), 3);
        assert_eq!(material.declared.len(), 1);
        assert_eq!(material.pairs.len(), 1, "a1-a2 declared, a3 superseded");
        let pair = &material.pairs[0];
        assert_eq!((pair.from.as_str(), pair.to.as_str()), ("a1", "b1"));
        assert!(pair.crosses_abouts);
        assert_eq!(pair.origin.name(), "kernel");
        assert!(
            material.orphans().is_empty(),
            "a1 and a2 are declared, b1 is paired"
        );
    }
}
