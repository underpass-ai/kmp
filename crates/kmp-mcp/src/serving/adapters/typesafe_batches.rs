use std::collections::BTreeMap;

use serde_json::Value;

use super::typesafe_request_body::typesafe_request_body;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;

pub(super) const STATE_AND_QUESTION_TOKENS: usize = 32_000;
pub(super) const REQUEST_TOKENS: usize = 60_000;
pub(super) const REQUEST_BYTES: usize = 256 * 1024;

/// Conservative: three bytes of JSON per token.
fn estimated_tokens(value: &Value) -> usize {
    value.to_string().len().div_ceil(3)
}

/// Splits the questions into requests that each repeat the state and fit the
/// provider's budgets, in key order.
pub(super) fn typesafe_batches(
    request: &JudgementRequest,
) -> Result<Vec<BTreeMap<String, JudgementQuestion>>, String> {
    let state_tokens = estimated_tokens(&request.state);
    let mut batches = Vec::new();
    let mut current = BTreeMap::new();
    let mut current_tokens = state_tokens;
    for (key, question) in &request.questions {
        if let JudgementQuestion::Choice { options, .. } = question
            && !(2..=255).contains(&options.len())
        {
            return Err(format!("choice `{key}` needs between 2 and 255 options"));
        }
        let single = BTreeMap::from([(key.clone(), question.clone())]);
        let tokens = estimated_tokens(&typesafe_request_body("", &Value::Null, &single));
        if state_tokens + tokens > STATE_AND_QUESTION_TOKENS {
            return Err(format!(
                "state plus question `{key}` exceeds the 32k-token judgement budget"
            ));
        }
        if !current.is_empty() && current_tokens + tokens > REQUEST_TOKENS {
            batches.push(std::mem::take(&mut current));
            current_tokens = state_tokens;
        }
        current.insert(key.clone(), question.clone());
        current_tokens += tokens;
    }
    if !current.is_empty() {
        batches.push(current);
    }
    Ok(batches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn noul(text: &str) -> JudgementQuestion {
        JudgementQuestion::Noul {
            instructions: json!(text),
        }
    }

    #[test]
    fn small_requests_stay_whole_and_large_ones_split_in_key_order() {
        let small = JudgementRequest {
            state: json!("s"),
            questions: BTreeMap::from([("a".into(), noul("q")), ("b".into(), noul("q"))]),
        };
        assert_eq!(typesafe_batches(&small).expect("fits").len(), 1);

        let passage = "x".repeat(20_000);
        let large = JudgementRequest {
            state: json!("s"),
            questions: (0..20)
                .map(|n| (format!("c{n:02}"), noul(&passage)))
                .collect(),
        };
        let batches = typesafe_batches(&large).expect("splits");
        assert!(batches.len() > 1);
        let keys = batches
            .iter()
            .flat_map(|batch| batch.keys().cloned())
            .collect::<Vec<_>>();
        assert_eq!(keys, large.questions.keys().cloned().collect::<Vec<_>>());
    }

    #[test]
    fn over_budget_questions_and_bad_choices_are_refused() {
        let huge = JudgementRequest {
            state: json!("x".repeat(100_000)),
            questions: BTreeMap::from([("a".into(), noul("q"))]),
        };
        assert!(typesafe_batches(&huge).is_err());
        for options in [
            vec!["only".to_string()],
            (0..256).map(|n| n.to_string()).collect(),
        ] {
            let bad = JudgementRequest {
                state: json!("s"),
                questions: BTreeMap::from([(
                    "a".into(),
                    JudgementQuestion::Choice {
                        instructions: json!("q"),
                        options,
                    },
                )]),
            };
            assert!(typesafe_batches(&bad).is_err());
        }
    }

    #[test]
    fn no_questions_means_no_requests() {
        let empty = JudgementRequest {
            state: json!("s"),
            questions: BTreeMap::new(),
        };
        assert!(typesafe_batches(&empty).expect("empty").is_empty());
    }
}
