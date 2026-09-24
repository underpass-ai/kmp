use super::apply_doubt::ApplyDoubt;

/// A pre-write check as it was made: which model checked the items, if any,
/// and what it doubted. Frozen whole, so resuming an apply writes the same
/// packet with the same provenance.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct FrozenCheck {
    pub checked_by: Option<String>,
    pub doubts: Vec<ApplyDoubt>,
}
