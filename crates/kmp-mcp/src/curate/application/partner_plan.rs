use std::collections::BTreeMap;

use serde_json::json;

use crate::curate::application::judgement_plan::excerpt;
use crate::curate::domain::curate_fact::CurateFact;
use crate::curate::domain::curate_thresholds::{NONE, PARTNER_AT};
use crate::curate::domain::partner_cap::PartnerCap;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;

const PARTNER_CHARS: usize = 400;
const PARTNER_ORPHANS: usize = 30;

/// The Jev partner round of one about in a review without `focus`: for each
/// orphan (at most 30), the fact of the about with the most direct relation
/// to it, or none. Facts are keyed `f<n>` in the about's order.
///
/// An about of at most 255 facts is one request, one choice per orphan over
/// every other fact, as it always was. A larger one (up to the cap, at most
/// 512) cannot be one choice: a choice offers at most 255 options. It is read
/// in two deterministic stages: the facts are cut into even windows of at
/// most 239 ([`PartnerCap::windows`]); each window is one request whose
/// choices offer that window's facts and `none`, and gives each orphan at
/// most one winner; an orphan with two or more winners then gets a final
/// choice between them and `none`. One winner stands as its window chose it.
/// Every stage is a judgement of its own, so every answer goes through the
/// verdict book.
pub(crate) struct PartnerPlan<'a> {
    facts: Vec<&'a CurateFact>,
    /// The orphans asked, as `(key, fact)`.
    orphans: Vec<(String, &'a CurateFact)>,
}

