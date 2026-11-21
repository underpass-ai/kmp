//! How code reads the answers to the questions `judgement_plan` asks: the
//! decisions live here, never in the model (TypeSafe's truth table).

use crate::curate::domain::jev_verdict::JevVerdict;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::noul_reading::NoulReading;

/// The probability of yes of the noul `key`, when it was answered.
pub(crate) fn yes_of(response: &JudgementResponse, key: &str) -> Option<f64> {
    match response.answers.get(key) {
        Some(JudgementAnswer::Noul { yes }) => Some(*yes),
        _ => None,
    }
}

/// How the noul `key` reads, when it was answered.
pub(crate) fn reading_of(response: &JudgementResponse, key: &str) -> Option<NoulReading> {
    yes_of(response, key).map(NoulReading::of)
}

/// A choice answer as the review reports it.
pub(crate) fn verdict_of(answer: &JudgementAnswer) -> Option<JevVerdict> {
    match answer {
        JudgementAnswer::Choice {
            choice,
            probabilities,
            confidence,
        } => Some(JevVerdict {
            choice: choice.clone(),
            probabilities: probabilities.clone(),
            confidence: *confidence,
        }),
        JudgementAnswer::Noul { .. } | JudgementAnswer::Score { .. } => None,
    }
}

/// The type of pair `n` of a `pair_request`: its typing choice, read only
/// when its companion (`r<n>`, any direct relation?) reads yes. Unsettled,
/// no, or unanswered: not proposed.
pub(crate) fn typed_pair(response: &JudgementResponse, n: usize) -> Option<JevVerdict> {
    if reading_of(response, &format!("r{n}")) != Some(NoulReading::Yes) {
        return None;
    }
    response.answers.get(&format!("t{n}")).and_then(verdict_of)
}

/// The probability of the option a choice chose: what a gate reads, never
/// its `confidence`, which rescales the top probability by the number of
/// options offered.
pub(crate) fn chosen_probability(verdict: &JevVerdict) -> f64 {
    verdict
        .probabilities
        .get(&verdict.choice)
        .copied()
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn response(answers: Vec<(&str, JudgementAnswer)>) -> JudgementResponse {
        let mut response = JudgementResponse::empty("jev");
        response.answers = answers
            .into_iter()
            .map(|(key, answer)| (key.to_string(), answer))
            .collect();
        response
    }

    fn choice(top: &str, p: f64) -> JudgementAnswer {
        JudgementAnswer::Choice {
            choice: top.into(),
            probabilities: BTreeMap::from([(top.to_string(), p), ("other".to_string(), 1.0 - p)]),
            confidence: 0.1,
        }
    }

    #[test]
    fn a_pair_is_typed_only_when_its_companion_reads_yes() {
        let typed = |companion: f64| {
            typed_pair(
                &response(vec![
                    ("t0", choice("supports", 0.6)),
                    ("r0", JudgementAnswer::Noul { yes: companion }),
                ]),
                0,
            )
        };
        assert_eq!(typed(0.9).map(|v| v.choice), Some("supports".into()));
        assert!(typed(0.5).is_none(), "unsettled is not a proposal");
        assert!(typed(0.1).is_none());
        assert!(
            typed_pair(&response(vec![("t0", choice("supports", 0.6))]), 0).is_none(),
            "an unanswered companion is not judged, not a yes"
        );
    }

    #[test]
    fn a_gate_reads_the_chosen_probability_not_the_confidence() {
        let verdict = verdict_of(&choice("supports", 0.6)).expect("choice");
        assert!((chosen_probability(&verdict) - 0.6).abs() < 1e-9);
    }
}
