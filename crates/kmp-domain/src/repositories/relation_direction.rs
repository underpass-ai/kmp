/// Which adjacency index to read. Incoming never reverses the stored link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationDirection {
    Outgoing,
    Incoming,
}
