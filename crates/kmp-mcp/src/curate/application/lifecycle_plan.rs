//! The write-time lifecycle proposal (DESIGN L4 4e): a new fact that names
//! the same principal anchor as a current fact of its about, and is an entry
//! of the same kind, may replace it or report its later state. The review
//! proposes `supersedes` or `updates_state`; the writer declares one, in its
//! own why, or neither. Optionally Jev reads each such pair once.

use std::collections::BTreeMap;

use kmp_proto_mapping::v1beta1::PartnerShortlist;
use serde_json::json;

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::judgement_plan::{excerpt, text_of};
use crate::curate::domain::candidate_pair::CandidatePair;
use crate::curate::domain::pair_origin::PairOrigin;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;

/// Most lifecycle proposals for one new fact: the latest current facts that
/// share its anchor and kind.
const PER_FACT: usize = 3;
const LIFECYCLE_CHARS: usize = 1_000;

/// The relation a writer is offered for a lifecycle pair before any reading.
pub(crate) const PROPOSED_REL: &str = "supersedes";

/// Jev's options for a lifecycle pair, and the relation each proposes; the
/// ones mapped to `None` withdraw the proposal.
pub(crate) const CHOICES: [(&str, Option<&str>); 5] = [
    ("duplicate", Some("restates")),
    ("update_state", Some("updates_state")),
    ("supersede", Some("supersedes")),
    ("contradict", Some("contradicts")),
    ("novel", None),
];

/// For each focused fact, the current facts of its about that name its
/// principal anchor and share its entry kind, latest first, at most
/// [`PER_FACT`], as pairs `(focused → other)`. Declared pairs are left out.
pub(crate) fn lifecycle_candidates(
    material: &CurateMaterial,
    focus: &[String],
) -> Vec<CandidatePair> {
    let mut shortlists = BTreeMap::new();
    let mut pairs = Vec::new();
    for focused in focus {
        let Some(fact) = material.fact(focused) else {
            continue;
        };
        if fact.kind.is_empty() {
            continue;
        }
        let shortlist = shortlists.entry(fact.about.clone()).or_insert_with(|| {
            PartnerShortlist::over(
                material
                    .facts
                    .iter()
                    .filter(|other| other.about == fact.about)
                    .map(|other| (other.reference.as_str(), other.text.as_str())),
            )
        });
        let partners = shortlist.partners(focused, &fact.text, 0);
        let Some(anchor) = partners.principal_anchor else {
            continue;
        };
        let latest = partners
            .sharing_principal
            .iter()
            .rev()
            .filter(|other| !focus.contains(other) && !declared(material, focused, other))
            .filter(|other| material.fact(other).is_some_and(|o| o.kind == fact.kind))
            .take(PER_FACT)
            .map(|other| CandidatePair {
                from: focused.clone(),
                to: other.clone(),
                origin: PairOrigin::Lifecycle {
                    anchor: anchor.clone(),
                    kind: fact.kind.clone(),
                },
                crosses_abouts: false,
            })
            .collect::<Vec<_>>();
        pairs.extend(latest);
    }
    pairs
}

/// One choice per lifecycle pair (`l<n>`): how the new fact stands to the
/// current one, dated.
pub(crate) fn lifecycle_request(
    material: &CurateMaterial,
    pairs: &[CandidatePair],
) -> JudgementRequest {
    let questions = pairs
        .iter()
        .enumerate()
        .map(|(n, pair)| {
            (
                format!("l{n}"),
                JudgementQuestion::Choice {
                    instructions: json!({
                        "new": excerpt(&text_of(material, &pair.from), LIFECYCLE_CHARS),
                        "old": excerpt(&text_of(material, &pair.to), LIFECYCLE_CHARS),
                        "question": "Both name the same identifier. How does `new` stand to `old`? duplicate: it says the same thing again. update_state: it reports a later state of the same thing. supersede: it replaces `old`, which no longer holds. contradict: both cannot hold and neither is later. novel: it is about something else that happens to share the identifier.",
                    }),
                    options: CHOICES.iter().map(|(option, _)| (*option).to_string()).collect(),
                },
            )
        })
        .collect();
    JudgementRequest {
        state: json!("A memory just written and an earlier one of the same knowledge base."),
        questions,
    }
}

