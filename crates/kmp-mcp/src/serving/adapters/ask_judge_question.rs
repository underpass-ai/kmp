use serde::Deserialize;

/// Which question a doubt band asks the judge about each passage.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum AskJudgeQuestion {
    /// Yes/no: does the passage answer the question?
    #[default]
    Noul,
    /// Four grades, best first: answers, answers in part, related without
    /// answering, unrelated (DESIGN L4 4f, the `score` experiment).
    Score,
}

impl AskJudgeQuestion {
    /// The scale of the graded question, best first.
    pub(super) const LEVELS: [&'static str; 4] =
        ["answers", "partly", "related_not_answering", "unrelated"];
}
