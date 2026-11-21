use std::collections::BTreeMap;

use serde_json::json;

use crate::curate::application::fact_keys::fact_keys;
use crate::curate::application::judgement_plan::{criteria_noul, excerpt};
use crate::curate::application::pair_reading::{chosen_probability, reading_of, verdict_of};
use crate::curate::application::relation_wording::partner;
use crate::curate::domain::curate_fact::CurateFact;
use crate::curate::domain::curate_thresholds::PARTNER_AT;
use crate::curate::domain::partner_cap::PartnerCap;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::noul_reading::NoulReading;

const PARTNER_CHARS: usize = 400;
const PARTNER_ORPHANS: usize = 30;
/// What a companion question's key adds to its orphan's key (question keys
/// are not sent to the model).
const ANY: &str = ".any";

/// The Jev partner round of one about in a review without `focus`: for each
/// orphan (at most 30), the fact of the about with the most direct relation
/// to it, read only when its companion (does any fact relate to it?) reads
/// yes and the chosen fact holds at least [`PARTNER_AT`] of the probability.
/// Facts are keyed by their first informative words ([`fact_keys`]), in the
/// about's order; the options are those keys, with no `none`.
///
/// An about of at most 255 facts is one request, one choice per orphan over
/// every other fact. A larger one (up to the cap, at most 512) cannot be one
/// choice: a choice offers at most 255 options. It is read in two
/// deterministic stages: the facts are cut into even windows of at most 239
/// ([`PartnerCap::windows`]); each window is one request whose choices offer
/// that window's facts, and gives each orphan at most one winner; an orphan
/// with two or more winners then gets a final choice between them. One
/// winner stands as its window chose it. Every stage is a judgement of its
/// own, so every answer goes through the verdict book.
pub(crate) struct PartnerPlan<'a> {
    facts: Vec<&'a CurateFact>,
    /// Each fact's key, in the about's order.
    keys: Vec<String>,
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
        let keys = fact_keys(facts.iter().map(|fact| fact.text.as_str()));
        let orphans = orphans
            .iter()
            .take(PARTNER_ORPHANS)
            .filter_map(|orphan| {
                facts
                    .iter()
                    .position(|fact| fact.reference == orphan.reference)
                    .map(|n| (keys[n].clone(), *orphan))
            })
            .collect::<Vec<_>>();
        (!orphans.is_empty()).then(|| Self {
            facts: facts.to_vec(),
            keys,
            orphans,
        })
    }

    /// The ref behind a key.
    pub(crate) fn reference(&self, key: &str) -> Option<&str> {
        self.facts
            .get(self.position(key)?)
            .map(|fact| fact.reference.as_str())
    }

    /// Where the fact keyed `key` stands in the about's order.
    pub(crate) fn position(&self, key: &str) -> Option<usize> {
        self.keys.iter().position(|own| own == key)
    }

    /// The first stage: one request, or one per window.
    pub(crate) fn first_requests(&self) -> Vec<JudgementRequest> {
        PartnerCap::windows(self.facts.len())
            .into_iter()
            .map(|window| {
                let offered = window.map(|n| self.keys[n].clone()).collect::<Vec<_>>();
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
    /// `(orphan key, partner key)` (an orphan read whole, or with one window
    /// winner), and the final choice for orphans with two or more winners,
    /// when there is any. `answers` are the first stage's, in the order of
    /// [`Self::first_requests`].
    pub(crate) fn after_first(
        &self,
        answers: &[JudgementResponse],
    ) -> (Vec<(String, String)>, Option<JudgementRequest>) {
        let mut winners = BTreeMap::<&str, Vec<String>>::new();
        for window in answers {
            for (own, _) in &self.orphans {
                if let Some(partner) = chosen(window, own) {
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
    pub(crate) fn after_final(&self, answers: &JudgementResponse) -> Vec<(String, String)> {
        self.orphans
            .iter()
            .filter_map(|(own, _)| Some((own.clone(), chosen(answers, own)?)))
            .collect()
    }

    /// One request asking each `(orphan, offered)` a choice over `offered`
    /// (never the orphan itself) and its companion; the state holds every
    /// fact named, orphans included. An orphan offered fewer than two facts
    /// is not asked: a choice needs two options.
    fn request(&self, asked: Vec<(String, Vec<String>)>) -> JudgementRequest {
        let mut shown = std::collections::BTreeSet::new();
        let mut questions = BTreeMap::new();
        for (own, offered) in asked {
            let options = offered
                .into_iter()
                .filter(|key| *key != own)
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            if options.len() < 2 {
                continue;
            }
            shown.extend(options.iter().cloned());
            shown.insert(own.clone());
            let (choice, companion, when_true, when_false) = partner(&own);
            questions.insert(
                format!("{own}{ANY}"),
                criteria_noul(json!(companion), &when_true, &when_false),
            );
            questions.insert(own, JudgementQuestion::choice(json!(choice), options));
        }
        let state = json!({ "facts": self.keys.iter().zip(&self.facts)
            .filter(|(key, _)| shown.contains(*key))
            .map(|(key, fact)| (key.clone(), json!(excerpt(&fact.text, PARTNER_CHARS))))
            .collect::<serde_json::Map<_, _>>() });
        JudgementRequest { state, questions }
    }
}

/// The partner the choice for `own` names, when its companion reads yes and
/// the named fact holds enough of the probability (never `confidence`).
fn chosen(response: &JudgementResponse, own: &str) -> Option<String> {
    if reading_of(response, &format!("{own}{ANY}")) != Some(NoulReading::Yes) {
        return None;
    }
    let verdict = response.answers.get(own).and_then(verdict_of)?;
    (chosen_probability(&verdict) >= PARTNER_AT).then_some(verdict.choice)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serving::judgement_answer::JudgementAnswer;

    fn fact(n: usize) -> CurateFact {
        CurateFact {
            reference: format!("r{n}"),
            about: "a".into(),
            kind: String::new(),
            text: format!("fact number {n}"),
            occurred: None,
            occurred_at: None,
            labels: Vec::new(),
        }
    }

    fn key(n: usize) -> String {
        format!("fact_number_{n}")
    }

    /// A response choosing `choice` for `own` with probability `p`, its
    /// companion at `any`.
    fn answered(picks: &[(usize, &str, f64, f64)]) -> JudgementResponse {
        let mut response = JudgementResponse::empty("jev");
        for (own, choice, p, any) in picks {
            response.answers.insert(
                key(*own),
                JudgementAnswer::Choice {
                    choice: (*choice).into(),
                    probabilities: BTreeMap::from([((*choice).to_string(), *p)]),
                    // Confidence is never what decides.
                    confidence: 0.0,
                },
            );
            response.answers.insert(
                format!("{}{ANY}", key(*own)),
                JudgementAnswer::Noul { yes: *any },
            );
        }
        response
    }

    fn options(request: &JudgementRequest, key: &str) -> Vec<String> {
        match &request.questions[key] {
            JudgementQuestion::Choice { options, .. } => options.clone(),
            JudgementQuestion::Noul { .. }
            | JudgementQuestion::CriteriaNoul { .. }
            | JudgementQuestion::Score { .. } => {
                panic!("a choice")
            }
        }
    }

    #[test]
    fn an_about_of_at_most_255_facts_is_one_choice_over_every_other_fact_and_a_companion() {
        let facts = (0..255).map(fact).collect::<Vec<_>>();
        let all = facts.iter().collect::<Vec<_>>();
        let plan = PartnerPlan::new(&all, &[&facts[3]], PartnerCap::named("512").expect("cap"))
            .expect("a round");
        let requests = plan.first_requests();
        assert_eq!(requests.len(), 1);
        let offered = options(&requests[0], &key(3));
        assert_eq!(offered.len(), 254, "every other fact, no `none`");
        assert!(!offered.contains(&key(3)) && !offered.contains(&"none".to_string()));
        assert!(matches!(
            requests[0].questions[&format!("{}{ANY}", key(3))],
            JudgementQuestion::CriteriaNoul { .. }
        ));
        assert_eq!(
            requests[0].state["facts"].as_object().map(|f| f.len()),
            Some(255)
        );
        assert_eq!(plan.reference(&key(7)), Some("r7"));
        let (standing, last) = plan.after_first(&[answered(&[(3, &key(9), 0.8, 0.9)])]);
        assert_eq!(standing, vec![(key(3), key(9))]);
        assert!(last.is_none());
    }

    #[test]
    fn a_partner_needs_its_companion_to_read_yes_and_half_of_the_probability() {
        let facts = (0..20).map(fact).collect::<Vec<_>>();
        let all = facts.iter().collect::<Vec<_>>();
        let plan = PartnerPlan::new(&all, &[&facts[3]], PartnerCap::DEFAULT).expect("a round");
        let standing = |p: f64, any: f64| plan.after_first(&[answered(&[(3, &key(9), p, any)])]).0;
        assert_eq!(standing(0.6, 0.9).len(), 1);
        assert!(standing(0.6, 0.5).is_empty(), "an unsettled companion");
        assert!(standing(0.6, 0.2).is_empty(), "no fact relates");
        assert!(standing(0.4, 0.9).is_empty(), "the choice is split");
        let unanswered = JudgementResponse::empty("jev");
        assert!(plan.after_first(&[unanswered]).0.is_empty());
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
                if let JudgementQuestion::Choice { options, .. } = question {
                    assert!(options.len() <= 239, "{}", options.len());
                }
            }
            // The state shows the window and every orphan asked, and only them.
            let shown = request.state["facts"].as_object().expect("facts");
            assert!(shown.contains_key(&key(0)) && shown.contains_key(&key(500)));
            assert!(shown.len() <= 171 + 4);
        }
        assert_eq!(options(&requests[0], &key(200)).len(), 171);
        assert_eq!(options(&requests[0], &key(0)).len(), 170, "never itself");

        let answers = vec![
            answered(&[
                (0, &key(10), 0.9, 0.9),
                (200, &key(20), 0.9, 0.9),
                (400, &key(30), 0.3, 0.9),
            ]),
            answered(&[
                (0, &key(180), 0.9, 0.1),
                (200, &key(250), 0.7, 0.8),
                (400, &key(300), 0.8, 0.95),
            ]),
            answered(&[(200, &key(450), 0.6, 0.75)]),
        ];
        let (standing, last) = plan.after_first(&answers);
        assert_eq!(
            standing,
            vec![(key(0), key(10)), (key(400), key(300))],
            "one winner stands; a split choice or a no companion is no winner"
        );
        let last = last.expect("fact 200 has three winners");
        assert_eq!(last.questions.len(), 2, "its choice and its companion");
        let mut final_options = vec![key(20), key(250), key(450)];
        final_options.sort();
        assert_eq!(options(&last, &key(200)), final_options);
        let kept = plan.after_final(&answered(&[(200, &key(250), 0.55, 0.8)]));
        assert_eq!(kept, vec![(key(200), key(250))]);
        assert!(
            plan.after_final(&answered(&[(200, &key(250), 0.9, 0.2)]))
                .is_empty()
        );
    }
}
