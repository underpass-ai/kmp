use serde_json::Value;

/// One typed question for a judgement model. `instructions` may be a string
/// or structured JSON that names fields in backticks.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum JudgementQuestion {
    /// Yes/no: the answer is the probability of yes.
    Noul { instructions: Value },
    /// One option of a closed set: the answer is the choice plus the
    /// distribution over every option offered.
    Choice {
        instructions: Value,
        options: Vec<String>,
    },
    /// One grade of an ordered scale, best first: the answer is the expected
    /// grade (0 is the first level) plus the distribution over every level.
    Score {
        instructions: Value,
        levels: Vec<String>,
    },
}
