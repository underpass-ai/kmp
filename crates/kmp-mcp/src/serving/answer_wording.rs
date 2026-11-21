//! The words KMP asks Jev with about whether a passage states what a user
//! asked: one wording for re-ranking, the doubt band and the cross-language
//! judge, written to TypeSafe's guide (a named state field, one judgement,
//! and what a yes and a no mean).

/// The state field that holds the user's question.
pub(crate) const ASKED: &str = "user_question";
/// One question per passage: does it state what was asked?
pub(crate) const STATES: &str = "Does `passage` state the answer to `user_question`?";
pub(crate) const STATES_TRUE: &str = "the passage states the fact `user_question` asks for";
pub(crate) const STATES_FALSE: &str =
    "the passage is about the same subject but does not state the asked fact";
/// The graded variant of the doubt band (an experiment, `ask-judge.json`
/// `question: score`).
pub(crate) const HOW_WELL: &str = "How well does `passage` answer `user_question`?";
