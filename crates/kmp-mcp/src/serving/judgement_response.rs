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
