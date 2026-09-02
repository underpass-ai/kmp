/// How far a candidate is from something the question actually matched.
///
/// The memory that explains a failure is usually upstream of it and shares
/// no vocabulary with the error text — that is exactly the case a lexical or
/// vector match cannot reach and a typed graph can. What travels back with
/// the candidate is how it was reached, so a reader can audit the path
/// instead of taking the hop on faith.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RelationReach {
    pub(super) hops: u32,
    pub(super) weight: u32,
    pub(super) from_ref: String,
    pub(super) via_relation: String,
}
