/// What a question asks of time, read off its words. `as_of` and `interval`
/// still stand where the caller put them; this only says what the words
/// asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum QuestionTime {
    #[default]
    /// No word about time.
    Unstated,
    /// The state that holds now: current, remaining, still.
    State,
    /// How it changed: replaced, previous, former.
    History,
}