/// The relation a Jev choice proposes, or `None` when it withdraws it.
pub(crate) fn proposed_by_choice(choice: &str) -> Option<&'static str> {
    CHOICES
        .iter()
        .find(|(option, _)| *option == choice)
        .and_then(|(_, rel)| *rel)
}

fn declared(material: &CurateMaterial, a: &str, b: &str) -> bool {
    material
        .declared
        .iter()
        .any(|link| (link.from == a && link.to == b) || (link.from == b && link.to == a))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curate::domain::curate_fact::CurateFact;
    use crate::curate::domain::declared_link::DeclaredLink;

    fn fact(reference: &str, kind: &str, text: &str) -> CurateFact {
        CurateFact {
            reference: reference.into(),
            about: "a".into(),
            kind: kind.into(),
            text: text.into(),
            occurred: None,
            labels: Vec::new(),
        }
    }

    fn material(declared: Vec<DeclaredLink>) -> CurateMaterial {
        CurateMaterial {
            facts: vec![
                fact("old1", "decision", "Deploy 2.4.1 is scheduled for Friday."),
                fact("other", "observation", "Deploy 2.4.1 reached production."),
                fact("old2", "decision", "Deploy 2.4.1 needs a second approver."),
                fact("far", "decision", "Invoices are generated as PDF."),
                fact("new", "decision", "Deploy 2.4.1 moves to Monday."),
            ],
            declared,
            pairs: Vec::new(),
            selection: "fp".into(),
            past: Vec::new(),
        }
    }

    #[test]
    fn a_shared_principal_anchor_and_kind_is_proposed_latest_first() {
        let pairs = lifecycle_candidates(&material(Vec::new()), &["new".to_string()]);
        let to = pairs.iter().map(|p| p.to.as_str()).collect::<Vec<_>>();
        assert_eq!(to, ["old2", "old1"], "the observation is another kind");
        assert!(pairs.iter().all(|p| p.from == "new"));
        assert_eq!(
            pairs[0].origin,
            PairOrigin::Lifecycle {
                anchor: "2.4.1".into(),
                kind: "decision".into()
            }
        );
    }

    #[test]
    fn a_declared_pair_or_a_fact_without_anchor_proposes_nothing() {
        let declared = vec![DeclaredLink {
            from: "new".into(),
            to: "old2".into(),
            rel: "supersedes".into(),
            why: "w".into(),
            evidence: "e".into(),
        }];
        let pairs = lifecycle_candidates(&material(declared), &["new".to_string()]);
        assert_eq!(
            pairs.iter().map(|p| p.to.as_str()).collect::<Vec<_>>(),
            ["old1"]
        );
        assert!(lifecycle_candidates(&material(Vec::new()), &["far".to_string()]).is_empty());
    }

    #[test]
    fn the_choice_maps_to_a_relation_or_withdraws_it() {
        assert_eq!(proposed_by_choice("supersede"), Some("supersedes"));
        assert_eq!(proposed_by_choice("update_state"), Some("updates_state"));
        assert_eq!(proposed_by_choice("duplicate"), Some("restates"));
        assert_eq!(proposed_by_choice("contradict"), Some("contradicts"));
        assert_eq!(proposed_by_choice("novel"), None);
        assert_eq!(proposed_by_choice("other"), None);
        let request = lifecycle_request(
            &material(Vec::new()),
            &lifecycle_candidates(&material(Vec::new()), &["new".to_string()]),
        );
        assert_eq!(request.questions.len(), 2);
        assert!(request.questions.contains_key("l0"));
    }
}
