use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::serving::judgement_question::JudgementQuestion;

/// The provider body for one batch: `state`, the pinned `model`, and one
/// typed question per key. A choice option carries its description when the
/// question gives one, and `null` otherwise.
pub(super) fn typesafe_request_body(
    model: &str,
    state: &Value,
    questions: &BTreeMap<String, JudgementQuestion>,
) -> Value {
    let questions = questions
        .iter()
        .map(|(key, question)| {
            let typed = match question {
                JudgementQuestion::Noul { instructions } => {
                    json!({"type": "noul", "instructions": instructions})
                }
                JudgementQuestion::CriteriaNoul {
                    instructions,
                    criteria,
                } => json!({
                    "type": "noul",
                    "instructions": instructions,
                    "criteria": {"true": criteria.when_true, "false": criteria.when_false},
                }),
                JudgementQuestion::Choice {
                    instructions,
                    options,
                    described,
                } => json!({
                    "type": "choice",
                    "instructions": instructions,
                    "criteria": options
                        .iter()
                        .map(|option| {
                            let description = described
                                .get(option)
                                .map_or(Value::Null, |text| Value::String(text.clone()));
                            (option.clone(), description)
                        })
                        .collect::<Map<_, _>>(),
                }),
                JudgementQuestion::Score {
                    instructions,
                    levels,
                } => json!({
                    "type": "score",
                    "instructions": instructions,
                    "criteria": levels,
                }),
            };
            (key.clone(), typed)
        })
        .collect::<Map<_, _>>();
    json!({"state": state, "model": model, "questions": questions})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_names_the_pinned_model_and_keeps_question_keys() {
        let questions = BTreeMap::from([
            (
                "c0".to_string(),
                JudgementQuestion::Noul {
                    instructions: json!("Does it rain?"),
                },
            ),
            (
                "c1".to_string(),
                JudgementQuestion::choice(
                    json!({"question": "Which?"}),
                    vec!["a".into(), "none".into()],
                ),
            ),
            (
                "c2".to_string(),
                JudgementQuestion::Score {
                    instructions: json!("How well?"),
                    levels: vec!["answers".into(), "unrelated".into()],
                },
            ),
        ]);
        let body = typesafe_request_body("jev-1.14.0", &json!("state"), &questions);
        assert_eq!(
            body,
            json!({
                "state": "state",
                "model": "jev-1.14.0",
                "questions": {
                    "c0": {"type": "noul", "instructions": "Does it rain?"},
                    "c1": {"type": "choice", "instructions": {"question": "Which?"},
                           "criteria": {"a": null, "none": null}},
                    "c2": {"type": "score", "instructions": "How well?",
                           "criteria": ["answers", "unrelated"]}
                }
            })
        );
    }

    #[test]
    fn a_described_choice_sends_each_description_and_null_for_the_rest() {
        let mut described = JudgementQuestion::described_choice(
            json!("Which way?"),
            vec![("from_x_to".into(), "`from` x `to`.".into())],
        );
        if let JudgementQuestion::Choice { options, .. } = &mut described {
            options.push("other".into());
        }
        let questions = BTreeMap::from([("d0".to_string(), described)]);
        let body = typesafe_request_body("jev-1.14.0", &json!({}), &questions);
        assert_eq!(
            body["questions"]["d0"]["criteria"],
            json!({"from_x_to": "`from` x `to`.", "other": null})
        );
    }
}
