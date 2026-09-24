use super::pair_origin::PairOrigin;

/// Two facts with no declared relation between them, proposed for a look.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CandidatePair {
    pub from: String,
    pub to: String,
    pub origin: PairOrigin,
    pub crosses_abouts: bool,
}
