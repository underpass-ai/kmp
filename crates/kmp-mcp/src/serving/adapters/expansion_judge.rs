//! Whether each search expansion a writer proposed belongs to its memory
//! (P15, Doc2Query--, arXiv 2301.03266).
//!
//! The write dispatcher hands over, per memory, its stored text and the
//! expansions that passed the deterministic lint. Jev reads each pair with
//! one noul question, through the verdict book, and the dispatcher keeps
//! only those at or above the store's bar. Nothing is written here, and a
//! judge that cannot answer keeps nothing.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::write_expansions_config::WriteExpansionsConfig;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::ports::judgement_model::JudgementModel;

/// What Jev is asked of each expansion. Changing its meaning bumps
/// `JudgementSite::Expansions`'s template version.
const QUESTION: &str = "Is `expansion` a way a reader would ask for or name `memory` — a question `memory` answers, a paraphrase of it, or the same in Spanish or English — that states no fact, name, number or answer `memory` does not state?";
const STATE: &str = "Search expansions a writer proposed for its memories, each read against the memory it expands.";

/// Judges every expansion of `arguments.items` (`[{ref, text, expansions}]`)
/// and answers `{enabled, accept_at, judged_by, verdicts, jev}`, where
/// `verdicts` holds, per item, the probability of yes for each expansion in
/// order. A failed judgement answers `enabled: false` with its reason.
pub(crate) async fn judge_expansions(
    model: &dyn JudgementModel,
    config: &WriteExpansionsConfig,
    arguments: &Value,
) -> Value {
    let items = arguments
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut questions = BTreeMap::new();
    for (item_index, item) in items.iter().enumerate() {
        let text = item.get("text").and_then(Value::as_str).unwrap_or_default();
        let memory = text
            .chars()
            .take(config.excerpt_chars())
            .collect::<String>();
        for (index, expansion) in expansions(item).iter().enumerate() {
            questions.insert(
                key(item_index, index),
                JudgementQuestion::Noul {
                    instructions: json!({
                        "memory": memory,
                        "expansion": expansion,
                        "question": QUESTION,
                    }),
                },
            );
        }
    }
    let judged_by = format!("{} noul>={:.2}", model.model(), config.accept_at());
    if questions.is_empty() {
        return json!({"enabled": true, "accept_at": config.accept_at(),
                      "judged_by": judged_by, "verdicts": []});
    }
    let request = JudgementRequest {
        state: json!(STATE),
        questions,
    };
    let response = match model.evaluate(&request).await {
        Ok(response) => response,
        Err(error) => {
            return json!({"enabled": false, "reason": format!("Jev could not judge the expansions: {error}")});
        }
    };
    let mut verdicts = Vec::new();
    for (item_index, item) in items.iter().enumerate() {
        let mut yes = Vec::new();
        for index in 0..expansions(item).len() {
            match response.answers.get(&key(item_index, index)) {
                Some(JudgementAnswer::Noul { yes: probability }) => yes.push(json!(probability)),
                // An expansion the judge did not answer is not kept.
                _ => yes.push(Value::Null),
            }
        }
        verdicts.push(json!({"ref": item.get("ref").cloned().unwrap_or(Value::Null), "yes": yes}));
    }
    json!({
        "enabled": true,
        "accept_at": config.accept_at(),
        "judged_by": judged_by,
        "verdicts": verdicts,
        "jev": {
            "model": response.model,
            "requests": response.requests,
            "input_tokens": response.input_tokens,
            "elapsed_ms": response.elapsed_us / 1_000,
        },
    })
}

fn expansions(item: &Value) -> Vec<&str> {
    item.get("expansions")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn key(item: usize, expansion: usize) -> String {
    format!("x{item:04}_{expansion}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curate::application::use_cases::scripted_judgement::Scripted;
    use std::sync::Mutex;

    fn scripted(noul: f64) -> Scripted {
        Scripted {
            noul,
            choice: "none",
            confidence: 0.9,
            calls: Mutex::new(0),
        }
    }

    fn items() -> Value {
        json!({"items": [
            {"ref": "a:1", "text": "The rollout slipped.", "expansions": ["Why was the launch postponed?", "launch delay"]},
            {"ref": "a:2", "text": "Menu changed.", "expansions": []}
        ]})
    }

    #[tokio::test]
    async fn every_expansion_gets_its_probability_in_order() {
        let model = scripted(0.8);
        let answer = judge_expansions(&model, &WriteExpansionsConfig::default(), &items()).await;
        assert_eq!(answer["enabled"], true);
        assert_eq!(answer["verdicts"][0]["ref"], "a:1");
        assert_eq!(answer["verdicts"][0]["yes"], json!([0.8, 0.8]));
        assert_eq!(answer["verdicts"][1]["yes"], json!([]));
        assert_eq!(answer["accept_at"], 0.5);
        assert!(
            answer["judged_by"]
                .as_str()
                .expect("judge")
                .ends_with("noul>=0.50")
        );
        assert_eq!(answer["jev"]["requests"], 1);
        assert_eq!(*model.calls.lock().expect("calls"), 1);
    }

    #[tokio::test]
    async fn nothing_to_judge_asks_nothing() {
        let model = scripted(0.8);
        let answer = judge_expansions(
            &model,
            &WriteExpansionsConfig::default(),
            &json!({"items": []}),
        )
        .await;
        assert_eq!(answer["verdicts"], json!([]));
        assert_eq!(*model.calls.lock().expect("calls"), 0);
    }
}
