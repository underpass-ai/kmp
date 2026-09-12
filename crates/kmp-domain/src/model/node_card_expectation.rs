/// The card revision a writer believes it is replacing.
///
/// There is no "whatever is there" option on purpose. Two readers condensing
/// the same node at the same time must not silently overwrite each other:
/// one of them declared a state that is no longer true and is told so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeCardExpectation {
    /// No card is stored for this node and language yet.
    Absent,
    /// The stored card is exactly this revision.
    CardRevision(u64),
}
