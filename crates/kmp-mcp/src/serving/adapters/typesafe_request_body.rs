use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::serving::judgement_question::JudgementQuestion;

/// The provider body for one batch: `state`, the pinned `model`, and one
/// typed question per key. Choice options carry no description.
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
                JudgementQuestion::Choice {
                    instructions,
                    options,
                } => json!({
                    "type": "choice",
                    "instructions": instructions,
                    "criteria": options
                        .iter()
                        .map(|option| (option.clone(), Value::Null))
                        .collect::<Map<_, _>>(),
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
                JudgementQuestion::Choice {
                    instructions: json!({"question": "Which?"}),
                    options: vec!["a".into(), "none".into()],
                },
            ),
        ]);
        let body = typesafe_request_body("jev-1.13.0", &json!("state"), &questions);
        assert_eq!(
            body,
            json!({
                "state": "state",
                "model": "jev-1.13.0",
                "questions": {
                    "c0": {"type": "noul", "instructions": "Does it rain?"},
                    "c1": {"type": "choice", "instructions": {"question": "Which?"},
                           "criteria": {"a": null, "none": null}}
                }
            })
        );
    }
}
