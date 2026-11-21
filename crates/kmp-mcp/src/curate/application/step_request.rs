use std::collections::BTreeMap;

use serde_json::json;

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::fact_keys::fact_keys;
use crate::curate::application::judgement_plan::{PATH_CHARS, criteria_noul, excerpt, text_of};
use crate::curate::application::pair_reading::yes_of;
use crate::curate::application::relation_wording::{cause_step, next_step};
use crate::curate::domain::curate_thresholds::NONE;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::noul_reading::NoulReading;

/// The step round of a path search without a corridor: for each fact
/// (index `k`), which other fact is its direct consequence (`n<k>`) and which
/// its direct cause (`c<k>`), each beside its companion (`an<k>`, `ac<k>`: is
/// there one at all?). A chain needs both directions, and a fact can lead to more than one
/// thing. The facts are the state, dated, keyed by their first informative
/// words ([`fact_keys`]); the options are those keys, with no `none`. A fact
/// with a single candidate gets only the companion: that candidate is the
/// step when the companion reads yes.
///
/// With `neighbours` (a goal search's corridor, DESIGN L7), each fact's two
/// choices offer only the facts named for it instead of every other fact,
/// and the round keeps the form it had ([`Self::corridor`]): facts keyed
/// `f<n>`, a `none` option, no companion. Measured on 2026-11-21, the guide's
/// form proposed more wrong steps there, because a companion about the whole
/// walk reads yes for a step that is not among the few offered
/// (`jev-evaluation.md`).
pub(crate) struct StepRequest {
    pub request: JudgementRequest,
    /// The fact behind each key.
    pub refs: BTreeMap<String, String>,
    /// Per fact index: its key and the options it was offered (sorted).
    pub asked: Vec<(String, Vec<String>)>,
    /// Whether this is the corridor's round, asked in its earlier form.
    corridor: bool,
}

impl StepRequest {
    pub(crate) fn new(
        material: &CurateMaterial,
        refs: &[String],
        neighbours: Option<&BTreeMap<String, Vec<String>>>,
    ) -> Self {
        match neighbours {
            Some(neighbours) => Self::corridor(material, refs, neighbours),
            None => Self::guided(material, refs),
        }
    }

    /// The corridor's round, as it was asked before the guide's rewrite.
    fn corridor(
        material: &CurateMaterial,
        refs: &[String],
        neighbours: &BTreeMap<String, Vec<String>>,
    ) -> Self {
        let keys = refs
            .iter()
            .enumerate()
            .map(|(n, reference)| (format!("f{n}"), reference.clone()))
            .collect::<BTreeMap<_, _>>();
        let state = json!({ "facts": keys
            .iter()
            .map(|(key, reference)| (key.clone(), json!(excerpt(&text_of(material, reference), PATH_CHARS))))
            .collect::<serde_json::Map<_, _>>() });
        let key_of = keys
            .iter()
            .map(|(key, reference)| (reference.as_str(), key.as_str()))
            .collect::<BTreeMap<_, _>>();
        let mut questions = BTreeMap::new();
        let mut asked = Vec::new();
        for (own, reference) in &keys {
            let offered = |key: &String| {
                neighbours.get(reference).is_some_and(|options| {
                    options
                        .iter()
                        .any(|other| key_of.get(other.as_str()) == Some(&key.as_str()))
                })
            };
            let options = keys
                .keys()
                .filter(|key| *key != own && offered(key))
                .cloned()
                .chain(std::iter::once(NONE.to_string()))
                .collect::<Vec<_>>();
            questions.insert(
                format!("n{}", &own[1..]),
                JudgementQuestion::choice(
                    json!(format!(
                        "Which fact in `facts` is a direct consequence of `facts.{own}`: \
                     caused by it, decided because of it, or the next state of the same \
                     thing? Answer none when none is."
                    )),
                    options.clone(),
                ),
            );
            questions.insert(
                format!("c{}", &own[1..]),
                JudgementQuestion::choice(
                    json!(format!(
                        "Which fact in `facts` is a direct cause of `facts.{own}`: it led to \
                     it, it is why it was decided, or it is the earlier state of the same \
                     thing? Answer none when none is."
                    )),
                    options.clone(),
                ),
            );
            asked.push((own.clone(), options));
        }
        Self {
            request: JudgementRequest { state, questions },
            refs: keys,
            asked,
            corridor: true,
        }
    }

