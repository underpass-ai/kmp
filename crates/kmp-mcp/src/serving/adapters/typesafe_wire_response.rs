use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_response::JudgementResponse;

/// The provider body as received. Nothing in it is trusted until
/// `into_response` has matched every answer to the question it answers.
#[derive(Debug, Deserialize)]
pub(super) struct TypeSafeWireResponse {
    model: String,
    answers: BTreeMap<String, Value>,
    #[serde(default)]
    usage: Value,
}

impl TypeSafeWireResponse {
    pub(super) fn into_response(
        self,
        model: &str,
        questions: &BTreeMap<String, JudgementQuestion>,
    ) -> Result<JudgementResponse, String> {
        if self.model != model {
            return Err("TypeSafe answered with a different model than configured".into());
        }
        if self.answers.len() != questions.len()
            || !questions.keys().all(|key| self.answers.contains_key(key))
        {
            return Err("TypeSafe answers do not match the questions sent".into());
        }
        let mut answers = BTreeMap::new();
        for (key, question) in questions {
            answers.insert(key.clone(), typed_answer(&self.answers[key], question)?);
        }
        Ok(JudgementResponse {
            model: self.model,
            answers,
            input_tokens: self
                .usage
                .get("input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            requests: 1,
            elapsed_us: 0,
        })
    }
}

fn probability(value: Option<&Value>) -> Result<f64, String> {
    value
        .and_then(Value::as_f64)
        .filter(|p| p.is_finite() && (0.0..=1.0).contains(p))
        .ok_or_else(|| "TypeSafe returned a probability outside [0, 1]".to_string())
}

fn typed_answer(raw: &Value, question: &JudgementQuestion) -> Result<JudgementAnswer, String> {
    match question {
        JudgementQuestion::Noul { .. } => {
            if raw.get("type").and_then(Value::as_str) != Some("noul") {
                return Err("TypeSafe answered a noul with another type".into());
            }
            Ok(JudgementAnswer::Noul {
                yes: probability(raw.get("noul"))?,
            })
        }
        JudgementQuestion::Choice { options, .. } => {
            if raw.get("type").and_then(Value::as_str) != Some("choice") {
                return Err("TypeSafe answered a choice with another type".into());
            }
            let choice = raw
                .get("choice")
                .and_then(Value::as_str)
                .filter(|choice| options.iter().any(|option| option == choice))
                .ok_or("TypeSafe chose an option that was not offered")?
                .to_string();
            let listed = raw
                .get("probabilities")
                .and_then(Value::as_object)
                .filter(|listed| listed.len() == options.len())
                .ok_or("TypeSafe probabilities do not cover the options offered")?;
            let mut probabilities = BTreeMap::new();
            for option in options {
                probabilities.insert(option.clone(), probability(listed.get(option))?);
            }
            Ok(JudgementAnswer::Choice {
                choice,
                probabilities,
                confidence: probability(raw.get("confidence"))?,
            })
        }
        JudgementQuestion::Score { levels, .. } => {
            if raw.get("type").and_then(Value::as_str) != Some("score") {
                return Err("TypeSafe answered a score with another type".into());
            }
            // The legend names each level by its index; it must be the
            // scale that was asked, in its order.
            let legend = raw
                .get("legend")
                .and_then(Value::as_object)
                .filter(|legend| {
                    legend.len() == levels.len()
                        && levels.iter().enumerate().all(|(index, level)| {
                            legend.get(&index.to_string()).and_then(Value::as_str)
                                == Some(level.as_str())
                        })
                })
                .ok_or("TypeSafe scored on another scale than the one asked")?;
            let listed = raw
                .get("probabilities")
                .and_then(Value::as_object)
                .filter(|listed| listed.len() == legend.len())
                .ok_or("TypeSafe probabilities do not cover the levels asked")?;
            let probabilities = (0..levels.len())
                .map(|index| probability(listed.get(&index.to_string())))
                .collect::<Result<Vec<_>, _>>()?;
            let score = raw
                .get("score")
                .and_then(Value::as_f64)
                .filter(|score| {
                    score.is_finite() && (0.0..=(levels.len() - 1) as f64).contains(score)
                })
                .ok_or("TypeSafe returned a score outside its scale")?;
            Ok(JudgementAnswer::Score {
                score,
                probabilities,
                confidence: probability(raw.get("confidence"))?,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn questions() -> BTreeMap<String, JudgementQuestion> {
        BTreeMap::from([
            (
                "n".to_string(),
                JudgementQuestion::Noul {
                    instructions: json!("q"),
                },
            ),
            (
                "c".to_string(),
                JudgementQuestion::Choice {
                    instructions: json!("q"),
                    options: vec!["a".into(), "b".into()],
                },
            ),
        ])
    }

    fn wire(body: Value) -> TypeSafeWireResponse {
        serde_json::from_value(body).expect("wire body")
    }

    fn valid() -> Value {
        json!({
            "model": "jev-1.13.0",
            "answers": {
                "n": {"type": "noul", "noul": 0.95},
                "c": {"type": "choice", "choice": "a",
                      "probabilities": {"a": 0.8, "b": 0.2}, "confidence": 0.7}
            },
            "usage": {"input_tokens": 296, "output_tokens": 20}
        })
    }

    #[test]
    fn valid_answers_are_typed_and_usage_is_kept() {
        let response = wire(valid())
            .into_response("jev-1.13.0", &questions())
            .expect("valid");
        assert_eq!(response.input_tokens, 296);
        assert_eq!(response.requests, 1);
        assert_eq!(response.answers["n"], JudgementAnswer::Noul { yes: 0.95 });
        assert_eq!(
            response.answers["c"],
            JudgementAnswer::Choice {
                choice: "a".into(),
                probabilities: BTreeMap::from([("a".into(), 0.8), ("b".into(), 0.2)]),
                confidence: 0.7
            }
        );
    }

    fn scored() -> BTreeMap<String, JudgementQuestion> {
        BTreeMap::from([(
            "s".to_string(),
            JudgementQuestion::Score {
                instructions: json!("q"),
                levels: vec!["answers".into(), "partly".into(), "unrelated".into()],
            },
        )])
    }

    fn valid_score() -> Value {
        json!({
            "model": "jev-1.13.0",
            "answers": {"s": {"type": "score", "score": 0.4, "confidence": 0.8,
                "legend": {"0": "answers", "1": "partly", "2": "unrelated"},
                "probabilities": {"0": 0.7, "1": 0.2, "2": 0.1}}},
            "usage": {"input_tokens": 354}
        })
    }

    #[test]
    fn a_score_keeps_its_levels_in_the_order_asked() {
        let response = wire(valid_score())
            .into_response("jev-1.13.0", &scored())
            .expect("valid");
        assert_eq!(
            response.answers["s"],
            JudgementAnswer::Score {
                score: 0.4,
                probabilities: vec![0.7, 0.2, 0.1],
                confidence: 0.8
            }
        );
        let mut cases = Vec::new();
        let mut other_scale = valid_score();
        other_scale["answers"]["s"]["legend"]["1"] = json!("unrelated");
        cases.push(other_scale);
        let mut short = valid_score();
        short["answers"]["s"]["probabilities"] = json!({"0": 1.0});
        cases.push(short);
        let mut outside = valid_score();
        outside["answers"]["s"]["score"] = json!(2.5);
        cases.push(outside);
        let mut noul = valid_score();
        noul["answers"]["s"] = json!({"type": "noul", "noul": 0.5});
        cases.push(noul);
        for case in cases {
            assert!(
                wire(case.clone())
                    .into_response("jev-1.13.0", &scored())
                    .is_err(),
                "accepted {case}"
            );
        }
    }

    #[test]
    fn answers_that_do_not_match_the_questions_are_rejected() {
        let mut cases = Vec::new();
        let mut other_model = valid();
        other_model["model"] = json!("jev-1.14.0");
        cases.push(other_model);
        let mut missing = valid();
        missing["answers"]
            .as_object_mut()
            .expect("answers")
            .remove("n");
        cases.push(missing);
        let mut extra = valid();
        extra["answers"]["x"] = json!({"type": "noul", "noul": 0.1});
        cases.push(extra);
        let mut out_of_range = valid();
        out_of_range["answers"]["n"]["noul"] = json!(1.2);
        cases.push(out_of_range);
        let mut wrong_type = valid();
        wrong_type["answers"]["n"] = valid()["answers"]["c"].clone();
        cases.push(wrong_type);
        let mut not_offered = valid();
        not_offered["answers"]["c"]["choice"] = json!("z");
        cases.push(not_offered);
        let mut partial = valid();
        partial["answers"]["c"]["probabilities"] = json!({"a": 1.0});
        cases.push(partial);
        let mut no_confidence = valid();
        no_confidence["answers"]["c"]
            .as_object_mut()
            .expect("choice")
            .remove("confidence");
        cases.push(no_confidence);
        for case in cases {
            assert!(
                wire(case.clone())
                    .into_response("jev-1.13.0", &questions())
                    .is_err(),
                "accepted {case}"
            );
        }
    }
}
