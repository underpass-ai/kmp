use super::path_hop::PathHop;

/// A declared relation a path would have walked, left out because the judge
/// found its why and evidence do not hold.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AvoidedHop {
    pub hop: PathHop,
    /// The judge's probability that the declared why and evidence hold.
    pub support: f64,
}