impl<'a> PartnerPlan<'a> {
    /// None when the about is larger than `cap` or has nothing to pair.
    pub(crate) fn new(
        facts: &[&'a CurateFact],
        orphans: &[&'a CurateFact],
        cap: PartnerCap,
    ) -> Option<Self> {
        if facts.len() < 2 || !cap.admits(facts.len()) || orphans.is_empty() {
            return None;
        }
        let orphans = orphans
            .iter()
            .take(PARTNER_ORPHANS)
            .filter_map(|orphan| {
                facts
                    .iter()
                    .position(|fact| fact.reference == orphan.reference)
                    .map(|n| (format!("f{n}"), *orphan))
            })
            .collect::<Vec<_>>();
        (!orphans.is_empty()).then(|| Self {
            facts: facts.to_vec(),
            orphans,
        })
    }

    /// The ref behind a key `f<n>`.
    pub(crate) fn reference(&self, key: &str) -> Option<&str> {
        let n = key.strip_prefix('f')?.parse::<usize>().ok()?;
        self.facts.get(n).map(|fact| fact.reference.as_str())
    }

    /// The first stage: one request, or one per window.
    pub(crate) fn first_requests(&self) -> Vec<JudgementRequest> {
        PartnerCap::windows(self.facts.len())
            .into_iter()
            .map(|window| {
                let offered = window.map(|n| format!("f{n}")).collect::<Vec<_>>();
                self.request(
                    self.orphans
                        .iter()
                        .map(|(own, _)| (own.clone(), offered.clone()))
                        .collect(),
                )
            })
            .collect()
    }

    /// What the first stage decided: the partners that stand as chosen,
    /// `(orphan key, partner key)` (an
    /// orphan read whole, or with one window winner), and the final choice
    /// for orphans with two or more winners, when there is any. `answers`
    /// are the first stage's, in the order of [`Self::first_requests`].
    pub(crate) fn after_first(
        &self,
        answers: &[BTreeMap<String, JudgementAnswer>],
    ) -> (Vec<(String, String)>, Option<JudgementRequest>) {
        let mut winners = BTreeMap::<&str, Vec<String>>::new();
        for window in answers {
            for (own, _) in &self.orphans {
                if let Some(partner) = window.get(own).and_then(chosen) {
                    winners.entry(own).or_default().push(partner);
                }
            }
        }
        let mut standing = Vec::new();
        let mut contested = Vec::new();
        for (own, _) in &self.orphans {
            match winners.remove(own.as_str()) {
                Some(mut found) if found.len() == 1 => {
                    standing.push((own.clone(), found.remove(0)));
                }
                Some(found) if found.len() > 1 => contested.push((own.clone(), found)),
                _ => {}
            }
        }
        let last = (!contested.is_empty()).then(|| self.request(contested));
        (standing, last)
    }

    /// The partners the final choice kept.
    pub(crate) fn after_final(
        &self,
        answers: &BTreeMap<String, JudgementAnswer>,
    ) -> Vec<(String, String)> {
        self.orphans
            .iter()
            .filter_map(|(own, _)| Some((own.clone(), answers.get(own).and_then(chosen)?)))
            .collect()
    }

    /// One request asking each `(orphan, offered)` a choice over `offered`
    /// (never the orphan itself) and `none`; the state holds every fact
    /// named, orphans included, as the about's order has them.
    fn request(&self, asked: Vec<(String, Vec<String>)>) -> JudgementRequest {
        let mut shown = std::collections::BTreeSet::new();
        let mut questions = BTreeMap::new();
        for (own, offered) in asked {
            // Offered in the keys' text order (`f0`, `f1`, `f10`, ...), as the
            // single choice always offered them: the same about asks the
            // same request, and recorded answers still apply.
            let options = offered
                .into_iter()
                .filter(|key| *key != own)
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .chain(std::iter::once(NONE.to_string()))
                .collect::<Vec<_>>();
            if options.len() < 2 {
                continue;
            }
            shown.extend(options.iter().filter(|key| *key != NONE).cloned());
            shown.insert(own.clone());
            questions.insert(
                own.clone(),
                JudgementQuestion::Choice {
                    instructions: json!(format!(
                        "Which fact in `facts` has the most direct relation to `facts.{own}`: \
                         it causes, explains, supports, contradicts, answers or repeats it? \
                         Answer none when no fact does."
                    )),
                    options,
                },
            );
        }
        let state = json!({ "facts": self.facts.iter().enumerate()
            .map(|(n, fact)| (format!("f{n}"), fact))
            .filter(|(key, _)| shown.contains(key))
            .map(|(key, fact)| (key, json!(excerpt(&fact.text, PARTNER_CHARS))))
            .collect::<serde_json::Map<_, _>>() });
        JudgementRequest { state, questions }
    }
}

/// The partner a choice names, when it names one confidently enough.
fn chosen(answer: &JudgementAnswer) -> Option<String> {
    match answer {
        JudgementAnswer::Choice {
            choice, confidence, ..
        } if choice != NONE && *confidence >= PARTNER_AT => Some(choice.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(n: usize) -> CurateFact {
        CurateFact {
            reference: format!("r{n}"),
            about: "a".into(),
            kind: String::new(),
            text: format!("fact number {n}"),
            occurred: None,
            labels: Vec::new(),
        }
    }

    fn picked(choice: &str, confidence: f64) -> JudgementAnswer {
        JudgementAnswer::Choice {
            choice: choice.into(),
            probabilities: BTreeMap::new(),
            confidence,
        }
    }

    fn options(request: &JudgementRequest, key: &str) -> Vec<String> {
        match &request.questions[key] {
            JudgementQuestion::Choice { options, .. } => options.clone(),
            JudgementQuestion::Noul { .. } => panic!("a choice"),
        }
    }

    #[test]
    fn an_about_of_at_most_255_facts_is_one_choice_over_every_other_fact() {
        let facts = (0..255).map(fact).collect::<Vec<_>>();
        let all = facts.iter().collect::<Vec<_>>();
        let plan = PartnerPlan::new(&all, &[&facts[3]], PartnerCap::named("512").expect("cap"))
            .expect("a round");
        let requests = plan.first_requests();
        assert_eq!(requests.len(), 1);
        let offered = options(&requests[0], "f3");
        assert_eq!(offered.len(), 255, "254 facts and none");
        assert!(!offered.contains(&"f3".to_string()));
        assert_eq!(
            requests[0].state["facts"].as_object().map(|f| f.len()),
            Some(255)
        );
        assert_eq!(plan.reference("f7"), Some("r7"));
        let (standing, last) =
            plan.after_first(&[BTreeMap::from([("f3".into(), picked("f9", 0.8))])]);
        assert_eq!(standing, vec![("f3".to_string(), "f9".to_string())]);
        assert!(last.is_none());
    }

    #[test]
    fn past_the_cap_or_without_orphans_there_is_no_round() {
        let facts = (0..121).map(fact).collect::<Vec<_>>();
        let all = facts.iter().collect::<Vec<_>>();
        assert!(PartnerPlan::new(&all, &[&facts[0]], PartnerCap::DEFAULT).is_none());
        assert!(PartnerPlan::new(&all[..120], &[&facts[0]], PartnerCap::DEFAULT).is_some());
        assert!(PartnerPlan::new(&all[..120], &[], PartnerCap::DEFAULT).is_none());
    }

    #[test]
    fn a_larger_about_is_read_in_windows_and_contested_winners_get_a_final_choice() {
        let facts = (0..512).map(fact).collect::<Vec<_>>();
        let all = facts.iter().collect::<Vec<_>>();
        let orphans = [&facts[0], &facts[200], &facts[400], &facts[500]];
        let plan = PartnerPlan::new(&all, &orphans, PartnerCap::named("512").expect("cap"))
            .expect("a round");
        let requests = plan.first_requests();
        assert_eq!(requests.len(), 3, "171 + 171 + 170");
        assert_eq!(requests, plan.first_requests(), "deterministic");
        for request in &requests {
            for question in request.questions.values() {
                let JudgementQuestion::Choice { options, .. } = question else {
                    panic!("a choice");
                };
                assert!(options.len() <= 240, "{}", options.len());
            }
            // The state shows the window and every orphan asked, and only them.
            let shown = request.state["facts"].as_object().expect("facts");
            assert!(shown.contains_key("f0") && shown.contains_key("f500"));
            assert!(shown.len() <= 171 + 4);
        }
        assert_eq!(
            options(&requests[0], "f200").len(),
            172,
            "171 facts and none"
        );
        assert_eq!(options(&requests[0], "f0").len(), 171, "never itself");
        assert_eq!(options(&requests[1], "f0")[0], "f171");

        let answers = vec![
            BTreeMap::from([
                ("f0".to_string(), picked("f10", 0.9)),
                ("f200".to_string(), picked("f20", 0.9)),
                ("f400".to_string(), picked("f30", 0.3)),
            ]),
            BTreeMap::from([
                ("f0".to_string(), picked("none", 0.9)),
                ("f200".to_string(), picked("f250", 0.7)),
                ("f400".to_string(), picked("f300", 0.8)),
            ]),
            BTreeMap::from([("f200".to_string(), picked("f450", 0.6))]),
        ];
        let (standing, last) = plan.after_first(&answers);
        assert_eq!(
            standing,
            vec![
                ("f0".to_string(), "f10".to_string()),
                ("f400".to_string(), "f300".to_string())
            ],
            "one confident winner stands; a weak one is no winner"
        );
        let last = last.expect("f200 has three winners");
        assert_eq!(last.questions.len(), 1);
        assert_eq!(options(&last, "f200"), vec!["f20", "f250", "f450", "none"]);
        assert_eq!(
            last.state["facts"]
                .as_object()
                .map(|f| f.keys().cloned().collect::<Vec<_>>()),
            Some(vec![
                "f20".into(),
                "f200".into(),
                "f250".into(),
                "f450".into()
            ])
        );
        let kept = plan.after_final(&BTreeMap::from([(
            "f200".to_string(),
            picked("f250", 0.55),
        )]));
        assert_eq!(kept, vec![("f200".to_string(), "f250".to_string())]);
        assert!(
            plan.after_final(&BTreeMap::from([("f200".to_string(), picked("none", 0.9))]))
                .is_empty()
        );
    }
}
