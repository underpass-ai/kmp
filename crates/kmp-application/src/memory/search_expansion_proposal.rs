/// One entry's proposed search expansions (P15, Doc2Query--): short ways a
/// later reader may ask for it, carried beside the write and never part of
/// the entry it commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchExpansionProposal {
    /// The entry the expansions are for, as written.
    pub entry_ref: String,
    /// The entry's text, which every expansion is read against.
    pub text: String,
    /// The expansions, in the writer's order.
    pub expansions: Vec<String>,
}