    /// The round written to TypeSafe's guide (every fact offered).
    fn guided(material: &CurateMaterial, refs: &[String]) -> Self {
        let named = fact_keys(refs.iter().map(|reference| {
            material
                .fact(reference)
                .map(|fact| fact.text.as_str())
                .unwrap_or_default()
        }));
        let state = json!({ "facts": named
            .iter()
            .zip(refs)
            .map(|(key, reference)| (key.clone(), json!(excerpt(&text_of(material, reference), PATH_CHARS))))
            .collect::<serde_json::Map<_, _>>() });
        let mut questions = BTreeMap::new();
        let mut asked = Vec::new();
        for (index, own) in named.iter().enumerate() {
            // Every other fact is offered, so the companion and the choice
            // read the same facts; with two facts in all, the one other fact
            // is asked through the companion alone.
            let mut options = named
                .iter()
                .filter(|key| *key != own)
                .cloned()
                .collect::<Vec<_>>();
            options.sort();
            if !options.is_empty() {
                for (prefix, (choice, companion, when_true, when_false)) in
                    [("n", next_step(own)), ("c", cause_step(own))]
                {
                    if options.len() >= 2 {
                        questions.insert(
                            format!("{prefix}{index}"),
                            JudgementQuestion::choice(json!(choice), options.clone()),
                        );
                    }
                    questions.insert(
                        format!("a{prefix}{index}"),
                        criteria_noul(json!(companion), &when_true, &when_false),
                    );
                }
            }
            asked.push((own.clone(), options));
        }
        Self {
            request: JudgementRequest { state, questions },
            refs: named.into_iter().zip(refs.iter().cloned()).collect(),
            asked,
            corridor: false,
        }
    }

    /// The steps the answers propose, `(from, to, probability)` by ref: from
    /// each fact, in each direction, only when its companion reads yes, the
    /// options at `step_at` or above, best first, at most `per_fact` (a lone
    /// candidate stands at its companion's probability). Unsettled, no or
    /// unanswered proposes nothing from that fact.
    pub(crate) fn read(
        &self,
        response: &JudgementResponse,
        step_at: f64,
        per_fact: usize,
    ) -> Vec<(String, String, f64)> {
        if self.corridor {
            return self.read_corridor(response, step_at, per_fact);
        }
        let mut steps = Vec::new();
        for (index, (own, offered)) in self.asked.iter().enumerate() {
            let Some(reference) = self.refs.get(own) else {
                continue;
            };
            for (question, forward) in [("n", true), ("c", false)] {
                let Some(companion) = yes_of(response, &format!("a{question}{index}")) else {
                    continue;
                };
                if NoulReading::of(companion) != NoulReading::Yes {
                    continue;
                }
                let ranked = match (
                    offered.as_slice(),
                    response.answers.get(&format!("{question}{index}")),
                ) {
                    ([only], _) => vec![(only.clone(), companion)],
                    (_, Some(JudgementAnswer::Choice { probabilities, .. })) => {
                        let mut ranked = probabilities
                            .iter()
                            .filter(|(_, p)| **p >= step_at)
                            .map(|(option, p)| (option.clone(), *p))
                            .collect::<Vec<_>>();
                        ranked.sort_by(|left, right| {
                            right
                                .1
                                .total_cmp(&left.1)
                                .then_with(|| left.0.cmp(&right.0))
                        });
                        ranked
                    }
                    _ => continue,
                };
                for (option, p) in ranked.into_iter().take(per_fact) {
                    let Some(other) = self.refs.get(&option) else {
                        continue;
                    };
                    let (from, to) = if forward {
                        (reference.clone(), other.clone())
                    } else {
                        (other.clone(), reference.clone())
                    };
                    steps.push((from, to, p));
                }
            }
        }
        steps
    }

