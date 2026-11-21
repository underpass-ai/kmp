//! The write-time lifecycle proposal (DESIGN L4 4e): a new fact that names
//! the same principal anchor as a current fact of its about, and is an entry
//! of the same kind, may replace it or report its later state. The review
//! proposes `supersedes` or `updates_state`; the writer declares one, in its
//! own why, or neither. Optionally Jev reads each such pair once.

use std::collections::BTreeMap;

use kmp_proto_mapping::v1beta1::PartnerShortlist;
use serde_json::json;

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::judgement_plan::{criteria_noul, excerpt, order_of, text_of};
use crate::curate::application::pair_reading::{verdict_of, yes_of};
use crate::curate::application::relation_wording::{
    LIFECYCLE_OPTIONS, LIFECYCLE_Q, SAME_FALSE, SAME_Q, SAME_TRUE,
};
use crate::curate::domain::candidate_pair::CandidatePair;
use crate::curate::domain::jev_verdict::JevVerdict;
use crate::curate::domain::pair_origin::PairOrigin;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;

/// Most lifecycle proposals for one new fact: the latest current facts that
/// share its anchor and kind.
const PER_FACT: usize = 3;
const LIFECYCLE_CHARS: usize = 1_000;

/// The relation a writer is offered for a lifecycle pair before any reading.
pub(crate) const PROPOSED_REL: &str = "supersedes";

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

/// For each lifecycle pair, how the new fact stands to the current one
/// (`l<n>`, four described options) and the companion (`s<n>`): are they
/// about the same specific thing at all? The order they were recorded in is
/// stated when known; nothing is shared, so the state is empty.
pub(crate) fn lifecycle_request(
    material: &CurateMaterial,
    pairs: &[CandidatePair],
) -> JudgementRequest {
    let mut questions = BTreeMap::new();
    for (n, pair) in pairs.iter().enumerate() {
        let mut texts = json!({
            "new": excerpt(&text_of(material, &pair.from), LIFECYCLE_CHARS),
            "old": excerpt(&text_of(material, &pair.to), LIFECYCLE_CHARS),
        });
        if let Some(order) = order_of(material, (&pair.from, "new"), (&pair.to, "old")) {
            texts["order"] = json!(order);
        }
        let mut choice = texts.clone();
        choice["question"] = json!(LIFECYCLE_Q);
        questions.insert(
            format!("l{n}"),
            JudgementQuestion::described_choice(
                choice,
                LIFECYCLE_OPTIONS
                    .iter()
                    .map(|(option, means, _)| ((*option).to_string(), (*means).to_string()))
                    .collect(),
            ),
        );
        let mut same = texts;
        same["question"] = json!(SAME_Q);
        questions.insert(format!("s{n}"), criteria_noul(same, SAME_TRUE, SAME_FALSE));
    }
    JudgementRequest {
        state: json!({}),
        questions,
    }
}

/// The line the companion of a lifecycle pair (are `new` and `old` the same
/// specific thing?) must reach for the pair to be proposed. The site's truth
/// table, on the companion's probability `p`:
///
/// | `p` | reading | outcome |
/// |---|---|---|
/// | p >= 0.7 | yes | proposed, with the relation its choice maps to |
/// | 0.5 <= p < 0.7 | unsettled, upper half | proposed, the same way |
/// | 0.3 < p < 0.5 | unsettled, lower half | withdrawn |
/// | p <= 0.3 | no | withdrawn |
/// | no answer | not judged | withdrawn (the pair was asked; it is never typed unread) |
///
/// The owner chose 0.5 on 21 Nov 2026 between three measured options
/// (`docs/development/jev-evaluation.md`): at the guide's 0.7 the companion,
/// unsettled on "same setting, new value" pairs, withdrew most real
/// supersessions; 0.5 keeps about half of them with few false proposals.
pub(crate) const SAME_AT: f64 = 0.5;

/// The relation lifecycle pair `n` proposes and its reading, when its
/// companion reaches [`SAME_AT`]; below it, or unanswered, it is withdrawn.
pub(crate) fn lifecycle_reading(
    response: &JudgementResponse,
    n: usize,
    answer: &JudgementAnswer,
) -> Option<(&'static str, JevVerdict)> {
    if !yes_of(response, &format!("s{n}")).is_some_and(|same| same >= SAME_AT) {
        return None;
    }
    let verdict = verdict_of(answer)?;
    let rel = proposed_by_choice(&verdict.choice)?;
    Some((rel, verdict))
}

/// The relation a Jev choice proposes.
pub(crate) fn proposed_by_choice(choice: &str) -> Option<&'static str> {
    LIFECYCLE_OPTIONS
        .iter()
        .find(|(option, _, _)| *option == choice)
        .map(|(_, _, rel)| *rel)
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
            occurred_at: None,
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
    fn the_choice_maps_to_a_relation_and_its_companion_withdraws_it() {
        assert_eq!(proposed_by_choice("replaces"), Some("supersedes"));
        assert_eq!(proposed_by_choice("later_state"), Some("updates_state"));
        assert_eq!(proposed_by_choice("restates"), Some("restates"));
        assert_eq!(proposed_by_choice("conflicts"), Some("contradicts"));
        assert_eq!(proposed_by_choice("other"), None);
        let request = lifecycle_request(
            &material(Vec::new()),
            &lifecycle_candidates(&material(Vec::new()), &["new".to_string()]),
        );
        assert_eq!(
            request.questions.len(),
            4,
            "a choice and its companion per pair"
        );
        assert_eq!(request.state, json!({}));
        let JudgementQuestion::Choice { described, .. } = &request.questions["l0"] else {
            panic!("a choice");
        };
        assert_eq!(described.len(), 4, "every option described, no `novel`");
        let answer = JudgementAnswer::Choice {
            choice: "replaces".into(),
            probabilities: [("replaces".to_string(), 0.8)].into_iter().collect(),
            confidence: 0.7,
        };
        let mut response = JudgementResponse::empty("jev");
        for (yes, expected) in [
            (0.9, Some("supersedes")),
            (0.6, Some("supersedes")),
            (0.5, Some("supersedes")),
            (0.49, None),
            (0.31, None),
            (0.1, None),
        ] {
            response
                .answers
                .insert("s0".into(), JudgementAnswer::Noul { yes });
            assert_eq!(
                lifecycle_reading(&response, 0, &answer).map(|(rel, _)| rel),
                expected,
                "companion {yes}"
            );
        }
    }
}
