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
