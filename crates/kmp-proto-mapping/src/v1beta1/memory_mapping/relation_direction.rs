/// Which way a stored relation points from the candidate that carries it.
///
/// A memory is as much described by what points at it as by what it points
/// to, so both directions become features and the direction stays part of
/// the feature rather than being flattened away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum RelationDirection {
    Incoming,
    Outgoing,
}
