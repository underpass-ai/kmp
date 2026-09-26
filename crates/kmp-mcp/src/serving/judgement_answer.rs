use std::collections::BTreeMap;

/// A validated answer: probabilities are finite and within [0, 1], and a
/// choice is one of the options offered.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum JudgementAnswer {
    Noul {
        yes: f64,
    },
    Choice {
        choice: String,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
    /// `score` is the expected grade, in [0, levels - 1]; `probabilities`
    /// are by level, in the order the question listed them.
    Score {
        score: f64,
        probabilities: Vec<f64>,
        confidence: f64,
    },
}