    /// The corridor round's reading, as it was: options other than `none` at
    /// `step_at` or above, best first, at most `per_fact`.
    fn read_corridor(
        &self,
        response: &JudgementResponse,
        step_at: f64,
        per_fact: usize,
    ) -> Vec<(String, String, f64)> {
        let mut steps = Vec::new();
        for (own, reference) in &self.refs {
            for (question, forward) in [("n", true), ("c", false)] {
                let Some(JudgementAnswer::Choice { probabilities, .. }) =
                    response.answers.get(&format!("{question}{}", &own[1..]))
                else {
                    continue;
                };
                let mut options = probabilities
                    .iter()
                    .filter(|(option, p)| option.as_str() != NONE && **p >= step_at)
                    .collect::<Vec<_>>();
                options.sort_by(|left, right| {
                    right.1.total_cmp(left.1).then_with(|| left.0.cmp(right.0))
                });
                for (option, p) in options.into_iter().take(per_fact) {
                    let Some(other) = self.refs.get(option.as_str()) else {
                        continue;
                    };
                    let (from, to) = if forward {
                        (reference.clone(), other.clone())
                    } else {
                        (other.clone(), reference.clone())
                    };
                    steps.push((from, to, *p));
                }
            }
        }
        steps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curate::domain::curate_fact::CurateFact;

    fn material() -> CurateMaterial {
        let fact = |reference: &str, text: &str| CurateFact {
            reference: reference.into(),
            about: "a".into(),
            kind: String::new(),
            text: text.into(),
            occurred: None,
            occurred_at: None,
            labels: Vec::new(),
        };
        CurateMaterial {
            facts: vec![
                fact("x", "Billing batch stalled"),
                fact("y", "Invoice run slipped"),
                fact("z", "Office lunch moved"),
            ],
            declared: Vec::new(),
            pairs: Vec::new(),
            selection: "fp".into(),
            past: Vec::new(),
        }
    }

    #[test]
    fn steps_are_read_only_behind_a_yes_companion_and_options_carry_no_none() {
        let refs = ["x".to_string(), "y".to_string(), "z".to_string()];
        let steps = StepRequest::new(&material(), &refs, None);
        let JudgementQuestion::Choice { options, .. } = &steps.request.questions["n0"] else {
            panic!("a choice");
        };
        assert_eq!(options, &["invoice_run_slipped", "office_lunch_moved"]);
        let mut response = JudgementResponse::empty("jev");
        response.answers.insert(
            "n0".into(),
            JudgementAnswer::Choice {
                choice: "invoice_run_slipped".into(),
                probabilities: [
                    ("invoice_run_slipped".to_string(), 0.8),
                    ("office_lunch_moved".to_string(), 0.2),
                ]
                .into_iter()
                .collect(),
                confidence: 0.6,
            },
        );
        for (companion, expected) in [(0.9, 1), (0.5, 0), (0.1, 0)] {
            response
                .answers
                .insert("an0".into(), JudgementAnswer::Noul { yes: companion });
            let read = steps.read(&response, 0.3, 2);
            assert_eq!(read.len(), expected, "companion {companion}");
        }
        response
            .answers
            .insert("an0".into(), JudgementAnswer::Noul { yes: 0.9 });
        assert_eq!(
            steps.read(&response, 0.3, 2),
            [("x".to_string(), "y".to_string(), 0.8)],
            "x leads to y; the option under the step line is not a step"
        );
    }

    #[test]
    fn the_corridor_round_keeps_its_earlier_form() {
        let refs = ["x".to_string(), "y".to_string(), "z".to_string()];
        let neighbours = BTreeMap::from([
            ("x".to_string(), vec!["y".to_string()]),
            ("y".to_string(), vec!["x".to_string(), "z".to_string()]),
        ]);
        let steps = StepRequest::new(&material(), &refs, Some(&neighbours));
        assert!(
            steps.request.state["facts"].get("f0").is_some(),
            "f<n> keys"
        );
        let JudgementQuestion::Choice { options, .. } = &steps.request.questions["n0"] else {
            panic!("a choice");
        };
        assert_eq!(options, &["f1", "none"]);
        assert!(!steps.request.questions.contains_key("an0"), "no companion");
        let mut response = JudgementResponse::empty("jev");
        response.answers.insert(
            "n0".into(),
            JudgementAnswer::Choice {
                choice: "f1".into(),
                probabilities: [("f1".to_string(), 0.6), ("none".to_string(), 0.4)]
                    .into_iter()
                    .collect(),
                confidence: 0.2,
            },
        );
        assert_eq!(
            steps.read(&response, 0.3, 2),
            [("x".to_string(), "y".to_string(), 0.6)]
        );
    }
}
