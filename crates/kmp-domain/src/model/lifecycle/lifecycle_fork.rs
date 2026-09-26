use super::LifecycleSide;

/// A memory with more than one lifecycle neighbour on one side: two
/// memories that each replaced it, or one that replaced two.
///
/// The members are ordered by `(occurred, id)`, an absent clock first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleFork {
    pub at: String,
    pub side: LifecycleSide,
    pub members: Vec<String>,
}
