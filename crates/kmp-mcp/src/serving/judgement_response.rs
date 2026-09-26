use std::collections::BTreeMap;

use super::judgement_answer::JudgementAnswer;

/// Answers for every question of one request, merged across the provider
/// calls the budget required.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct JudgementResponse {
    pub model: String,
    pub answers: BTreeMap<String, JudgementAnswer>,
    pub input_tokens: u64,
    pub requests: usize,
    /// Wall time the caller waited for these answers, stamped by the
    /// telemetry wrapper. Never recorded in a cassette.
    #[serde(default, skip_serializing)]
    pub elapsed_us: u64,
}

impl JudgementResponse {
    /// No questions asked: nothing answered, nothing spent.
    pub(crate) fn empty(model: &str) -> Self {
        Self {
            model: model.into(),
            answers: BTreeMap::new(),
            input_tokens: 0,
            requests: 0,
            elapsed_us: 0,
        }
    }
}
