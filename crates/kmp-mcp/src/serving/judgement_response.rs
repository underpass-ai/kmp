use std::collections::BTreeMap;

use super::judgement_answer::JudgementAnswer;

/// Answers for every question of one request, merged across the provider
/// calls the budget required.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JudgementResponse {
    pub model: String,
    pub answers: BTreeMap<String, JudgementAnswer>,
    pub input_tokens: u64,
    pub requests: usize,
}

impl JudgementResponse {
    /// No questions asked: nothing answered, nothing spent.
    pub(crate) fn empty(model: &str) -> Self {
        Self {
            model: model.into(),
            answers: BTreeMap::new(),
            input_tokens: 0,
            requests: 0,
        }
    }
}
