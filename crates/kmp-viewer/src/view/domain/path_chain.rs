//! One whole chain on the loom.

use crate::view::domain::likelihood::Likelihood;
use crate::view::domain::path_step::PathStep;

/// A chain of steps from the path's start, in walking order, and how much of
/// it the judge proposes rather than a writer declared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathChain {
    /// The steps, first to last.
    pub steps: Vec<PathStep>,
    /// The product of the steps' confidences, as the search reported it.
    pub likelihood: Likelihood,
}

impl PathChain {
    /// How many steps rest on the judge rather than on a declaration.
    pub fn proposed(&self) -> usize {
        self.steps.iter().filter(|step| step.is_proposed()).count()
    }
}
