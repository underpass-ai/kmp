/// What shape of answer a question asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum QuestionForm {
    #[default]
    /// One thing: which cluster, what time, why. Half of it is not an answer.
    Singular,
    /// A list of facets over a subject, or whether something is stored at
    /// all: what is found answers part of it.
    Enumerative,
}
