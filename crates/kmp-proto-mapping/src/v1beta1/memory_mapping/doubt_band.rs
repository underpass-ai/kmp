use super::doubt_entry::DoubtEntry;
use super::doubt_passage::DoubtPassage;

/// At most this many passages go to the judge in one batch (DESIGN L4 4f).
pub(super) const MAX_DOUBT_PASSAGES: usize = 8;

/// An ask the deterministic reading settled in doubt, and the passages a
/// judge is asked about: the cited core first, then the admitted memories
/// that could be cited, in rank order, at most [`MAX_DOUBT_PASSAGES`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoubtBand {
    pub entry: DoubtEntry,
    pub passages: Vec<DoubtPassage>,
}
