use super::path_hop::PathHop;

/// A chain of hops from one fact to another, and how much of it rests on
/// the judge rather than on declarations.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FoundPath {
    pub hops: Vec<PathHop>,
}

impl FoundPath {
    pub(crate) fn proposed(&self) -> usize {
        self.hops.iter().filter(|hop| !hop.declared).count()
    }

    pub(crate) fn confidence(&self) -> f64 {
        self.hops.iter().map(|hop| hop.confidence).product()
    }
}
