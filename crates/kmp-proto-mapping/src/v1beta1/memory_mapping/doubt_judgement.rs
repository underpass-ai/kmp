/// What a judge said of one passage, in thousandths: how likely it answers
/// the question, and how likely it does not. For a yes/no question the two
/// add up to 1000; for a graded one, the grades split between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoubtJudgement {
    pub answers: u16,
    pub not_answers: u16,
}
