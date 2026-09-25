//! One step of a drawn path.

use crate::view::domain::likelihood::Likelihood;
use crate::view::domain::path_end::PathEnd;

/// A step from one fact to the next: a relation a writer declared, or one
/// the judge proposes and nobody has declared yet. A proposed step is a
/// suggestion drawn as one, never as proof; its `item_id` names it in the
/// frozen review so a person or agent can declare it through `kmp_curate`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathStep {
    /// Where the step starts, in walking order.
    pub from: PathEnd,
    /// Where it arrives.
    pub to: PathEnd,
    /// The declared relation, or the type the judge would choose.
    pub rel: Option<String>,
    /// Whether a writer declared it.
    pub declared: bool,
    /// Whether the walk crosses it against its stored direction.
    pub reversed: bool,
    /// The judge's confidence; certain for a declared step. For an avoided
    /// declaration, how much the judge believes its why and evidence.
    pub likelihood: Likelihood,
    /// The proposed step's id in the frozen review.
    pub item_id: Option<String>,
}

impl PathStep {
    /// Whether this step is the judge's proposal rather than a declaration.
    pub fn is_proposed(&self) -> bool {
        !self.declared
    }
}
